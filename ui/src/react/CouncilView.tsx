/**
 * The council — several models on one question.
 *
 * One conversation is one train of thought. Here the same question goes to
 * several models at once and the answers sit side by side, which is the only
 * way to see where they actually differ.
 *
 * Two things this screen must never do. It must never spend money the user
 * did not expect: the forecast is on screen before the button is pressed, and
 * the real cost replaces it afterwards. And it must never decide for itself
 * how many seats to open — every seat here was added by hand.
 *
 * A seat's configuration is folded away once it has an answer. While you are
 * setting the council up the provider and model are the whole point; the
 * moment the answers arrive they are noise between you and the text you asked
 * for.
 */
import { useEffect, useRef, useState } from "react";
import {
    api,
    on,
    type CouncilDeltaEvent,
    type CouncilDoneEvent,
    type CouncilSeatDoneEvent,
    type CouncilSeatFailedEvent,
    type Forecast,
    type Seat,
} from "../lib/api";
import Icon from "./Icon";
import { renderMarkdown } from "../lib/markdown";
import { useStore } from "./store/useStore";
import { chat, chatSignal } from "./store/chat";
import { toast } from "./store/toast";
import "./styles/councilview.css";

type Phase = "idle" | "waiting" | "streaming" | "done" | "failed";

interface Panel {
    seat: Seat;
    phase: Phase;
    text: string;
    label: string;
    error: string;
    inputTokens: number;
    outputTokens: number;
    dollars: number | null;
    elapsedMs: number;
}

