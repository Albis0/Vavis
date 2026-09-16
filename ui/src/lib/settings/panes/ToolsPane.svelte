<!--
    Tools: what the assistant can do, and what it asks before doing.

    The switch used to stretch the full width of the pane and push its own
    label off to the side, because a blanket `width: 100%` reached every
    direct child. It sits in a row of its own now.

    The list is long enough -- 58 entries -- that finding one by eye is work,
    so it gets a filter and a count. Read-only either way: what a tool may do
    is decided by its risk level, not per tool.
-->
<script lang="ts">
    import type { Tool } from "../../api";
    import Section from "../Section.svelte";

    interface Props {
        tools: Tool[];
        fullAuthority: boolean;
        ontoggle: (on: boolean) => void;
    }

    let { tools, fullAuthority, ontoggle }: Props = $props();

    let filter = $state("");
    /** Empty means every risk level. */
    let risk = $state("");

    const shown = $derived(
        tools.filter((t) => {
            if (risk && t.risk !== risk) return false;
            const q = filter.trim().toLowerCase();
            if (!q) return true;
            return `${t.name} ${t.description} ${t.domain}`.toLowerCase().includes(q);
        }),
    );

    /** How many tools sit at each risk level, for the filter buttons. */
    const counts = $derived({
        safe: tools.filter((t) => t.risk === "safe").length,
        moderate: tools.filter((t) => t.risk === "moderate").length,
        destructive: tools.filter((t) => t.risk === "destructive").length,
    });
</script>

<h2>Tools</h2>

<Section
    title="Permissions"
    blurb="By default Vavis asks before anything destructive, and asks again after three such actions in one turn — or after reading a web page that tried to give it orders."
>
    <label class="switch">
        <input
            type="checkbox"
            checked={fullAuthority}
            onchange={(e) => ontoggle(e.currentTarget.checked)}
        />
        <span>Full authority — never ask me anything</span>
    </label>
</Section>

<Section
    title="Registry"
    blurb="Each request is offered only the tools it needs — how many that can be at most depends on the model, since a small one loses its way in a long list where a large one does not."
>
    <div class="controls">
        <input
            bind:value={filter}
            placeholder="Filter {tools.length} tools…"
            spellcheck="false"
            aria-label="Filter tools"
        />
        <div class="risks">
            <button class="chip" class:on={risk === ""} onclick={() => (risk = "")}>
                all {tools.length}
            </button>
            {#each ["safe", "moderate", "destructive"] as level (level)}
                <button
                    class="chip"
                    data-risk={level}
                    class:on={risk === level}
                    onclick={() => (risk = risk === level ? "" : level)}
                >
                    {level}
                    {counts[level as keyof typeof counts]}
                </button>
            {/each}
        </div>
    </div>

    <div class="list">
        {#each shown as tool (tool.name)}
            <div class="entry">
                <div class="main">
                    <span class="name">
                        {tool.name}
                        <span class="risk" data-risk={tool.risk}>{tool.risk}</span>
                    </span>
                    <span class="desc">{tool.description}</span>
                </div>
                <span class="domain">{tool.domain}</span>
            </div>
        {/each}

        {#if shown.length === 0}
            <p class="empty">Nothing matches.</p>
        {/if}
    </div>
</Section>

<style>
    .controls {
        display: flex;
        gap: var(--sp-2);
        flex-wrap: wrap;
        align-items: center;
    }
    .controls input {
        flex: 1 1 200px;
        min-width: 0;
        background: var(--surface-sunken);
        border: 1px solid var(--line);
        border-radius: var(--r-md);
        padding: var(--sp-2) var(--sp-3);
        font-size: var(--text-sm);
    }

    .risks {
        display: flex;
        gap: var(--sp-1);
        flex-wrap: wrap;
    }

    .chip {
        font-size: var(--text-xs);
        padding: var(--sp-1) var(--sp-2);
        border-radius: var(--r-full);
        border: 1px solid var(--line);
        background: transparent;
        color: var(--text-muted);
        cursor: pointer;
        font-variant-numeric: tabular-nums;
    }
    .chip.on {
        border-color: var(--accent-line, var(--line));
        color: var(--text);
        background: var(--surface-sunken);
    }

    .list {
        display: flex;
        flex-direction: column;
        gap: 2px;
        max-height: 380px;
        overflow-y: auto;
    }

    .entry {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: var(--sp-3);
        padding: var(--sp-2) 0;
        border-bottom: 1px solid var(--line);
        min-width: 0;
    }

    .main {
        display: flex;
        flex-direction: column;
        gap: 2px;
        min-width: 0;
    }

    .name {
        display: flex;
        align-items: center;
        gap: var(--sp-2);
        font-family: var(--font-mono);
        font-size: var(--text-sm);
        color: var(--text);
    }

    .desc {
        font-size: var(--text-xs);
        color: var(--text-muted);
        line-height: 1.45;
    }

    .risk {
        font-size: var(--text-xs);
        padding: 0 var(--sp-1);
        border-radius: var(--r-sm);
        color: var(--text-faint);
    }
    .risk[data-risk="moderate"] {
        color: var(--warn, #e0af68);
    }
    .risk[data-risk="destructive"] {
        color: var(--danger, #f7768e);
    }

    .domain {
        flex: 0 0 auto;
        font-size: var(--text-xs);
        color: var(--text-faint);
        font-family: var(--font-mono);
    }

    .empty {
        margin: 0;
        padding: var(--sp-4) 0;
        font-size: var(--text-xs);
        color: var(--text-faint);
    }
</style>
