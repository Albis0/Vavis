<!--
    Release checks.

    Vavis never updates itself. The check reads the project's release page
    and reports what it found; installing stays a decision someone makes.
    A check that could not run is reported as exactly that, never folded
    into "up to date" — silence is not evidence.
-->
<script lang="ts">
    import type { Status, UpdateCheck } from "../../api";
    import { api } from "../../api";
    import Section from "../Section.svelte";

    interface Props {
        status: Status | null;
        update: UpdateCheck | null;
        checking: boolean;
        oncheck: () => void;
    }

    let { status, update, checking, oncheck }: Props = $props();
</script>

<h2>Updates</h2>

<Section
    title="This build"
    blurb="Vavis does not install updates by itself and does not check in the background. Nothing about you is sent with the check."
>
    <div class="row">
        <span class="label">Version</span>
        <span class="value">{status?.version ?? ""}</span>
    </div>

    <div class="actions">
        <button onclick={oncheck} disabled={checking}>
            {checking ? "checking…" : "check for updates"}
        </button>
    </div>
</Section>

{#if update?.status === "available"}
    <Section title="Available">
        <div class="update-box">
            <p class="update-head">
                Version {update.latest} is out — you have {update.current}.
            </p>
            {#if update.notes}
                <pre class="snippet selectable">{update.notes}</pre>
            {/if}
            <div class="actions">
                <button class="primary" onclick={() => api.openReleasePage()}>
                    open the download page
                </button>
            </div>
            <p class="blurb">
                The page has the installer and a checksum. Close Vavis before running it.
            </p>
        </div>
    </Section>
{:else if update?.status === "upToDate"}
    <Section title="Result">
        <p class="blurb">You are on the newest release ({update.current}).</p>
    </Section>
{:else if update?.status === "failed"}
    <Section title="Result">
        <!-- Deliberately not phrased as "up to date": a check that could not
             run has not established anything. -->
        <p class="blurb warn-text">
            Could not check: {update.error}. Your build is {update.current}.
        </p>
        <div class="actions">
            <button onclick={() => api.openReleasePage()}>open the release page anyway</button>
        </div>
    </Section>
{/if}

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

    .actions {
        display: flex;
        gap: var(--sp-2);
        flex-wrap: wrap;
    }

    .blurb {
        margin: 0;
        font-size: var(--text-xs);
        color: var(--text-muted);
        line-height: 1.5;
        max-width: 62ch;
    }

    .update-box {
        display: flex;
        flex-direction: column;
        gap: var(--sp-3);
        padding: var(--sp-3);
        border: 1px solid var(--accent-line, var(--line));
        border-radius: var(--r-md);
        background: var(--surface-sunken);
    }

    .update-head {
        margin: 0;
        font-size: var(--text-sm);
        color: var(--text);
    }
</style>
