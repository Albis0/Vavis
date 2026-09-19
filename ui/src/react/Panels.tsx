/**
 * Memory, automations and tools.
 *
 * These three answer "what does it remember", "what fires when" and "what can
 * it do". They used to live in a rail permanently docked to the right of the
 * window; now they open as a sheet over the stage and close again, because
 * they are things you consult occasionally rather than watch.
 *
 * Settings is not among them — fourteen categories outgrew this shape long
 * ago and has its own window in Settings.
 */
import { useEffect, useState } from "react";
import Icon, { type IconName } from "./Icon";
import Modal from "./Modal";
import { api, type Automation, type Fact, type Tool } from "../lib/api";
import { ask } from "./store/confirm";
import { useStore } from "./store/useStore";
import { chat, chatSignal } from "./store/chat";
import { toast } from "./store/toast";
import "./styles/panels.css";

const TITLES: Record<string, string> = {
    memory: "Remembered facts",
    automations: "Automations",
    tools: "Tools",
};

/**
 * What an empty panel says.
 *
 * Each one names the thing that is missing and then shows how to create
 * it, in the words the user would actually type. "No items" tells a
 * first-time reader nothing they had not already worked out from the
 * blank space.
 */
const EMPTY: Record<string, { icon: IconName; title: string; body: string }> = {
    memory: {
        icon: "memory",
        title: "Nothing remembered yet",
        body: "Say “remember that I prefer metric units” and it will be kept here, across every conversation.",
    },
    automations: {
        icon: "clock",
        title: "No automations",
        body: "Say “every morning at 09:00 tell me the weather” and it will run on its own from then on.",
    },
    tools: {
        icon: "tool",
        title: "No tools registered",
        body: "Tools come from the built-in set and from any MCP servers you add in settings.",
    },
};

/** Below this many rows, a filter field costs more than it saves. */
const SEARCHABLE = 8;

/** Case-insensitive substring match, used by all three lists. */
function hit(query: string, ...fields: string[]): boolean {
    const q = query.trim().toLowerCase();
    if (!q) return true;
    return fields.join(" ").toLowerCase().includes(q);
}

