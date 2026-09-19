<!--
    The handful of settings that belong to no integration.

    Each applies on change rather than on a save button, so the reader sees
    the effect of a choice while still looking at the control that made it.
-->
<script lang="ts">
    import type { Status } from "../../api";
    import Field from "../Field.svelte";
    import Section from "../Section.svelte";

    interface Props {
        status: Status | null;
        languages: readonly (readonly [string, string])[];
        windowModes: readonly string[];
        onchange: (key: string, value: string) => void;
    }

    let { status, languages, windowModes, onchange }: Props = $props();
</script>

<h2>General</h2>

<Section title="Identity">
    <Field label="Assistant name" hint="spoken aloud, so pick something sayable">
        <input
            value={status?.assistantName ?? ""}
            onchange={(e) => onchange("name", e.currentTarget.value)}
        />
    </Field>

    <Field label="Language" inline>
        <!-- `selected` on the option rather than `value` on the select: the
             select renders before its options exist, so a value naming an
             option that is not there yet is dropped and the box shows the
             first entry instead. -->
        <select onchange={(e) => onchange("language", e.currentTarget.value)}>
            {#each languages as [code, name] (code)}
                <option value={code} selected={code === (status?.language ?? "en")}>
                    {name}
                </option>
            {/each}
        </select>
    </Field>
</Section>

<Section title="Window">
    <Field label="Mode" inline>
        <select onchange={(e) => onchange("windowMode", e.currentTarget.value)}>
            {#each windowModes as mode (mode)}
                <option value={mode} selected={mode === (status?.windowMode ?? "windowed")}>
                    {mode}
                </option>
            {/each}
        </select>
    </Field>

    <Field label="Font size" inline hint="8–32">
        <input
            type="number"
            min="8"
            max="32"
            value={status?.fontSize ?? 14}
            onchange={(e) => onchange("fontSize", e.currentTarget.value)}
        />
    </Field>
</Section>

<Section title="Build">
    <div class="row">
        <span class="label">Version</span>
        <span class="value">{status?.version ?? "—"}</span>
    </div>
</Section>

<style>

    .label {
        font-size: var(--text-sm);
        color: var(--text-muted);
    }

    .value {
        font-size: var(--text-sm);
        color: var(--text);
        font-variant-numeric: tabular-nums;
    }

    /* A number box has no reason to span the pane; left at full width it
       read as a text field. */
    input[type="number"] {
        width: 5rem;
    }
</style>
