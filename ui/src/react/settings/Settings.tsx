/**
 * Settings.
 *
 * Categories on the left, content on the right — the layout Obsidian and the
 * Claude web app both use, and the only one that carries this many settings
 * without drowning the reader. A narrow rail panel could not: by the time
 * every integration had a section it was a two-thousand-pixel scroll.
 *
 * Three rules hold here.
 *
 * **Everything applies immediately.** There is no save button, because there
 * is no such thing as losing a change you forgot to save. Secrets are the one
 * exception: a key field commits on Enter or on leaving the field, not on
 * every keystroke, or half a key would be stored and tested.
 *
 * **A key is never shown again.** Once stored it is masked and reported as
 * stored. The value lives encrypted in `keys.dat` and does not come back
 * across this bridge.
 *
 * **Status is visible, and testable.** Every provider and integration says
 * whether it is connected, and "test" makes a real request rather than
 * checking that a field is non-empty. Nobody should learn that their key is
 * wrong by starting a conversation.
 */

import { useEffect, useState } from "react";
import {
    api,
    type CanvasSettings,
    type VirusTotalSettings,
    type ConnectionTest,
    type Fact,
    type MemorySettings,
    type McpServerInfo,
    type SearchSettings,
    type SpotifySettings,
    type SteamSettings,
    type VoiceSettings,
    type Tool,
    type UpdateCheck,
    type VaultInfo,
} from "../../lib/api";
import { ask } from "../store/confirm";
import Icon from "../Icon";
import Modal from "../Modal";
import { chat } from "../store/chat";
import { useStore } from "../store/useStore";
import { chatSignal } from "../store/chat";
import { toast } from "../store/toast";
import { filterGroups } from "../../lib/settings/registry";
import KeysPane from "./panes/KeysPane";
import SearchPane from "./panes/SearchPane";
import CanvasPane from "./panes/CanvasPane";
import ToolsPane from "./panes/ToolsPane";
import ShortcutsPane from "./panes/ShortcutsPane";
import DataPane from "./panes/DataPane";
import MemoryPane from "./panes/MemoryPane";
import PhonePane from "./panes/PhonePane";
import ObsidianPane from "./panes/ObsidianPane";
import SpotifyPane from "./panes/SpotifyPane";
import SteamPane from "./panes/SteamPane";
import McpPane, { type Draft as McpDraft } from "./panes/McpPane";
import UpdatesPane from "./panes/UpdatesPane";
import GeneralPane from "./panes/GeneralPane";
import ProviderPane from "./panes/ProviderPane";
import VoicePane from "./panes/VoicePane";
import "../styles/settings.css";

const LANGUAGES: [string, string][] = [
    ["en", "English"],
    ["tr", "Türkçe"],
    ["de", "Deutsch"],
    ["fr", "Français"],
    ["es", "Español"],
];

const WINDOW_MODES = ["windowed", "borderless", "fullscreen"];

const SHORTCUTS: [string, string][] = [
    ["Ctrl + ,", "open settings"],
    ["Ctrl + M", "cycle voice mode"],
    ["Ctrl + L", "clear the conversation"],
    ["Ctrl + S", "save the open file (code interface)"],
    ["Esc", "stop speaking, or close this"],
    ["F11", "cycle window mode"],
    ["Enter", "send"],
    ["Shift + Enter", "newline"],
];

const EMPTY_DRAFT: McpDraft = {
    id: "",
    transport: "stdio",
    command: "",
    args: "",
    url: "",
    headerName: "",
    headerValue: "",
    secret: "",
};

