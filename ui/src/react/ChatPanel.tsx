/**
 * The chat panel.
 *
 * A narrow column docked to the right of the stage, resizable by its left
 * edge and dismissible with a keystroke. Everything else in the window is
 * the reactor and empty space, so this is the only piece of chrome the user
 * works in and it has to earn the room it takes.
 *
 * Width is persisted, so the panel comes back the size it was left. A panel
 * that resets to a default every launch is one the user has to re-drag every
 * launch, which is exactly the sort of friction the brief rules out.
 */
import {
    forwardRef,
    useEffect,
    useImperativeHandle,
    useRef,
    useState,
    type ChangeEvent,
    type ForwardedRef,
    type KeyboardEvent,
    type PointerEvent as ReactPointerEvent,
} from "react";
import ConversationList from "./ConversationList";
import Icon from "./Icon";
import Message from "./Message";
import { chat, chatSignal } from "./store/chat";
import { useStore } from "./store/useStore";
import "./styles/chatPanel.css";

interface Props {
    /** Collapses the panel. The stage keeps the shortcut to bring it back. */
    onClose: () => void;
}

/** Focused by the stage when the panel is opened by shortcut. */
export interface ChatPanelHandle {
    focus: () => void;
}

/** Bounds for the drag. Below the minimum the composer stops being usable;
    above the maximum the reactor is squeezed off centre. */
const MIN_WIDTH = 320;
const MAX_WIDTH = 720;
const DEFAULT_WIDTH = 400;
const WIDTH_KEY = "vavis.chat.width";

