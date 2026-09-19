<!--
    MCP servers: other people's tools, running as processes on this machine.

    Two things are deliberately loud here. The command line is shown in
    full, because it is what actually runs and it should be readable before
    it does. And the add form is folded away — most people connect a server
    once and never look at this pane again.
-->
<script lang="ts">
    import type { McpServerInfo } from "../../api";
    import Field from "../Field.svelte";
    import Section from "../Section.svelte";

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
        ontoggleserver: (id: string, enabled: boolean) => void;
        onremove: (id: string) => void;
        ontoggletool: (id: string, tool: string, enabled: boolean) => void;
        onsave: () => void;
    }

    let { servers, draft = $bindable(), ontoggleserver, onremove, ontoggletool, onsave }: Props =
        $props();

    let addOpen = $state(false);
    let expanded = $state<string | null>(null);

    /** The id is what tools are namespaced under, so it cannot be empty. */
    const canSave = $derived(
        draft.id.trim() !== "" &&
            (draft.transport === "stdio" ? draft.command.trim() !== "" : draft.url.trim() !== ""),
    );
</script>

<h2>MCP servers</h2>

<Section
    title="Connected"
    blurb="Connect any MCP server and its tools become available. A server runs as a process on this machine, so its tools always ask before running."
>
    {#if servers.length === 0}
        <p class="empty">No servers yet.</p>
    {/if}

    {#each servers as server (server.id)}
        <div class="entry" class:off={!server.enabled}>
            <div class="entry-main">
                <span class="tool-name">
                    {server.id}
                    <span class="risk" data-risk={server.connected ? "safe" : "destructive"}>
                        {server.connected ? "connected" : "offline"}
                    </span>
                </span>
                <!-- Exactly what runs, so it can be judged before it does. -->
                <span class="tool-desc selectable">{server.commandLine}</span>
                {#if server.connected}
                    <button
                        class="disclosure"
                        onclick={() => (expanded = expanded === server.id ? null : server.id)}
                    >
                        {expanded === server.id ? "▾" : "▸"}
                        {server.tools.length + server.disabled.length} tools
                    </button>
                {/if}
            </div>
            <div class="entry-actions">
                <button class="tiny" onclick={() => ontoggleserver(server.id, !server.enabled)}>
                    {server.enabled ? "disable" : "enable"}
                </button>
                <button class="danger tiny" onclick={() => onremove(server.id)}>remove</button>
            </div>
        </div>

        {#if expanded === server.id}
            <div class="list">
                {#each [...server.tools, ...server.disabled].sort() as tool (tool)}
                    {@const on = !server.disabled.includes(tool)}
                    <button
                        class="row"
                        class:active={on}
                        onclick={() => ontoggletool(server.id, tool, !on)}
                    >
                        {on ? "● " : "○ "}{tool}
                    </button>
                {/each}
            </div>
        {/if}
    {/each}
</Section>

<Section title="Add">
    <button class="disclosure" onclick={() => (addOpen = !addOpen)}>
        {addOpen ? "▾" : "▸"} add a server
    </button>

    {#if addOpen}
        <Field label="Id" required hint="tools are namespaced under this">
            <input bind:value={draft.id} placeholder="github" spellcheck="false" />
        </Field>

        <Field label="Transport" inline>
            <select bind:value={draft.transport}>
                <option value="stdio">stdio</option>
                <option value="http">http</option>
            </select>
        </Field>

        {#if draft.transport === "stdio"}
            <Field label="Command" required>
                <input bind:value={draft.command} placeholder="npx" spellcheck="false" />
            </Field>
            <Field label="Arguments" hint="space separated">
                <input
                    bind:value={draft.args}
                    placeholder="-y @modelcontextprotocol/server-github"
                    spellcheck="false"
                />
            </Field>
        {:else}
            <Field label="URL" required>
                <input bind:value={draft.url} placeholder="https://…/mcp" spellcheck="false" />
            </Field>
            <Field label="Auth header" hint="optional">
                <input bind:value={draft.headerName} placeholder="Authorization" spellcheck="false" />
            </Field>
            <Field label="Header value" hint="{'{key}'} is replaced with the secret below">
                <input
                    bind:value={draft.headerValue}
                    placeholder="Bearer {'{key}'}"
                    spellcheck="false"
                />
            </Field>
        {/if}

        <Field label="Secret" hint="optional — stored encrypted, never shown again">
            <input type="password" bind:value={draft.secret} placeholder="paste secret…" />
        </Field>

        <div class="actions">
            <button class="primary" onclick={onsave} disabled={!canSave}>add and connect</button>
        </div>
    {/if}
</Section>

<style>

    .actions {
        display: flex;
        gap: var(--sp-2);
        flex-wrap: wrap;
    }

    .empty {
        margin: 0;
        font-size: var(--text-xs);
        color: var(--text-faint);
    }
</style>
