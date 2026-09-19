/**
 * Application state.
 *
 * Plain fields on a class, wrapped in an `observable` proxy so writes
 * announce themselves. This used to be Svelte `$state`, which only works in
 * files the Svelte compiler processes -- React cannot see it.
 *
 * The one rule the proxy imposes: **reassign, never mutate in place.**
 * `messages.push(x)` writes through the array and the proxy never sees it;
 * `messages = [...messages, x]` does. The Svelte version had the same
 * requirement for arrays crossing a component boundary, so the call sites
 * were already close -- but the six places that did mutate are rewritten
 * below, because here they fail silently rather than loudly.
 *
 * The store owns *interface* state only. Conversation history for the
 * model, keys and settings live in Rust; this mirrors what needs drawing.
 */

import {
    api,
    on,
    type ApprovalEvent,
    type AutomationEvent,
    type DeltaEvent,
    type DoneEvent,
    type ErrorEvent,
    type NoticeEvent,
    type Status,
    type ToolDoneEvent,
    type ToolStartEvent,
    type VoiceEvent,
} from "../../lib/api";
import { observable, Signal } from "./reactive";
import { ask } from "./confirm";
import { toast } from "./toast";

export type Speaker =
    | "user"
    | "assistant"
    | "system"
    | "error"
    | "tool"
    /**
   * A permission request, asked inline.
   *
   * It is a message rather than a modal on purpose: a modal steals focus the
   * moment it appears, which is intolerable when the thing it interrupts is
   * the sentence you were typing. Inline, the agent still waits — it just
   * waits where the rest of the conversation is.
   */
    | "approval";

export interface Message {
    id: number;
    speaker: Speaker;
    text: string;
    /** Tool messages carry their outcome, for the ✓ / ✗ marker. */
    ok?: boolean;
    /** True while a reply is still streaming in. */
    streaming?: boolean;
    /** Tool messages: what it was called with, shown when expanded. */
    args?: string;
    /** Tool messages: the fuller output, shown when expanded. */
    detail?: string;
    /** Approval messages: why permission is being asked. */
    reason?: "risk" | "budget" | "tainted";
    /** Approval messages: what the user chose, once they have chosen. */
    decision?: "allow" | "always" | "deny";
    /**
     * An error the user can do something about, and the thing to do.
     *
     * An error that names a remedy in its text but offers no way to reach it
     * is barely better than one that says nothing: "clear the conversation"
     * still leaves you hunting for where that lives. When there is a fix,
     * the message carries the button for it.
     */
    recovery?: {
        label: string;
        run: () => void | Promise<void>;
        /** Set once it has been used, so it cannot be run twice. */
        done?: string;
    };
    at: number;
}

export interface PendingApproval {
    tool: string;
    args: string;
    reason: "risk" | "budget" | "tainted";
    /** The feed message showing this request, so the answer can land on it. */
    messageId: number;
}

/**
 * Which interface is on screen.
 *
 * Chat is the default; the others are modes reached from it rather than
 * separate windows, so the conversation is never left behind.
 */
export type Interface = "chat" | "code" | "canvas" | "council";

/** What the assistant is doing — drives the core animation. */
export type CoreState =
    | "idle"
    | "listening"
    | "thinking"
    | "speaking"
    | "working";

let nextId = 1;

/**
 * How many sent lines ↑/↓ can reach.
 *
 * Twenty is what a shell session's worth of arrowing actually covers; past
 * that people search rather than step.
 */
const HISTORY_LIMIT = 20;

/**
 * Exported for the tests, which need an instance of their own: the singleton
 * below is shared, and a test that arrowed through its history would leave
 * that history behind for the next one.
 */
export class ChatStore {
    messages: Message[] = [];
    input = "";
    status: Status | null = null;
    approval: PendingApproval | null = null;
    runningTool: string | null = null;
    /** Arguments of the running tool, held until its result arrives. */
    runningArgs = "";
    /** Microphone level, 0.0–1.0. Polled faster than the rest of the status. */
    micLevel = 0;

