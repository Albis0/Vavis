/**
 * The window.
 *
 * A single full-bleed stage with the reactor at its centre and nothing else,
 * and a chat panel docked to the right that can be resized or hidden. That is
 * the whole layout. Everything the old interface kept permanently on screen --
 * telemetry rails, meters, a shortcut list, a view switcher -- now lives in
 * the command palette on Ctrl+K, where it is one keystroke away instead of
 * occupying a column full time.
 *
 * The other interfaces (code, canvas, council) take the stage over when they
 * are open, keeping the chat panel beside them so the conversation is never
 * left behind.
 */

import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../lib/api";
import CanvasView from "./CanvasView";
import ChatPanel, { type ChatPanelHandle } from "./ChatPanel";
import CodeView from "./CodeView";
import CommandPalette, { type Command } from "./CommandPalette";
import ConfirmDialog from "./ConfirmDialog";
import CouncilView from "./CouncilView";
import Icon from "./Icon";
import Panels from "./Panels";
import Reactor from "./Reactor";
import Settings from "./settings/Settings";
import SpotifyPanel from "./SpotifyPanel";
import StatusBar from "./StatusBar";
import Toasts from "./Toasts";
import { chat, chatSignal } from "./store/chat";
import { nowPlaying, nowPlayingSignal } from "./store/nowplaying";
import { toast } from "./store/toast";
import { useStore } from "./store/useStore";
import "./styles/app.css";

/**
 * The Tauri window handle, or `null` when there is no Tauri runtime.
 *
 * `getCurrentWindow()` reads `window.__TAURI_INTERNALS__.metadata`, which
 * only exists inside Tauri's WebView. This runs at module scope, so outside
 * Tauri it threw before anything mounted and the whole interface was a blank
 * black page with no error on screen -- discovered when a browser-driven test
 * run found zero interactive elements at the dev server.
 *
 * Failing this softly costs two window buttons and keeps everything else
 * working, which is the right trade: it makes the interface openable in an
 * ordinary browser for inspection, and it means one missing global can never
 * again take the entire app down silently.
 */
const appWindow = (() => {
    try {
        return getCurrentWindow();
    } catch {
        return null;
    }
})();

const THEME_KEY = "vavis.theme";
/**
 * The release the user has already been told about.
 *
 * Per-machine and per-version, which is why localStorage rather than the
 * config: it records what this person has seen, not how the app behaves.
 */
const UPDATE_SEEN_KEY = "vavis.update.seen";

const WINDOW_MODES = ["windowed", "borderless", "fullscreen"];

