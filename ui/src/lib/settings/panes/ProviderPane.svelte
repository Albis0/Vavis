<!--
    One card per provider: whether it holds a key, whether it answers, and
    which model is doing the answering.

    The card is the unit because the alternative — a chip row here and a key
    list on another screen — meant picking a provider marked "no key" told
    you what was wrong and then made you go somewhere else to fix it.
-->
<script lang="ts">
    import type { ConnectionTest, Status } from "../../api";
    import Field from "../Field.svelte";
    import Section from "../Section.svelte";

    interface Props {
        status: Status | null;
        tests: Record<string, ConnectionTest>;
        testing: string | null;
        models: string[];
        loadingModels: boolean;
        keyOpen: string | null;
        keyDraft: string;
        onpickprovider: (id: string) => void;
        onpickmodel: (model: string) => void;
        onopenkey: (id: string) => void;
        onsavekey: (id: string) => void;
        oncancelkey: () => void;
        onfetchmodels: () => void;
        ontest: (id: string) => void;
        onchange: (key: string, value: string) => void;
    }

    let {
        status,
        tests,
        testing,
        models,
        loadingModels,
        keyOpen,
        keyDraft = $bindable(),
        onpickprovider,
        onpickmodel,
        onopenkey,
        onsavekey,
        oncancelkey,
        onfetchmodels,
        ontest,
        onchange,
    }: Props = $props();
</script>

<h2>Model &amp; keys</h2>

<Section
    title="Providers"
    blurb="Keys are encrypted with Windows DPAPI, never written to the settings file, and never shown again once saved."
