<!--
    Settings.

    Categories on the left, content on the right — the layout Obsidian and the
    Claude web app both use, and the only one that carries this many settings
    without drowning the reader. A narrow rail panel could not: by the time
    every integration had a section it was a two-thousand-pixel scroll.

    Three rules hold here.

    **Everything applies immediately.** There is no save button, because there
    is no such thing as losing a change you forgot to save. Secrets are the one
    exception: a key field commits on Enter or on leaving the field, not on
    every keystroke, or half a key would be stored and tested.

    **A key is never shown again.** Once stored it is masked and reported as
    stored. The value lives encrypted in `keys.dat` and does not come back
    across this bridge.

    **Status is visible, and testable.** Every provider and integration says
    whether it is connected, and "test" makes a real request rather than
    checking that a field is non-empty. Nobody should learn that their key is
    wrong by starting a conversation.
-->
<script lang="ts">
    import {
        api,
        type CanvasSettings,
        type ConnectionTest,
        type Fact,
        type McpServerInfo,
        type SearchSettings,
        type SpotifySettings,
        type SteamSettings,
        type VoiceSettings,
        type Tool,
        type UpdateCheck,
        type VaultInfo,
    } from "./api";
    import { ask } from "./confirm.svelte";
    import Icon from "./Icon.svelte";
    import Modal from "./Modal.svelte";
    import { chat } from "./store.svelte";
    import { toast } from "./toast.svelte";
    import SearchPane from "./settings/panes/SearchPane.svelte";
    import CanvasPane from "./settings/panes/CanvasPane.svelte";
    import ToolsPane from "./settings/panes/ToolsPane.svelte";
    import ShortcutsPane from "./settings/panes/ShortcutsPane.svelte";
    import DataPane from "./settings/panes/DataPane.svelte";
    import MemoryPane from "./settings/panes/MemoryPane.svelte";
    import ObsidianPane from "./settings/panes/ObsidianPane.svelte";
    import SpotifyPane from "./settings/panes/SpotifyPane.svelte";
    import SteamPane from "./settings/panes/SteamPane.svelte";
    import McpPane from "./settings/panes/McpPane.svelte";
    import UpdatesPane from "./settings/panes/UpdatesPane.svelte";
    import GeneralPane from "./settings/panes/GeneralPane.svelte";
    import ProviderPane from "./settings/panes/ProviderPane.svelte";
    import VoicePane from "./settings/panes/VoicePane.svelte";
    import { onMount } from "svelte";
    import { filterGroups } from "./settings/registry";

    const status = $derived(chat.status);


    let active = $state("general");
    let query = $state("");

    /**
     * The last release check.
     *
     * `null` means nobody has asked yet -- distinct from a check that ran and
     * failed, which the pane reports as a failure rather than as silence.
     */
    let update = $state<UpdateCheck | null>(null);
    let checkingUpdate = $state(false);

    async function checkUpdate() {
        checkingUpdate = true;
        try {
            update = await api.checkUpdate();
        } finally {
            checkingUpdate = false;
        }
    }

    /** Test results, keyed by target. */
    let tests = $state<Record<string, ConnectionTest>>({});
    let testing = $state<string | null>(null);

    const groups = $derived(filterGroups(query));
    const matches = $derived(groups.flatMap((g) => g.categories));

    // A search that narrows to one category should land on it rather than
    // leaving the reader looking at an unrelated pane.
    $effect(() => {
        if (matches.length > 0 && !matches.some((c) => c.id === active)) {
            active = matches[0].id;
        }
    });

    async function test(target: string) {
        testing = target;
        try {
            tests[target] = await api.testConnection(target);
        } catch (e) {
            tests[target] = { ok: false, detail: String(e) };
        } finally {
            testing = null;
        }
    }

    // ── Loaded state ───────────────────────────────────────────────────

    let search = $state<SearchSettings | null>(null);
    let voice = $state<VoiceSettings | null>(null);
    let canvas = $state<CanvasSettings | null>(null);
    let vaults = $state<VaultInfo[]>([]);
    let spotify = $state<SpotifySettings | null>(null);
    let steam = $state<SteamSettings | null>(null);
    let mcp = $state<McpServerInfo[]>([]);
    let facts = $state<Fact[]>([]);
    let tools = $state<Tool[]>([]);
    let models = $state<string[]>([]);
    let loadingModels = $state(false);

    /**
     * Which provider's key box is open, if any.
     *
     * One at a time. Every provider showing an empty password field at once
     * reads as a form demanding five keys, when the truth is that one is
     * enough and the rest are alternatives.
     */
    let keyOpen = $state<string | null>(null);
    let keyDraft = $state("");
    let steamIdDraft = $state("");
    let steamKeyDraft = $state("");
    let spotifyIdDraft = $state("");
    let draft = $state({
        id: "",
        transport: "stdio",
        command: "",
        args: "",
        url: "",
        headerName: "",
        headerValue: "",
        secret: "",
    });

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

    // `onMount`, not `$effect`: this fires eleven IPC calls and an effect
    // re-runs whenever anything it touched changes. One load per open is all
    // this screen needs, and the repeated bursts were what made moving
    // between categories feel slow.
    onMount(() => {
        void load();
    });

    async function load() {
        try {
            steamIdDraft = (await api.steamSettings()).steamId;
            [search, canvas, vaults, spotify, steam, mcp, facts, tools, voice] =
                await Promise.all([
                    api.searchSettings(),
                    api.canvasSettings(),
                    api.listVaults(),
                    api.spotifySettings(),
                    api.steamSettings(),
                    api.listMcpServers(),
                    api.listFacts(),
                    api.listTools(),
                    api.voiceSettings(),
                ]);
            spotifyIdDraft = spotify?.clientId ?? "";
        } catch (e) {
            toast.failure("Some settings could not be loaded.", e);
        }
    }

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
                    "Every approval prompt is turned off. Files can be deleted, "
                    + "commands run and settings changed without asking you first, "
                    + "including when the model is acting on a web page it just "
                    + "read. Nothing is undone by turning this back off.",
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
            voice = await api.voiceSettings();
            toast.success("Saved.");
        });
    }

    async function saveVoiceKey(key: string) {
        await run(async () => {
            await api.setVoiceKey(key);
            voice = await api.voiceSettings();
        }, "ElevenLabs key saved, encrypted.");
    }

    async function saveKey(provider: string) {
        // Nothing typed is not a failure — it is the ordinary case of opening
        // the box and thinking better of it. Close, say nothing.
        if (!keyDraft.trim()) {
            keyOpen = null;
            return;
        }
        await run(async () => {
            await api.setKey(provider, keyDraft.trim());
            keyDraft = "";
            keyOpen = null;
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
        keyDraft = "";
        keyOpen = keyOpen === provider ? null : provider;
    }

    async function pickProvider(id: string) {
        await run(async () => {
            await api.setProvider(id);
            models = [];
            await chat.refresh();
        });
    }

    async function fetchModels() {
        loadingModels = true;
        await run(async () => {
            models = await api.listModels();
        });
        loadingModels = false;
    }

    async function pickModel(model: string) {
        await run(async () => {
            await api.setModel(model);
            models = [];
            await chat.refresh();
        });
    }


    async function pickVault(path: string) {
        await run(async () => {
            await api.setVault(path);
            vaults = await api.listVaults();
            void test("obsidian");
        });
    }

    async function saveSteam() {
        await run(async () => {
            // The backend verifies and reports what it found, including the
            // private-profile case that otherwise looks like an empty library.
            toast.success(await api.setSteam(steamIdDraft.trim(), steamKeyDraft.trim()));
            steamKeyDraft = "";
            steam = await api.steamSettings();
        });
    }

    async function connectSpotify() {
        await run(async () => {
            await api.setSpotifyClientId(spotifyIdDraft.trim());
            await api.connectSpotify();
        }, "Browser opened — approve the request there.");
    }

    async function disconnectSpotify() {
        await run(async () => {
            await api.disconnectSpotify();
            spotify = await api.spotifySettings();
        }, "Spotify disconnected.");
    }

    async function mcpAction(fn: () => Promise<unknown>) {
        await run(async () => {
            await fn();
            mcp = await api.listMcpServers();
            await chat.refresh();
        });
    }

    async function saveMcp() {
        await run(async () => {
            toast.success(await api.saveMcpServer(draft));
            draft = {
                id: "",
                transport: "stdio",
                command: "",
                args: "",
                url: "",
                headerName: "",
                headerValue: "",
                secret: "",
            };
            mcp = await api.listMcpServers();
            await chat.refresh();
        });
    }


    function close() {
        chat.panel = "none";
    }
</script>

<!-- Its own header rather than the modal's: the title belongs at the top of
     the category rail, above the search field that filters it, not on a bar
     spanning both columns. -->
<Modal label="Settings" size="full" bare showClose={false} onClose={close}>
    <div class="settings">
        <nav class="categories">
            <div class="nav-head">
                <span class="nav-title">Settings</span>
                <button class="nav-close" onclick={close} aria-label="Close (Esc)">
                    <Icon name="close" size={15} />
                </button>
            </div>

            <input
                class="search"
                data-autofocus
                bind:value={query}
                placeholder="Search settings…"
                spellcheck="false"
                aria-label="Search settings"
            />

            {#each groups as group (group.title)}
                <!-- The heading is a signpost while browsing and noise while
                     searching: a search already narrows the list, and four
                     headings over one match each is worse than none. -->
                {#if !query.trim()}
                    <span class="group-title">{group.title}</span>
                {/if}
                {#each group.categories as category (category.id)}
                    <button
                        class="category"
                        class:active={active === category.id}
                        onclick={() => (active = category.id)}
                    >
                        <span class="cat-icon">{category.icon}</span>
                        {category.label}
                    </button>
                {/each}
            {/each}

            {#if matches.length === 0}
                <div class="nav-empty">
                    <p>Nothing matches “{query}”.</p>
                    <button onclick={() => (query = "")}>Clear</button>
                </div>
            {/if}
        </nav>

        <section class="pane">

            {#if active === "general"}
                <GeneralPane
                    {status}
                    languages={LANGUAGES}
                    windowModes={WINDOW_MODES}
                    onchange={updateSetting}
                />
            {:else if active === "provider"}
                <ProviderPane
                    {status}
                    {tests}
                    {testing}
                    {models}
                    {loadingModels}
                    {keyOpen}
                    bind:keyDraft
                    onpickprovider={pickProvider}
                    onpickmodel={pickModel}
                    onopenkey={openKey}
                    onsavekey={saveKey}
                    oncancelkey={() => {
                        keyDraft = "";
                        keyOpen = null;
                    }}
                    onfetchmodels={fetchModels}
                    ontest={test}
                    onchange={updateSetting}
                />
            {:else if active === "voice"}
                <VoicePane
                    {status}
                    {voice}
                    onupdate={updateVoice}
                    onsavekey={saveVoiceKey}
                />
            {:else if active === "memory"}
                <MemoryPane
                    {facts}
                    onforget={(id) =>
                        run(async () => {
                            await api.forgetFact(id);
                            facts = await api.listFacts();
                            await chat.refresh();
                        })}
                />
            {:else if active === "search"}
                <SearchPane {search} reload={load} />
            {:else if active === "canvas"}
                <CanvasPane {canvas} reload={load} />
            {:else if active === "obsidian"}
                <ObsidianPane
                    {vaults}
                    result={tests.obsidian}
                    testing={testing === "obsidian"}
                    onpick={pickVault}
                    ontest={() => test("obsidian")}
                />
            {:else if active === "spotify"}
                <SpotifyPane
                    {spotify}
                    clientId={spotifyIdDraft}
                    result={tests.spotify}
                    testing={testing === "spotify"}
                    onconnect={connectSpotify}
                    ondisconnect={disconnectSpotify}
                    ontest={() => test("spotify")}
                    onsaveid={(id) =>
                        run(async () => {
                            spotifyIdDraft = id;
                            await api.setSpotifyClientId(id);
                            spotify = await api.spotifySettings();
                        }, "Client id saved.")}
                />
            {:else if active === "steam"}
                <SteamPane
                    {steam}
                    steamId={steamIdDraft}
                    result={tests.steam}
                    testing={testing === "steam"}
                    onsave={(id, key) => {
                        steamIdDraft = id;
                        steamKeyDraft = key;
                        void saveSteam();
                    }}
                    ontest={() => test("steam")}
                />
            {:else if active === "mcp"}
                <McpPane
                    servers={mcp}
                    bind:draft
                    ontoggleserver={(id, enabled) =>
                        mcpAction(() => api.toggleMcpServer(id, enabled))}
                    onremove={(id) => mcpAction(() => api.removeMcpServer(id))}
                    ontoggletool={(id, tool, enabled) =>
                        mcpAction(() => api.toggleMcpTool(id, tool, enabled))}
                    onsave={saveMcp}
                />
            {:else if active === "tools"}
                <ToolsPane
                    {tools}
                    fullAuthority={status?.fullAuthority ?? false}
                    ontoggle={toggleFullAuthority}
                />
            {:else if active === "shortcuts"}
                <ShortcutsPane shortcuts={SHORTCUTS} />
            {:else if active === "data"}
                <DataPane
                    {status}
                    {canvas}
                    onclear={() => chat.clearWithConfirm()}
                />
            {:else if active === "updates"}
                <UpdatesPane
                    {status}
                    {update}
                    checking={checkingUpdate}
                    oncheck={checkUpdate}
                />
            {/if}
        </section>
    </div>
</Modal>

<style>
    .settings {
        display: flex;
        height: 100%;
        min-height: 0;
    }

    /* The rail is sunken rather than tinted with a literal colour: the old
       `rgba(10, 17, 26, 0.7)` was a dark-theme value that turned the rail into
       a grey slab on the light theme. */
    .categories {
        width: 208px;
        flex: 0 0 auto;
        display: flex;
        flex-direction: column;
        gap: 1px;
        padding: var(--sp-3);
        border-right: 1px solid var(--line);
        background: var(--surface-sunken);
        overflow-y: auto;
    }

    .nav-head {
        display: flex;
        align-items: center;
        justify-content: space-between;
        margin-bottom: var(--sp-3);
        padding-left: var(--sp-2);
    }

    .nav-title {
        font-size: var(--text-md);
        font-weight: 600;
        color: var(--text);
    }

    .nav-close {
        padding: var(--sp-2);
        color: var(--text-faint);
    }

    .search {
        font-size: var(--text-sm);
        background: var(--surface);
        border: 1px solid var(--line);
        border-radius: var(--r-md);
        padding: var(--sp-2) var(--sp-3);
        margin-bottom: var(--sp-3);
        transition: border-color var(--fast) var(--ease);
    }
    .search:focus {
        border-color: var(--accent-line);
    }

    .nav-empty {
        display: flex;
        flex-direction: column;
        align-items: flex-start;
        gap: var(--sp-1);
        padding: var(--sp-3) var(--sp-2);
    }
    .nav-empty p {
        font-size: var(--text-sm);
        color: var(--text-faint);
        line-height: 1.5;
    }
    .nav-empty button {
        padding: 0;
        color: var(--accent-text);
    }

    /* Four signposts over fourteen entries: enough to turn "where would that
       be?" into one guess, quiet enough not to compete with the entries. */
    .group-title {
        padding: var(--sp-4) var(--sp-3) var(--sp-1);
        font-size: var(--text-xs);
        font-weight: 600;
        letter-spacing: 0.08em;
        text-transform: uppercase;
        color: var(--text-faint);
    }
    .group-title:first-of-type {
        padding-top: var(--sp-2);
    }

    /* Selection is a filled row, not an outlined one. A border on the active
       item and nothing on the rest made the rail read as a form. */
    .category {
        display: flex;
        align-items: center;
        gap: var(--sp-3);
        width: 100%;
        text-align: left;
        padding: var(--sp-2) var(--sp-3);
        font-size: var(--text-base);
        color: var(--text-muted);
        border-radius: var(--r-md);
        transition:
            color var(--fast) var(--ease),
            background var(--fast) var(--ease);
    }
    .category:hover {
        color: var(--text);
        background: var(--surface-hover);
    }
    .category.active {
        color: var(--text);
        background: var(--surface-active);
        font-weight: 500;
    }
    .category.active .cat-icon {
        color: var(--accent);
    }

    .cat-icon {
        width: 14px;
        text-align: center;
        color: var(--text-faint);
        flex: 0 0 auto;
    }

    .pane {
        flex: 1;
        min-width: 0;
        overflow-y: auto;
        padding: var(--sp-5) var(--sp-6) var(--sp-7);
        display: flex;
        flex-direction: column;
        align-items: stretch;
        gap: var(--sp-3);
    }

    /* A reading column, not the width of the window.
       Every row here is a label on the left and its control on the right; at
       full width on a wide monitor that put the two nine hundred pixels apart,
       and a form you have to track across the screen to read is not a form. */
    /* `max-width` only. `width: 100%` used to be here too, and it applied to
       every direct child including flex rows: a <select> beside an input ate
       the whole row and left the input a few pixels wide, which is why the
       search key box looked like it did not exist, and why the "full
       authority" checkbox pushed its own label off to the side. Block
       children already fill the column; the ones that lay out their own
       contents now get to. */
    .pane > :global(*) {
        max-width: 760px;
    }
</style>