export default function Settings() {
    const state = useStore(chatSignal, chat);
    const status = state.status;

    const [active, setActive] = useState("general");
    const [query, setQuery] = useState("");

    /**
     * The last release check.
     *
     * `null` means nobody has asked yet -- distinct from a check that ran and
     * failed, which the pane reports as a failure rather than as silence.
     */
    const [update, setUpdate] = useState<UpdateCheck | null>(null);
    const [checkingUpdate, setCheckingUpdate] = useState(false);

    async function checkUpdate() {
        setCheckingUpdate(true);
        try {
            setUpdate(await api.checkUpdate());
        } finally {
            setCheckingUpdate(false);
        }
    }

    /** Test results, keyed by target. */
    const [tests, setTests] = useState<Record<string, ConnectionTest>>({});
    const [testing, setTesting] = useState<string | null>(null);

    const groups = filterGroups(query);
    const matches = groups.flatMap((g) => g.categories);

    // A search that narrows to one category should land on it rather than
    // leaving the reader looking at an unrelated pane.
    useEffect(() => {
        if (matches.length > 0 && !matches.some((c) => c.id === active)) {
            setActive(matches[0].id);
        }
        // Re-run only when the match set actually changes shape; `active` is
        // read, not depended on, so picking a category does not retrigger this.
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [query]);

    async function test(target: string) {
        setTesting(target);
        try {
            const outcome = await api.testConnection(target);
            setTests((prev) => ({ ...prev, [target]: outcome }));
        } catch (e) {
            setTests((prev) => ({ ...prev, [target]: { ok: false, detail: String(e) } }));
        } finally {
            setTesting(null);
        }
    }

    // ── Loaded state ───────────────────────────────────────────────────

    const [search, setSearch] = useState<SearchSettings | null>(null);
    const [voice, setVoice] = useState<VoiceSettings | null>(null);
    const [canvas, setCanvas] = useState<CanvasSettings | null>(null);
    const [virustotal, setVirustotal] = useState<VirusTotalSettings | null>(null);
    const [vaults, setVaults] = useState<VaultInfo[]>([]);
    const [spotify, setSpotify] = useState<SpotifySettings | null>(null);
    const [steam, setSteam] = useState<SteamSettings | null>(null);
    const [mcp, setMcp] = useState<McpServerInfo[]>([]);
    const [facts, setFacts] = useState<Fact[]>([]);
    const [memory, setMemory] = useState<MemorySettings | null>(null);
    const [tools, setTools] = useState<Tool[]>([]);
    const [models, setModels] = useState<string[]>([]);
    const [loadingModels, setLoadingModels] = useState(false);
    /** The code provider's own model list, kept apart from the chat one so
        opening one picker does not fill the other with the wrong provider's
        models. */
    const [codeModels, setCodeModels] = useState<string[]>([]);
    const [loadingCodeModels, setLoadingCodeModels] = useState(false);

    /**
     * Which provider's key box is open, if any.
     *
     * One at a time. Every provider showing an empty password field at once
     * reads as a form demanding five keys, when the truth is that one is
     * enough and the rest are alternatives.
     */
    const [keyOpen, setKeyOpen] = useState<string | null>(null);
    const [keyDraft, setKeyDraft] = useState("");
    const [steamIdDraft, setSteamIdDraft] = useState("");
    const [steamKeyDraft, setSteamKeyDraft] = useState("");
    const [spotifyIdDraft, setSpotifyIdDraft] = useState("");
    const [draft, setDraft] = useState<McpDraft>(EMPTY_DRAFT);

    async function load() {
        try {
            setSteamIdDraft((await api.steamSettings()).steamId);
            const [
                searchResult,
                canvasResult,
                vaultsResult,
                spotifyResult,
                steamResult,
                mcpResult,
                factsResult,
                toolsResult,
                voiceResult,
                virustotalResult,
                memoryResult,
            ] = await Promise.all([
                api.searchSettings(),
                api.canvasSettings(),
                api.listVaults(),
                api.spotifySettings(),
                api.steamSettings(),
                api.listMcpServers(),
                api.listFacts(),
                api.listTools(),
                api.voiceSettings(),
                api.virusTotalSettings(),
                api.memorySettings(),
            ]);
            setSearch(searchResult);
            setCanvas(canvasResult);
            setVirustotal(virustotalResult);
            setVaults(vaultsResult);
            setSpotify(spotifyResult);
            setSteam(steamResult);
            setMcp(mcpResult);
            setFacts(factsResult);
            setMemory(memoryResult);
            setTools(toolsResult);
            setVoice(voiceResult);
            setSpotifyIdDraft(spotifyResult?.clientId ?? "");
        } catch (e) {
            toast.failure("Some settings could not be loaded.", e);
        }
    }

    // Runs once on mount, not on every render: this fires eleven IPC calls
    // and a dependency on anything it touched would re-run it whenever that
    // changed. One load per open is all this screen needs, and the repeated
    // bursts were what made moving between categories feel slow.
    useEffect(() => {
        void load();
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    /**
     * Wraps an action so every outcome lands in the same place.
     *
     * Results used to go to a notice strip at the top of the pane, which had
     * two faults: it never cleared, so "Saved." from ten minutes ago was still
     * up; and it sat above the fold, so a failure from a control at the bottom
     * of a long pane was reported off screen. A toast has neither problem.
     */
    async function run(action: () => Promise<unknown>, said = "") {
        try {
            await action();
            if (said) toast.success(said);
        } catch (e) {
            toast.failure("That did not work.", e);
        }
    }

    async function updateSetting(field: string, value: string) {
        await run(async () => {
            await api.setSetting(field, value);
            await chat.refresh();
            if (field === "fontSize") toast.info("Applies after restart.");
            else toast.success("Saved.");
        });
    }

    /**
     * Full authority: every approval and budget off.
     *
     * Warned about once, on the way in, and never again -- re-asking would be
     * the exact thing the switch exists to stop. Turning it back off needs no
     * confirmation: restoring a guard is not the risky direction.
     */
    async function toggleFullAuthority(on: boolean) {
        if (on) {
            const confirmed = await ask({
                title: "Give Vavis full authority?",
                body:
                    "Approval prompts are turned off. Files can be deleted, " +
                    "commands run and settings changed without asking you first. " +
                    "Two exceptions stay: if a page or file the assistant reads " +
                    "tries to give it orders, destructive actions still ask; and " +
                    "anything asked from your phone always asks. Nothing is undone " +
                    "by turning this back off.",
                confirmLabel: "Give full authority",
                danger: true,
            });
            if (!confirmed) {
                // Put the switch back: the click already moved it.
                await chat.refresh();
                return;
            }
        }
        await updateSetting("fullAuthority", on ? "true" : "false");
    }

    /**
     * Saves a speech setting and reloads the section.
     *
     * Reloading matters: switching engine changes which voice list applies,
     * and a stale list would offer voices the new engine has never heard of.
     */
    async function updateVoice(field: string, value: string) {
        await run(async () => {
            await api.setSetting(field, value);
            setVoice(await api.voiceSettings());
            toast.success("Saved.");
        });
    }

    async function saveVoiceKey(key: string) {
        await run(async () => {
            await api.setVoiceKey(key);
            setVoice(await api.voiceSettings());
        }, "ElevenLabs key saved, encrypted.");
    }

    async function saveKey(provider: string) {
        // Nothing typed is not a failure — it is the ordinary case of opening
        // the box and thinking better of it. Close, say nothing.
        if (!keyDraft.trim()) {
            setKeyOpen(null);
            return;
        }
        const value = keyDraft.trim();
        await run(async () => {
            await api.setKey(provider, value);
            setKeyDraft("");
            setKeyOpen(null);
            await chat.refresh();
            // The stored key is proved immediately rather than at the next
            // conversation, which is the entire point of this screen.
            void test(provider);
        }, "Key saved, encrypted.");
    }

    /**
     * Opens the key box for one provider, closing any other.
     *
     * The draft is dropped rather than carried across, so a key half typed for
     * one provider can never be saved against another.
     */
    function openKey(provider: string) {
        setKeyDraft("");
        setKeyOpen(keyOpen === provider ? null : provider);
    }

    async function pickProvider(id: string) {
        await run(async () => {
            await api.setProvider(id);
            setModels([]);
            await chat.refresh();
        });
    }

    async function setFallback(providers: string[]) {
        await run(async () => {
            await api.setFallback(providers);
            await chat.refresh();
        });
    }

    async function fetchModels() {
        setLoadingModels(true);
        await run(async () => {
            setModels(await api.listModels());
        });
        setLoadingModels(false);
    }

    async function pickModel(model: string) {
        await run(async () => {
            await api.setModel(model);
            setModels([]);
            await chat.refresh();
        });
    }

    async function pickCodeProvider(id: string) {
        await run(async () => {
            await api.setCodeProvider(id);
            setCodeModels([]);
            await chat.refresh();
        });
    }

    async function fetchCodeModels() {
        setLoadingCodeModels(true);
        await run(async () => {
            setCodeModels(await api.listCodeModels());
        });
        setLoadingCodeModels(false);
    }

    async function pickCodeModel(model: string) {
        await run(async () => {
            await api.setCodeModel(model);
            setCodeModels([]);
            await chat.refresh();
        });
    }

    async function pickVault(path: string) {
        await run(async () => {
            await api.setVault(path);
            setVaults(await api.listVaults());
            void test("obsidian");
        });
    }

    async function saveSteam() {
        await run(async () => {
            // The backend verifies and reports what it found, including the
            // private-profile case that otherwise looks like an empty library.
            toast.success(await api.setSteam(steamIdDraft.trim(), steamKeyDraft.trim()));
            setSteamKeyDraft("");
            setSteam(await api.steamSettings());
        });
    }

    async function connectSpotify() {
        await run(async () => {
            await api.setSpotifyClientId(spotifyIdDraft);
            await api.connectSpotify();
        }, "Browser opened — approve the request there.");
    }

    async function disconnectSpotify() {
        await run(async () => {
            await api.disconnectSpotify();
            setSpotify(await api.spotifySettings());
        }, "Spotify disconnected.");
    }

    async function mcpAction(fn: () => Promise<unknown>) {
        await run(async () => {
            await fn();
            setMcp(await api.listMcpServers());
            await chat.refresh();
        });
    }

    async function saveMcp() {
        await run(async () => {
            toast.success(await api.saveMcpServer(draft));
            setDraft(EMPTY_DRAFT);
            setMcp(await api.listMcpServers());
            await chat.refresh();
        });
    }

    function close() {
        chat.panel = "none";
    }

    return (
        // Its own header rather than the modal's: the title belongs at the top of
        // the category rail, above the search field that filters it, not on a bar
        // spanning both columns.
        <Modal label="Settings" size="full" bare showClose={false} onClose={close}>
            <div className="settings">
                <nav className="categories">
                    <div className="nav-head">
                        <span className="nav-title">Settings</span>
                        <button className="nav-close" onClick={close} aria-label="Close (Esc)">
                            <Icon name="close" size={15} />
                        </button>
                    </div>

                    <input
                        className="search"
                        data-autofocus
                        value={query}
                        onChange={(e) => setQuery(e.target.value)}
                        placeholder="Search settings…"
                        spellCheck={false}
                        aria-label="Search settings"
                    />

                    {groups.map((group) => (
                        <div key={group.title}>
                            {/* The heading is a signpost while browsing and noise while
                                searching: a search already narrows the list, and four
                                headings over one match each is worse than none. */}
                            {!query.trim() && <span className="group-title">{group.title}</span>}
                            {group.categories.map((category) => (
                                <button
                                    className={active === category.id ? "category active" : "category"}
                                    onClick={() => setActive(category.id)}
                                    key={category.id}
                                >
                                    <span className="cat-icon">{category.icon}</span>
                                    {category.label}
                                </button>
                            ))}
                        </div>
                    ))}

                    {matches.length === 0 && (
                        <div className="nav-empty">
                            <p>Nothing matches "{query}".</p>
                            <button onClick={() => setQuery("")}>Clear</button>
                        </div>
                    )}
                </nav>

                <section className="pane">
                    {active === "general" && (
                        <GeneralPane
                            status={status}
                            languages={LANGUAGES}
                            windowModes={WINDOW_MODES}
                            onchange={updateSetting}
                        />
                    )}
                    {active === "provider" && (
                        <ProviderPane
                            status={status}
                            tests={tests}
                            testing={testing}
                            models={models}
                            loadingModels={loadingModels}
                            keyOpen={keyOpen}
                            keyDraft={keyDraft}
                            onKeyDraftChange={setKeyDraft}
                            onpickprovider={pickProvider}
                            onpickmodel={pickModel}
                            onopenkey={openKey}
                            onsavekey={saveKey}
                            oncancelkey={() => {
                                setKeyDraft("");
                                setKeyOpen(null);
                            }}
                            onfetchmodels={fetchModels}
                            codeModels={codeModels}
                            loadingCodeModels={loadingCodeModels}
                            onpickcodeprovider={pickCodeProvider}
                            onpickcodemodel={pickCodeModel}
                            onfetchcodemodels={fetchCodeModels}
                            ontest={test}
                            onchange={updateSetting}
                            onsetfallback={setFallback}
                        />
                    )}
                    {active === "voice" && (
                        <VoicePane
                            status={status}
                            voice={voice}
                            onupdate={updateVoice}
                            onsavekey={saveVoiceKey}
                            onreload={async () => setVoice(await api.voiceSettings())}
                        />
                    )}
                    {active === "memory" && (
                        <MemoryPane
                            facts={facts}
                            settings={memory}
                            onchange={async (field, value) => {
                                await updateSetting(field, value);
                                setMemory(await api.memorySettings());
                            }}
                            onforget={(id) =>
                                run(async () => {
                                    await api.forgetFact(id);
                                    setFacts(await api.listFacts());
                                    await chat.refresh();
                                })
                            }
                        />
                    )}
                    {active === "keys" && (
                        <KeysPane
                            status={status}
                            search={search}
                            canvas={canvas}
                            virustotal={virustotal}
                            reload={load}
                        />
                    )}
                    {active === "search" && <SearchPane search={search} reload={load} />}
                    {active === "canvas" && <CanvasPane canvas={canvas} reload={load} />}
                    {active === "phone" && <PhonePane />}
                    {active === "obsidian" && (
                        <ObsidianPane
                            vaults={vaults}
                            result={tests.obsidian}
                            testing={testing === "obsidian"}
                            onpick={pickVault}
                            ontest={() => test("obsidian")}
                        />
                    )}
                    {active === "spotify" && (
                        <SpotifyPane
                            spotify={spotify}
                            clientId={spotifyIdDraft}
                            result={tests.spotify}
                            testing={testing === "spotify"}
                            onconnect={connectSpotify}
                            ondisconnect={disconnectSpotify}
                            ontest={() => test("spotify")}
                            onsaveid={(id) =>
                                run(async () => {
                                    setSpotifyIdDraft(id);
                                    await api.setSpotifyClientId(id);
                                    setSpotify(await api.spotifySettings());
                                }, "Client id saved.")
                            }
                        />
                    )}
                    {active === "steam" && (
                        <SteamPane
                            steam={steam}
                            steamId={steamIdDraft}
                            result={tests.steam}
                            testing={testing === "steam"}
                            onsave={(id, key) => {
                                setSteamIdDraft(id);
                                setSteamKeyDraft(key);
                                void saveSteam();
                            }}
                            ontest={() => test("steam")}
                        />
                    )}
                    {active === "mcp" && (
                        <McpPane
                            servers={mcp}
                            draft={draft}
                            onDraftChange={setDraft}
                            ontoggleserver={(id, enabled) =>
                                mcpAction(() => api.toggleMcpServer(id, enabled))
                            }
                            onremove={(id) => mcpAction(() => api.removeMcpServer(id))}
                            ontoggletool={(id, toolName, enabled) =>
                                mcpAction(() => api.toggleMcpTool(id, toolName, enabled))
                            }
                            onsave={saveMcp}
                        />
                    )}
                    {active === "tools" && (
                        <ToolsPane
                            tools={tools}
                            fullAuthority={status?.fullAuthority ?? false}
                            ontoggle={toggleFullAuthority}
                        />
                    )}
                    {active === "shortcuts" && <ShortcutsPane shortcuts={SHORTCUTS} />}
                    {active === "data" && (
                        <DataPane status={status} canvas={canvas} onclear={() => chat.clearWithConfirm()} />
                    )}
                    {active === "updates" && (
                        <UpdatesPane
                            status={status}
                            update={update}
                            checking={checkingUpdate}
                            oncheck={checkUpdate}
                        />
                    )}
                </section>
            </div>
        </Modal>
    );
}
