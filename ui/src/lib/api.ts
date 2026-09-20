/**
 * The bridge to Rust.
 *
 * Every call the interface can make lives here, typed. Components never
 * touch `invoke` directly — that keeps the contract in one place, so a
 * rename on the Rust side breaks in exactly one file.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface ProviderInfo {
    id: string;
    needsKey: boolean;
    hasKey: boolean;
    defaultModel: string;
}

export interface Status {
    version: string;
    assistantName: string;
    language: string;
    provider: string;
    model: string;
    /** Cheap model that picks tools. Empty when routing is off. */
    routerModel: string;
    /** Provider for code work. Empty means code uses the chat provider. */
    codeProvider: string;
    /** Model for code work. Meaningless while `codeProvider` is empty. */
    codeModel: string;
    /**
     * Every approval and budget is off.
     *
     * Surfaced in the status because a mode whose whole effect is the absence
     * of prompts would otherwise be invisible -- the user would have no way to
     * tell it apart from a quiet session.
     */
    fullAuthority: boolean;
    providers: ProviderInfo[];
    keys: string[];
    toolCount: number;
    historyLen: number;
    factCount: number;
    automationCount: number;
    messageCount: number;
    voiceMode: "off" | "continuous" | "wake";
    /** Microphone level, 0.0–1.0. Drives the meter next to the mic button. */
    micLevel: number;
    busy: boolean;
    speaking: boolean;
    cpu: number | null;
    battery: number | null;
    uptimeSecs: number;
    dataDir: string;
    windowMode: string;
    fontSize: number;
    /** Steam game running right now, or null when none is. */
    steamGame: string | null;
}

export interface StoredLine {
    role: string;
    content: string;
}

export interface Fact {
    id: number;
    text: string;
}

export interface Automation {
    id: number;
    prompt: string;
    trigger: string;
    enabled: boolean;
}

export interface Tool {
    name: string;
    description: string;
    domain: string;
    risk: "safe" | "moderate" | "destructive";
}

/** A user-described JSON search endpoint (self-hosted SearxNG, and the like). */
export interface CustomSearch {
    url: string;
    headerName: string;
    headerValue: string;
    resultsPath: string;
    titleKey: string;
    urlKey: string;
    snippetKey: string;
}

/** A configured MCP server and how it is currently doing. */
export interface McpServerInfo {
    id: string;
    transport: string;
    /** Exactly what gets run — shown before the user allows it. */
    commandLine: string;
    enabled: boolean;
    connected: boolean;
    tools: string[];
    disabled: string[];
    error: string | null;
    hasSecret: boolean;
}

export interface WorkspaceEntry {
    path: string;
    name: string;
    isDir: boolean;
}

export interface SearchHit {
    path: string;
    line: number;
    text: string;
}

export interface SpotifySettings {
    clientId: string;
    connected: boolean;
    /** Must be registered on the user's Spotify developer dashboard. */
    redirectUri: string;
}

/** What Spotify is playing, for the rail panel. */
export interface SpotifyNowPlaying {
    track: string;
    artist: string;
    albumArt: string | null;
    durationMs: number;
    progressMs: number;
    playing: boolean;
    device: string | null;
}

export interface SteamSettings {
    steamId: string;
    /** Whether a key is stored. The key itself never crosses this bridge. */
    hasKey: boolean;
}

/** An Obsidian vault Vavis can read and write. */
export interface VaultInfo {
    path: string;
    name: string;
    active: boolean;
}

/** One text-to-speech engine, as the settings list shows it. */
export interface VoiceEngineInfo {
    id: string;
    label: string;
    /** Needs an API key before it can speak. */
    needsKey: boolean;
}

/** What a release check came back with. */
export type UpdateCheck =
    | { status: "upToDate"; current: string }
    | {
          status: "available";
          current: string;
          latest: string;
          url: string;
          notes: string;
      }
    | { status: "failed"; current: string; error: string };

/** Everything the speech section of settings needs. */
export interface VoiceSettings {
    engines: VoiceEngineInfo[];
    engine: string;
    rate: number;
    volume: number;
    sapiVoice: string;
    edgeVoice: string;
    kokoroUrl: string;
    kokoroVoice: string;
    elevenVoice: string;
    openaiVoice: string;
    geminiVoice: string;
    /** Whether a key is stored. Keys themselves never cross this bridge. */
    hasElevenKey: boolean;
    hasOpenaiKey: boolean;
    hasGeminiKey: boolean;
    /** Speak with the chat provider's own voice when it has one. */
    matchProvider: boolean;
    /**
     * The engine actually speaking. Differs from `engine` when
     * `matchProvider` swapped it, so the screen can say why the picker and
     * what you hear disagree.
     */
    effectiveEngine: string;
    sapiVoices: string[];
    /** [id, label] pairs, so the user picks a name rather than typing an id. */
    edgeVoices: [string, string][];
    kokoroVoices: [string, string][];
    elevenVoices: [string, string][];
    openaiVoices: [string, string][];
    geminiVoices: [string, string][];
    /** Where Kokoro listens by default — shown as the placeholder. */
    kokoroDefaultUrl: string;
    /** Which Edge voice an empty choice resolves to, for the current language. */
    defaultEdgeVoice: string;
}

