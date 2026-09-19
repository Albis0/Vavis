<!--
    Listening and speaking.

    The engine decides which voice list applies, which key matters, and which
    setting field the picker writes to — so all four are derived from it here
    rather than being remembered by whoever changes the select.

    Everything applies on change. The one exception is the ElevenLabs key,
    which commits on Enter or on leaving the field: a key saved per keystroke
    would store and test half of one.
-->
<script lang="ts">
    import { api, type Status, type VoiceSettings } from "../../api";
    import { chat } from "../../store.svelte";
    import Field from "../Field.svelte";
    import Section from "../Section.svelte";

    interface Props {
        status: Status | null;
        voice: VoiceSettings | null;
        onupdate: (field: string, value: string) => void;
        onsavekey: (key: string) => void;
    }

    let { status, voice, onupdate, onsavekey }: Props = $props();

    let keyDraft = $state("");

    function commitKey() {
        if (!keyDraft.trim()) return;
        onsavekey(keyDraft.trim());
        keyDraft = "";
    }

    /** The voices that apply to whichever engine is selected. */
    const voiceOptions = $derived.by((): [string, string][] => {
        if (!voice) return [];
        switch (voice.engine) {
            case "edge":
                return voice.edgeVoices;
            case "kokoro":
                return voice.kokoroVoices;
            case "elevenlabs":
                return voice.elevenVoices;
            case "openai":
                return voice.openaiVoices;
            case "gemini":
                return voice.geminiVoices;
            // SAPI reports whatever Windows has installed, as plain names.
            default:
                return voice.sapiVoices.map((v) => [v, v] as [string, string]);
        }
    });

    /** Which setting field the voice picker writes to. */
    const voiceField = $derived(
        (
            {
                edge: "edgeVoice",
                kokoro: "kokoroVoice",
                elevenlabs: "elevenVoice",
                openai: "openaiVoice",
                gemini: "geminiVoice",
            } as Record<string, string>
        )[voice?.engine ?? "sapi"] ?? "sapiVoice",
    );

    const selectedVoice = $derived(
        (
            {
                edge: voice?.edgeVoice,
                kokoro: voice?.kokoroVoice,
                elevenlabs: voice?.elevenVoice,
                openai: voice?.openaiVoice,
                gemini: voice?.geminiVoice,
            } as Record<string, string | undefined>
        )[voice?.engine ?? "sapi"] ??
            voice?.sapiVoice ??
            "",
    );

    /**
     * What picking "default" will actually get you.
     *
     * An empty value is stored as "follow the language", which is the right
     * default but an opaque one to read in a list -- so the option says which
     * voice that resolves to.
     */
    const defaultVoiceLabel = $derived.by(() => {
        if (!voice || voice.engine !== "edge") return "system choice";
        const match = voice.edgeVoices.find(([id]) => id === voice!.defaultEdgeVoice);
        return match?.[1] ?? "follows your language";
    });

    /** Whether the chosen engine is missing the key it needs. */
    const keyMissing = $derived(
        voice?.engine === "elevenlabs"
            ? !voice.hasElevenKey
            : voice?.engine === "openai"
              ? !voice.hasOpenaiKey
              : voice?.engine === "gemini"
                ? !voice.hasGeminiKey
                : false,
    );

    /**
     * True when the chat provider's own voice is speaking instead of the one
     * in the picker. Worth saying out loud: otherwise the picker says one
     * thing and the speaker does another, with nothing to explain the gap.
     */
    const swapped = $derived(
        !!voice && voice.matchProvider && voice.effectiveEngine !== voice.engine,
    );
</script>

<h2>Voice</h2>

<Section
    title="Listening"
    blurb="Off, wake word, or always listening. The rail on the left switches between them, and so does Ctrl+M."
