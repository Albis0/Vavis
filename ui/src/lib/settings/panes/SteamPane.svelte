<!--
    Steam: a Web API key and a SteamID64.

    Both are required together — the key alone returns an empty library,
    and so does a private profile, which is the failure people spend the
    longest on because Steam reports neither as an error.
-->
<script lang="ts">
    import type { ConnectionTest, SteamSettings } from "../../api";
    import Field from "../Field.svelte";
    import Section from "../Section.svelte";

    interface Props {
        steam: SteamSettings | null;
        steamId: string;
        result: ConnectionTest | undefined;
        testing: boolean;
        onsave: (steamId: string, key: string) => void;
        ontest: () => void;
    }

    let { steam, steamId, result, testing, onsave, ontest }: Props = $props();

    // Seeded from the prop rather than mirroring it: the settings reload
    // after a save, and a plain mirror would wipe what is being typed the
    // moment that lands.
    let typedId = $state<string | null>(null);
    const idDraft = $derived(typedId ?? steamId);

    let keyDraft = $state("");

    /** Steam ids are exactly 17 digits; anything else is a copy-paste slip. */
    const idError = $derived(
        idDraft.trim() === "" || /^\d{17}$/.test(idDraft.trim())
            ? ""
            : "should be 17 digits",
    );

    function save() {
        onsave(idDraft.trim(), keyDraft);
        keyDraft = "";
    }
</script>

<h2>Steam</h2>

<Section
    title="Account"
    blurb="Game details must be public, or Steam returns an empty library without saying why."
>
    <Field label="SteamID64" required error={idError} hint="17 digits">
        <input
            value={idDraft}
            oninput={(e) => (typedId = e.currentTarget.value)}
            placeholder="76561198000000000"
            spellcheck="false"
        />
    </Field>

    <Field
        label="Web API key"
        required
        hint={steam?.hasKey ? "stored — paste to replace" : "from steamcommunity.com/dev/apikey"}
    >
        <input
            type="password"
            bind:value={keyDraft}
            placeholder={steam?.hasKey ? "••••••••" : "paste key…"}
            onkeydown={(e) => e.key === "Enter" && save()}
        />
    </Field>

    <div class="actions">
        <button class="primary" onclick={save}>save and check</button>
        <button onclick={ontest} disabled={testing || !steam?.hasKey}>
            {testing ? "asking…" : "test"}
        </button>
    </div>

    {#if result}
        <p class="result" class:bad={!result.ok}>
            {result.ok ? "✓" : "✕"}
            {result.detail}
        </p>
    {/if}
</Section>

<Section title="Without a key">
    <p class="blurb">
        Which game is running is detected locally, so that part works even on a
        private profile. The key is only needed for the library and playtime.
    </p>
</Section>

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
</style>
