/**
 * One card per provider: whether it holds a key, whether it answers, and
 * which model is doing the answering.
 *
 * The card is the unit because the alternative — a chip row here and a key
 * list on another screen — meant picking a provider marked "no key" told
 * you what was wrong and then made you go somewhere else to fix it.
 */

import type { ConnectionTest, Status } from "../../../lib/api";
import Field from "../Field";
import Section from "../Section";

interface Props {
    status: Status | null;
    tests: Record<string, ConnectionTest>;
    testing: string | null;
    models: string[];
    loadingModels: boolean;
    keyOpen: string | null;
    keyDraft: string;
    onKeyDraftChange: (value: string) => void;
    onpickprovider: (id: string) => void;
    onpickmodel: (model: string) => void;
    onopenkey: (id: string) => void;
    onsavekey: (id: string) => void;
    oncancelkey: () => void;
    onfetchmodels: () => void;
    codeModels: string[];
    loadingCodeModels: boolean;
    onpickcodeprovider: (id: string) => void;
    onpickcodemodel: (model: string) => void;
    onfetchcodemodels: () => void;
    ontest: (id: string) => void;
    onchange: (key: string, value: string) => void;
    onsetfallback: (providers: string[]) => void;
}

/** One line under a card, for the providers that need more explaining
    than a model name. */
const NOTES: Record<string, string> = {
    "claude-code":
        "Your Claude Pro or Max plan, through the Claude Code CLI — no API key. Install it from claude.com/code and run `claude` once in a terminal to sign in. Vavis's tools reach it over a private local connection, and every destructive one still asks you first.",
    openrouter: "Models ending in :free cost nothing; they are listed first.",
    github: "Create a token at github.com/settings/tokens with the models: read permission.",
    cerebras: "Free key at cloud.cerebras.ai.",
    custom: "Any OpenAI-compatible server. Set its URL below.",
    local: "Ollama by default. Set a URL below for LM Studio or another port.",
};

