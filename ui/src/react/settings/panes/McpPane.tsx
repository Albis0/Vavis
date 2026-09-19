/**
 * MCP servers: other people's tools, running as processes on this machine.
 *
 * Two things are deliberately loud here. The command line is shown in
 * full, because it is what actually runs and it should be readable before
 * it does. And the add form is folded away — most people connect a server
 * once and never look at this pane again.
 */

import { useState } from "react";
import type { McpServerInfo } from "../../../lib/api";
import Field from "../Field";
import Section from "../Section";

export interface Draft {
    id: string;
    transport: string;
    command: string;
    args: string;
    url: string;
    headerName: string;
    headerValue: string;
    secret: string;
}

interface Props {
    servers: McpServerInfo[];
    draft: Draft;
    onDraftChange: (draft: Draft) => void;
    ontoggleserver: (id: string, enabled: boolean) => void;
    onremove: (id: string) => void;
    ontoggletool: (id: string, tool: string, enabled: boolean) => void;
    onsave: () => void;
}

export default function McpPane({
    servers,
    draft,
    onDraftChange,
    ontoggleserver,
    onremove,
    ontoggletool,
    onsave,
}: Props) {
    const [addOpen, setAddOpen] = useState(false);
    const [expanded, setExpanded] = useState<string | null>(null);

    /** The id is what tools are namespaced under, so it cannot be empty. */
    const canSave =
        draft.id.trim() !== "" &&
        (draft.transport === "stdio" ? draft.command.trim() !== "" : draft.url.trim() !== "");

    function patch(fields: Partial<Draft>) {
        onDraftChange({ ...draft, ...fields });
    }

    return (
        <>
            <h2>MCP servers</h2>

            <Section
                title="Connected"
                blurb="Connect any MCP server and its tools become available. A server runs as a process on this machine, so its tools always ask before running."
            >
                {servers.length === 0 && <p className="empty">No servers yet.</p>}

                {servers.map((server) => (
                    <div key={server.id}>
                        <div className={server.enabled ? "entry" : "entry off"}>
                            <div className="entry-main">
                                <span className="tool-name">
                                    {server.id}
                                    <span
                                        className="risk"
                                        data-risk={server.connected ? "safe" : "destructive"}
                                    >
                                        {server.connected ? "connected" : "offline"}
                                    </span>
                                </span>
                                {/* Exactly what runs, so it can be judged before it does. */}
                                <span className="tool-desc selectable">{server.commandLine}</span>
                                {server.connected && (
                                    <button
                                        className="disclosure"
                                        onClick={() =>
                                            setExpanded(expanded === server.id ? null : server.id)
                                        }
                                    >
                                        {expanded === server.id ? "▾" : "▸"}
                                        {" "}
                                        {server.tools.length + server.disabled.length} tools
                                    </button>
                                )}
                            </div>
                            <div className="entry-actions">
                                <button
                                    className="tiny"
                                    onClick={() => ontoggleserver(server.id, !server.enabled)}
                                >
                                    {server.enabled ? "disable" : "enable"}
                                </button>
                                <button className="danger tiny" onClick={() => onremove(server.id)}>
                                    remove
                                </button>
                            </div>
                        </div>

                        {expanded === server.id && (
                            <div className="list">
                                {[...server.tools, ...server.disabled].sort().map((tool) => {
                                    const on = !server.disabled.includes(tool);
                                    return (
                                        <button
                                            className={on ? "row active" : "row"}
                                            onClick={() => ontoggletool(server.id, tool, !on)}
                                            key={tool}
                                        >
                                            {on ? "● " : "○ "}
                                            {tool}
                                        </button>
                                    );
                                })}
                            </div>
                        )}
                    </div>
                ))}
            </Section>

            <Section title="Add">
                <button className="disclosure" onClick={() => setAddOpen(!addOpen)}>
                    {addOpen ? "▾" : "▸"} add a server
                </button>

                {addOpen && (
                    <>
                        <Field label="Id" required hint="tools are namespaced under this">
                            <input
                                value={draft.id}
                                onChange={(e) => patch({ id: e.target.value })}
                                placeholder="github"
                                spellCheck={false}
                            />
                        </Field>

                        <Field label="Transport" inline>
                            <select
                                value={draft.transport}
                                onChange={(e) => patch({ transport: e.target.value })}
                            >
                                <option value="stdio">stdio</option>
                                <option value="http">http</option>
                            </select>
                        </Field>

                        {draft.transport === "stdio" ? (
                            <>
                                <Field label="Command" required>
                                    <input
                                        value={draft.command}
                                        onChange={(e) => patch({ command: e.target.value })}
                                        placeholder="npx"
                                        spellCheck={false}
                                    />
                                </Field>
                                <Field label="Arguments" hint="space separated">
                                    <input
                                        value={draft.args}
                                        onChange={(e) => patch({ args: e.target.value })}
                                        placeholder="-y @modelcontextprotocol/server-github"
                                        spellCheck={false}
                                    />
                                </Field>
                            </>
                        ) : (
                            <>
                                <Field label="URL" required>
                                    <input
                                        value={draft.url}
                                        onChange={(e) => patch({ url: e.target.value })}
                                        placeholder="https://…/mcp"
                                        spellCheck={false}
                                    />
                                </Field>
                                <Field label="Auth header" hint="optional">
                                    <input
                                        value={draft.headerName}
                                        onChange={(e) => patch({ headerName: e.target.value })}
                                        placeholder="Authorization"
                                        spellCheck={false}
                                    />
                                </Field>
                                <Field label="Header value" hint="{key} is replaced with the secret below">
                                    <input
                                        value={draft.headerValue}
                                        onChange={(e) => patch({ headerValue: e.target.value })}
                                        placeholder="Bearer {key}"
                                        spellCheck={false}
                                    />
                                </Field>
                            </>
                        )}

                        <Field label="Secret" hint="optional — stored encrypted, never shown again">
                            <input
                                type="password"
                                value={draft.secret}
                                onChange={(e) => patch({ secret: e.target.value })}
                                placeholder="paste secret…"
                            />
                        </Field>

                        <div className="actions">
                            <button className="primary" onClick={onsave} disabled={!canSave}>
                                add and connect
                            </button>
                        </div>
                    </>
                )}
            </Section>
        </>
    );
}
