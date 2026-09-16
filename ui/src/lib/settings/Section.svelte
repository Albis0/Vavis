<!--
    A titled block inside a pane: heading, optional explanation, contents.

    Exists so panes stop hand-rolling `<h3>` plus a `<p class="hint">` plus a
    wrapper div in four slightly different ways.
-->
<script lang="ts">
    interface Props {
        title?: string;
        blurb?: string;
        children: import("svelte").Snippet;
    }

    let { title = "", blurb = "", children }: Props = $props();
</script>

<section class="section">
    {#if title}
        <h3>{title}</h3>
    {/if}
    {#if blurb}
        <p class="blurb">{blurb}</p>
    {/if}
    <div class="body">
        {@render children()}
    </div>
</section>

<style>
    .section {
        display: flex;
        flex-direction: column;
        gap: var(--sp-3);
    }

    h3 {
        margin: 0;
        font-size: var(--text-xs);
        font-weight: 600;
        letter-spacing: 0.08em;
        text-transform: uppercase;
        color: var(--text-faint);
    }

    .blurb {
        margin: 0;
        font-size: var(--text-xs);
        color: var(--text-muted);
        line-height: 1.5;
        max-width: 62ch;
    }

    .body {
        display: flex;
        flex-direction: column;
        gap: var(--sp-3);
    }
</style>
