<!--
    What Vavis stores, where, and how to get rid of it.

    Read-only except for one destructive button. The counts come from the
    status poll rather than their own call, so this pane costs nothing to
    open.
-->
<script lang="ts">
    import type { CanvasSettings, Status } from "../../api";
    import { openFolder } from "../../actions";
    import Section from "../Section.svelte";

    interface Props {
        status: Status | null;
        canvas: CanvasSettings | null;
        onclear: () => void;
    }

    let { status, canvas, onclear }: Props = $props();

    function bytes(n: number): string {
        if (n < 1024) return `${n} B`;
        if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
        if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
        return `${(n / 1024 / 1024 / 1024).toFixed(1)} GB`;
    }

    /** Rows shown under "Stored". Built as data so the markup stays one loop. */
    const rows = $derived([
        { label: "Conversation", value: `${status?.messageCount ?? 0} messages` },
        { label: "Remembered", value: `${status?.factCount ?? 0} facts` },
        { label: "Scheduled", value: `${status?.automationCount ?? 0} automations` },
        ...(canvas
            ? [
                  {
                      label: "Generated",
                      value: `${canvas.items} files · ${bytes(canvas.bytes)}`,
                  },
              ]
            : []),
    ]);
</script>

<h2>Data</h2>

<Section title="Location" blurb="Everything Vavis stores lives in this folder.">
    <div class="path-row">
        <code class="path selectable">{status?.dataDir ?? ""}</code>
        <button class="tiny" onclick={() => openFolder()}>open</button>
    </div>
</Section>

<Section title="Stored">
    <div class="rows">
        {#each rows as row (row.label)}
            <div class="row">
                <span class="label">{row.label}</span>
                <span class="value">{row.value}</span>
            </div>
        {/each}
    </div>
</Section>

<Section
    title="Clear"
    blurb="Remembered facts and generated files survive this — forget facts in Memory, clear files in Image &amp; video."
>
    <div class="actions">
        <button class="danger" onclick={onclear}>Clear the conversation</button>
    </div>
</Section>

<style>
    .path-row {
        display: flex;
        align-items: center;
        gap: var(--sp-2);
        min-width: 0;
    }

    /* The path is the one thing here someone copies, so it wraps rather
       than being cut off at the edge of the column. */
    .path {
        flex: 1 1 auto;
        min-width: 0;
        font-family: var(--font-mono);
        font-size: var(--text-xs);
        color: var(--text-muted);
        word-break: break-all;
    }

    .rows {
        display: flex;
        flex-direction: column;
    }

    .label {
        font-size: var(--text-sm);
        color: var(--text-muted);
    }

    .value {
        font-size: var(--text-sm);
        color: var(--text);
        font-variant-numeric: tabular-nums;
    }

    .actions {
        display: flex;
        gap: var(--sp-2);
        flex-wrap: wrap;
    }
</style>
