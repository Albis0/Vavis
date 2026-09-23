/**
 * The conversation list: a drawer over the chat feed.
 *
 * Opened from the clock in the panel header. Newest first, searchable by
 * title and content, and each row can be renamed or deleted in place.
 * Opening one replaces the feed; the one you left stays in the list, so
 * starting something new never costs the old thread.
 */
import { useEffect, useState, type KeyboardEvent } from "react";
import Icon from "./Icon";
import { chat, chatSignal } from "./store/chat";
import { useStore } from "./store/useStore";
import "./styles/conversationList.css";

interface Props {
    onClose: () => void;
}

/** "3 min", "2 h", "yesterday", "12 Sep" -- enough to recognise, no more. */
export function when(unixSeconds: number, now = Date.now()): string {
    const diff = Math.max(0, now / 1000 - unixSeconds);
    if (diff < 60) return "now";
    if (diff < 3600) return `${Math.floor(diff / 60)} min`;
    if (diff < 86400) return `${Math.floor(diff / 3600)} h`;
    if (diff < 172800) return "yesterday";
    return new Date(unixSeconds * 1000).toLocaleDateString(undefined, {
        day: "numeric",
        month: "short",
    });
}

export default function ConversationList({ onClose }: Props) {
    const state = useStore(chatSignal, chat);
    const [query, setQuery] = useState("");
    const [editing, setEditing] = useState<number | null>(null);
    const [draft, setDraft] = useState("");

    // Searched in the backend: titles alone would miss the conversation you
    // remember by something that was said in it.
    useEffect(() => {
        const t = setTimeout(() => void chat.loadConversations(query), query ? 200 : 0);
        return () => clearTimeout(t);
    }, [query]);

    function startRename(id: number, title: string) {
        setEditing(id);
        setDraft(title);
    }

    async function commitRename() {
        if (editing !== null && draft.trim()) {
            await chat.renameConversation(editing, draft.trim());
        }
        setEditing(null);
    }

    function onRenameKey(e: KeyboardEvent<HTMLInputElement>) {
        if (e.key === "Enter") void commitRename();
        if (e.key === "Escape") setEditing(null);
    }

    return (
        <div className="convs" role="dialog" aria-label="Conversations">
            <div className="convs-head">
                <input
                    autoFocus
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                    onKeyDown={(e) => e.key === "Escape" && onClose()}
                    placeholder="Search conversations…"
                    spellCheck={false}
                    aria-label="Search conversations"
                />
                <button
                    className="icon-btn"
                    title="New conversation (Ctrl+L)"
                    onClick={async () => {
                        await chat.newConversation();
                        onClose();
                    }}
                >
                    <Icon name="plus" size={16} />
                </button>
                <button className="icon-btn" title="Close" onClick={onClose}>
                    <Icon name="close" size={16} />
                </button>
            </div>

            <div className="convs-list">
                {state.conversations.length === 0 && (
                    <p className="convs-empty">{query ? "Nothing matches." : "No conversations yet."}</p>
                )}
                {state.conversations.map((c) => (
                    <div className={c.current ? "conv-row current" : "conv-row"} key={c.id}>
                        {editing === c.id ? (
                            <input
                                className="conv-rename"
                                autoFocus
                                value={draft}
                                onChange={(e) => setDraft(e.target.value)}
                                onKeyDown={onRenameKey}
                                onBlur={() => void commitRename()}
                                aria-label="Conversation title"
                            />
                        ) : (
                            <button
                                className="conv-open"
                                onClick={async () => {
                                    if (!c.current) await chat.openConversation(c.id);
                                    onClose();
                                }}
                                onDoubleClick={() => startRename(c.id, c.title)}
                                title="Open · double-click to rename"
                            >
                                <span className="conv-title">{c.title || "Untitled"}</span>
                                <span className="conv-meta">
                                    {c.messageCount} · {when(c.updatedAt)}
                                </span>
                            </button>
                        )}
                        <button
                            className="icon-btn conv-delete"
                            title="Delete"
                            onClick={() => void chat.deleteConversation(c.id)}
                        >
                            <Icon name="trash" size={14} />
                        </button>
                    </div>
                ))}
            </div>
        </div>
    );
}