export interface SearchSettings {
    /** Provider ids in the order they are tried. */
    order: string[];
    /** Ids that have a key stored. Keys themselves never cross this bridge. */
    configured: string[];
    custom: CustomSearch;
}

/** What actually happened when a connection was tried. */
export interface ConnectionTest {
    ok: boolean;
    /** One line, fit to show next to the setting it tested. */
    detail: string;
}

/** One participant in a council run. */
export interface Seat {
    id: string;
    provider: string;
    model: string;
    /** Off means an independent answer; on means it reads the others first. */
    seesOthers: boolean;
    /** An angle for this seat alone — "argue the opposite". Usually empty. */
    brief: string;
}

/** What a council run would cost, before it runs. */
export interface Forecast {
    requests: number;
    tokens: number;
    dollars: number;
    /** Seats whose model has no published price, local ones included. */
    unpriced: number;
}

/** One generated file, with everything needed to make it again. */
export interface GalleryItem {
    id: number;
    kind: "image" | "video";
    /** Absolute path; turn it into a URL with `convertFileSrc`. */
    path: string;
    prompt: string;
    provider: string;
    model: string;
    /** JSON: size, aspect, count, negative, duration, strength. */
    params: string;
    /** Null when the provider reported none — this one cannot be repeated. */
    seed: number | null;
    width: number;
    height: number;
    bytes: number;
    /** The result this was made from, for variations and animations. */
    parentId: number | null;
    favourite: boolean;
    createdAt: number;
}

export interface CanvasSettings {
    imageOrder: string[];
    videoOrder: string[];
    imageModel: string;
    videoModel: string;
    size: string;
    count: number;
    /** Ids with a key stored. Keys themselves never cross this bridge. */
    configured: string[];
    canImage: boolean;
    canVideo: boolean;
    /** Only Stability and Replicate have a real upscale endpoint. */
    canUpscale: boolean;
    customUrl: string;
    customHeaderName: string;
    customHeaderValue: string;
    customModel: string;
    /** How much the gallery is costing in disk. */
    items: number;
    bytes: number;
}

/** A generation request, as the canvas builds it. */
export interface GenerateRequest {
    prompt: string;
    kind: "image" | "video";
    model: string;
    width: number;
    height: number;
    count: number;
    seed: number | null;
    negative: string;
    durationSecs: number;
    /** Continue from this result: a variation, or its first frame. */
    fromId: number | null;
    strength: number;
    /** Enlarge `fromId` instead of drawing something new. */
    upscale: boolean;
}

// ── Commands ─────────────────────────────────────────────────────────