function ChatPanel({ onClose }: Props, ref: ForwardedRef<ChatPanelHandle>) {
    const state = useStore(chatSignal, chat);
    const status = state.status;

    const [width, setWidth] = useState(DEFAULT_WIDTH);
    const [dragging, setDragging] = useState(false);
    const [atBottom, setAtBottom] = useState(true);
    const [listOpen, setListOpen] = useState(false);

    /** Matches `App`'s switch, read from the same store rather than passed
        down: both are answering "is there anything to read yet", and two
        copies of that question can disagree. */
    const conversing = state.view === "chat" && state.messages.length > 0;

    /** Whether the chosen provider can answer. Not "is any key stored":
        Claude Code and local servers need none. */
    const ready = status?.providers?.find((p) => p.id === status.provider)?.usable ?? false;

    const feedRef = useRef<HTMLDivElement | null>(null);
    const inputRef = useRef<HTMLTextAreaElement | null>(null);

    useImperativeHandle(ref, () => ({
        focus: () => inputRef.current?.focus(),
    }));

    useEffect(() => {
        const saved = Number(localStorage.getItem(WIDTH_KEY));
        if (Number.isFinite(saved) && saved >= MIN_WIDTH && saved <= MAX_WIDTH) {
            setWidth(saved);
        }
        inputRef.current?.focus();
        // Mount only, matching the Svelte `onMount` this replaces.
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    /**
     * Follows the conversation as it grows -- but only when the user is
     * already at the bottom. Yanking the view while they read back is the
     * fastest way to make a chat feel hostile.
     */
    useEffect(() => {
        const feedEl = feedRef.current;
        if (atBottom && feedEl) {
            queueMicrotask(() =>
                feedEl.scrollTo({ top: feedEl.scrollHeight, behavior: "smooth" }),
            );
        }
    }, [
        atBottom,
        state.messages.length,
        state.messages[state.messages.length - 1]?.text,
    ]);

    function trackScroll() {
        const feedEl = feedRef.current;
        if (!feedEl) return;
        const slack = feedEl.scrollHeight - feedEl.scrollTop - feedEl.clientHeight;
        setAtBottom(slack < 80);
    }

    // -- Resize -----------------------------------------------------------

    function startResize(event: ReactPointerEvent<HTMLDivElement>) {
        event.preventDefault();
        setDragging(true);

        // Pointer capture keeps the drag alive when the cursor outruns the
        // handle, which it always does on a fast throw.
        const handle = event.currentTarget;
        handle.setPointerCapture(event.pointerId);

        const startX = event.clientX;
        const startWidth = width;
        let current = width;

        function move(e: PointerEvent) {
            // Dragging left widens: the panel is anchored to the right edge.
            const next = startWidth - (e.clientX - startX);
            current = Math.min(Math.max(next, MIN_WIDTH), MAX_WIDTH);
            setWidth(current);
        }

        function end() {
            setDragging(false);
            handle.releasePointerCapture(event.pointerId);
            handle.removeEventListener("pointermove", move);
            handle.removeEventListener("pointerup", end);
            handle.removeEventListener("pointercancel", end);
            localStorage.setItem(WIDTH_KEY, String(Math.round(current)));
        }

        handle.addEventListener("pointermove", move);
        handle.addEventListener("pointerup", end);
        handle.addEventListener("pointercancel", end);
    }

    /** Double-click on the handle restores the default width. */
    function resetWidth() {
        setWidth(DEFAULT_WIDTH);
        localStorage.setItem(WIDTH_KEY, String(DEFAULT_WIDTH));
    }

    // -- Composer ---------------------------------------------------------

    function submit() {
        void chat.send(chat.input);
        if (inputRef.current) inputRef.current.style.height = "auto";
    }

    function onKeydown(event: KeyboardEvent<HTMLTextAreaElement>) {
        // Enter sends; Shift+Enter writes a newline. Multi-line input matters
        // when pasting code or a log.
        if (event.key === "Enter" && !event.shiftKey) {
            event.preventDefault();
            submit();
            return;
        }

        // ↑/↓ walk back through what was sent, the way a shell does.
        //
        // The catch is that this is a textarea, not a one-line prompt: inside
        // a pasted log the arrows have to keep moving the caret. So history
        // only takes the key at the edge of the text -- ↑ on the first line,
        // ↓ on the last -- which is exactly where the caret has nowhere left
        // to go. A modifier means the user is selecting or jumping words;
        // leave those alone entirely.
        if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
        if (event.shiftKey || event.ctrlKey || event.altKey || event.metaKey) return;

        const inputEl = inputRef.current;
        if (!inputEl) return;

        const atStart = inputEl.selectionStart === 0 && inputEl.selectionEnd === 0;
        const end = inputEl.value.length;
        const atEnd =
            inputEl.selectionStart === end && inputEl.selectionEnd === end;

        if (event.key === "ArrowUp" && atStart) {
            if (chat.recallOlder()) {
                event.preventDefault();
                caretToEnd();
            }
        } else if (event.key === "ArrowDown" && atEnd) {
            if (chat.recallNewer()) {
                event.preventDefault();
                caretToEnd();
            }
        }
    }

    /**
     * Puts the caret after the recalled line and resizes to fit it.
     *
     * Waits a tick: the value arrives through the store, so the textarea
     * still holds the old text when the handler runs.
     */
    function caretToEnd() {
        queueMicrotask(() => {
            const inputEl = inputRef.current;
            if (!inputEl) return;
            const end = inputEl.value.length;
            inputEl.setSelectionRange(end, end);
            autoGrow();
        });
    }

    /** Grows the input up to a limit, then scrolls inside itself. */
    function autoGrow() {
        const inputEl = inputRef.current;
        if (!inputEl) return;
        inputEl.style.height = "auto";
        inputEl.style.height = `${Math.min(inputEl.scrollHeight, 200)}px`;
    }

    /**
     * Editing a recalled line makes it a new one.
     *
     * Without this, ↑ ↑ then a few keystrokes then ↓ would throw the edit
     * away and jump to the next entry — the same surprise a shell avoids.
     */
    function onInput(event: ChangeEvent<HTMLTextAreaElement>) {
        chat.leaveHistory();
        chat.input = event.target.value;
        autoGrow();
    }

    return (
        <aside
            className={dragging ? "chat-panel dragging" : "chat-panel"}
            /* While there is a conversation the panel is the main column and
               the layout sizes it, so the dragged width becomes its floor
               rather than its size -- an inline `width` would win over the
               stylesheet and pin it back to a strip. Dragging still works:
               it sets the floor, and the panel never goes below it. */
            style={
                conversing
                    ? { minWidth: `${width}px` }
                    : { width: `${width}px` }
            }
        >
            {/* Resize handle. Wider than it looks: a 1px target is a target you
                miss, so the hit area is 9px and only the line inside it is drawn. */}
            <div
                className="handle"
                role="separator"
                aria-orientation="vertical"
                aria-label="Resize chat panel"
                tabIndex={-1}
                onPointerDown={startResize}
                onDoubleClick={resetWidth}
            >
                <span className="handle-line"></span>
            </div>

            <header>
                <div className="title">
                    <span className="chat-dot" data-state={chat.coreState}></span>
                    <span className="chat-name">{status?.assistantName || "Vavis"}</span>
                </div>

                <div className="chat-header-actions">
                    <button
                        className={status?.live ? "icon-btn live-on" : "icon-btn"}
                        title={status?.live ? "End the live conversation" : "Live conversation (Gemini) — talk freely, interrupt any time"}
                        aria-pressed={status?.live ?? false}
                        onClick={() => void chat.toggleLive()}
                    >
                        <Icon name={status?.live ? "stop" : "mic"} size={16} />
                    </button>
                    <button
                        className="icon-btn"
                        title="Conversations"
                        aria-pressed={listOpen}
                        onClick={() => setListOpen((open) => !open)}
                    >
                        <Icon name="clock" size={16} />
                    </button>
                    <button
                        className="icon-btn"
                        title="New conversation (Ctrl+L)"
                        onClick={() => void chat.newConversation()}
                    >
                        <Icon name="plus" size={16} />
                    </button>
                    <button className="icon-btn" title="Hide panel (Ctrl+B)" onClick={onClose}>
                        <Icon name="panelRight" size={16} />
                    </button>
                </div>
            </header>

            {listOpen && <ConversationList onClose={() => setListOpen(false)} />}

            <div className="feed" ref={feedRef} onScroll={trackScroll}>
                {state.messages.length === 0 ? (
                    <div className="chat-empty">
                        <p className="empty-title">
                            {ready ? "What can I do?" : "Pick a provider to start"}
                        </p>
                        <p className="empty-sub">
                            {ready
                                ? "Ask a question, or tell me to do something on this machine."
                                : "Open settings: Claude Code uses your Claude plan with no key, and several providers have free tiers."}
                        </p>
                    </div>
                ) : null}

                {state.messages.map((message) => (
                    <Message key={message.id} message={message} />
                ))}

                {/* The running tool, shown live rather than only once it finishes.
                    A long tool call with no indication it is running looks like a
                    hang. */}
                {state.runningTool ? (
                    <div className="running">
                        <span className="chat-spinner"></span>
                        <span>{state.runningTool}</span>
                    </div>
                ) : null}
            </div>

            {!atBottom ? (
                <button
                    className="jump"
                    onClick={() => {
                        setAtBottom(true);
                        feedRef.current?.scrollTo({
                            top: feedRef.current.scrollHeight,
                            behavior: "smooth",
                        });
                    }}
                >
                    <Icon name="arrowDown" size={13} />
                    Latest
                </button>
            ) : null}

            <div className="composer">
                <div className={status?.busy ? "input-wrap busy" : "input-wrap"}>
                    <textarea
                        ref={inputRef}
                        value={chat.input}
                        onKeyDown={onKeydown}
                        onChange={onInput}
                        placeholder={status?.busy ? "Working…" : "Message Vavis…"}
                        rows={1}
                        spellCheck={false}
                    ></textarea>

                    <div className="tools">
                        {/* Voice is a primary way to use this, so the microphone sits in
                            the composer at the same weight as send rather than being
                            tucked into a corner. The ring around it is the live level:
                            it answers "is it hearing me" while you are still talking,
                            which no amount of status text does. */}
                        <button
                            className="icon-btn mic"
                            data-mode={status?.voiceMode ?? "off"}
                            style={{ ["--level" as string]: Math.min(state.micLevel, 1) }}
                            onClick={() => chat.cycleVoice()}
                            title={`Voice: ${status?.voiceMode ?? "off"} — click to cycle (Ctrl+M)`}
                        >
                            <Icon
                                name={status?.voiceMode === "off" ? "micOff" : "mic"}
                                size={16}
                            />
                        </button>

                        {status?.speaking ? (
                            <button
                                className="icon-btn"
                                title="Stop speaking (Esc)"
                                onClick={() => chat.stopSpeaking()}
                            >
                                <Icon name="stop" size={16} />
                            </button>
                        ) : null}

                        <button
                            className="icon-btn send"
                            onClick={submit}
                            disabled={!chat.input.trim() || status?.busy}
                            title="Send (Enter)"
                        >
                            <Icon name="send" size={16} />
                        </button>
                    </div>
                </div>
            </div>
        </aside>
    );
}

export default forwardRef(ChatPanel);
