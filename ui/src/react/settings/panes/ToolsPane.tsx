/**
 * Tools: what the assistant can do, and what it asks before doing.
 *
 * The switch used to stretch the full width of the pane and push its own
 * label off to the side, because a blanket `width: 100%` reached every
 * direct child. It sits in a row of its own now.
 *
 * The list is long enough -- 58 entries -- that finding one by eye is work,
 * so it gets a filter and a count. Read-only either way: what a tool may do
 * is decided by its risk level, not per tool.
 */

import { useMemo, useState } from "react";
import type { Tool } from "../../../lib/api";
import Section from "../Section";

interface Props {
    tools: Tool[];
    fullAuthority: boolean;
    ontoggle: (on: boolean) => void;
}

type Risk = "safe" | "moderate" | "destructive";
const LEVELS: Risk[] = ["safe", "moderate", "destructive"];

export default function ToolsPane({ tools, fullAuthority, ontoggle }: Props) {
    const [filter, setFilter] = useState("");
    /** Empty means every risk level. */
    const [risk, setRisk] = useState("");

    // Filtering 58 tools on every render is real work, unlike the plain
    // expressions elsewhere in this pane -- worth memoising.
    const shown = useMemo(
        () =>
            tools.filter((t) => {
                if (risk && t.risk !== risk) return false;
                const q = filter.trim().toLowerCase();
                if (!q) return true;
                return `${t.name} ${t.description} ${t.domain}`.toLowerCase().includes(q);
            }),
        [tools, filter, risk],
    );

    /** How many tools sit at each risk level, for the filter buttons. */
    const counts = useMemo(
        () => ({
            safe: tools.filter((t) => t.risk === "safe").length,
            moderate: tools.filter((t) => t.risk === "moderate").length,
            destructive: tools.filter((t) => t.risk === "destructive").length,
        }),
        [tools],
    );

    return (
        <>
            <h2>Tools</h2>

            <Section
                title="Permissions"
                blurb="By default Vavis asks before anything destructive, and asks again after three such actions in one turn — or after reading a web page that tried to give it orders."
            >
                <label className="switch">
                    <input
                        type="checkbox"
                        checked={fullAuthority}
                        onChange={(e) => ontoggle(e.target.checked)}
                    />
                    <span>Full authority — never ask me anything</span>
                </label>
            </Section>

            <Section
                title="Registry"
                blurb="Each request is offered only the tools it needs — how many that can be at most depends on the model, since a small one loses its way in a long list where a large one does not."
            >
                <div className="controls">
                    <input
                        value={filter}
                        onChange={(e) => setFilter(e.target.value)}
                        placeholder={`Filter ${tools.length} tools…`}
                        spellCheck={false}
                        aria-label="Filter tools"
                    />
                    <div className="risks">
                        <button className={risk === "" ? "settings-chip on" : "settings-chip"} onClick={() => setRisk("")}>
                            all {tools.length}
                        </button>
                        {LEVELS.map((level) => (
                            <button
                                className={risk === level ? "settings-chip on" : "settings-chip"}
                                data-risk={level}
                                onClick={() => setRisk(risk === level ? "" : level)}
                                key={level}
                            >
                                {level} {counts[level]}
                            </button>
                        ))}
                    </div>
                </div>

                <div className="list tool-list">
                    {shown.map((tool) => (
                        <div className="entry" key={tool.name}>
                            <div className="main">
                                <span className="name">
                                    {tool.name}
                                    <span className="risk" data-risk={tool.risk}>
                                        {tool.risk}
                                    </span>
                                </span>
                                <span className="desc">{tool.description}</span>
                            </div>
                            <span className="domain">{tool.domain}</span>
                        </div>
                    ))}

                    {shown.length === 0 && <p className="empty">Nothing matches.</p>}
                </div>
            </Section>
        </>
    );
}