>
    <div class="cards">
        {#each status?.providers ?? [] as p (p.id)}
            {@const selected = p.id === status?.provider}
            {@const blocked = p.needsKey && !p.hasKey}
            <!-- A div, not a button: the card holds a key field and its own
                 buttons, and nesting those inside a button is invalid and
                 swallows their clicks. Selection is the separate control at
                 the end of the header row. -->
            <div class="card" class:selected class:blocked>
                <div class="card-head">
                    <span class="card-name">{p.id}</span>
                    <!-- Beside the name, not under it. On its own line it
                         was a row of its own for one short string, which
                         made every card taller than it had anything to
                         say. -->
                    <span class="card-model">
                        {selected ? (status?.model ?? p.defaultModel) : p.defaultModel}
                    </span>

                    <span class="card-spacer"></span>

                    {#if !p.needsKey}
                        <span class="tag key-tag" data-tone="neutral">no key needed</span>
                    {:else if p.hasKey}
                        <span class="tag key-tag" data-tone="good">key stored</span>
                    {:else}
                        <span class="tag key-tag" data-tone="warn">no key</span>
                    {/if}

                    {#if selected}
                        <span class="tag pick" data-tone="accent">answering</span>
                    {:else}
                        <button
                            class="tiny pick"
                            onclick={() => onpickprovider(p.id)}
                            title={blocked
                                ? "can be selected, but will not answer until a key is stored"
                                : `default model: ${p.defaultModel}`}
                        >
                            use this
                        </button>
                    {/if}
                </div>

                {#if tests[p.id]}
                    <p class="result" class:bad={!tests[p.id].ok}>
                        {tests[p.id].ok ? "✓" : "✕"}
                        {tests[p.id].detail}
                    </p>
                {/if}

                {#if keyOpen === p.id}
                    <!-- svelte-ignore a11y_autofocus -->
                    <input
                        class="key-input"
                        type="password"
                        autofocus
                        bind:value={keyDraft}
                        placeholder={p.hasKey
                            ? "paste a new key to replace the stored one…"
                            : "paste key…"}
                        onkeydown={(e) => {
                            if (e.key === "Enter") onsavekey(p.id);
                            if (e.key === "Escape") oncancelkey();
                        }}
                        onblur={() => onsavekey(p.id)}
                    />
                    <p class="hint small">
                        Saved on Enter or when you leave the field. Esc discards it.
                    </p>
                {/if}

                <div class="card-actions">
                    {#if p.needsKey}
                        <button class="tiny" onclick={() => onopenkey(p.id)}>
                            {keyOpen === p.id
                                ? "cancel"
                                : p.hasKey
                                  ? "replace key"
                                  : "add key"}
                        </button>
                    {/if}
                    <button
                        class="tiny"
                        disabled={blocked || testing !== null}
                        onclick={() => ontest(p.id)}
                        title={blocked ? "store a key first" : "make a real request"}
                    >
                        {testing === p.id ? "testing…" : "test"}
                    </button>
                    {#if selected}
                        <button class="tiny" onclick={onfetchmodels} disabled={loadingModels}>
                            {loadingModels ? "loading…" : "change model"}
                        </button>
                    {/if}
                </div>

                <!-- Under the card whose provider they belong to, so there is
                     never a list of models with no visible owner. -->
                {#if selected && models.length}
                    <div class="list scroll">
                        {#each models as m (m)}
                            <button
                                class="row"
                                class:active={m === status?.model}
                                onclick={() => onpickmodel(m)}
                            >
                                {m}
                            </button>
                        {/each}
                    </div>
                {/if}
            </div>
        {/each}
    </div>
</Section>

<Section
    title="Tool routing"
    blurb="A small, cheap model reads your request and decides which tools the main model needs, so only those are sent. Leave this empty to match tools by keyword instead — no extra call, no extra cost."
>
    <Field label="Router model" fallback="off — keyword matching">
        <input
            type="text"
            placeholder="e.g. llama-3.1-8b-instant"
            value={status?.routerModel ?? ""}
            onchange={(e) => onchange("routerModel", e.currentTarget.value)}
        />
    </Field>

    <p class="blurb">
        Runs on the provider and key you already use. If it is slow or fails,
        keyword matching takes over — the assistant keeps working.
    </p>
</Section>

<style>
    /* ── Provider cards ───────────────────────────────────────────────
       One provider, everything about it: its key, its model, its proof that
       it works. The alternative — a chip row here and a key list on another
       screen — meant picking a provider marked "no key" told you what was
       wrong and then made you go somewhere else to fix it. */

    .cards {
        display: flex;
        flex-direction: column;
        gap: var(--sp-3);
    }
    .card {
        display: flex;
        flex-direction: column;
        gap: var(--sp-2);
        padding: var(--sp-3);
        background: var(--surface-raised);
        border: 1px solid var(--line);
        border-radius: var(--r-lg);
        transition:
            border-color var(--fast) var(--ease),
            background var(--fast) var(--ease);
    }
    .card:hover {
        border-color: var(--line-strong);
    }
    /* Dimmed, never hidden. A provider you have no key for is still a provider
       you can choose to set up, and greying it out of existence hides the very
       card carrying the button that fixes it. */
    .card.blocked:not(.selected) .card-name,
    .card.blocked:not(.selected) .card-model {
        color: var(--text-muted);
    }
    /* The one that answers is bordered, not filled: a filled card at this size
       reads as pressed rather than as chosen, and there are several of them. */
    .card.selected {
        border-color: var(--accent-line);
        background: var(--accent-muted);
    }
    .card-head {
        display: flex;
        align-items: center;
        gap: var(--sp-2);
    }
    /* The model id gets whatever room is left over but never pushes; the spacer
       after it is what actually holds the tags and the select button against
       the right edge, so those line up down the column no matter how long each
       provider's model id happens to be. */
    .card-model {
        flex: 0 1 auto;
    }
    .card-spacer {
        flex: 1 1 var(--sp-2);
    }
    .card-name {
        font-size: var(--text-md);
        font-weight: 600;
        color: var(--text);
    }
    .card-model {
        font-family: var(--font-mono);
        font-size: var(--text-xs);
        color: var(--text-faint);
        /* Truncates rather than wraps: a long model id pushing the tags and the
           select button onto a second line would rearrange the header row of one
           card and not the others. */
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        min-width: 0;
    }
    /* Same width whether it says "use this" or "answering", so the right edge of
       the column is a straight line rather than a ragged one that shifts as the
       selection moves from card to card. */
    .pick {
        flex: 0 0 74px;
        text-align: center;
    }
    /* Wide enough for the longest of the three ("no key needed"), so the key
       state sits in a column of its own rather than sliding left and right as
       the wording changes between providers. */
    .key-tag {
        flex: 0 0 96px;
        text-align: center;
    }
    .card-actions {
        display: flex;
        gap: var(--sp-2);
        flex-wrap: wrap;
    }
    .key-input {
        width: 100%;
        font-family: var(--font-mono);
    }
    .hint.small {
        font-size: var(--text-xs);
    }
    /* Status words, one shape for all of them. `.risk` elsewhere is a 9px pill
       that was never meant to carry a phrase like "no key needed". */
    .tag {
        font-size: var(--text-xs);
        padding: 1px 8px;
        border-radius: var(--r-full);
        border: 1px solid var(--line);
        color: var(--text-faint);
        white-space: nowrap;
        flex: 0 0 auto;
    }
    .tag[data-tone="good"] {
        color: var(--success);
        border-color: rgba(74, 222, 128, 0.35);
    }
    .tag[data-tone="warn"] {
        color: var(--warning);
        border-color: rgba(245, 158, 11, 0.4);
    }
    .tag[data-tone="accent"] {
        color: var(--accent-text);
        border-color: var(--accent-line);
        background: var(--accent-muted);
    }
    .blurb {
        margin: 0;
        font-size: var(--text-xs);
        color: var(--text-muted);
        line-height: 1.5;
        max-width: 62ch;
    }
</style>