>
    <div class="row">
        <span class="label">Mode</span>
        <span class="value">{status?.voiceMode ?? "off"}</span>
    </div>

    <div class="actions">
        <button onclick={() => chat.cycleVoice()}>cycle mode</button>
        {#if status?.speaking}
            <button onclick={() => chat.stopSpeaking()}>stop speaking</button>
        {/if}
    </div>

    <p class="blurb">
        Speech recognition runs through Groq, so it needs the Groq key even when
        another provider is answering.
    </p>
</Section>

{#if voice}
    <Section
        title="Speaking"
        blurb="If the voice you pick cannot be reached, Vavis says so out loud and falls back to one that works — it will not go silent on you."
    >
        <Field label="Engine" error={keyMissing ? "needs a key before it can speak" : ""}>
            <select onchange={(e) => onupdate("voiceEngine", e.currentTarget.value)}>
                <!-- `selected` on the option, not `value` on the select: the
                     select is rendered before its options exist, so a value
                     naming an option that is not there yet is discarded and
                     the box silently snaps back to the first entry. That is
                     why changing the voice looked like it did nothing. -->
                {#each voice.engines as e (e.id)}
                    <option value={e.id} selected={e.id === voice.engine}>{e.label}</option>
                {/each}
            </select>
        </Field>

        <Field label="Voice">
            <select onchange={(e) => onupdate(voiceField, e.currentTarget.value)}>
                <option value="" selected={!selectedVoice}>
                    default ({defaultVoiceLabel})
                </option>
                {#each voiceOptions as [id, label] (id)}
                    <option value={id} selected={id === selectedVoice}>{label}</option>
                {/each}
            </select>
        </Field>

        <Field label="Speed" inline hint="-10 to 10">
            <input
                type="number"
                min="-10"
                max="10"
                value={voice.rate}
                onchange={(e) => onupdate("voiceRate", e.currentTarget.value)}
            />
        </Field>

        <Field label="Volume" inline hint="0 to 100">
            <input
                type="number"
                min="0"
                max="100"
                value={voice.volume}
                onchange={(e) => onupdate("voiceVolume", e.currentTarget.value)}
            />
        </Field>

        <div class="actions">
            <button onclick={() => api.previewVoice()}>hear this voice</button>
        </div>
    </Section>

    <Section title="Match the provider">
        <label class="switch">
            <input
                type="checkbox"
                checked={voice.matchProvider}
                onchange={(e) => onupdate("matchProvider", String(e.currentTarget.checked))}
            />
            <span>Use the chat provider's own voice when it has one</span>
        </label>
        <p class="blurb">
            Talking to Gemini sounds like Gemini. Only swaps between engines that
            already need a key — a free offline voice is left alone, so this cannot
            quietly move you onto a metered one.
        </p>
        {#if swapped}
            <p class="result">
                Speaking with <strong>{voice.effectiveEngine}</strong> right now, because
                that is who you are chatting with.
            </p>
        {/if}
    </Section>

    {#if voice.engine === "kokoro"}
        <Section
            title="Kokoro server"
            blurb="Kokoro runs on your own machine, so nothing is sent anywhere and it costs nothing. Vavis does not install or start it — run the server yourself and point this at it:"
        >
            <!-- Kokoro is a model the user runs themselves, so the one thing
                 they need from us is the command. -->
            <pre class="snippet selectable">docker run -p 8880:8880 ghcr.io/remsky/kokoro-fastapi-cpu</pre>

            <Field label="Address" fallback={voice.kokoroDefaultUrl}>
                <input
                    type="text"
                    placeholder={voice.kokoroDefaultUrl}
                    value={voice.kokoroUrl}
                    onchange={(e) => onupdate("kokoroUrl", e.currentTarget.value)}
                />
            </Field>
        </Section>
    {/if}

    {#if voice.engine === "elevenlabs"}
        <Section title="ElevenLabs key">
            <Field
                label="Key"
                required
                hint={voice.hasElevenKey
                    ? "stored — paste to replace"
                    : "saved on Enter or when you leave the field"}
            >
                <input
                    type="password"
                    placeholder={voice.hasElevenKey ? "••••••••" : "paste key…"}
                    bind:value={keyDraft}
                    onkeydown={(e) => e.key === "Enter" && commitKey()}
                    onblur={commitKey}
                />
            </Field>
        </Section>
    {/if}

    {#if voice.engine === "openai"}
        <Section title="OpenAI key">
            <p class="blurb">
                Uses the OpenAI key from Model &amp; keys — the same one chat uses, so
                there is nothing extra to paste.{voice.hasOpenaiKey
                    ? ""
                    : " No key stored yet."}
            </p>
        </Section>
    {/if}

    {#if voice.engine === "gemini"}
        <Section title="Gemini key">
            <p class="blurb">
                Uses the Gemini key from Model &amp; keys.{voice.hasGeminiKey
                    ? ""
                    : " No key stored yet."}
            </p>
        </Section>
    {/if}
{/if}

<style>
    .row {
        display: flex;
        align-items: baseline;
        justify-content: space-between;
        gap: var(--sp-4);
        padding: var(--sp-2) 0;
        border-bottom: 1px solid var(--line);
    }

    .label {
        font-size: var(--text-sm);
        color: var(--text-muted);
    }

    .value {
        font-size: var(--text-sm);
        color: var(--text);
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

    /* A number box has no reason to span the pane; left at full width it read
       as a text field. */
    input[type="number"] {
        width: 5rem;
    }
</style>
