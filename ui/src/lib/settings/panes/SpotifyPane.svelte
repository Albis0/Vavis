<!--
    Spotify, which is one button in the normal case.

    The built-in application means nobody has to register anything, so the
    client id lives behind a disclosure. It used to be the first thing on
    the screen, which turned a one-click connection into a form.
-->
<script lang="ts">
    import type { ConnectionTest, SpotifySettings } from "../../api";
    import Field from "../Field.svelte";
    import Section from "../Section.svelte";

    interface Props {
        spotify: SpotifySettings | null;
        clientId: string;
        result: ConnectionTest | undefined;
        testing: boolean;
        onconnect: () => void;
        ondisconnect: () => void;
        ontest: () => void;
        onsaveid: (id: string) => void;
    }

    let {
        spotify,
        clientId,
        result,
        testing,
        onconnect,
        ondisconnect,
        ontest,
        onsaveid,
    }: Props = $props();

    // Seeded from the prop rather than mirroring it: `load()` refreshes
    // settings after a save, and a plain mirror would wipe what is being
    // typed the moment that lands.
    let typed = $state<string | null>(null);
    const draft = $derived(typed ?? clientId);

    let ownApp = $state(false);
</script>

<h2>Spotify</h2>

{#if spotify?.connected}
    <Section title="Connection">
        <p class="result">✓ Connected.</p>
        <div class="actions">
            <button onclick={ontest} disabled={testing}>
                {testing ? "asking…" : "test"}
            </button>
            <button class="danger" onclick={ondisconnect}>disconnect</button>
        </div>
        {#if result}
            <p class="result" class:bad={!result.ok}>
                {result.ok ? "✓" : "✕"}
                {result.detail}
            </p>
        {/if}
    </Section>
{:else}
    <Section
        title="Connection"
        blurb="Opens Spotify in your browser. Approve there and you are done — there is nothing to set up first."
    >
        <div class="actions">
            <button class="primary" onclick={onconnect}>connect</button>
        </div>
    </Section>

    <Section title="Advanced">
        <button class="disclosure" onclick={() => (ownApp = !ownApp)}>
            {ownApp ? "▾" : "▸"} use my own Spotify app
        </button>

        {#if ownApp}
            <p class="blurb">
                Only worth doing if you want your own name on the consent screen.
                Register this exact redirect URI on the app, then paste its client id
                here.
            </p>

            <Field label="Redirect URI" hint="register this on your app">
                <code class="path selectable">{spotify?.redirectUri ?? ""}</code>
            </Field>

            <Field label="Client id" fallback="the built-in app">
                <input
                    value={draft}
                    oninput={(e) => (typed = e.currentTarget.value)}
                    placeholder="client id (optional)"
                    spellcheck="false"
                />
            </Field>

            <div class="actions">
                <button onclick={() => onsaveid(draft.trim())}>save id</button>
            </div>
        {/if}
    </Section>
{/if}

<style>
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

    .path {
        font-family: var(--font-mono);
        font-size: var(--text-xs);
        color: var(--text-muted);
        word-break: break-all;
        min-width: 0;
    }
</style>
