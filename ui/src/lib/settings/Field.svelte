<!--
    One labelled setting: a name, an optional explanation, and the control.

    Why a component rather than a class: the old screen relied on
    `.pane > * { width: 100% }` to lay rows out, which also stretched every
    flex child. A select beside an input ate the whole row and squeezed the
    input to nothing — that is why the web-search key box looked like it did
    not exist, and why the "full authority" checkbox pushed its own label off
    the side. Layout belongs to the row that owns it, not to a rule applied to
    everything in the pane.
-->
<script lang="ts">
    interface Props {
        label: string;
        /** Sits under the label, in quieter type. */
        hint?: string;
        /** Marks the field as the one that must be filled. */
        required?: boolean;
        /** What an empty value falls back to, named rather than implied. */
        fallback?: string;
        /** Shown in place of the hint when something is wrong. */
        error?: string;
        /** Puts the control beside the label instead of under it. */
        inline?: boolean;
        children: import("svelte").Snippet;
    }

    let {
        label,
        hint = "",
        required = false,
        fallback = "",
        error = "",
        inline = false,
        children,
    }: Props = $props();
</script>

<div class="setting" class:inline>
    <div class="head">
        <span class="label">
            {label}
            {#if required}<span class="req" title="Required">*</span>{/if}
        </span>
        {#if error}
            <span class="error">{error}</span>
        {:else if fallback}
            <span class="fallback">default: {fallback}</span>
        {:else if hint}
            <span class="hint">{hint}</span>
        {/if}
    </div>
    <div class="control">
        {@render children()}
    </div>
</div>

<style>
    .setting {
        display: flex;
        flex-direction: column;
        gap: var(--sp-2);
    }

    /* Label and control side by side, for a switch or a short select where a
       full-width row would put the two far apart on a wide window. */
    .setting.inline {
        flex-direction: row;
        align-items: center;
        justify-content: space-between;
        gap: var(--sp-4);
    }
    .setting.inline .control {
        flex: 0 0 auto;
    }

    .head {
        display: flex;
        align-items: baseline;
        gap: var(--sp-2);
        flex-wrap: wrap;
        min-width: 0;
    }

    .label {
        font-size: var(--text-sm);
        color: var(--text);
    }

    .req {
        color: var(--accent, #7aa2f7);
        margin-left: 2px;
    }

    .hint,
    .fallback {
        font-size: var(--text-xs);
        color: var(--text-faint);
    }

    .fallback {
        font-family: var(--font-mono);
    }

    .error {
        font-size: var(--text-xs);
        color: var(--danger, #f7768e);
    }

    /* The control fills its own row, which is what the old blanket rule was
       reaching for -- but scoped here, so a row holding two controls can
       still divide the space between them. */
    /* The bordered box lives on the control, not the row: the global
       `.field` class does this app-wide, but this row also holds a label and
       a hint that must sit outside the box. */
    .control {
        display: flex;
        gap: var(--sp-2);
        min-width: 0;
        background: var(--surface-sunken);
        border: 1px solid var(--line);
        border-radius: var(--r-md);
        padding: var(--sp-2) var(--sp-3);
        transition: border-color var(--fast) var(--ease);
    }
    .control:focus-within {
        border-color: var(--accent-line);
    }
    .control > :global(input),
    .control > :global(select),
    .control > :global(textarea) {
        flex: 1 1 auto;
        min-width: 0;
    }
    /* A select next to something else keeps its own width rather than
       crowding out its neighbour. */
    .control > :global(select:not(:only-child)) {
        flex: 0 0 auto;
        max-width: 40%;
    }
</style>