export const api = {
    status: () => invoke<Status>("get_status"),
    loadHistory: () => invoke<StoredLine[]>("load_history"),

    /**
     * Sends a message.
     *
     * `code` marks the turn as code work so it can go to the code model when
     * the user has set one. Only the interface knows which pane the message
     * came from, so only the interface can answer this.
     */
    send: (text: string, code = false) =>
        invoke<void>("send_message", { text, code }),
    answerApproval: (decision: "allow" | "always" | "deny") =>
        invoke<void>("answer_approval", { decision }),
    clear: () => invoke<void>("clear_conversation"),
    /** Drops the oldest half of the conversation. Returns how many went. */
    forgetOldest: () => invoke<number>("forget_oldest"),

    setKey: (provider: string, key: string) =>
        invoke<void>("set_key", { provider, key }),
    setProvider: (provider: string) =>
        invoke<string>("set_provider", { provider }),
    setModel: (model: string) => invoke<void>("set_model", { model }),
    listModels: () => invoke<string[]>("list_models"),
    /** Empty string clears it, meaning code shares the chat model. */
    setCodeProvider: (provider: string) =>
        invoke<string>("set_code_provider", { provider }),
    setCodeModel: (model: string) => invoke<void>("set_code_model", { model }),
    listCodeModels: () => invoke<string[]>("list_code_models"),
    setSetting: (field: string, value: string) =>
        invoke<void>("set_setting", { field, value }),

    /**
     * Moves the window and saves the mode in one call.
     *
     * The move belongs to the backend rather than here: doing it from the
     * frontend meant two implementations of "borderless", and the one that ran
     * on F11 did not actually fill the screen.
     */
    setWindowMode: (mode: string) => invoke<void>("set_window_mode", { mode }),

    voiceSettings: () => invoke<VoiceSettings>("get_voice_settings"),
    /** ElevenLabs speaks but does not chat, so its key has its own command. */
    setVoiceKey: (key: string) => invoke<void>("set_voice_key", { key }),
    /** Speaks a sample line — the only honest way to audition a voice. */
    previewVoice: () => invoke<void>("preview_voice"),

    /**
     * Asks whether a newer release exists.
     *
     * Never throws: a failed check comes back as a `failed` result, because
     * "could not check" and "you are up to date" are different answers and
     * collapsing them would leave someone stranded on an old build believing
     * it was current.
     */
    checkUpdate: () => invoke<UpdateCheck>("check_update"),
    /** Opens the release page. The address is fixed in the binary. */
    openReleasePage: () => invoke<void>("open_release_page"),
    appVersion: () => invoke<string>("app_version"),

    cycleVoice: () => invoke<string>("cycle_voice"),
    stopSpeaking: () => invoke<void>("stop_speaking"),

    listFacts: () => invoke<Fact[]>("list_facts"),
    forgetFact: (id: number) => invoke<boolean>("forget_fact", { id }),

    listAutomations: () => invoke<Automation[]>("list_automations"),
    deleteAutomation: (id: number) =>
        invoke<boolean>("delete_automation", { id }),
    toggleAutomation: (id: number, enabled: boolean) =>
        invoke<boolean>("toggle_automation", { id, enabled }),

    listTools: () => invoke<Tool[]>("list_tools"),

    searchSettings: () => invoke<SearchSettings>("get_search_settings"),
    setSearchKey: (provider: string, key: string) =>
        invoke<void>("set_search_key", { provider, key }),
    setSearchOrder: (order: string[]) =>
        invoke<void>("set_search_order", { order }),
    openWorkspace: (path: string) => invoke<string>("open_workspace", { path }),
    currentWorkspace: () => invoke<string | null>("current_workspace"),
    listWorkspace: (path: string) =>
        invoke<WorkspaceEntry[]>("list_workspace", { path }),
    readWorkspaceFile: (path: string) =>
        invoke<string>("read_workspace_file", { path }),
    writeWorkspaceFile: (path: string, content: string) =>
        invoke<void>("write_workspace_file", { path, content }),
    searchWorkspace: (query: string) =>
        invoke<SearchHit[]>("search_workspace", { query }),

    listMcpServers: () => invoke<McpServerInfo[]>("list_mcp_servers"),
    /** Returns a human-readable result: connected with N tools, or why not. */
    saveMcpServer: (server: {
        id: string;
        transport: string;
        command: string;
        args: string;
        url: string;
        headerName: string;
        headerValue: string;
        secret: string;
    }) => invoke<string>("save_mcp_server", server),
    removeMcpServer: (id: string) => invoke<void>("remove_mcp_server", { id }),
    toggleMcpServer: (id: string, enabled: boolean) =>
        invoke<void>("toggle_mcp_server", { id, enabled }),
    toggleMcpTool: (id: string, tool: string, enabled: boolean) =>
        invoke<void>("toggle_mcp_tool", { id, tool, enabled }),

    spotifySettings: () => invoke<SpotifySettings>("get_spotify_settings"),
    setSpotifyClientId: (clientId: string) =>
        invoke<void>("set_spotify_client_id", { clientId }),
    /** Opens the browser; the result arrives as a `spotify:auth` event. */
    connectSpotify: () => invoke<void>("connect_spotify"),
    disconnectSpotify: () => invoke<void>("disconnect_spotify"),
    spotifyControl: (action: "play" | "pause" | "next" | "previous") =>
        invoke<void>("spotify_control", { action }),
    /** Album art as a data: URI, fetched and cached by the backend. */
    spotifyAlbumArt: (url: string) => invoke<string>("spotify_album_art", { url }),
    spotifyNowPlaying: () =>
        invoke<SpotifyNowPlaying | null>("spotify_now_playing"),

    steamSettings: () => invoke<SteamSettings>("get_steam_settings"),
    /** Returns a human-readable result: connected, or why not. */
    setSteam: (steamId: string, key: string) =>
        invoke<string>("set_steam", { steamId, key }),

    listVaults: () => invoke<VaultInfo[]>("list_vaults"),
    setVault: (path: string) => invoke<void>("set_vault", { path }),

    canvasSettings: () => invoke<CanvasSettings>("get_canvas_settings"),
    setCanvasKey: (provider: string, key: string) =>
        invoke<void>("set_canvas_key", { provider, key }),
    setCanvasOrder: (kind: "image" | "video", order: string[]) =>
        invoke<void>("set_canvas_order", { kind, order }),
    setCanvasDefaults: (settings: {
        imageModel: string;
        videoModel: string;
        size: string;
        count: number;
        customUrl: string;
        customHeaderName: string;
        customHeaderValue: string;
        customModel: string;
    }) => invoke<void>("set_canvas_defaults", settings),
    /** Returns at once; the result arrives as `canvas:done` or `canvas:error`. */
    canvasGenerate: (request: GenerateRequest) =>
        invoke<void>("canvas_generate", {
            prompt: request.prompt,
            kind: request.kind,
            model: request.model,
            width: request.width,
            height: request.height,
            count: request.count,
            seed: request.seed,
            negative: request.negative,
            durationSecs: request.durationSecs,
            fromId: request.fromId,
            strength: request.strength,
            upscale: request.upscale,
        }),
    listGallery: (limit?: number) => invoke<GalleryItem[]>("list_gallery", { limit }),
    deleteGalleryItem: (id: number) =>
        invoke<void>("delete_gallery_item", { id }),
    favouriteGalleryItem: (id: number, favourite: boolean) =>
        invoke<void>("favourite_gallery_item", { id, favourite }),
    /** Returns the number of bytes freed. */
    clearGallery: (keepFavourites: boolean) =>
        invoke<number>("clear_gallery", { keepFavourites }),
    openMediaFolder: () => invoke<void>("open_media_folder"),

    /**
   * Makes a real request against `target` and reports the outcome.
   *
   * `target` is a chat provider id, or one of "search", "obsidian", "steam",
   * "spotify", "canvas".
   */
    testConnection: (target: string) =>
        invoke<ConnectionTest>("test_connection", { target }),

    /** Just the level, for the meter. Cheap enough to poll ten times a second. */
    micLevel: () => invoke<number>("mic_level"),

    councilForecast: (task: string, seats: Seat[]) =>
        invoke<Forecast>("council_forecast", { task, seats }),
    /** Returns at once; progress arrives as `council:*` events. */
    councilRun: (task: string, seats: Seat[]) =>
        invoke<void>("council_run", { task, seats }),
    /** Puts one seat's answer into the conversation. */
    councilKeep: (text: string) => invoke<void>("council_keep", { text }),

    setCustomSearch: (custom: CustomSearch) =>
        invoke<void>("set_custom_search", {
            url: custom.url,
            headerName: custom.headerName,
            headerValue: custom.headerValue,
            resultsPath: custom.resultsPath,
            titleKey: custom.titleKey,
            urlKey: custom.urlKey,
            snippetKey: custom.snippetKey,
        }),
};

