<!--
    Keyboard shortcuts. A reference table, nothing to configure.

    The bindings live in the components that listen for them; this is a
    written copy, which means it can drift. Keeping it here rather than
    deriving it from the handlers is deliberate — the handlers are spread
    across four components, and a wrong list is easier to spot than a
    missing one.
-->
<script lang="ts">
    interface Props {
        /** [key, what it does] pairs. */
        shortcuts: readonly (readonly [string, string])[];
    }

    let { shortcuts }: Props = $props();
</script>

<h2>Shortcuts</h2>

<div class="list">
    {#each shortcuts as [key, action] (key)}
        <div class="row">
            <kbd>{key}</kbd>
            <span>{action}</span>
        </div>
    {/each}
</div>

<style>

    /* A fixed column so the descriptions line up rather than stepping in
       and out with the width of each key combination. */
    kbd {
        flex: 0 0 120px;
        font-family: var(--font-mono);
        font-size: var(--text-xs);
        color: var(--text);
        background: var(--surface-sunken);
        border: 1px solid var(--line);
        border-radius: var(--r-sm);
        padding: 2px var(--sp-2);
        text-align: center;
    }
</style>