export default function CouncilView() {
    const chatState = useStore(chatSignal, chat);

    const [task, setTask] = useState("");
    const [panels, setPanels] = useState<Panel[]>([]);
    const [forecast, setForecast] = useState<Forecast | null>(null);
    const [running, setRunning] = useState(false);
    const [summary, setSummary] = useState("");
    const nextSeatRef = useRef(1);

    // Read fresh inside event handlers and async callbacks without
    // re-subscribing every render.
    const panelsRef = useRef(panels);
    panelsRef.current = panels;
    const taskRef = useRef(task);
    taskRef.current = task;

    const providers = chatState.status?.providers ?? [];
    const canRun = !running && task.trim().length > 0 && panels.length > 0;

    /** True once anything has been asked, which is when setup stops mattering. */
    const started = panels.some((p) => p.phase !== "idle");

    function addSeat() {
        setPanels((prev) => {
            if (prev.length >= 8) {
                toast.warning("Eight seats is the most this will run at once.");
                return prev;
            }
            const seat: Seat = {
                id: `seat-${nextSeatRef.current++}`,
                provider: chat.status?.provider ?? "groq",
                model: "",
                seesOthers: false,
                brief: "",
            };
            return [
                ...prev,
                {
                    seat,
                    phase: "idle",
                    text: "",
                    label: "",
                    error: "",
                    inputTokens: 0,
                    outputTokens: 0,
                    dollars: null,
                    elapsedMs: 0,
                },
            ];
        });
    }

    // Re-reads the estimate whenever a seat is added or removed, matching the
    // Svelte version's explicit `refreshForecast` call after each mutation.
    useEffect(() => {
        void refreshForecast();
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [panels.length]);

    function removeSeat(id: string) {
        setPanels((prev) => prev.filter((p) => p.seat.id !== id));
    }

    /** Re-reads the estimate. Called on every change that affects the bill. */
    async function refreshForecast() {
        const currentPanels = panelsRef.current;
        const currentTask = taskRef.current;
        if (currentPanels.length === 0 || !currentTask.trim()) {
            setForecast(null);
            return;
        }
        try {
            setForecast(await api.councilForecast(currentTask, currentPanels.map((p) => p.seat)));
        } catch {
            // An estimate that cannot be made is not worth an error message;
            // the run itself will say what is wrong.
            setForecast(null);
        }
    }

    function updateSeat(id: string, patch: Partial<Seat>) {
        setPanels((prev) =>
            prev.map((p) => (p.seat.id === id ? { ...p, seat: { ...p.seat, ...patch } } : p)),
        );
    }

    useEffect(() => {
        // Two seats to begin with: one is not a council, and more than two is a
        // decision the user should make rather than inherit.
        if (panelsRef.current.length === 0) {
            addSeat();
            addSeat();
        }

        const listeners = Promise.all([
            on<CouncilDeltaEvent>("council:delta", (p) => {
                setPanels((prev) =>
                    prev.map((panel) =>
                        panel.seat.id === p.seat
                            ? { ...panel, phase: "streaming", text: panel.text + p.text }
                            : panel,
                    ),
                );
            }),

            on<CouncilSeatDoneEvent>("council:seat-done", (p) => {
                setPanels((prev) =>
                    prev.map((panel) =>
                        panel.seat.id === p.seat
                            ? {
                                  ...panel,
                                  phase: "done",
                                  text: p.text,
                                  label: p.label,
                                  inputTokens: p.inputTokens,
                                  outputTokens: p.outputTokens,
                                  dollars: p.dollars,
                                  elapsedMs: p.elapsedMs,
                              }
                            : panel,
                    ),
                );
            }),

            on<CouncilSeatFailedEvent>("council:seat-failed", (p) => {
                // One seat down is one seat down; the rest keep going.
                setPanels((prev) =>
                    prev.map((panel) =>
                        panel.seat.id === p.seat
                            ? { ...panel, phase: "failed", error: p.message }
                            : panel,
                    ),
                );
            }),

            on<CouncilDoneEvent>("council:done", (p) => {
                setRunning(false);
                const cost = p.dollars > 0 ? ` · ~$${p.dollars.toFixed(4)}` : "";
                const rest = p.unpriced > 0 ? ` (+${p.unpriced} unpriced)` : "";
                setSummary(`${p.answered} answered, ${p.failed} failed${cost}${rest}`);
            }),
        ]);

        return () => void listeners.then((offs) => offs.forEach((off) => off()));
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    async function run() {
        if (!canRun) return;
        setSummary("");
        setRunning(true);

        setPanels((prev) =>
            prev.map((panel) => ({
                ...panel,
                phase: panel.seat.seesOthers ? "waiting" : "streaming",
                text: "",
                error: "",
                dollars: null,
                elapsedMs: 0,
            })),
        );

        try {
            await api.councilRun(taskRef.current, panelsRef.current.map((p) => p.seat));
        } catch (e) {
            setRunning(false);
            toast.failure("The council could not start.", e);
            setPanels((prev) => prev.map((panel) => ({ ...panel, phase: "idle" })));
        }
    }

    /** Puts one answer into the conversation, where the work continues. */
    async function keep(panel: Panel) {
        try {
            await api.councilKeep(panel.text);
            chat.add("assistant", panel.text);
            chat.view = "chat";
        } catch (e) {
            toast.failure("Could not keep that answer.", e);
        }
    }

    function copy(panel: Panel) {
        // Unlike the copy button on a chat message, this one has nowhere to
        // show a tick — the label is one word in a crowded column header — so
        // the confirmation goes to a toast instead of nothing at all.
        void navigator.clipboard
            .writeText(panel.text)
            .then(() => toast.success(`Copied ${panel.label || panel.seat.model}.`))
            .catch((e) => toast.failure("Could not copy that.", e));
    }

    function seconds(ms: number): string {
        return `${(ms / 1000).toFixed(1)}s`;
    }

    /** A seat's name, once it has one. Falls back to what was asked for. */
    function seatName(panel: Panel): string {
        return panel.label || panel.seat.model || `${panel.seat.provider} default`;
    }

    return (
        <div className="council">
            <header className="task-bar">
                <textarea
                    value={task}
                    onChange={(e) => setTask(e.target.value)}
                    onBlur={refreshForecast}
                    className="task"
                    rows={2}
                    placeholder="The question every seat answers…"
                    aria-label="The question every seat answers"
                ></textarea>

                <div className="go-column">
                    <button className="primary council-go" disabled={!canRun} onClick={run}>
                        {running ? (
                            <>
                                <span className="council-spinner" aria-hidden="true"></span>
                                Running…
                            </>
                        ) : (
                            <>
                                <Icon name="council" size={15} />
                                Ask the council
                            </>
                        )}
                    </button>

                    {summary ? (
                        <span className="summary">{summary}</span>
                    ) : forecast ? (
                        // On screen before the button is pressed. Four seats is four
                        // times the tokens, and nobody should discover that from a
                        // bill.
                        <span className="forecast">
                            {forecast.requests} requests · ~{forecast.tokens.toLocaleString()}
                            tokens
                            {forecast.dollars > 0 ? ` · ~$${forecast.dollars.toFixed(4)}` : ""}
                            {forecast.unpriced > 0 ? ` · ${forecast.unpriced} unpriced` : ""}
                        </span>
                    ) : null}
                </div>
            </header>

            <div className="seats">
                {panels.map((panel) => (
                    <section className="seat" data-phase={panel.phase} key={panel.seat.id}>
                        <div className="panel-head">
                            <span className="seat-name" title={seatName(panel)}>
                                {seatName(panel)}
                            </span>
                            <span className="phase" data-phase={panel.phase}>
                                {panel.phase === "streaming"
                                    ? "Writing"
                                    : panel.phase === "waiting"
                                      ? "Waiting"
                                      : panel.phase === "failed"
                                        ? "Failed"
                                        : panel.phase === "done"
                                          ? "Done"
                                          : ""}
                            </span>
                            <button
                                className="seat-close"
                                title="Remove this seat"
                                aria-label="Remove this seat"
                                onClick={() => removeSeat(panel.seat.id)}
                            >
                                <Icon name="close" size={13} />
                            </button>
                        </div>

                        {/* Folded away once the council has run: while you are setting
                             it up these are the point, and afterwards they sit between
                             you and the answer you asked for. */}
                        {!started ? (
                            <div className="setup">
                                <div className="setup-row">
                                    <select
                                        value={panel.seat.provider}
                                        onChange={(e) => {
                                            updateSeat(panel.seat.id, { provider: e.target.value });
                                            void refreshForecast();
                                        }}
                                        aria-label="Provider"
                                    >
                                        {providers.map((p) => (
                                            <option key={p.id} value={p.id}>
                                                {p.id}
                                                {p.hasKey || !p.needsKey ? "" : " — no key"}
                                            </option>
                                        ))}
                                    </select>
                                    <input
                                        value={panel.seat.model}
                                        onChange={(e) => updateSeat(panel.seat.id, { model: e.target.value })}
                                        onBlur={refreshForecast}
                                        placeholder="Default model"
                                        aria-label="Model"
                                        title="Leave empty for the provider's default."
                                    />
                                </div>

                                <input
                                    className="brief"
                                    value={panel.seat.brief}
                                    onChange={(e) => updateSeat(panel.seat.id, { brief: e.target.value })}
                                    placeholder="Angle for this seat (optional)"
                                    aria-label="Angle for this seat"
                                />

                                <label
                                    className="sees"
                                    title="Off gives an independent answer. On makes this seat read the others first, so it can build on them or argue with them."
                                >
                                    <input
                                        type="checkbox"
                                        checked={panel.seat.seesOthers}
                                        onChange={(e) => {
                                            updateSeat(panel.seat.id, { seesOthers: e.target.checked });
                                            void refreshForecast();
                                        }}
                                    />
                                    Reads the others first
                                </label>
                            </div>
                        ) : null}

                        <div className="answer">
                            {panel.phase === "failed" ? (
                                <p className="failed">
                                    <Icon name="warning" size={14} />
                                    <span className="selectable">{panel.error}</span>
                                </p>
                            ) : panel.phase === "waiting" ? (
                                <p className="dim">Waiting for the others…</p>
                            ) : panel.text ? (
                                <div
                                    className="md selectable"
                                    // eslint-disable-next-line react/no-danger
                                    dangerouslySetInnerHTML={{ __html: renderMarkdown(panel.text) }}
                                ></div>
                            ) : panel.phase === "streaming" ? (
                                <div className="skeletons" aria-hidden="true">
                                    {Array.from({ length: 5 }, (_, row) => (
                                        <div key={row} className="skeleton" style={{ width: `${92 - row * 11}%` }}></div>
                                    ))}
                                </div>
                            ) : (
                                <p className="dim">No answer yet.</p>
                            )}
                        </div>

                        {panel.phase === "done" ? (
                            <div className="panel-foot">
                                <span className="stats">
                                    {panel.inputTokens + panel.outputTokens} tok ·{" "}
                                    {seconds(panel.elapsedMs)}
                                    {panel.dollars !== null
                                        ? ` · ~$${panel.dollars.toFixed(4)}`
                                        : " · unpriced"}
                                </span>
                                <span className="panel-actions">
                                    <button onClick={() => copy(panel)}>
                                        <Icon name="copy" size={13} />
                                        Copy
                                    </button>
                                    <button className="primary" onClick={() => keep(panel)}>
                                        Keep
                                    </button>
                                </span>
                            </div>
                        ) : null}
                    </section>
                ))}

                <button className="add" onClick={addSeat} disabled={panels.length >= 8}>
                    <Icon name="plus" size={14} />
                    Seat
                </button>
            </div>
        </div>
    );
}