export default function ProviderPane({
    status,
    tests,
    testing,
    models,
    loadingModels,
    keyOpen,
    keyDraft,
    onKeyDraftChange,
    onpickprovider,
    onpickmodel,
    onopenkey,
    onsavekey,
    oncancelkey,
    onfetchmodels,
    codeModels,
    loadingCodeModels,
    onpickcodeprovider,
    onpickcodemodel,
    onfetchcodemodels,
    ontest,
    onchange,
    onsetfallback,
}: Props) {
    const codeProvider = status?.codeProvider ?? "";
    const codeOn = codeProvider !== "";
    const fallback = status?.fallback ?? [];
    const providers = status?.providers ?? [];
    const addable = providers.filter(
        (p) => p.id !== status?.provider && !fallback.includes(p.id),
    );
    return (
        <>
            <h2>Model & keys</h2>

            <Section
                title="Providers"
                blurb="Keys are encrypted with Windows DPAPI, never written to the settings file, and never shown again once saved."
            >
                <div className="cards">
                    {providers.map((p) => {
                        const selected = p.id === status?.provider;
                        const blocked = !p.usable;
                        return (
                            // A div, not a button: the card holds a key field and its own
                            // buttons, and nesting those inside a button is invalid and
                            // swallows their clicks. Selection is the separate control at
                            // the end of the header row.
                            <div
                                className={`card${selected ? " selected" : ""}${blocked ? " blocked" : ""}`}
                                key={p.id}
                            >
                                <div className="card-head">
                                    <span className="card-name">{p.id}</span>
                                    {/* Beside the name, not under it. On its own line it
                                        was a row of its own for one short string, which
                                        made every card taller than it had anything to
                                        say. */}
                                    <span className="card-model">
                                        {selected ? (status?.model ?? p.defaultModel) : p.defaultModel}
                                    </span>

                                    <span className="card-spacer"></span>

                                    {p.freeTier && (
                                        <span className="tag" data-tone="good">
                                            free
                                        </span>
                                    )}

                                    {p.id === "claude-code" ? (
                                        <span className="tag key-tag" data-tone="neutral">
                                            your plan
                                        </span>
                                    ) : !p.takesKey ? (
                                        <span className="tag key-tag" data-tone="neutral">
                                            no key needed
                                        </span>
                                    ) : !p.needsKey && !p.hasKey ? (
                                        <span className="tag key-tag" data-tone="neutral">
                                            key optional
                                        </span>
                                    ) : p.hasKey ? (
                                        <span className="tag key-tag" data-tone="good">
                                            key stored
                                        </span>
                                    ) : (
                                        <span className="tag key-tag" data-tone="warn">
                                            no key
                                        </span>
                                    )}

                                    {selected ? (
                                        <span className="tag pick" data-tone="accent">
                                            answering
                                        </span>
                                    ) : (
                                        <button
                                            className="tiny pick"
                                            onClick={() => onpickprovider(p.id)}
                                            title={
                                                blocked
                                                    ? "can be selected, but will not answer until it is set up"
                                                    : `default model: ${p.defaultModel}`
                                            }
                                        >
                                            use this
                                        </button>
                                    )}
                                </div>

                                {NOTES[p.id] && <p className="hint small">{NOTES[p.id]}</p>}

                                {tests[p.id] && (
                                    <p className={tests[p.id].ok ? "result" : "result bad"}>
                                        {tests[p.id].ok ? "✓" : "✕"} {tests[p.id].detail}
                                    </p>
                                )}

                                {keyOpen === p.id && (
                                    <>
                                        <input
                                            className="key-input"
                                            type="password"
                                            autoFocus
                                            value={keyDraft}
                                            onChange={(e) => onKeyDraftChange(e.target.value)}
                                            placeholder={
                                                p.hasKey
                                                    ? "paste a new key to replace the stored one…"
                                                    : "paste key…"
                                            }
                                            onKeyDown={(e) => {
                                                if (e.key === "Enter") onsavekey(p.id);
                                                if (e.key === "Escape") oncancelkey();
                                            }}
                                            onBlur={() => onsavekey(p.id)}
                                        />
                                        <p className="hint small">
                                            Saved on Enter or when you leave the field. Esc discards it.
                                        </p>
                                    </>
                                )}

                                <div className="card-actions">
                                    {p.takesKey && (
                                        <button className="tiny" onClick={() => onopenkey(p.id)}>
                                            {keyOpen === p.id
                                                ? "cancel"
                                                : p.hasKey
                                                  ? "replace key"
                                                  : "add key"}
                                        </button>
                                    )}
                                    <button
                                        className="tiny"
                                        disabled={blocked || testing !== null}
                                        onClick={() => ontest(p.id)}
                                        title={blocked ? "finish setting it up first" : "make a real request"}
                                    >
                                        {testing === p.id ? "testing…" : "test"}
                                    </button>
                                    {selected && (
                                        <button className="tiny" onClick={onfetchmodels} disabled={loadingModels}>
                                            {loadingModels ? "loading…" : "change model"}
                                        </button>
                                    )}
                                </div>

                                {/* Under the card whose provider they belong to, so there is
                                    never a list of models with no visible owner. */}
                                {selected && models.length > 0 && (
                                    <div className="list scroll">
                                        {models.map((m) => (
                                            <button
                                                className={m === status?.model ? "row active" : "row"}
                                                onClick={() => onpickmodel(m)}
                                                key={m}
                                            >
                                                {m}
                                            </button>
                                        ))}
                                    </div>
                                )}
                            </div>
                        );
                    })}
                </div>
            </Section>

            <Section
                title="Endpoints"
                blurb="For the two providers that live wherever you put them. Paste either the base (http://host/v1) or the full chat URL."
            >
                <Field label="Custom endpoint" fallback="not set — the custom provider stays off">
                    <input
                        type="text"
                        placeholder="e.g. https://my-proxy.example/v1"
                        defaultValue={status?.customUrl ?? ""}
                        key={`custom-${status?.customUrl ?? ""}`}
                        onBlur={(e) => onchange("customUrl", e.target.value)}
                    />
                </Field>
                <Field label="Local server" fallback="Ollama — http://127.0.0.1:11434/v1">
                    <input
                        type="text"
                        placeholder="e.g. http://127.0.0.1:1234/v1 for LM Studio"
                        defaultValue={status?.localUrl ?? ""}
                        key={`local-${status?.localUrl ?? ""}`}
                        onBlur={(e) => onchange("localUrl", e.target.value)}
                    />
                </Field>
            </Section>

            <Section
                title="Fallback"
                blurb="When the provider above cannot answer — its free quota is spent, it is down, its key was refused — the message goes to the next one here that is set up, on its default model. You are told each time it happens. Only the first request of a message moves on: once anything has been shown or done, a failure stays a failure, so nothing runs twice."
            >
                {fallback.length === 0 ? (
                    <p className="hint small">Off — a failure ends the message.</p>
                ) : (
                    <ol className="fallback-list">
                        {fallback.map((id, i) => {
                            const info = providers.find((p) => p.id === id);
                            return (
                                <li key={id}>
                                    <span className="card-name">{id}</span>
                                    {info && !info.usable && (
                                        <span className="tag" data-tone="warn">
                                            skipped — not set up
                                        </span>
                                    )}
                                    <span className="card-spacer"></span>
                                    <button
                                        className="tiny"
                                        disabled={i === 0}
                                        title="try earlier"
                                        onClick={() => {
                                            const next = [...fallback];
                                            [next[i - 1], next[i]] = [next[i], next[i - 1]];
                                            onsetfallback(next);
                                        }}
                                    >
                                        ↑
                                    </button>
                                    <button
                                        className="tiny"
                                        onClick={() => onsetfallback(fallback.filter((f) => f !== id))}
                                    >
                                        remove
                                    </button>
                                </li>
                            );
                        })}
                    </ol>
                )}
                {addable.length > 0 && (
                    <Field label="Add" fallback="">
                        <select
                            value=""
                            onChange={(e) => {
                                if (e.target.value) onsetfallback([...fallback, e.target.value]);
                            }}
                        >
                            <option value="">choose a provider…</option>
                            {addable.map((p) => (
                                <option value={p.id} key={p.id}>
                                    {p.id}
                                    {p.usable ? "" : " — not set up yet"}
                                </option>
                            ))}
                        </select>
                    </Field>
                )}
            </Section>

            <Section
                title="Code model"
                blurb="Code work can go to a different provider than chat. Chat wants an answer before the thought is gone; code wants the answer to be right and will wait — one model rarely does both well. Off by default, in which case code uses the model above."
            >
                <Field label="Provider for code" fallback="same as chat">
                    <select
                        value={codeProvider}
                        onChange={(e) => onpickcodeprovider(e.target.value)}
                    >
                        <option value="">same as chat</option>
                        {providers.map((p) => (
                            <option value={p.id} key={p.id}>
                                {p.id}
                                {p.usable ? "" : " — not set up yet"}
                            </option>
                        ))}
                    </select>
                </Field>

                {codeOn && (
                    <>
                        <Field label="Model" fallback={`the ${codeProvider} default`}>
                            <div className="actions">
                                <span className="current-model">
                                    {status?.codeModel || "provider default"}
                                </span>
                                <button
                                    className="tiny"
                                    onClick={onfetchcodemodels}
                                    disabled={loadingCodeModels}
                                >
                                    {loadingCodeModels ? "loading…" : "change model"}
                                </button>
                            </div>
                        </Field>

                        {codeModels.length > 0 && (
                            <div className="list scroll">
                                {codeModels.map((m) => (
                                    <button
                                        className={m === status?.codeModel ? "row active" : "row"}
                                        onClick={() => onpickcodemodel(m)}
                                        key={m}
                                    >
                                        {m}
                                    </button>
                                ))}
                            </div>
                        )}

                        <p className="blurb">
                            Used when you ask about a file from the code screen. Everything
                            else stays on the chat model. If this provider has no key
                            stored, code falls back to the chat model rather than failing.
                        </p>
                    </>
                )}
            </Section>

            <Section
                title="Tool routing"
                blurb="A small, cheap model reads your request and decides which tools the main model needs, so only those are sent. Leave this empty to match tools by keyword instead — no extra call, no extra cost."
            >
                <Field label="Router model" fallback="off — keyword matching">
                    <input
                        type="text"
                        placeholder="e.g. llama-3.1-8b-instant"
                        defaultValue={status?.routerModel ?? ""}
                        key={status?.routerModel ?? ""}
                        onBlur={(e) => onchange("routerModel", e.target.value)}
                    />
                </Field>

                <p className="blurb">
                    Runs on the provider and key you already use. If it is slow or fails,
                    keyword matching takes over — the assistant keeps working.
                </p>
            </Section>
        </>
    );
}
