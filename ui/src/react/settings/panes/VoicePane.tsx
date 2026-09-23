/**
 * Listening and speaking.
 *
 * The engine decides which voice list applies, which key matters, and which
 * setting field the picker writes to — so all four are derived from it here
 * rather than being remembered by whoever changes the select.
 *
 * Everything applies on change. The one exception is the ElevenLabs key,
 * which commits on Enter or on leaving the field: a key saved per keystroke
 * would store and test half of one.
 */

import { useEffect, useState } from "react";
import { api, type Status, type VoiceSettings } from "../../../lib/api";
import { chat, chatSignal } from "../../store/chat";
import { toast } from "../../store/toast";
import { useStore } from "../../store/useStore";
import Field from "../Field";
import Section from "../Section";

interface Props {
    status: Status | null;
    voice: VoiceSettings | null;
    onupdate: (field: string, value: string) => void;
    onsavekey: (key: string) => void;
    /** Re-reads voice settings, after training changes what is stored. */
    onreload: () => Promise<void>;
}

export default function VoicePane({ status, voice, onupdate, onsavekey, onreload }: Props) {
    const store = useStore(chatSignal, chat);
    const [liveModels, setLiveModels] = useState<string[]>([]);
    const [loadingLive, setLoadingLive] = useState(false);
    const training = store.enrol !== null;
    const [starting, setStarting] = useState(false);

    // Training finishes on the voice thread; re-read what is stored once it
    // reports back, so "trained" appears without reopening settings.
    const result = store.enrolResult;
    useEffect(() => {
        if (result) void onreload();
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [result]);

    async function train() {
        setStarting(true);
        try {
            chat.enrolResult = null;
            await api.startWakeTraining();
            chat.enrol = { count: 0, needed: 3 };
        } catch (e) {
            toast.failure("Could not start training.", e);
        } finally {
            setStarting(false);
        }
    }
    const [keyDraft, setKeyDraft] = useState("");

    function commitKey() {
        if (!keyDraft.trim()) return;
        onsavekey(keyDraft.trim());
        setKeyDraft("");
    }

    /** The voices that apply to whichever engine is selected. */
    function voiceOptions(): [string, string][] {
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
    }

    /** Which setting field the voice picker writes to. */
    const voiceField =
        (
            {
                edge: "edgeVoice",
                kokoro: "kokoroVoice",
                elevenlabs: "elevenVoice",
                openai: "openaiVoice",
                gemini: "geminiVoice",
            } as Record<string, string>
        )[voice?.engine ?? "sapi"] ?? "sapiVoice";

    const selectedVoice =
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
        "";

    /**
     * What picking "default" will actually get you.
     *
     * An empty value is stored as "follow the language", which is the right
     * default but an opaque one to read in a list -- so the option says which
     * voice that resolves to.
     */
    function defaultVoiceLabel(): string {
        if (!voice || voice.engine !== "edge") return "system choice";
        const match = voice.edgeVoices.find(([id]) => id === voice.defaultEdgeVoice);
        return match?.[1] ?? "follows your language";
    }

    /** Whether the chosen engine is missing the key it needs. */
    const keyMissing =
        voice?.engine === "elevenlabs"
            ? !voice.hasElevenKey
            : voice?.engine === "openai"
              ? !voice.hasOpenaiKey
              : voice?.engine === "gemini"
                ? !voice.hasGeminiKey
                : false;

    /**
     * True when the chat provider's own voice is speaking instead of the one
     * in the picker. Worth saying out loud: otherwise the picker says one
     * thing and the speaker does another, with nothing to explain the gap.
     */
    const swapped = !!voice && voice.matchProvider && voice.effectiveEngine !== voice.engine;

    return (
        <>
            <h2>Voice</h2>

            <Section
                title="Listening"
                blurb="Off, wake word, or always listening. The rail on the left switches between them, and so does Ctrl+M."
            >
                <div className="stat-row">
                    <span className="label">Mode</span>
                    <span className="value">{status?.voiceMode ?? "off"}</span>
                </div>

                <div className="actions">
                    <button onClick={() => chat.cycleVoice()}>cycle mode</button>
                    {status?.speaking && (
                        <button onClick={() => chat.stopSpeaking()}>stop speaking</button>
                    )}
                </div>

                <p className="blurb">
                    Speech recognition runs through Groq, so it needs the Groq key even when
                    another provider is answering.
                </p>
            </Section>

            {voice && (
                <Section
                    title="Live conversation"
                    blurb="Talk with the assistant the way you would with a person: it starts answering within a second, and you can interrupt it mid-sentence. Runs on Gemini's Live API, which the free Gemini key covers. Tools still work, and destructive ones still ask."
                >
                    <div className="actions">
                        <button onClick={() => void chat.toggleLive()}>
                            {status?.live ? "end live conversation" : "start live conversation"}
                        </button>
                    </div>

                    <Field label="Model" fallback={voice.liveDefaultModel}>
                        <div className="actions">
                            <span className="current-model">{voice.liveModel || voice.liveDefaultModel}</span>
                            <button
                                className="tiny"
                                disabled={loadingLive}
                                onClick={async () => {
                                    setLoadingLive(true);
                                    try {
                                        setLiveModels(await api.listLiveModels());
                                    } catch (e) {
                                        toast.failure("Could not list live models.", e);
                                    } finally {
                                        setLoadingLive(false);
                                    }
                                }}
                            >
                                {loadingLive ? "loading…" : "change model"}
                            </button>
                        </div>
                    </Field>
                    {liveModels.length > 0 && (
                        <div className="list scroll">
                            {liveModels.map((m) => (
                                <button
                                    className={m === voice.liveModel ? "row active" : "row"}
                                    key={m}
                                    onClick={() => {
                                        onupdate("liveModel", m);
                                        setLiveModels([]);
                                    }}
                                >
                                    {m}
                                </button>
                            ))}
                        </div>
                    )}

                    <Field label="Voice" fallback="Puck">
                        <select
                            value={voice.liveVoice || "Puck"}
                            onChange={(e) => onupdate("liveVoice", e.target.value)}
                        >
                            {voice.liveVoices.map((v) => (
                                <option value={v} key={v}>
                                    {v}
                                </option>
                            ))}
                        </select>
                    </Field>
                    <p className="blurb">
                        With speakers rather than headphones, the assistant's own voice is kept from
                        interrupting it by holding back quiet input while it talks — speak clearly
                        to cut in.
                    </p>
                </Section>
            )}

            {voice && (
                <Section
                    title="Wake word"
                    blurb="Teach this computer to recognise you saying the assistant's name. After that, wake-word mode decides on this machine whether it was addressed, and only what follows the name is sent for transcription — nothing else you say in the room leaves the computer."
                >
                    <div className="stat-row">
                        <span className="label">Status</span>
                        <span className="value">
                            {training
                                ? `listening — ${store.enrol?.count ?? 0} of ${store.enrol?.needed ?? 3}`
                                : voice.wakeTrained
                                  ? "trained on this computer"
                                  : "not trained — every utterance is transcribed to find the name"}
                        </span>
                    </div>

                    {training ? (
                        <>
                            <p className="hint">
                                Say “{status?.assistantName || "Vavis"}” on its own, with a short pause
                                after each — three times.
                            </p>
                            <div className="actions">
                                <button
                                    onClick={async () => {
                                        await api.cancelWakeTraining();
                                        chat.enrol = null;
                                    }}
                                >
                                    cancel
                                </button>
                            </div>
                        </>
                    ) : (
                        <div className="actions">
                            <button onClick={train} disabled={starting}>
                                {voice.wakeTrained ? "train again" : "train"}
                            </button>
                            {voice.wakeTrained && (
                                <button
                                    className="danger"
                                    onClick={async () => {
                                        try {
                                            await api.forgetWakeWord();
                                            await onreload();
                                        } catch (e) {
                                            toast.failure("Could not forget it.", e);
                                        }
                                    }}
                                >
                                    forget
                                </button>
                            )}
                        </div>
                    )}

                    {voice.wakeTrained && (
                        <Field label="Sensitivity" fallback="">
                            <input
                                type="range"
                                min={1}
                                max={10}
                                value={voice.wakeSensitivity}
                                onChange={(e) => onupdate("wakeSensitivity", e.target.value)}
                                aria-label="Wake word sensitivity"
                            />
                        </Field>
                    )}
                    <p className="blurb">
                        Higher wakes more easily, and more often by mistake. Tuned to the voice that
                        trained it: someone else saying the name may not wake it.
                    </p>
                </Section>
            )}

            {voice && (
                <>
                    <Section
                        title="Speaking"
                        blurb="If the voice you pick cannot be reached, Vavis says so out loud and falls back to one that works — it will not go silent on you."
                    >
                        <Field
                            label="Engine"
                            error={keyMissing ? "needs a key before it can speak" : ""}
                        >
                            {/* `defaultValue` on the select, not a controlled `value`: the
                                Svelte version put `selected` on the option rather than
                                `value` on the select, because the select renders before its
                                options exist and a value naming an option that is not there
                                yet is discarded. `defaultValue`, re-keyed to the engine, is
                                the same idea -- read once, at mount, not fought over on every
                                render. */}
                            <select
                                defaultValue={voice.engine}
                                key={voice.engine}
                                onChange={(e) => onupdate("voiceEngine", e.target.value)}
                            >
                                {voice.engines.map((e) => (
                                    <option key={e.id} value={e.id}>
                                        {e.label}
                                    </option>
                                ))}
                            </select>
                        </Field>

                        <Field label="Voice">
                            <select
                                defaultValue={selectedVoice || ""}
                                key={`${voice.engine}:${selectedVoice}`}
                                onChange={(e) => onupdate(voiceField, e.target.value)}
                            >
                                <option value="">default ({defaultVoiceLabel()})</option>
                                {voiceOptions().map(([id, label]) => (
                                    <option key={id} value={id}>
                                        {label}
                                    </option>
                                ))}
                            </select>
                        </Field>

                        <Field label="Speed" inline hint="-10 to 10">
                            <input
                                type="number"
                                min="-10"
                                max="10"
                                defaultValue={voice.rate}
                                key={voice.rate}
                                onBlur={(e) => onupdate("voiceRate", e.target.value)}
                            />
                        </Field>

                        <Field label="Volume" inline hint="0 to 100">
                            <input
                                type="number"
                                min="0"
                                max="100"
                                defaultValue={voice.volume}
                                key={voice.volume}
                                onBlur={(e) => onupdate("voiceVolume", e.target.value)}
                            />
                        </Field>

                        <div className="actions">
                            <button onClick={() => api.previewVoice()}>hear this voice</button>
                        </div>
                    </Section>

                    <Section title="Match the provider">
                        <label className="switch">
                            <input
                                type="checkbox"
                                checked={voice.matchProvider}
                                onChange={(e) => onupdate("matchProvider", String(e.target.checked))}
                            />
                            <span>Use the chat provider's own voice when it has one</span>
                        </label>
                        <p className="blurb">
                            Talking to Gemini sounds like Gemini. Only swaps between engines that
                            already need a key — a free offline voice is left alone, so this cannot
                            quietly move you onto a metered one.
                        </p>
                        {swapped && (
                            <p className="result">
                                Speaking with <strong>{voice.effectiveEngine}</strong> right now, because
                                that is who you are chatting with.
                            </p>
                        )}
                    </Section>

                    {voice.engine === "kokoro" && (
                        <Section
                            title="Kokoro server"
                            blurb="Kokoro runs on your own machine, so nothing is sent anywhere and it costs nothing. Vavis does not install or start it — run the server yourself and point this at it:"
                        >
                            {/* Kokoro is a model the user runs themselves, so the one thing
                                they need from us is the command. */}
                            <pre className="snippet selectable">
                                docker run -p 8880:8880 ghcr.io/remsky/kokoro-fastapi-cpu
                            </pre>

                            <Field label="Address" fallback={voice.kokoroDefaultUrl}>
                                <input
                                    type="text"
                                    placeholder={voice.kokoroDefaultUrl}
                                    defaultValue={voice.kokoroUrl}
                                    key={voice.kokoroUrl}
                                    onBlur={(e) => onupdate("kokoroUrl", e.target.value)}
                                />
                            </Field>
                        </Section>
                    )}

                    {voice.engine === "elevenlabs" && (
                        <Section title="ElevenLabs key">
                            <Field
                                label="Key"
                                required
                                hint={
                                    voice.hasElevenKey
                                        ? "stored — paste to replace"
                                        : "saved on Enter or when you leave the field"
                                }
                            >
                                <input
                                    type="password"
                                    placeholder={voice.hasElevenKey ? "••••••••" : "paste key…"}
                                    value={keyDraft}
                                    onChange={(e) => setKeyDraft(e.target.value)}
                                    onKeyDown={(e) => e.key === "Enter" && commitKey()}
                                    onBlur={commitKey}
                                />
                            </Field>
                        </Section>
                    )}

                    {voice.engine === "openai" && (
                        <Section title="OpenAI key">
                            <p className="blurb">
                                Uses the OpenAI key from Model & keys — the same one chat uses, so
                                there is nothing extra to paste.
                                {voice.hasOpenaiKey ? "" : " No key stored yet."}
                            </p>
                        </Section>
                    )}

                    {voice.engine === "gemini" && (
                        <Section title="Gemini key">
                            <p className="blurb">
                                Uses the Gemini key from Model & keys.
                                {voice.hasGeminiKey ? "" : " No key stored yet."}
                            </p>
                        </Section>
                    )}
                </>
            )}
        </>
    );
}