    /**
     * What the user has sent, newest last — the shell history behind ↑/↓.
     *
     * Kept apart from `messages` on purpose: the feed holds replies, tool
     * lines and approvals too, and arrowing through those would step past
     * things the user never typed. Capped, because a session can run long
     * and nobody arrows back past twenty.
     */
    history: string[] = [];

    /**
     * Where ↑/↓ currently sits. `null` means "not browsing" — the composer
     * holds a live draft rather than a recalled line.
     */
    historyAt: number | null = null;

    /**
     * What was being typed when browsing started, so ↓ past the newest entry
     * gives it back instead of clearing the box.
     */
    private draft = "";
    /** Panel currently open in the right rail, if any. */
    panel: "none" | "settings" | "memory" | "automations" | "tools" = "none";
    /** Which interface is showing. */
    view: Interface = "chat";

    private unlisteners: (() => void)[] = [];

    /** The state the core visual should show. */
    get coreState(): CoreState {
        if (this.status?.speaking) return "speaking";
        if (this.runningTool) return "working";
        if (this.status?.busy) return "thinking";
        if (this.status && this.status.voiceMode !== "off") return "listening";
        return "idle";
    }

    add(speaker: Speaker, text: string, extra: Partial<Message> = {}): Message {
        const message: Message = {
            id: nextId++,
            speaker,
            text,
            at: Date.now(),
            ...extra,
        };
        // Reassign: the proxy watches the field, not the array.
        this.messages = [...this.messages, message];
        return message;
    }

    /**
   * Appends a chunk to the streaming reply, starting one if needed.
   *
   * Deltas arrive many times a second; creating a message per chunk would
   * fill the feed with fragments.
   */
    appendDelta(text: string) {
        const last = this.messages[this.messages.length - 1];
        if (last?.speaker === "assistant" && last.streaming) {
            // A new object for the last message, not `last.text += text`:
            // writing through to the existing one never reaches the proxy,
            // and this is the hottest path in the app.
            this.messages = [
                ...this.messages.slice(0, -1),
                { ...last, text: last.text + text },
            ];
        } else {
            this.add("assistant", text, { streaming: true });
        }
    }

    finishStreaming() {
        const last = this.messages[this.messages.length - 1];
        if (!last?.streaming) return;

        // A reply that produced only tool calls has no text of its own; an
        // empty bubble would just be noise.
        this.messages = last.text.trim()
            ? [...this.messages.slice(0, -1), { ...last, streaming: false }]
            : this.messages.slice(0, -1);
    }

    async refresh() {
        try {
            this.status = await api.status();
        } catch (e) {
            console.error("status failed", e);
        }
    }

    async send(text: string) {
        const trimmed = text.trim();
        if (!trimmed || this.status?.busy) return;

        this.add("user", trimmed);
        this.remember(trimmed);
        this.input = "";

        try {
            await api.send(trimmed);
            await this.refresh();
        } catch (e) {
            this.add("error", String(e));
        }
    }

    /**
     * Files a sent line into the history and leaves browsing.
     *
     * Repeating the last line does not add a second copy — arrowing past four
     * identical "devam" entries is nobody's idea of history.
     */
    private remember(text: string) {
        if (this.history[this.history.length - 1] !== text) {
            const grown = [...this.history, text];
            this.history =
                grown.length > HISTORY_LIMIT ? grown.slice(-HISTORY_LIMIT) : grown;
        }
        this.historyAt = null;
        this.draft = "";
    }

    /**
     * ↑ — one step towards older. Returns false when there is nothing to
     * recall, so the composer can let the key do its ordinary job.
     */
    recallOlder(): boolean {
        if (this.history.length === 0) return false;

        if (this.historyAt === null) {
            // Hold the half-typed line so ↓ can hand it back.
            this.draft = this.input;
            this.historyAt = this.history.length - 1;
        } else if (this.historyAt > 0) {
            this.historyAt -= 1;
        } else {
            return true; // Already at the oldest; stay there.
        }

        this.input = this.history[this.historyAt];
        return true;
    }

