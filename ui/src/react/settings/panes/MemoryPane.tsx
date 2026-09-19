/**
 * Remembered facts: what the assistant carries between conversations.
 *
 * A filter, because this list only grows. Forgetting is per-fact and
 * immediate — no confirmation, because a single fact is cheap to lose and
 * the assistant can be told again, where a dialog on every row would make
 * tidying up tedious enough that nobody does it.
 */

import { useState } from "react";
import type { Fact } from "../../../lib/api";
import Section from "../Section";

interface Props {
    facts: Fact[];
    onforget: (id: number) => void;
}

export default function MemoryPane({ facts, onforget }: Props) {
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
