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
    ontest: (id: string) => void;
    onchange: (key: string, value: string) => void;
}

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
    ontest,
    onchange,
}: Props) {
    return (
        <>
            <h2>Model & keys</h2>

            <Section
                title="Providers"
                blurb="Keys are encrypted with Windows DPAPI, never written to the settings file, and never shown again once saved."
            >
                <div className="cards">
                    {(status?.providers ?? []).map((p) => {
                        const selected = p.id === status?.provider;
                        const blocked = p.needsKey && !p.hasKey;
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

                                    {!p.needsKey ? (
                                        <span className="tag key-tag" data-tone="neutral">
                                            no key needed
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
                                                    ? "can be selected, but will not answer until a key is stored"
                                                    : `default model: ${p.defaultModel}`
                                            }
                                        >
                                            use this
                                        </button>
                                    )}
                                </div>

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
                                    {p.needsKey && (
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
                                        title={blocked ? "store a key first" : "make a real request"}
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
