<!--
    A row per provider: whether a key is stored, and a way to set one.

    Replaces an unlabelled select-plus-password pair that sat between the
    provider chain and the test button with no heading of its own. It worked,
    but nothing said what it was for, and a layout bug squeezed the input to a
    few pixels wide — so the honest summary of the old design is that the user
    could not find where to put a key.

    Keys travel one way. Nothing here ever receives a stored key; `configured`
    is a list of ids, and a saved key is replaced, never revealed.
-->
<script lang="ts">
    interface Provider {
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

    let { providers, configured, onsave }: Props = $props();

    /** Which row has its input open. One at a time: two open password boxes
        invite pasting a key into the wrong one. */
    let open = $state<string | null>(null);
    let draft = $state("");
    let saving = $state(false);

    function begin(id: string) {
        open = id;
        draft = "";
    }

    function cancel() {
        open = null;
        draft = "";
    }

    async function commit(id: string) {
        const key = draft.trim();
        if (!key || saving) {
            cancel();
            return;
        }
        saving = true;
        try {
            await onsave(id, key);
            cancel();
        } finally {
            saving = false;
            // The draft never outlives the save, successful or not.
            draft = "";
        }
    }
</script>

<div class="keys">
    {#each providers as p (p.id)}
        {@const has = configured.includes(p.id)}
        <div class="key-row">
            <div class="who">
                <span class="name">{p.label ?? p.id}</span>
                {#if p.keyless}
                    <span class="state keyless">no key needed</span>
                {:else if has}
                    <span class="state ok">saved</span>
                {:else}
                    <span class="state missing">not set</span>
                {/if}
                {#if p.note}
                    <span class="note">{p.note}</span>
                {/if}
            </div>

            {#if !p.keyless}
                {#if open === p.id}
                    <div class="entry">
                        <!-- svelte-ignore a11y_autofocus -->
                        <input
                            type="password"
                            autofocus
                            bind:value={draft}
                            placeholder="paste key…"
                            disabled={saving}
                            onkeydown={(e) => {
                                if (e.key === "Enter") commit(p.id);
                                if (e.key === "Escape") cancel();
                            }}
                        />
                        <button onclick={() => commit(p.id)} disabled={saving}>
                            {saving ? "saving…" : "save"}
                        </button>
                        <button class="ghost" onclick={cancel} disabled={saving}>
                            cancel
                        </button>
                    </div>
                {:else}
                    <button class="tiny" onclick={() => begin(p.id)}>
                        {has ? "replace" : "add key"}
                    </button>
                {/if}
            {/if}
        </div>
    {/each}
</div>

<style>
    .keys {
        display: flex;
        flex-direction: column;
        gap: var(--sp-1);
    }

    .key-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--sp-3);
        padding: var(--sp-2) var(--sp-3);
        border: 1px solid var(--line);
        border-radius: var(--r-md);
        background: var(--surface-sunken);
        min-width: 0;
    }

    .who {
        display: flex;
        align-items: baseline;
        gap: var(--sp-2);
        flex-wrap: wrap;
        min-width: 0;
    }

    .name {
        font-size: var(--text-sm);
        color: var(--text);
        font-family: var(--font-mono);
    }

    .state {
        font-size: var(--text-xs);
    }
    .state.ok {
        color: var(--ok, #9ece6a);
    }
    .state.missing {
        color: var(--text-faint);
    }
    .state.keyless {
        color: var(--text-muted);
    }

    .note {
        font-size: var(--text-xs);
        color: var(--text-faint);
    }

    /* The input gets the room, the buttons take what they need. Sized here
       rather than inherited, which is what used to collapse it to nothing. */
    .entry {
        display: flex;
        gap: var(--sp-1);
        flex: 1 1 320px;
        min-width: 0;
    }
    .entry input {
        flex: 1 1 auto;
        min-width: 0;
    }
    .entry button {
        flex: 0 0 auto;
    }

    .ghost {
        background: transparent;
    }
</style>