export default function App() {
    const state = useStore(chatSignal, chat);
    const playing = useStore(nowPlayingSignal, nowPlaying);
    const status = state.status;

    const [chatOpen, setChatOpen] = useState(true);
    const [paletteOpen, setPaletteOpen] = useState(false);
    // Seeded from the DOM rather than from storage. `main.tsx` has already
    // applied the saved theme before the app mounted -- components read
    // `data-theme` as they initialise -- and reading it back keeps this the
    // single source of truth afterwards.
    const [theme, setTheme] = useState<"dark" | "light">(() =>
        document.documentElement.dataset.theme === "light" ? "light" : "dark",
    );

    const chatPanel = useRef<ChatPanelHandle | null>(null);

    /**
     * Whether the conversation has started.
     *
     * This is the whole layout switch. An empty app gave the reactor 73% of
     * the window and the conversation a 400px strip down the side -- the
     * largest, brightest, only moving thing on screen was the one thing with
     * nothing to do, and the actual work happened in the margin.
     *
     * So the reactor keeps the stage while there is nothing to read, which is
     * when it is the point, and steps aside the moment there is. Only the
     * chat view swaps: code, canvas and council already own the stage, and
     * there the panel beside them is exactly what it should be.
     */
    const conversing = state.view === "chat" && state.messages.length > 0;

    // Started once, for the life of the app. `chat.start()` subscribes to
    // backend events and opens two pollers, so the cleanup matters: without
    // it a remount leaves the old ones running and the app gets slower the
    // longer it is open.
    useEffect(() => {
        void chat.start();
        // Polls whether or not the box is on screen: music starting is what
        // puts it there, so detection cannot live inside the thing it shows.
        nowPlaying.start();
        return () => {
            chat.stop();
            nowPlaying.stop();
        };
    }, []);

    /**
     * Mentions a new release once, at startup, and only when there is one.
     *
     * Three deliberate restraints. It says nothing when the check fails --
     * someone offline does not need a startup error about a background task
     * they did not ask for; the Updates pane reports failures, because there
     * the user asked. It says nothing when already current, for the same
     * reason. And it stays quiet for a version already dismissed, so the
     * answer to "not now" is not the same notice again tomorrow morning.
     */
    useEffect(() => {
        let cancelled = false;
        void (async () => {
            const update = await api.checkUpdate().catch(() => null);
            if (cancelled || update?.status !== "available") return;

            // Per-version, not a global "don't ask": skipping 0.8.0 should
            // not also hide 0.9.0.
            if (localStorage.getItem(UPDATE_SEEN_KEY) === update.latest) return;

            toast.info(`Version ${update.latest} is available.`, {
                duration: 0,
                action: {
                    label: "download",
                    run: () => {
                        localStorage.setItem(UPDATE_SEEN_KEY, update.latest);
                        void api.openReleasePage();
                    },
                },
            });
        })();
        return () => {
            cancelled = true;
        };
    }, []);

    // Theme is presentation and per-machine, so it lives in localStorage
    // rather than in the app config: there is nothing for the backend or
    // another device to do with it.
    useEffect(() => {
        document.documentElement.dataset.theme = theme;
        localStorage.setItem(THEME_KEY, theme);
    }, [theme]);

    const toggleChat = useCallback(() => {
        setChatOpen((open) => {
            // Opening the panel should put the caret in it. Making the user
            // click into a panel they just summoned is exactly the friction
            // this layout is meant to remove.
            if (!open) queueMicrotask(() => chatPanel.current?.focus());
            return !open;
        });
    }, []);

    const cycleWindowMode = useCallback(async () => {
        const current = status?.windowMode ?? "windowed";
        const next =
            WINDOW_MODES[(WINDOW_MODES.indexOf(current) + 1) % WINDOW_MODES.length];

        try {
            // Moving and saving are one backend call. They used to be
            // separate steps here, which is how "borderless" ended up meaning
            // two different things: this path asked the window to maximize,
            // and an undecorated window does not reliably read that as "fill
            // the screen". Startup had the working version; now there is one.
            await api.setWindowMode(next);
            await chat.refresh();
        } catch (e) {
            // The window is now in a state the config does not describe,
            // which is worth saying: the next launch will not reproduce it.
            toast.failure("Could not change the window mode.", e);
        }
    }, [status?.windowMode]);

    /**
     * Everything the interface can do, in one list.
     *
     * This is the only registry of actions: the palette reads it, and so does
     * the shortcut handler below, so a command and its key can never drift
     * apart.
     */
    const commands = useMemo<Command[]>(
        () => [
            {
                id: "view.chat",
                label: "Reactor",
                group: "Go to",
                icon: "chat",
                keywords: "home stage main",
                run: () => (chat.view = "chat"),
            },
            {
                id: "view.code",
                label: "Code",
                group: "Go to",
                icon: "code",
                keywords: "workspace files editor",
                run: () => (chat.view = "code"),
            },
            {
                id: "view.canvas",
                label: "Canvas",
                group: "Go to",
                icon: "canvas",
                keywords: "image video generate gallery",
                run: () => (chat.view = "canvas"),
            },
            {
                id: "view.council",
                label: "Council",
                group: "Go to",
                icon: "council",
                keywords: "models compare panel",
                run: () => (chat.view = "council"),
            },
            {
                id: "chat.toggle",
                label: chatOpen ? "Hide chat panel" : "Show chat panel",
                group: "Chat",
                icon: "panelRight",
                hint: "Ctrl+B",
                run: toggleChat,
            },
            {
                id: "chat.clear",
                label: "New conversation",
                group: "Chat",
                icon: "plus",
                hint: "Ctrl+L",
                keywords: "clear reset",
                run: () => void chat.clearWithConfirm(),
            },
            {
                id: "voice.cycle",
                label: `Voice: ${status?.voiceMode ?? "off"}`,
                group: "Chat",
                icon: status?.voiceMode === "off" ? "micOff" : "mic",
                hint: "Ctrl+M",
                keywords: "microphone listen speech wake",
                run: () => void chat.cycleVoice(),
            },
            {
                id: "spotify.nowPlaying",
                label: playing.visible ? "Hide now playing" : "Show now playing",
                group: "Chat",
                icon: "info",
                keywords: "spotify music track player playing drag box popup",
                run: () => nowPlaying.toggle(),
            },
            {
                id: "app.settings",
                label: "Settings",
                group: "Application",
                icon: "settings",
                hint: "Ctrl+,",
                keywords: "keys providers preferences api language",
                run: () => (chat.panel = "settings"),
            },
            {
                id: "app.theme",
                label:
                    theme === "dark"
                        ? "Switch to light theme"
                        : "Switch to dark theme",
                group: "Application",
                icon: theme === "dark" ? "sun" : "moon",
                keywords: "appearance colour color mode",
                run: () => setTheme((t) => (t === "dark" ? "light" : "dark")),
            },
            {
                id: "app.window",
                label: `Window: ${status?.windowMode ?? "windowed"}`,
                group: "Application",
                icon: "maximise",
                hint: "F11",
                keywords: "fullscreen borderless maximise",
                run: () => void cycleWindowMode(),
            },
            {
                id: "app.memory",
                label: "Remembered facts",
                group: "Application",
                icon: "memory",
                keywords: "memory knows about me",
                run: () => (chat.panel = "memory"),
            },
            {
                id: "app.automations",
                label: "Automations",
                group: "Application",
                icon: "clock",
                keywords: "schedule triggers timers",
                run: () => (chat.panel = "automations"),
            },
            {
                id: "app.tools",
                label: "Tools",
                group: "Application",
                icon: "tool",
                keywords: "abilities capabilities integrations",
                run: () => (chat.panel = "tools"),
            },
        ],
        [
            chatOpen,
            playing.visible,
            status?.voiceMode,
            status?.windowMode,
            theme,
            toggleChat,
            cycleWindowMode,
        ],
    );

    useEffect(() => {
        function onGlobalKey(event: KeyboardEvent) {
            const ctrl = event.ctrlKey || event.metaKey;

            if (event.key === "Escape") {
                // Escape unwinds one layer at a time, most transient first.
                // Closing everything at once would lose a panel the user was
                // reading because they wanted the speech to stop.
                //
                // Modals answer Escape themselves and stop the event, so the
                // overlay branches below are only reached when focus has
                // somehow left the trapped layer. They stay as that fallback.
                if (paletteOpen) setPaletteOpen(false);
                else if (chat.status?.speaking) void chat.stopSpeaking();
                else if (chat.approval) void chat.answerApproval("deny");
                else if (chat.panel !== "none") chat.panel = "none";
                else if (chat.view !== "chat") chat.view = "chat";
                return;
            }

            if (ctrl && event.key.toLowerCase() === "k") {
                event.preventDefault();
                setPaletteOpen((open) => !open);
                return;
            }
            if (ctrl && event.key.toLowerCase() === "b") {
                event.preventDefault();
                toggleChat();
                return;
            }
            if (ctrl && event.key.toLowerCase() === "m") {
                event.preventDefault();
                void chat.cycleVoice();
                return;
            }
            if (ctrl && event.key.toLowerCase() === "l") {
                event.preventDefault();
                void chat.clearWithConfirm();
                return;
            }
            if (ctrl && event.key === ",") {
                event.preventDefault();
                chat.panel = chat.panel === "settings" ? "none" : "settings";
                return;
            }
            if (event.key === "F11") {
                event.preventDefault();
                void cycleWindowMode();
            }
        }

        window.addEventListener("keydown", onGlobalKey);
        return () => window.removeEventListener("keydown", onGlobalKey);
    }, [paletteOpen, toggleChat, cycleWindowMode]);

    return (
        <>
            <div className="shell">
                {/* Title strip. The window is undecorated, so this is what the
                    user grabs to move it. It stays deliberately sparse: a
                    name, and the window controls. */}
                <div className="titlebar" data-tauri-drag-region>
                    <span className="brand" data-tauri-drag-region>
                        Vavis
                    </span>

                    <div className="titlebar-actions">
                        <button
                            className="chip"
                            onClick={() => setPaletteOpen(true)}
                            title="Commands (Ctrl+K)"
                        >
                            <Icon name="chevronRight" size={13} />
                            <span>Commands</span>
                            <kbd>Ctrl K</kbd>
                        </button>

                        {!chatOpen && (
                            <button
                                className="wb"
                                title="Show chat (Ctrl+B)"
                                onClick={toggleChat}
                            >
                                <Icon name="panelRight" size={15} />
                            </button>
                        )}

                        {/* Hidden outside Tauri, where there is no window to
                            command. */}
                        {appWindow && (
                            <button
                                className="wb"
                                title="Minimise"
                                onClick={() => appWindow.minimize()}
                            >
                                <Icon name="minimise" size={15} />
                            </button>
                        )}
                        <button
                            className="wb"
                            title="Window mode (F11)"
                            onClick={() => void cycleWindowMode()}
                        >
                            <Icon name="maximise" size={15} />
                        </button>
                        {appWindow && (
                            <button
                                className="wb close"
                                title="Close"
                                onClick={() => appWindow.close()}
                            >
                                <Icon name="close" size={15} />
                            </button>
                        )}
                    </div>
                </div>

                <div
                    className={`shell-body${conversing ? " conversing" : ""}`}
                    /* Drives the dimmed reactor back up while the assistant is
                       actually doing something, so it still reads as a status
                       light once it has stepped aside. */
                    data-busy={state.coreState !== "idle" ? "true" : "false"}
                >
                    <main className="stage">
                        {state.view === "code" ? (
                            <CodeView />
                        ) : state.view === "canvas" ? (
                            <CanvasView />
                        ) : state.view === "council" ? (
                            <CouncilView />
                        ) : (
                            /* The stage: the reactor, and nothing else. */
                            <Reactor mode={state.coreState} level={state.micLevel} />
                        )}
                    </main>

                    {chatOpen && <ChatPanel ref={chatPanel} onClose={toggleChat} />}
                </div>

                <StatusBar onOpenSettings={() => (chat.panel = "settings")} />
            </div>

            {/* Outside `.shell` so it floats over every view rather than being
                clipped by the stage, and above the chat panel it may be
                dragged across. */}
            {playing.visible && <SpotifyPanel onClose={() => nowPlaying.dismiss()} />}

            {paletteOpen && (
                <CommandPalette
                    commands={commands}
                    onClose={() => setPaletteOpen(false)}
                />
            )}

            {/* Settings is a window of its own rather than a sheet: fourteen
                categories never fit the shape the other panels use. */}
            {state.panel === "settings" ? (
                <Settings />
            ) : state.panel !== "none" ? (
                <Panels />
            ) : null}

            {/* Both are mounted once, here, and driven by their stores:
                anything anywhere can raise a question or report an outcome
                without needing somewhere of its own on screen to put it. */}
            <ConfirmDialog />
            <Toasts />
        </>
    );
}