// ── Events ───────────────────────────────────────────────────────────

export interface DeltaEvent {
    text: string;
}
export interface DoneEvent {
    text: string;
}
export interface ErrorEvent {
    message: string;
    /**
     * The request was refused for its size.
     *
     * A flag rather than matching on the message text, which is translated
     * and would silently stop offering the fix the moment it was reworded.
     */
    tooLong: boolean;
}
export interface ToolStartEvent {
    tool: string;
    /** What it was called with, for the expanded view. */
    args: string;
}
/**
 * The turn saying something about itself while still working — currently only
 * "waiting out a rate limit".
 *
 * Separate from a delta because a delta is the model's answer: folding this
 * into it would leave "waiting 20s" inside the saved reply.
 */
export interface NoticeEvent {
    text: string;
}
export interface ToolDoneEvent {
    tool: string;
    ok: boolean;
    /** One line, for the collapsed note in the feed. */
    summary: string;
    /** The fuller output, shown when the note is opened. */
    detail: string;
}
export interface ApprovalEvent {
    tool: string;
    args: string;
    reason: "risk" | "budget" | "tainted";
}
export interface AutomationEvent {
    id: number;
    prompt: string;
    trigger: string;
}
export interface CouncilDeltaEvent {
    seat: string;
    text: string;
}
export interface CouncilSeatDoneEvent {
    seat: string;
    text: string;
    label: string;
    inputTokens: number;
    outputTokens: number;
    /** Null for a model with no published price. */
    dollars: number | null;
    elapsedMs: number;
}
export interface CouncilSeatFailedEvent {
    seat: string;
    message: string;
}
export interface CouncilDoneEvent {
    answered: number;
    failed: number;
    dollars: number;
    unpriced: number;
}
export interface CanvasDoneEvent {
    items: GalleryItem[];
    provider: string;
    /** Providers that failed before the one that worked. A note, not an error. */
    notes: string[];
}

export type VoiceEvent =
    | { kind: "heard"; text: string }
    | { kind: "woke" }
    | { kind: "notice"; text: string }
    | { kind: "speaking"; active: boolean };

/**
 * Subscribes to a backend event.
 *
 * Returns the unsubscribe function; callers must keep it and call it on
 * teardown, or handlers accumulate across component remounts.
 */
export function on<T>(
    event: string,
    handler: (payload: T) => void,
): Promise<UnlistenFn> {
    return listen<T>(event, (e) => handler(e.payload));
}
