<!--
    Remembered facts: what the assistant carries between conversations.

    A filter, because this list only grows. Forgetting is per-fact and
    immediate — no confirmation, because a single fact is cheap to lose and
    the assistant can be told again, where a dialog on every row would make
    tidying up tedious enough that nobody does it.
-->
<script lang="ts">
    import type { Fact } from "../../api";
    import Section from "../Section.svelte";

    interface Props {
        facts: Fact[];
        onforget: (id: number) => void;
    }

    let { facts, onforget }: Props = $props();

    let filter = $state("");

    /**
     * The facts to show, given what has been typed.
     *
     * Every word has to appear, in any order. Facts are written as
     * sentences ("remember that I take my coffee black"), so a plain
     * substring match would make anyone typing two words get nothing —
     * the words are almost never adjacent in the stored text.
     */
    const shown = $derived.by((): Fact[] => {
        const words = filter.toLowerCase().split(/\s+/).filter(Boolean);
        if (words.length === 0) return facts;
        return facts.filter((fact) => {
            const text = fact.text.toLowerCase();
            return words.every((word) => text.includes(word));
        });
    });
</script>

<h2>Memory</h2>

<Section
    title="Remembered facts"
    blurb="Carried between conversations. Clearing the conversation does not touch these."
>
    {#if facts.length === 0}
        <p class="empty">Nothing remembered yet. Try “remember that I…”.</p>
    {:else}
        <input
            bind:value={filter}
            placeholder="Filter {facts.length} facts…"
            spellcheck="false"
            aria-label="Filter facts"
        />

        <div class="list">
            {#each shown as fact (fact.id)}
                <div class="entry">
                    <span class="text">{fact.text}</span>
                    <button class="danger tiny" onclick={() => onforget(fact.id)}>
                        forget
                    </button>
                </div>
            {/each}

            {#if shown.length === 0}
                <p class="empty">Nothing matches.</p>
            {/if}
        </div>
    {/if}
</Section>

<style>
    input {
        background: var(--surface-sunken);
        border: 1px solid var(--line);
        border-radius: var(--r-md);
        padding: var(--sp-2) var(--sp-3);
        font-size: var(--text-sm);
    }

    .text {
        font-size: var(--text-sm);
        color: var(--text);
        line-height: 1.45;
        min-width: 0;
    }

    .entry button {
        flex: 0 0 auto;
    }

    .empty {
        margin: 0;
        padding: var(--sp-4) 0;
        font-size: var(--text-xs);
        color: var(--text-faint);
    }
</style>
