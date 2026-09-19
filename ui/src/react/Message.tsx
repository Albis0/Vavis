/**
 * One turn in the conversation.
 *
 * The three kinds of line are told apart by shape before colour:
 *
 *     - The user gets a filled bubble, right-aligned and inset. What you said
 *         is findable by scanning the right edge alone.
 *     - The assistant gets plain text at full width, no bubble. Long replies
 *         in a bubble get a ragged right edge and turn into a wall; unframed
 *         prose is what every tool that handles long answers well does.
 *     - Tool calls, notices and errors are small unframed notes. They are
 *         margin annotations, and should not interrupt the thread.
 *
 * A permission request is the exception and does get a card, because it is
 * the one thing here that has to be answered. It stays in the feed rather
 * than becoming a modal: a modal takes the keyboard away from whatever was
 * being typed, every single time.
 */
import { useMemo, useState, type MouseEvent } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import Icon from "./Icon";
import { renderMarkdown } from "../lib/markdown";
import { chat, chatSignal, type Message as ChatMessage } from "./store/chat";
import { useStore } from "./store/useStore";
import { toast } from "./store/toast";
import "./styles/message.css";

interface Props {
    message: ChatMessage;
}

export default function Message({ message }: Props) {
    // Needed for `awaiting`, below, which reads the store's own idea of which
    // request is still open -- a field on `message` alone cannot tell that.
    const state = useStore(chatSignal, chat);

    const [expanded, setExpanded] = useState(false);
    const [copied, setCopied] = useState(false);

    /** Whether this tool line has anything behind it worth opening. */
    const openable =
        message.speaker === "tool" &&
        Boolean(message.args?.trim() || message.detail?.trim());

    /** True while this request is the one the agent is waiting on. */
    const awaiting =
        message.speaker === "approval" &&
        !message.decision &&
        state.approval?.messageId === message.id;

    // Only assistant replies contain markdown. Rendering user input as
    // markdown would mangle anything they paste. Memoized because parsing is
    // real work and the message this runs against does not change every
    // render -- only when its own text does.
    const html = useMemo(
        () => (message.speaker === "assistant" ? renderMarkdown(message.text) : ""),
        [message.speaker, message.text],
    );

    /**
     * Copy buttons live inside rendered markdown, so they cannot have React
     * handlers of their own -- the markup is a raw HTML string. One delegated
     * listener on the container handles them all.
     */
    function handleClick(event: MouseEvent<HTMLDivElement>) {
        const target = event.target as HTMLElement;
        if (!target.classList.contains("copy")) return;

        const code = decodeURIComponent(target.dataset.code ?? "");

        // The label used to flip to "copied" whether or not the write landed,
        // which is the one thing a confirmation must never do.
        void writeText(code)
            .then(() => {
                const original = target.textContent;
                target.textContent = "copied";
                setTimeout(() => {
                    target.textContent = original;
                }, 1200);
            })
            .catch((e) => toast.failure("Could not copy that.", e));
    }

    async function copyMessage() {
        try {
            await writeText(message.text);
            setCopied(true);
            setTimeout(() => setCopied(false), 1400);
        } catch (e) {
            toast.failure("Could not copy that.", e);
        }
    }

    async function runRecovery() {
        const fix = message.recovery;
        if (!fix || fix.done) return;
        // Marked before it runs, not after: the retry it starts can take a
        // while, and a button that stays live through it invites a second
        // press that would shorten the conversation twice.
        fix.done = "Retrying…";
        await fix.run();
        fix.done = "Done.";
    }

    if (message.speaker === "user") {
        return (
            <div className="msg-row user">
                <div className="bubble selectable">{message.text}</div>
            </div>
        );
    }

    if (message.speaker === "assistant") {
        return (
            <div className="msg-row assistant">
                <div className="reply selectable">
                    <div
                        className="msg-md"
                        onClick={handleClick}
                        role="presentation"
                        dangerouslySetInnerHTML={{ __html: html }}
                    />
                    {message.streaming ? <span className="caret"></span> : null}
                </div>

                {/* Actions appear on hover. Always-visible buttons under every reply
                    are clutter on the 90% of turns nobody copies. */}
                {!message.streaming ? (
                    <div className="msg-actions">
                        <button className="ghost-action" onClick={copyMessage} title="Copy">
                            <Icon name={copied ? "check" : "copy"} size={14} />
                            {copied ? "Copied" : "Copy"}
                        </button>
                    </div>
                ) : null}
            </div>
        );
    }

    if (message.speaker === "approval") {
        return (
            <div className={message.decision ? "approval answered" : "approval"}>
                <div className="approval-head">
                    <span className="approval-icon">
                        <Icon name="warning" size={16} />
                    </span>
                    <span className="msg-tool-name">{message.text}</span>
                    {message.decision ? (
                        <span className="decided" data-decision={message.decision}>
                            {message.decision === "deny" ? "Denied" : message.decision}
                        </span>
                    ) : null}
                </div>

                <p className="why">
                    {message.reason === "tainted"
                        ? "A page read in this turn tried to give the assistant instructions. Check this is something you asked for."
                        : message.reason === "budget"
                          ? "Several destructive actions have already run in this turn."
                          : "This action cannot be undone."}
                </p>

                <pre className="args selectable">{message.args ?? ""}</pre>

                {awaiting ? (
                    <div className="approval-actions">
                        <button className="primary" onClick={() => chat.answerApproval("allow")}>
                            Allow
                        </button>
                        <button className="outline" onClick={() => chat.answerApproval("always")}>
                            Always allow
                        </button>
                        <button className="danger" onClick={() => chat.answerApproval("deny")}>
                            Deny
                        </button>
                    </div>
                ) : null}
            </div>
        );
    }

    return (
        <div className={`msg-note ${message.speaker}`}>
            {openable ? (
                <button className="msg-note-line" onClick={() => setExpanded((v) => !v)}>
                    <span className={expanded ? "chev open" : "chev"}>
                        <Icon name="chevronRight" size={12} />
                    </span>
                    <Icon name="tool" size={13} />
                    <span className="msg-note-text">{message.text}</span>
                </button>
            ) : (
                <span className="msg-note-line static">
                    {message.speaker === "tool" ? (
                        <Icon name="tool" size={13} />
                    ) : message.speaker === "error" ? (
                        <Icon name="warning" size={13} />
                    ) : (
                        <Icon name="info" size={13} />
                    )}
                    <span className="msg-note-text selectable">{message.text}</span>
                </span>
            )}

            {message.recovery ? (
                <div className="recovery">
                    {message.recovery.done ? (
                        <span className="recovery-done">{message.recovery.done}</span>
                    ) : (
                        <button className="recovery-action" onClick={runRecovery}>
                            {message.recovery.label}
                        </button>
                    )}
                </div>
            ) : null}

            {openable && expanded ? (
                <div className="msg-detail">
                    {message.args?.trim() ? (
                        <>
                            <div className="detail-label">Called with</div>
                            <pre className="selectable">{message.args}</pre>
                        </>
                    ) : null}
                    {message.detail?.trim() ? (
                        <>
                            <div className="detail-label">Returned</div>
                            <pre className="selectable">{message.detail}</pre>
                        </>
                    ) : null}
                </div>
            ) : null}
        </div>
    );
}
