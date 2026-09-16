<!--
    A custom endpoint: the one field that is required, then the rest folded
    away.

    The old screen showed seven equal-looking inputs in a column with the
    defaults buried in placeholder text, so it read as seven things to fill
    in. Only the address is actually required — the backend's own check is
    `!url.is_empty() && url.contains("{query}")` and every other field falls
    back to a default at request time. Six optional boxes presented as
    mandatory is a worse lie than hiding them.
-->
<script lang="ts">
    import Field from "./Field.svelte";

    export interface OptionalField {
        key: string;
        label: string;
        /** What an empty value resolves to. Named, not implied. */
        fallback?: string;
        hint?: string;
    }

    interface Props {
        /** The required address. */
        url: string;
        urlPlaceholder: string;
        /** A substring the address must contain, if the backend demands one. */
        requires?: string;
        /** Explains the endpoint in a sentence. */
        blurb: string;
        fields: OptionalField[];
        /** Current values for the optional fields, keyed by `key`. */
        values: Record<string, string>;
        onchange: (patch: { url?: string; values?: Record<string, string> }) => void;
    }

    let {
        url = $bindable(),
        urlPlaceholder,
        requires = "",
        blurb,
        fields,
        values = $bindable(),
        onchange,
    }: Props = $props();

    let expanded = $state(false);

    /**
     * Checked here as well as in the backend, because the backend only
     * answers after a save: the old flow accepted the address, then raised a
     * toast about it, leaving a saved-but-broken value behind.
     */
    const urlError = $derived(
        url.trim() && requires && !url.includes(requires)
            ? `must contain ${requires}`
            : "",
    );

    /** How many optional fields the user has actually set. */
    const filled = $derived(fields.filter((f) => (values[f.key] ?? "").trim()).length);
</script>

<div class="custom">
    <p class="blurb">{blurb}</p>

    <Field
        label="Address"
        required
        error={urlError}
        hint={requires ? `must contain ${requires}` : ""}
    >
        <input
            bind:value={url}
            placeholder={urlPlaceholder}
            spellcheck="false"
            onchange={() => !urlError && onchange({ url })}
        />
    </Field>

    <button
        class="disclosure"
        onclick={() => (expanded = !expanded)}
        aria-expanded={expanded}
    >
        <span class="arrow" class:open={expanded}>›</span>
        Advanced — field mapping
        <span class="count">
            {filled > 0 ? `${filled} set` : `${fields.length} optional`}
        </span>
    </button>

    {#if expanded}
        <div class="advanced">
            {#each fields as f (f.key)}
                <Field label={f.label} fallback={f.fallback} hint={f.hint}>
                    <input
                        bind:value={values[f.key]}
                        placeholder={f.fallback ?? "optional"}
                        spellcheck="false"
                        onchange={() => onchange({ values })}
                    />
                </Field>
            {/each}
        </div>
    {/if}
</div>

<style>
    .custom {
        display: flex;
        flex-direction: column;
        gap: var(--sp-3);
    }

    .blurb {
        margin: 0;
        font-size: var(--text-xs);
        color: var(--text-muted);
        line-height: 1.5;
    }

    .disclosure {
        display: flex;
        align-items: center;
        gap: var(--sp-2);
        background: transparent;
        border: none;
        padding: var(--sp-1) 0;
        font-size: var(--text-sm);
        color: var(--text-muted);
        cursor: pointer;
        text-align: left;
    }
    .disclosure:hover {
        color: var(--text);
    }

    .arrow {
        display: inline-block;
        transition: transform var(--fast) var(--ease);
    }
    .arrow.open {
        transform: rotate(90deg);
    }

    .count {
        margin-left: auto;
        font-size: var(--text-xs);
        color: var(--text-faint);
    }

    .advanced {
        display: flex;
        flex-direction: column;
        gap: var(--sp-3);
        padding-left: var(--sp-4);
        border-left: 1px solid var(--line);
    }
</style>
