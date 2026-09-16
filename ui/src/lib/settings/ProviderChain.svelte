<!--
    An ordered fallback chain: tried top to bottom until one answers.

    This markup existed twice — once for web search, once for image
    generation — as near-identical copies that had already drifted apart in
    how they decided a provider counted as configured. One component, so the
    two cannot disagree again.
-->
<script lang="ts">
    export interface ChainItem {
        id: string;
        /** False greys the row and marks it skipped. */
        ready: boolean;
        /** Why it is skipped, or what it does when it is not. */
        note: string;
    }

    interface Props {
        items: ChainItem[];
        /** Given the new order. The caller persists it. */
        onreorder: (order: string[]) => void;
    }

    let { items, onreorder }: Props = $props();

    function move(from: number, to: number) {
        if (to < 0 || to >= items.length) return;
        const order = items.map((i) => i.id);
        const [moved] = order.splice(from, 1);
        order.splice(to, 0, moved);
        onreorder(order);
    }
</script>

<ol class="chain">
    {#each items as item, i (item.id)}
        <li class="link" class:skipped={!item.ready}>
            <span class="rank">{i + 1}</span>
            <span class="main">
                <span class="name">{item.id}</span>
                <span class="note">{item.note}</span>
            </span>
            <span class="actions">
                <button
                    class="tiny"
                    onclick={() => move(i, i - 1)}
                    disabled={i === 0}
                    aria-label="Move {item.id} up"
                >
                    ↑
                </button>
                <button
                    class="tiny"
                    onclick={() => move(i, i + 1)}
                    disabled={i === items.length - 1}
                    aria-label="Move {item.id} down"
                >
                    ↓
                </button>
            </span>
        </li>
    {/each}
</ol>

<style>
    .chain {
        display: flex;
        flex-direction: column;
        gap: var(--sp-1);
        list-style: none;
        margin: 0;
        padding: 0;
    }

    .link {
        display: flex;
        align-items: center;
        gap: var(--sp-3);
        padding: var(--sp-2) var(--sp-3);
        border: 1px solid var(--line);
        border-radius: var(--r-md);
        background: var(--surface-sunken);
        min-width: 0;
    }

    /* Dimmed, not hidden: the order still matters once a key is added, and a
       row that vanishes when unconfigured makes the list jump around. */
    .link.skipped .name,
    .link.skipped .rank {
        color: var(--text-faint);
    }

    .rank {
        flex: 0 0 auto;
        font-size: var(--text-xs);
        color: var(--text-muted);
        font-variant-numeric: tabular-nums;
    }

    .main {
        display: flex;
        flex-direction: column;
        gap: 1px;
        flex: 1 1 auto;
        min-width: 0;
    }

    .name {
        font-size: var(--text-sm);
        color: var(--text);
        font-family: var(--font-mono);
    }

    .note {
        font-size: var(--text-xs);
        color: var(--text-faint);
    }

    .actions {
        display: flex;
        gap: var(--sp-1);
        flex: 0 0 auto;
    }

    .actions button:disabled {
        opacity: 0.3;
        cursor: default;
    }
</style>
