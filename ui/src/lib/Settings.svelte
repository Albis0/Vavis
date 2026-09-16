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
    let voiceKeyDraft = $state("");
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
    /**
     * Whether the client id box is showing.
     *
     * Folded away by default because the built-in application means nobody
     * has to touch it. It was the first thing on the screen, which made a one
     * click integration read as a form to fill in.
     */
    let spotifyOwnApp = $state(false);

    let mcpOpen = $state(false);
    let mcpExpanded = $state<string | null>(null);
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

    async function saveVoiceKey() {
        if (!voiceKeyDraft.trim()) return;
        await run(async () => {
            await api.setVoiceKey(voiceKeyDraft.trim());
            voiceKeyDraft = "";
            voice = await api.voiceSettings();
        }, "ElevenLabs key saved, encrypted.");
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
        const match = voice.edgeVoices.find(
            ([id]) => id === voice!.defaultEdgeVoice,
        );
        return match?.[1] ?? "follows your language";
    });

    /** Whether the chosen engine is missing the key it needs. */
    const voiceKeyMissing = $derived(
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
    const voiceSwapped = $derived(
        !!voice && voice.matchProvider && voice.effectiveEngine !== voice.engine,
    );

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

    function bytes(n: number): string {
        if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
        if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
        return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
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
                <h2>General</h2>

                <label class="field">
                    <span>Assistant name</span>
                    <input
                        value={status?.assistantName ?? ""}
                        onchange={(e) => updateSetting("name", e.currentTarget.value)}
                    />
                </label>
                <p class="hint">Spoken aloud by the voice, so pick something sayable.</p>

                <label class="field">
                    <span>Language</span>
                    <!-- `selected` on the option rather than `value` on the
                         select: the select renders before its options exist,
                         so a value naming an option that is not there yet is
                         dropped and the box shows the first entry instead. -->
                    <select
                        onchange={(e) => updateSetting("language", e.currentTarget.value)}
                    >
                        {#each LANGUAGES as [code, name] (code)}
                            <option value={code} selected={code === (status?.language ?? "en")}>
                                {name}
                            </option>
                        {/each}
                    </select>
                </label>

                <label class="field">
                    <span>Window</span>
                    <select
                        onchange={(e) => updateSetting("windowMode", e.currentTarget.value)}
                    >
                        {#each WINDOW_MODES as mode (mode)}
                            <option
                                value={mode}
                                selected={mode === (status?.windowMode ?? "windowed")}
                            >
                                {mode}
                            </option>
                        {/each}
                    </select>
                </label>

                <label class="field">
                    <span>Font size</span>
                    <input
                        type="number"
                        min="8"
                        max="32"
                        value={status?.fontSize ?? 14}
                        onchange={(e) => updateSetting("fontSize", e.currentTarget.value)}
                    />
                </label>

                <p class="hint">Version {status?.version ?? "—"}</p>
            {:else if active === "provider"}
                <h2>Model &amp; keys</h2>
                <p class="hint">
                    One card per provider: whether it holds a key, whether it answers,
                    and which one is doing the answering. Keys are encrypted with
                    Windows DPAPI, never written to the settings file, and never shown
                    again once saved.
                </p>

                <div class="cards">
                    {#each status?.providers ?? [] as p (p.id)}
                        {@const selected = p.id === status?.provider}
                        {@const blocked = p.needsKey && !p.hasKey}
                        <!-- A div, not a button: the card holds a key field and its own
                             buttons, and nesting those inside a button is invalid and
                             swallows their clicks. Selection is the separate control at
                             the end of the header row. -->
                        <div class="card" class:selected class:blocked>
                            <div class="card-head">
                                <span class="card-name">{p.id}</span>
                                <!-- Beside the name, not under it. On its own line it
                                     was a row of its own for one short string, which
                                     made every card taller than it had anything to
                                     say. -->
                                <span class="card-model">
                                    {selected ? (status?.model ?? p.defaultModel) : p.defaultModel}
                                </span>

                                <span class="card-spacer"></span>

                                {#if !p.needsKey}
                                    <span class="tag key-tag" data-tone="neutral">no key needed</span>
                                {:else if p.hasKey}
                                    <span class="tag key-tag" data-tone="good">key stored</span>
                                {:else}
                                    <span class="tag key-tag" data-tone="warn">no key</span>
                                {/if}

                                {#if selected}
                                    <span class="tag pick" data-tone="accent">answering</span>
                                {:else}
                                    <button
                                        class="tiny pick"
                                        onclick={() => pickProvider(p.id)}
                                        title={blocked
                                            ? "can be selected, but will not answer until a key is stored"
                                            : `default model: ${p.defaultModel}`}
                                    >
                                        use this
                                    </button>
                                {/if}
                            </div>

                            {#if tests[p.id]}
                                <p class="result" class:bad={!tests[p.id].ok}>
                                    {tests[p.id].ok ? "✓" : "✕"}
                                    {tests[p.id].detail}
                                </p>
                            {/if}

                            {#if keyOpen === p.id}
                                <!-- svelte-ignore a11y_autofocus -->
                                <input
                                    class="key-input"
                                    type="password"
                                    autofocus
                                    bind:value={keyDraft}
                                    placeholder={p.hasKey
                                        ? "paste a new key to replace the stored one…"
                                        : "paste key…"}
                                    onkeydown={(e) => {
                                        if (e.key === "Enter") saveKey(p.id);
                                        if (e.key === "Escape") {
                                            keyDraft = "";
                                            keyOpen = null;
                                        }
                                    }}
                                    onblur={() => saveKey(p.id)}
                                />
                                <p class="hint small">
                                    Saved on Enter or when you leave the field. Esc discards it.
                                </p>
                            {/if}

                            <div class="card-actions">
                                {#if p.needsKey}
                                    <button class="tiny" onclick={() => openKey(p.id)}>
                                        {keyOpen === p.id
                                            ? "cancel"
                                            : p.hasKey
                                              ? "replace key"
                                              : "add key"}
                                    </button>
                                {/if}
                                <button
                                    class="tiny"
                                    disabled={blocked || testing !== null}
                                    onclick={() => test(p.id)}
                                    title={blocked ? "store a key first" : "make a real request"}
                                >
                                    {testing === p.id ? "testing…" : "test"}
                                </button>
                                {#if selected}
                                    <button class="tiny" onclick={fetchModels} disabled={loadingModels}>
                                        {loadingModels ? "loading…" : "change model"}
                                    </button>
                                {/if}
                            </div>

                            <!-- Under the card whose provider they belong to, so there is
                                 never a list of models with no visible owner. -->
                            {#if selected && models.length}
                                <div class="list scroll">
                                    {#each models as m (m)}
                                        <button
                                            class="row"
                                            class:active={m === status?.model}
                                            onclick={() => pickModel(m)}
                                        >
                                            {m}
                                        </button>
                                    {/each}
                                </div>
                            {/if}
                        </div>
                    {/each}
                </div>

                <h3>Tool routing</h3>
                <p class="hint">
                    A small, cheap model reads your request and decides which tools the
                    main model needs, so only those are sent. Leave this empty to match
                    tools by keyword instead — no extra call, no extra cost.
                </p>

                <label class="field">
                    <span>Router model</span>
                    <input
                        type="text"
                        placeholder="off — e.g. llama-3.1-8b-instant"
                        value={status?.routerModel ?? ""}
                        onchange={(e) =>
                            updateSetting("routerModel", e.currentTarget.value)}
                    />
                </label>
                <p class="hint">
                    Runs on the provider and key you already use. If it is slow or
                    fails, keyword matching takes over — the assistant keeps working.
                </p>
            {:else if active === "voice"}
                <h2>Voice</h2>
                <p class="hint">
                    Off, wake word, or always listening. The rail on the left switches
                    between them, and so does Ctrl+M.
                </p>

                <div class="field">
                    <span>Mode</span>
                    <span class="value">{status?.voiceMode ?? "off"}</span>
                </div>

                <div class="actions">
                    <button onclick={() => chat.cycleVoice()}>cycle mode</button>
                    {#if status?.speaking}
                        <button onclick={() => chat.stopSpeaking()}>stop speaking</button>
                    {/if}
                </div>

                <p class="hint">
                    Speech recognition runs through Groq, so it needs the Groq key even
                    when another provider is answering.
                </p>

                <h2>Speaking</h2>
                <p class="hint">
                    Which voice reads the replies. If the one you pick cannot be
                    reached, Vavis says so out loud and falls back to one that works —
                    it will not go silent on you.
                </p>

                {#if voice}
                    <label class="field">
                        <span>Engine</span>
                        <select
                            onchange={(e) =>
                                updateVoice("voiceEngine", e.currentTarget.value)}
                        >
                            <!-- `selected` on the option, not `value` on the
                                 select: the select is rendered before its
                                 options exist, so a value naming an option
                                 that is not there yet is discarded and the
                                 box silently snaps back to the first entry.
                                 That is why changing the voice looked like it
                                 did nothing. -->
                            {#each voice.engines as e (e.id)}
                                <option value={e.id} selected={e.id === voice.engine}>
                                    {e.label}
                                </option>
                            {/each}
                        </select>
                    </label>

                    {#if voiceKeyMissing}
                        <p class="hint warn-text">
                            This engine needs a key before it can speak. Until then
                            Vavis falls back to a free voice.
                        </p>
                    {/if}

                    <label class="switch">
                        <input
                            type="checkbox"
                            checked={voice.matchProvider}
                            onchange={(e) =>
                                updateVoice(
                                    "matchProvider",
                                    String(e.currentTarget.checked),
                                )}
                        />
                        <span>Use the chat provider's own voice when it has one</span>
                    </label>
                    <p class="hint">
                        Talking to Gemini sounds like Gemini. Only swaps between engines
                        that already need a key — a free offline voice is left alone, so
                        this cannot quietly move you onto a metered one.
                    </p>
                    {#if voiceSwapped}
                        <p class="hint">
                            Speaking with <strong>{voice.effectiveEngine}</strong> right
                            now, because that is who you are chatting with.
                        </p>
                    {/if}

                    <label class="field">
                        <span>Voice</span>
                        <select
                            onchange={(e) =>
                                updateVoice(voiceField, e.currentTarget.value)}
                        >
                            <option value="" selected={!selectedVoice}>
                                default ({defaultVoiceLabel})
                            </option>
                            {#each voiceOptions as [id, label] (id)}
                                <option value={id} selected={id === selectedVoice}>
                                    {label}
                                </option>
                            {/each}
                        </select>
                    </label>

                    <label class="field">
                        <span>Speed</span>
                        <input
                            type="number"
                            min="-10"
                            max="10"
                            value={voice.rate}
                            onchange={(e) =>
                                updateVoice("voiceRate", e.currentTarget.value)}
                        />
                    </label>

                    <label class="field">
                        <span>Volume</span>
                        <input
                            type="number"
                            min="0"
                            max="100"
                            value={voice.volume}
                            onchange={(e) =>
                                updateVoice("voiceVolume", e.currentTarget.value)}
                        />
                    </label>

                    <div class="actions">
                        <button onclick={() => api.previewVoice()}>
                            hear this voice
                        </button>
                    </div>

                    {#if voice.engine === "kokoro"}
                        <!-- Kokoro is a model the user runs themselves, so the
                             one thing they need from us is the command. -->
                        <p class="hint">
                            Kokoro runs on your own machine, so nothing is sent
                            anywhere and it costs nothing. Vavis does not install or
                            start it — run the server yourself and point this at it:
                        </p>
                        <pre class="snippet selectable">docker run -p 8880:8880 ghcr.io/remsky/kokoro-fastapi-cpu</pre>
                        <label class="field">
                            <span>Server</span>
                            <input
                                type="text"
                                placeholder={voice.kokoroDefaultUrl}
                                value={voice.kokoroUrl}
                                onchange={(e) =>
                                    updateVoice("kokoroUrl", e.currentTarget.value)}
                            />
                        </label>
                        <p class="hint">
                            Leave the address empty to use the default above.
                        </p>
                    {/if}

                    {#if voice.engine === "elevenlabs"}
                        <label class="field">
                            <span>
                                ElevenLabs key
                                <span
                                    class="risk"
                                    data-risk={voice.hasElevenKey
                                        ? "safe"
                                        : "destructive"}
                                >
                                    {voice.hasElevenKey ? "stored" : "no key"}
                                </span>
                            </span>
                            <input
                                type="password"
                                placeholder="paste and press Enter"
                                bind:value={voiceKeyDraft}
                                onkeydown={(e) => e.key === "Enter" && saveVoiceKey()}
                                onblur={saveVoiceKey}
                            />
                        </label>
                    {/if}

                    {#if voice.engine === "openai"}
                        <p class="hint">
                            Uses the OpenAI key from the API keys section — the same
                            one chat uses, so there is nothing extra to paste.
                            {voice.hasOpenaiKey ? "" : " No key stored yet."}
                        </p>
                    {/if}
                {/if}
            {:else if active === "memory"}
                <h2>Memory</h2>
                <p class="hint">
                    Facts the assistant keeps between conversations. Clearing the
                    conversation does not touch these.
                </p>

                {#if facts.length === 0}
                    <p class="hint">Nothing remembered yet. Try "remember that I…".</p>
                {:else}
                    <div class="list scroll">
                        {#each facts as fact (fact.id)}
                            <div class="entry">
                                <div class="entry-main">
                                    <span class="tool-desc">{fact.text}</span>
                                </div>
                                <div class="entry-actions">
                                    <button
                                        class="danger tiny"
                                        onclick={() =>
                                            run(async () => {
                                                await api.forgetFact(fact.id);
                                                facts = await api.listFacts();
                                                await chat.refresh();
                                            })}
                                    >
                                        forget
                                    </button>
                                </div>
                            </div>
                        {/each}
                    </div>
                {/if}
            {:else if active === "search"}
                <SearchPane {search} reload={load} />
            {:else if active === "canvas"}
                <CanvasPane {canvas} reload={load} />
            {:else if active === "obsidian"}
                <h2>Obsidian</h2>
                {#if vaults.length === 0}
                    <p class="hint">
                        No vault found. Obsidian does not have to be running — Vavis reads
                        the Markdown files directly — but it needs to know where the vault is.
                    </p>
                {:else}
                    <p class="hint">
                        Notes are read and written on disk, so this works whether or not
                        Obsidian is open.
                    </p>
                    <div class="list">
                        {#each vaults as v (v.path)}
                            <button
                                class="row"
                                class:active={v.active}
                                title={v.path}
                                onclick={() => pickVault(v.path)}
                            >
                                {v.active ? "● " : "○ "}{v.name}
                            </button>
                        {/each}
                    </div>

                    <div class="actions">
                        <button onclick={() => test("obsidian")} disabled={testing !== null}>
                            {testing === "obsidian" ? "reading…" : "test"}
                        </button>
                    </div>
                    {#if tests.obsidian}
                        <p class="result" class:bad={!tests.obsidian.ok}>
                            {tests.obsidian.ok ? "✓" : "✕"} {tests.obsidian.detail}
                        </p>
                    {/if}
                {/if}
            {:else if active === "spotify"}
                <h2>Spotify</h2>
                {#if spotify?.connected}
                    <p class="result">✓ Connected.</p>
                    <div class="actions">
                        <button onclick={() => test("spotify")} disabled={testing !== null}>
                            test
                        </button>
                        <button class="danger" onclick={disconnectSpotify}>disconnect</button>
                    </div>
                    {#if tests.spotify}
                        <p class="result" class:bad={!tests.spotify.ok}>
                            {tests.spotify.ok ? "✓" : "✕"} {tests.spotify.detail}
                        </p>
                    {/if}
                {:else}
                    <p class="hint">
                        Opens Spotify in your browser. Approve there and you are done —
                        there is nothing to set up first.
                    </p>
                    <div class="actions">
                        <button class="primary" onclick={connectSpotify}>connect</button>
                    </div>
                    <button class="disclosure" onclick={() => (spotifyOwnApp = !spotifyOwnApp)}>
                        {spotifyOwnApp ? "▾" : "▸"} use my own Spotify app
                    </button>
                    {#if spotifyOwnApp}
                        <p class="hint">
                            Only worth doing if you want your own name on the consent screen.
                            Register this exact redirect URI on the app, then paste its client
                            id here. Leaving it empty goes back to the built-in one.
                        </p>
                        <p class="path selectable">{spotify?.redirectUri ?? ""}</p>
                        <input bind:value={spotifyIdDraft} placeholder="client id (optional)" />
                        <div class="actions">
                            <button
                                onclick={() =>
                                    run(async () => {
                                        await api.setSpotifyClientId(spotifyIdDraft.trim());
                                        spotify = await api.spotifySettings();
                                    }, "Client id saved.")}
                            >
                                save id
                            </button>
                        </div>
                    {/if}
                {/if}
            {:else if active === "steam"}
                <h2>Steam</h2>
                <p class="hint">
                    Needs a Web API key and your SteamID64. Game details must be public, or
                    Steam returns an empty library without saying why.
                </p>
                <input bind:value={steamIdDraft} placeholder="SteamID64 (17 digits)" />
                <input
                    type="password"
                    bind:value={steamKeyDraft}
                    placeholder={steam?.hasKey ? "key stored — paste to replace" : "Web API key…"}
                    onkeydown={(e) => e.key === "Enter" && saveSteam()}
                />
                <div class="actions">
                    <button class="primary" onclick={saveSteam}>save and check</button>
                    <button
                        onclick={() => test("steam")}
                        disabled={testing !== null || !steam?.hasKey}
                    >
                        {testing === "steam" ? "asking…" : "test"}
                    </button>
                </div>
                {#if tests.steam}
                    <p class="result" class:bad={!tests.steam.ok}>
                        {tests.steam.ok ? "✓" : "✕"} {tests.steam.detail}
                    </p>
                {/if}
                <p class="hint">
                    Which game is running is detected locally, so that part works even on a
                    private profile.
                </p>
            {:else if active === "mcp"}
                <h2>MCP servers</h2>
                <p class="hint">
                    Connect any MCP server and its tools become available. A server runs as
                    a process on this machine, so its tools always ask before running.
                </p>

                {#each mcp as server (server.id)}
                    <div class="entry" class:off={!server.enabled}>
                        <div class="entry-main">
                            <span class="tool-name">
                                {server.id}
                                <span class="risk" data-risk={server.connected ? "safe" : "destructive"}>
                                    {server.connected ? "connected" : "offline"}
                                </span>
                            </span>
                            <!-- Exactly what runs, so it can be judged before it does. -->
                            <span class="tool-desc selectable">{server.commandLine}</span>
                            {#if server.connected}
                                <button
                                    class="disclosure"
                                    onclick={() =>
                                        (mcpExpanded = mcpExpanded === server.id ? null : server.id)}
                                >
                                    {mcpExpanded === server.id ? "▾" : "▸"}
                                    {server.tools.length + server.disabled.length} tools
                                </button>
                            {/if}
                        </div>
                        <div class="entry-actions">
                            <button
                                class="tiny"
                                onclick={() =>
                                    mcpAction(() => api.toggleMcpServer(server.id, !server.enabled))}
                            >
                                {server.enabled ? "disable" : "enable"}
                            </button>
                            <button
                                class="danger tiny"
                                onclick={() => mcpAction(() => api.removeMcpServer(server.id))}
                            >
                                remove
                            </button>
                        </div>
                    </div>

                    {#if mcpExpanded === server.id}
                        <div class="list">
                            {#each [...server.tools, ...server.disabled].sort() as tool (tool)}
                                {@const on = !server.disabled.includes(tool)}
                                <button
                                    class="row"
                                    class:active={on}
                                    onclick={() => mcpAction(() => api.toggleMcpTool(server.id, tool, !on))}
                                >
                                    {on ? "● " : "○ "}{tool}
                                </button>
                            {/each}
                        </div>
                    {/if}
                {/each}

                <button class="disclosure" onclick={() => (mcpOpen = !mcpOpen)}>
                    {mcpOpen ? "▾" : "▸"} add a server
                </button>
                {#if mcpOpen}
                    <input bind:value={draft.id} placeholder="id, e.g. github" />
                    <label class="field">
                        <span>Transport</span>
                        <select bind:value={draft.transport}>
                            <option value="stdio">stdio</option>
                            <option value="http">http</option>
                        </select>
                    </label>
                    {#if draft.transport === "stdio"}
                        <input bind:value={draft.command} placeholder="command, e.g. npx" />
                        <input bind:value={draft.args} placeholder="arguments, e.g. -y @modelcontextprotocol/server-github" />
                    {:else}
                        <input bind:value={draft.url} placeholder="https://…/mcp" />
                        <input bind:value={draft.headerName} placeholder="auth header (optional)" />
                        <input bind:value={draft.headerValue} placeholder="header value, e.g. Bearer {'{key}'}" />
                    {/if}
                    <input type="password" bind:value={draft.secret} placeholder="secret (optional)" />
                    <button class="primary" onclick={saveMcp}>add and connect</button>
                {/if}
            {:else if active === "tools"}
                <ToolsPane
                    {tools}
                    fullAuthority={status?.fullAuthority ?? false}
                    ontoggle={toggleFullAuthority}
                />
            {:else if active === "shortcuts"}
                <h2>Shortcuts</h2>
                <div class="list">
                    {#each SHORTCUTS as [key, action] (key)}
                        <div class="shortcut-row">
                            <kbd>{key}</kbd>
                            <span>{action}</span>
                        </div>
                    {/each}
                </div>
            {:else if active === "data"}
                <h2>Data</h2>
                <p class="hint">Everything Vavis stores lives here:</p>
                <p class="path selectable">{status?.dataDir ?? ""}</p>

                <div class="field">
                    <span>Conversation</span>
                    <span class="value">{status?.messageCount ?? 0} messages</span>
                </div>
                <div class="field">
                    <span>Remembered</span>
                    <span class="value">{status?.factCount ?? 0} facts</span>
                </div>
                {#if canvas}
                    <div class="field">
                        <span>Generated</span>
                        <span class="value">{canvas.items} files · {bytes(canvas.bytes)}</span>
                    </div>
                {/if}

                <div class="actions">
                    <button class="danger" onclick={() => chat.clearWithConfirm()}>
                        Clear the conversation
                    </button>
                </div>
                <p class="hint">Remembered facts survive that — forget them in Memory.</p>

            {:else if active === "updates"}
                <h2>Updates</h2>

                <div class="field">
                    <span>This build</span>
                    <span class="value">{status?.version ?? ""}</span>
                </div>

                <p class="hint">
                    Vavis does not install updates by itself, and does not update in
                    the background. It checks the project's release page and tells
                    you what it found; downloading is yours to start. Nothing about
                    you is sent with the check.
                </p>

                <div class="actions">
                    <button onclick={checkUpdate} disabled={checkingUpdate}>
                        {checkingUpdate ? "checking…" : "check for updates"}
                    </button>
                </div>

                {#if update?.status === "available"}
                    <div class="update-box">
                        <p class="update-head">
                            Version {update.latest} is out — you have {update.current}.
                        </p>
                        {#if update.notes}
                            <pre class="snippet selectable">{update.notes}</pre>
                        {/if}
                        <div class="actions">
                            <button class="primary" onclick={() => api.openReleasePage()}>
                                open the download page
                            </button>
                        </div>
                        <p class="hint">
                            The page has the installer and a checksum. Close Vavis
                            before running it.
                        </p>
                    </div>
                {:else if update?.status === "upToDate"}
                    <p class="hint">You are on the newest release ({update.current}).</p>
                {:else if update?.status === "failed"}
                    <!-- Deliberately not phrased as "up to date": a check that
                         could not run has not established anything. -->
                    <p class="hint warn-text">
                        Could not check: {update.error}. Your build is {update.current}.
                    </p>
                    <div class="actions">
                        <button onclick={() => api.openReleasePage()}>
                            open the release page anyway
                        </button>
                    </div>
                {/if}
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

    h2 {
        margin: 0 0 var(--sp-2);
        font-size: var(--text-lg);
        font-weight: 600;
        color: var(--text);
    }

    /* The one raised block in this pane: a waiting update is the only thing
       here that asks the reader to do something. */
    .update-box {
        margin-top: 0.75rem;
        padding: 0.85rem 1rem;
        border: 1px solid var(--line, #333);
        border-radius: 8px;
        background: var(--raised, rgba(255, 255, 255, 0.03));
    }

    .update-head {
        margin: 0 0 0.5rem;
        font-weight: 600;
    }

    /* Plain case and muted, matching `.section-label` in the design system.
       These were mono all-caps in the accent colour, which made every
       sub-heading louder than the setting under it. */
    h3 {
        margin: var(--sp-4) 0 0;
        font-size: var(--text-xs);
        font-weight: 600;
        letter-spacing: 0.02em;
        color: var(--text-faint);
    }

    .field {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--sp-3);
        font-size: var(--text-base);
        color: var(--text-muted);
        min-height: 34px;
    }
    .field > span:first-child {
        flex: 0 0 auto;
    }
    .field input,
    .field select {
        flex: 0 1 260px;
    }
    .field .value {
        font-family: var(--font-mono);
        font-size: var(--text-xs);
        color: var(--text);
    }

    /* A command the user is meant to copy and run. Monospace because the
       spacing is load-bearing, selectable because copying is the point. */
    .snippet {
        font-family: var(--font-mono);
        font-size: var(--text-xs);
        color: var(--text);
        background: var(--surface-sunken);
        border: 1px solid var(--line);
        border-radius: var(--r-sm);
        padding: var(--sp-2) var(--sp-3);
        margin: var(--sp-2) 0;
        /* A long command scrolls inside its box rather than widening the
           panel and pushing everything else off the edge. */
        overflow-x: auto;
        white-space: pre;
    }

    .warn-text {
        color: var(--warning);
    }

    /* A checkbox and its label as one clickable row. The label leads and the
       box follows, matching `.field`, so the two read as the same kind of
       control rather than two different ideas of a setting. */


    .actions {
        display: flex;
        gap: var(--sp-1);
        flex-wrap: wrap;
    }


    .result {
        margin: 0;
        font-size: var(--text-sm);
        color: var(--success);
    }
    .result.bad {
        color: var(--warning);
    }

    .hint {
        margin: 0;
        font-size: var(--text-sm);
        color: var(--text-faint);
        line-height: 1.55;
        max-width: 72ch;
    }

    .path {
        margin: 0;
        font-family: var(--font-mono);
        font-size: var(--text-xs);
        color: var(--text-muted);
        word-break: break-all;
    }

    /* A section header that happens to be clickable, not a control competing
       with the action above it. */
    .disclosure {
        align-self: flex-start;
        padding: 0;
        border: none;
        background: none;
        font-size: var(--text-xs);
        color: var(--text-faint);
    }
    .disclosure:hover {
        color: var(--text-muted);
    }

    .list {
        display: flex;
        flex-direction: column;
        gap: 2px;
    }
    .scroll {
        max-height: 320px;
        overflow-y: auto;
    }

    .row {
        border: none;
        background: none;
        text-align: left;
        font-family: var(--font-mono);
        font-size: var(--text-xs);
        color: var(--text-muted);
        padding: var(--sp-1) var(--sp-2);
        border-radius: var(--r-sm);
    }
    .row:hover {
        background: var(--surface-hover);
        color: var(--text);
    }
    .row.active {
        color: var(--accent-hover);
    }







    .entry-actions {
        display: flex;
        gap: var(--sp-1);
        flex: 0 0 auto;
        align-items: center;
    }

    .entry {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: var(--sp-2);
        padding: var(--sp-2) 0;
        border-bottom: 1px solid var(--line);
    }
    .entry.off {
        opacity: 0.5;
    }

    .entry-main {
        display: flex;
        flex-direction: column;
        gap: 2px;
        min-width: 0;
    }

    .tool-name {
        display: flex;
        align-items: center;
        gap: var(--sp-1);
        font-family: var(--font-mono);
        font-size: var(--text-xs);
        color: var(--text);
    }

    .tool-desc {
        font-size: 10px;
        color: var(--text-faint);
        word-break: break-word;
    }

    .risk {
        font-size: 9px;
        padding: 0 5px;
        border-radius: 8px;
        border: 1px solid var(--line);
        color: var(--text-faint);
    }
    .risk[data-risk="safe"] {
        color: var(--accent);
        border-color: var(--accent-line);
    }
    .risk[data-risk="destructive"] {
        color: var(--warning);
        border-color: rgba(245, 158, 11, 0.4);
    }


    .disclosure {
        align-self: flex-start;
        border: none;
        background: none;
        padding: 2px 0;
        font-size: var(--text-xs);
        color: var(--accent);
    }

    .shortcut-row {
        display: flex;
        align-items: center;
        gap: var(--sp-2);
        font-size: var(--text-xs);
        color: var(--text-muted);
        padding: 2px 0;
    }
    .shortcut-row kbd {
        flex: 0 0 130px;
    }

    .tiny {
        font-size: var(--text-xs);
        padding: 3px 10px;
        border-radius: var(--r-sm);
    }

    /* Controls sized to be hit, not to be small. These were 11.5px text in
       3px of padding — a target under twenty pixels tall, which is below what
       a pointer lands on reliably and well below what reads as an input at a
       normal viewing distance. */
    input,
    select {
        background: var(--surface-sunken);
        border: 1px solid var(--line);
        border-radius: var(--r-md);
        padding: var(--sp-2) var(--sp-3);
        font-size: var(--text-sm);
        color: var(--text);
        font-family: inherit;
        min-width: 0;
        transition:
            border-color var(--fast) var(--ease),
            background var(--fast) var(--ease);
    }
    input:focus,
    select:focus {
        border-color: var(--accent-line);
        background: var(--surface);
    }
    input::placeholder {
        color: var(--text-faint);
    }

    /* ── Provider cards ───────────────────────────────────────────────
       One provider, everything about it: its key, its model, its proof that
       it works. The alternative — a chip row here and a key list on another
       screen — meant picking a provider marked "no key" told you what was
       wrong and then made you go somewhere else to fix it. */

    .cards {
        display: flex;
        flex-direction: column;
        gap: var(--sp-3);
    }

    .card {
        display: flex;
        flex-direction: column;
        gap: var(--sp-2);
        padding: var(--sp-3);
        background: var(--surface-raised);
        border: 1px solid var(--line);
        border-radius: var(--r-lg);
        transition:
            border-color var(--fast) var(--ease),
            background var(--fast) var(--ease);
    }
    .card:hover {
        border-color: var(--line-strong);
    }

    /* The one that answers is bordered, not filled: a filled card at this size
       reads as pressed rather than as chosen, and there are several of them. */
    .card.selected {
        border-color: var(--accent-line);
        background: var(--accent-muted);
    }

    /* Dimmed, never hidden. A provider you have no key for is still a provider
       you can choose to set up, and greying it out of existence hides the very
       card carrying the button that fixes it. */
    .card.blocked:not(.selected) .card-name,
    .card.blocked:not(.selected) .card-model {
        color: var(--text-muted);
    }

    .card-head {
        display: flex;
        align-items: center;
        gap: var(--sp-2);
    }

    /* The model id gets whatever room is left over but never pushes; the spacer
       after it is what actually holds the tags and the select button against
       the right edge, so those line up down the column no matter how long each
       provider's model id happens to be. */
    .card-model {
        flex: 0 1 auto;
    }

    .card-spacer {
        flex: 1 1 var(--sp-2);
    }

    .card-name {
        font-size: var(--text-md);
        font-weight: 600;
        color: var(--text);
    }

    .card-model {
        font-family: var(--font-mono);
        font-size: var(--text-xs);
        color: var(--text-faint);
        /* Truncates rather than wraps: a long model id pushing the tags and the
           select button onto a second line would rearrange the header row of one
           card and not the others. */
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        min-width: 0;
    }

    /* Same width whether it says "use this" or "answering", so the right edge of
       the column is a straight line rather than a ragged one that shifts as the
       selection moves from card to card. */
    .pick {
        flex: 0 0 74px;
        text-align: center;
    }

    /* Wide enough for the longest of the three ("no key needed"), so the key
       state sits in a column of its own rather than sliding left and right as
       the wording changes between providers. */
    .key-tag {
        flex: 0 0 96px;
        text-align: center;
    }

    .card-actions {
        display: flex;
        gap: var(--sp-2);
        flex-wrap: wrap;
    }

    .key-input {
        width: 100%;
        font-family: var(--font-mono);
    }

    .hint.small {
        font-size: var(--text-xs);
    }

    /* Status words, one shape for all of them. `.risk` elsewhere is a 9px pill
       that was never meant to carry a phrase like "no key needed". */
    .tag {
        font-size: var(--text-xs);
        padding: 1px 8px;
        border-radius: var(--r-full);
        border: 1px solid var(--line);
        color: var(--text-faint);
        white-space: nowrap;
        flex: 0 0 auto;
    }
    .tag[data-tone="good"] {
        color: var(--success);
        border-color: rgba(74, 222, 128, 0.35);
    }
    .tag[data-tone="warn"] {
        color: var(--warning);
        border-color: rgba(245, 158, 11, 0.4);
    }
    .tag[data-tone="accent"] {
        color: var(--accent-text);
        border-color: var(--accent-line);
        background: var(--accent-muted);
    }

    .danger {
        color: var(--warning);
        border-color: rgba(245, 158, 11, 0.4);
    }
</style>
