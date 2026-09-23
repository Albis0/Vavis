/**
 * Remembered facts: what the assistant carries between conversations.
 *
 * A filter, because this list only grows. Forgetting is per-fact and
 * immediate — no confirmation, because a single fact is cheap to lose and
 * the assistant can be told again, where a dialog on every row would make
 * tidying up tedious enough that nobody does it.
 */

import { useState } from "react";
import type { Fact, MemorySettings } from "../../../lib/api";
import Field from "../Field";
import Section from "../Section";

interface Props {
    facts: Fact[];
    settings: MemorySettings | null;
    onforget: (id: number) => void;
    onchange: (field: string, value: string) => void;
}

export default function MemoryPane({ facts, settings, onforget, onchange }: Props) {
    const [filter, setFilter] = useState("");

    /**
     * The facts to show, given what has been typed.
     *
     * Every word has to appear, in any order. Facts are written as
     * sentences ("remember that I take my coffee black"), so a plain
     * substring match would make anyone typing two words get nothing —
     * the words are almost never adjacent in the stored text.
     */
    const words = filter.toLowerCase().split(/\s+/).filter(Boolean);
    const shown =
        words.length === 0
            ? facts
            : facts.filter((fact) => {
                  const text = fact.text.toLowerCase();
                  return words.every((word) => text.includes(word));
              });

    return (
        <>
            <h2>Memory</h2>

            {settings && (
                <Section
                    title="How it remembers"
                    blurb="Facts about you are kept on this machine, in the local database, and nowhere else."
                >
                    <label className="switch">
                        <input
                            type="checkbox"
                            checked={settings.inject}
                            onChange={(e) => onchange("memoryInject", String(e.target.checked))}
                        />
                        <span>Use what it knows — the facts relevant to each message go to the model with it</span>
                    </label>
                    <label className="switch">
                        <input
                            type="checkbox"
                            checked={settings.autoExtract}
                            onChange={(e) => onchange("memoryAutoExtract", String(e.target.checked))}
                        />
                        <span>Learn from conversation — when you mention something about yourself, remember it</span>
                    </label>
                    <p className="blurb">
                        Learning costs one small extra request, and only after messages where you talk about
                        yourself. Everything learned is marked “learned” below and can be forgotten.
                    </p>

                    <Field label="Search by meaning" fallback="words only">
                        <select
                            value={settings.embeddings}
                            onChange={(e) => onchange("memoryEmbeddings", e.target.value)}
                        >
                            <option value="auto">automatic — first provider with a key</option>
                            <option value="off">off — match words only</option>
                            {settings.embeddingProviders.map((p) => (
                                <option value={p} key={p}>
                                    {p}
                                </option>
                            ))}
                        </select>
                    </Field>
                    <p className="blurb">
                        {settings.embeddingsActive
                            ? `In use: ${settings.embeddingsActive}. Finds “espresso” when you say “coffee”.`
                            : "Not available — add a Gemini or GitHub key (both free) to find facts by meaning, not only by shared words."}
                    </p>
                </Section>
            )}

            <Section
                title="Remembered facts"
                blurb="Carried between conversations. Clearing the conversation does not touch these."
            >
                {facts.length === 0 ? (
                    <p className="empty">Nothing remembered yet. Try “remember that I…”.</p>
                ) : (
                    <>
                        <input
                            value={filter}
                            onChange={(e) => setFilter(e.target.value)}
                            placeholder={`Filter ${facts.length} facts…`}
                            spellCheck={false}
                            aria-label="Filter facts"
                        />

                        <div className="list">
                            {shown.map((fact) => (
                                <div className="entry" key={fact.id}>
                                    <span className="text">{fact.text}</span>
                                    {fact.source === "auto" && (
                                        <span className="tag" data-tone="neutral" title="Picked out of a conversation">
                                            learned
                                        </span>
                                    )}
                                    <button className="danger tiny" onClick={() => onforget(fact.id)}>
                                        forget
                                    </button>
                                </div>
                            ))}

                            {shown.length === 0 && <p className="empty">Nothing matches.</p>}
                        </div>
                    </>
                )}
            </Section>
        </>
    );
}