    /** ↓ — one step towards newer, ending on the draft that was interrupted. */
    recallNewer(): boolean {
        if (this.historyAt === null) return false;

        if (this.historyAt < this.history.length - 1) {
            this.historyAt += 1;
            this.input = this.history[this.historyAt];
        } else {
            // Past the newest entry: back to what was being typed.
            this.historyAt = null;
            this.input = this.draft;
            this.draft = "";
        }
        return true;
    }

    /** Leaves history browsing — typing anything counts as a new line. */
    leaveHistory() {
        this.historyAt = null;
    }

    /** The most recent thing the user said, for retrying it. */
    lastUserText(): string {
        for (let i = this.messages.length - 1; i >= 0; i--) {
            if (this.messages[i].speaker === "user") return this.messages[i].text;
        }
        return "";
    }

    /**
     * Makes room, then asks the question again.
     *
     * The feed is left alone. Only what the *model* is sent gets shorter —
     * scrolling back through what was said is not what made the request too
     * big, and taking it away would be a second loss on top of the first.
     */
    async forgetAndRetry(text: string) {
        try {
            const dropped = await api.forgetOldest();
            if (dropped === 0) {
                // A conversation too short to trim, which means the size is
                // coming from one very long message rather than from many.
                // Saying so beats a button that quietly does nothing.
                toast.warning(
                    "This conversation is already short — the last message is too long on its own.",
                );
                return;
            }

            this.add("system", `Forgot the oldest ${dropped} messages.`);
            await this.refresh();

            if (!text) return;

            // `send` returns quietly when a turn is already running, which
            // would leave the button reading "Done." over a retry that never
            // happened. The room has been made either way, so say what state
            // things are actually in rather than pretending it went.
            if (this.status?.busy) {
                toast.info("Made room. Send your message again when the reply finishes.");
                return;
            }
            await this.send(text);
        } catch (e) {
            toast.failure("Could not shorten the conversation.", e);
        }
    }

    async clear() {
        await api.clear();
        this.messages = [];
        this.add("system", "Conversation cleared. Remembered facts are kept.");
        await this.refresh();
    }

    /**
     * Clears the conversation, having asked first.
     *
     * Ctrl+L is one keystroke from Ctrl+K and there is no undo, so the raw
     * `clear` above is no longer wired to anything the user can hit by
     * accident. Both entry points — the shortcut and the button in settings —
     * come through here.
     */
    async clearWithConfirm() {
        const confirmed = await ask({
            title: "Start a new conversation?",
            body:
                this.messages.length > 0
                    ? "This conversation is discarded and cannot be brought back. Remembered facts are kept."
                    : "Remembered facts are kept.",
            confirmLabel: "New conversation",
            cancelLabel: "Keep it",
            danger: true,
        });
        if (!confirmed) return;

        try {
            await this.clear();
        } catch (e) {
            toast.failure("Could not clear the conversation.", e);
        }
    }

    async answerApproval(decision: "allow" | "always" | "deny") {
        const pending = this.approval;
        // Cleared first: the agent thread unblocks the moment the command lands,
        // and a second click while it is in flight would answer twice.
        this.approval = null;
        if (!pending) return;

        // The request stays in the feed, marked with what was decided. Removing
        // it would leave no record that a destructive action was ever offered.
        const message = this.messages.find((m) => m.id === pending.messageId);
        if (message) message.decision = decision;

        await api.answerApproval(decision);
    }

    async cycleVoice() {
        try {
            const mode = await api.cycleVoice();
            this.add("system", `Voice: ${mode}`);
            await this.refresh();
        } catch (e) {
            this.add("error", String(e));
        }
    }

    async stopSpeaking() {
        await api.stopSpeaking();
        await this.refresh();
    }

