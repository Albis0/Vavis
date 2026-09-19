<!--
    Which vault Vavis reads and writes.

    Vaults are discovered rather than typed in, because Obsidian already
    keeps a list of them and a hand-typed path is one typo away from a
    vault that silently contains nothing.
-->
<script lang="ts">
    import type { ConnectionTest, VaultInfo } from "../../api";
    import Section from "../Section.svelte";

    interface Props {
        vaults: VaultInfo[];
        result: ConnectionTest | undefined;
        testing: boolean;
        onpick: (path: string) => void;
        ontest: () => void;
    }

    let { vaults, result, testing, onpick, ontest }: Props = $props();
</script>

<h2>Obsidian</h2>

{#if vaults.length === 0}
    <Section
        title="No vault found"
        blurb="Obsidian does not have to be running — Vavis reads the Markdown files directly — but it needs to know where the vault is."
    >
        <p class="empty">
            Open a vault in Obsidian once, then come back and it will show up here.
        </p>
    </Section>
{:else}
    <Section
        title="Vault"
        blurb="Notes are read and written on disk, so this works whether or not Obsidian is open."
    >
        <div class="list">
            {#each vaults as vault (vault.path)}
                <button
                    class="row"
                    class:active={vault.active}
                    title={vault.path}
                    onclick={() => onpick(vault.path)}
                >
                    {vault.active ? "● " : "○ "}{vault.name}
                </button>
            {/each}
        </div>
    </Section>

    <Section title="Check">
        <div class="actions">
            <button onclick={ontest} disabled={testing}>
                {testing ? "reading…" : "test"}
            </button>
        </div>
        {#if result}
            <p class="result" class:bad={!result.ok}>
                {result.ok ? "✓" : "✕"}
                {result.detail}
            </p>
        {/if}
    </Section>
{/if}

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
