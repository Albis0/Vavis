/**
 * A row per provider: whether a key is stored, and a way to set one.
 *
 * Replaces an unlabelled select-plus-password pair that sat between the
 * provider chain and the test button with no heading of its own. It worked,
 * but nothing said what it was for, and a layout bug squeezed the input to a
 * few pixels wide — so the honest summary of the old design is that the user
 * could not find where to put a key.
 *
 * Keys travel one way. Nothing here ever receives a stored key; `configured`
 * is a list of ids, and a saved key is replaced, never revealed.
 */

import { useState } from "react";

export interface Provider {
    id: string;
    /** Shown instead of the id when the id is not a word people know. */
    label?: string;
    /** Providers that work without a key say so rather than looking unset. */
    keyless?: boolean;
    /** Where to get one. */
    note?: string;
}

interface Props {
    providers: Provider[];
    /** Ids that already have a key stored. */
    configured: string[];
    /** Saves a key. Returning a rejected promise leaves the box open. */
    onsave: (provider: string, key: string) => Promise<void>;
}

export default function KeyInput({ providers, configured, onsave }: Props) {
    /** Which row has its input open. One at a time: two open password boxes
        invite pasting a key into the wrong one. */
    const [open, setOpen] = useState<string | null>(null);
    const [draft, setDraft] = useState("");
    const [saving, setSaving] = useState(false);

    function begin(id: string) {
        setOpen(id);
        setDraft("");
    }

    function cancel() {
        setOpen(null);
        setDraft("");
    }

    async function commit(id: string) {
        const key = draft.trim();
        if (!key || saving) {
            cancel();
            return;
        }
        setSaving(true);
        try {
            await onsave(id, key);
            cancel();
        } finally {
            setSaving(false);
            // The draft never outlives the save, successful or not.
            setDraft("");
        }
    }

    return (
        <div className="keys">
            {providers.map((p) => {
                const has = configured.includes(p.id);
                return (
                    <div className="key-row" key={p.id}>
                        <div className="who">
                            <span className="name">{p.label ?? p.id}</span>
                            {p.keyless ? (
                                <span className="state keyless">no key needed</span>
                            ) : has ? (
                                <span className="state ok">saved</span>
                            ) : (
                                <span className="state missing">not set</span>
                            )}
                            {p.note && <span className="note">{p.note}</span>}
                        </div>

                        {!p.keyless &&
                            (open === p.id ? (
                                <div className="key-entry">
                                    <input
                                        type="password"
                                        autoFocus
                                        value={draft}
                                        onChange={(e) => setDraft(e.target.value)}
                                        placeholder="paste key…"
                                        disabled={saving}
                                        onKeyDown={(e) => {
                                            if (e.key === "Enter") void commit(p.id);
                                            if (e.key === "Escape") cancel();
                                        }}
                                    />
                                    <button onClick={() => void commit(p.id)} disabled={saving}>
                                        {saving ? "saving…" : "save"}
                                    </button>
                                    <button className="ghost" onClick={cancel} disabled={saving}>
                                        cancel
                                    </button>
                                </div>
                            ) : (
                                <button className="tiny" onClick={() => begin(p.id)}>
                                    {has ? "replace" : "add key"}
                                </button>
                            ))}
                    </div>
                );
            })}
        </div>
    );
}