    /** Wires up backend events and the status poll. */
    async start() {
        const restored = await api.loadHistory();
        for (const line of restored) {
            this.add(line.role === "user" ? "user" : "assistant", line.content);
        }

        // Seed ↑ from the restored conversation, so history survives a
        // restart the way a shell's does.
        this.history = restored
            .filter((line) => line.role === "user")
            .map((line) => line.content)
            .slice(-HISTORY_LIMIT);
        if (restored.length > 0) {
            this.add("system", `${restored.length} messages restored`);
        }

        await this.refresh();

        if (!this.status?.keys.length) {
            this.add(
                "system",
                "No API key yet. Open settings, or type /key groq <your-key>.",
            );
        }

        this.unlisteners = await Promise.all([
            on<DeltaEvent>("chat:delta", (p) => this.appendDelta(p.text)),

            on<DoneEvent>("chat:done", () => {
                this.finishStreaming();
                this.runningTool = null;
                void this.refresh();
            }),

            on<ErrorEvent>("chat:error", (p) => {
                this.finishStreaming();
                this.runningTool = null;

                // The failed question is still in the box below, because the
                // backend dropped it from the model's history rather than
                // leaving two user turns in a row. Handing it back means the
                // recovery is one press rather than retyping the message.
                const failed = this.lastUserText();

                this.add("error", p.message, {
                    recovery: p.tooLong
                        ? {
                              label: "Forget the oldest half and retry",
                              run: () => this.forgetAndRetry(failed),
                          }
                        : undefined,
                });
                void this.refresh();
            }),

            // A rate-limit wait, said out loud. Without it the turn just
            // stops for twenty seconds and reads as a hang.
            on<NoticeEvent>("chat:notice", (p) => {
                this.finishStreaming();
                this.add("system", p.text);
            }),

            on<ToolStartEvent>("chat:tool-start", (p) => {
                // Close the streaming bubble: the tool line belongs between the
                // model's text and whatever it says next.
                this.finishStreaming();
                this.runningTool = p.tool;
                this.runningArgs = p.args;
            }),

            on<ToolDoneEvent>("chat:tool-done", (p) => {
                this.add("tool", `${p.tool} — ${p.summary}`, {
                    ok: p.ok,
                    // Kept on the message so the line can be opened later, not only
                    // while it is the most recent thing that happened.
                    args: this.runningArgs,
                    detail: p.detail,
                });
                this.runningTool = null;
                this.runningArgs = "";
            }),

            on<ApprovalEvent>("chat:approval", (p) => {
                this.finishStreaming();
                const message = this.add("approval", p.tool, {
                    args: p.args,
                    reason: p.reason,
                });
                this.approval = { ...p, messageId: message.id };
            }),

            on<VoiceEvent>("voice", (event) => {
                switch (event.kind) {
                    case "heard":
                        void this.send(event.text);
                        break;
                    case "woke":
                        this.add("system", "Listening…");
                        break;
                    case "notice":
                        this.add("system", event.text);
                        break;
                    case "speaking":
                        void this.refresh();
                        break;
                }
            }),

            on<AutomationEvent>("automation", (p) => {
                this.add("system", `Automation fired — ${p.trigger}`);
                void this.send(p.prompt);
            }),

            // The Spotify consent round trip finishes on a worker thread, long
            // after the command that started it returned.
            on<{ ok: boolean; message: string }>("spotify:auth", (p) => {
                this.add("system", p.message);
            }),
        ]);

        // Poll for telemetry. One second is enough for CPU and battery, and
        // the core animation runs on CSS, not on this.
        const timer = setInterval(() => void this.refresh(), 1000);
        this.unlisteners.push(() => clearInterval(timer));

        // The level meter needs to move with the voice, not once a second — but
        // only while something is listening. `get_status` is far too heavy to ask
        // ten times a second, so this reads the level alone.
        const levelTimer = setInterval(() => {
            if (!this.status || this.status.voiceMode === "off") {
                this.micLevel = 0;
                return;
            }
            void api
                .micLevel()
                .then((level) => (this.micLevel = level))
                .catch(() => (this.micLevel = 0));
        }, 100);
        this.unlisteners.push(() => clearInterval(levelTimer));
    }

    stop() {
        for (const off of this.unlisteners) off();
        this.unlisteners = [];
    }
}

/**
 * The signal behind the singleton, for `useStore(chatSignal, chat)`.
 *
 * Exported separately rather than hung off the store: the store's fields are
 * the app's vocabulary, and a `signal` among them invites someone to read it
 * as state.
 */
export const chatSignal = new Signal();

export const chat = observable(new ChatStore(), chatSignal);