export default function Panels() {
    const state = useStore(chatSignal, chat);

    const [facts, setFacts] = useState<Fact[]>([]);
    const [automations, setAutomations] = useState<Automation[]>([]);
    const [tools, setTools] = useState<Tool[]>([]);
    /** Set when the load failed, so the panel can offer a retry rather than
        showing an empty list that looks like "you have none of these". */
    const [failure, setFailure] = useState("");
    const [loading, setLoading] = useState(false);
    const [query, setQuery] = useState("");

    const title = TITLES[state.panel] ?? "";

    /** Rows in the open panel, before filtering. */
    const listLength =
        state.panel === "memory"
            ? facts.length
            : state.panel === "automations"
              ? automations.length
              : state.panel === "tools"
                ? tools.length
                : 0;

    /** Reloads whatever the open panel shows. */
    async function load() {
        setFailure("");
        setLoading(true);
        try {
            switch (chat.panel) {
                case "memory":
                    setFacts(await api.listFacts());
                    break;
                case "automations":
                    setAutomations(await api.listAutomations());
                    break;
                case "tools":
                    setTools(await api.listTools());
                    break;
            }
        } catch (e) {
            setFailure(e instanceof Error ? e.message : String(e));
        } finally {
            setLoading(false);
        }
    }

    // Reload when the panel changes.
    useEffect(() => {
        setQuery("");
        void load();
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [state.panel]);

    function close() {
        chat.panel = "none";
    }

    const shownFacts = facts.filter((f) => hit(query, f.text));
    const shownAutomations = automations.filter((a) => hit(query, a.trigger, a.prompt));
    const shownTools = tools.filter((t) => hit(query, t.name, t.description));

    /** True once the list is loaded, succeeded, and has nothing in it. */
    const isEmpty =
        !loading && !failure
            ? state.panel === "memory"
                ? facts.length === 0
                : state.panel === "automations"
                  ? automations.length === 0
                  : state.panel === "tools"
                    ? tools.length === 0
                    : false
            : false;

    /** Loaded and non-empty, but the search matched nothing. */
    const noMatches =
        !loading && !failure && !isEmpty && query.trim()
            ? state.panel === "memory"
                ? shownFacts.length === 0
                : state.panel === "automations"
                  ? shownAutomations.length === 0
                  : state.panel === "tools"
                    ? shownTools.length === 0
                    : false
            : false;

    async function forget(fact: Fact) {
        const confirmed = await ask({
            title: "Forget this?",
            body: fact.text,
            confirmLabel: "Forget",
            danger: true,
        });
        if (!confirmed) return;

        try {
            await api.forgetFact(fact.id);
            toast.success("Forgotten.");
            await load();
            await chat.refresh();
        } catch (e) {
            toast.failure("Could not forget that.", e);
        }
    }

    async function toggle(automation: Automation) {
        try {
            await api.toggleAutomation(automation.id, !automation.enabled);
            toast.success(automation.enabled ? "Paused." : "Resumed.");
            await load();
        } catch (e) {
            toast.failure("Could not change that automation.", e);
        }
    }

    async function remove(automation: Automation) {
        const confirmed = await ask({
            title: "Delete this automation?",
            body: `${automation.trigger} — ${automation.prompt}`,
            confirmLabel: "Delete",
            danger: true,
        });
        if (!confirmed) return;

        try {
            await api.deleteAutomation(automation.id);
            toast.success("Automation deleted.");
            await load();
            await chat.refresh();
        } catch (e) {
            toast.failure("Could not delete that automation.", e);
        }
    }

    return (
        <Modal title={title} size="lg" bare onClose={close}>
            {/* The search field is offered only once a list is long enough to need
                 one. Below that it is a control that can only ever narrow three rows
                 to two, which is friction rather than help. */}
            {!loading && !failure && !isEmpty && listLength >= SEARCHABLE ? (
                <div className="panels-search">
                    <input
                        value={query}
                        onChange={(e) => setQuery(e.target.value)}
                        placeholder={`Filter ${title.toLowerCase()}…`}
                        spellCheck={false}
                        aria-label={`Filter ${title.toLowerCase()}`}
                    />
                    {query ? (
                        <button className="panels-clear" onClick={() => setQuery("")} aria-label="Clear filter">
                            <Icon name="close" size={13} />
                        </button>
                    ) : null}
                </div>
            ) : null}

            <div className="panels-content">
                {loading ? (
                    <>
                        {/* Skeleton rows rather than a spinner: they occupy the shape the
                             answer will, so the panel does not jump when it arrives. */}
                        <div className="panels-skeletons" aria-hidden="true">
                            {Array.from({ length: 4 }, (_, row) => (
                                <div key={row} className="panels-skeleton" style={{ width: `${88 - row * 9}%` }}></div>
                            ))}
                        </div>
                        <span className="sr-only">Loading…</span>
                    </>
                ) : failure ? (
                    <div className="panels-state">
                        <Icon name="warning" size={22} />
                        <p className="panels-state-title">That did not load.</p>
                        <p className="panels-state-body selectable">{failure}</p>
                        <button className="outline" onClick={load}>
                            Try again
                        </button>
                    </div>
                ) : isEmpty ? (
                    <div className="panels-state">
                        <Icon name={EMPTY[state.panel].icon} size={22} />
                        <p className="panels-state-title">{EMPTY[state.panel].title}</p>
                        <p className="panels-state-body">{EMPTY[state.panel].body}</p>
                    </div>
                ) : noMatches ? (
                    <div className="panels-state">
                        <p className="panels-state-title">Nothing matches "{query}"</p>
                        <button className="outline" onClick={() => setQuery("")}>
                            Clear filter
                        </button>
                    </div>
                ) : state.panel === "memory" ? (
                    shownFacts.map((fact) => (
                        <div className="panels-entry" key={fact.id}>
                            <span className="entry-text selectable">{fact.text}</span>
                            <div className="row-actions">
                                <button className="danger row-action" onClick={() => forget(fact)}>
                                    <Icon name="trash" size={13} />
                                    Forget
                                </button>
                            </div>
                        </div>
                    ))
                ) : state.panel === "automations" ? (
                    shownAutomations.map((a) => (
                        <div className={a.enabled ? "entry" : "entry off"} key={a.id}>
                            <div className="entry-main">
                                <span className="trigger">{a.trigger}</span>
                                <span className="entry-text selectable">{a.prompt}</span>
                            </div>
                            <div className="row-actions">
                                <button className="row-action" onClick={() => toggle(a)}>
                                    {a.enabled ? "Pause" : "Resume"}
                                </button>
                                <button
                                    className="danger row-action"
                                    onClick={() => remove(a)}
                                    aria-label="Delete automation"
                                >
                                    <Icon name="trash" size={13} />
                                </button>
                            </div>
                        </div>
                    ))
                ) : state.panel === "tools" ? (
                    <>
                        <p className="hint">
                            {tools.length} tools. At most 12 reach the model on any one request,
                            chosen by what the request is about.
                        </p>
                        {shownTools.map((tool) => (
                            <div className="panels-entry tool-entry" key={tool.name}>
                                <div className="entry-main">
                                    <span className="panels-tool-name">
                                        {tool.name}
                                        <span className="risk" data-risk={tool.risk}>
                                            {tool.risk}
                                        </span>
                                    </span>
                                    <span className="tool-desc">{tool.description}</span>
                                </div>
                            </div>
                        ))}
                    </>
                ) : null}
            </div>
        </Modal>
    );
}
