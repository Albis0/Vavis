/**
 * Now playing.
 *
 * A floating box you can pick up and put anywhere, not a docked rail and not
 * an always-on-top window of its own: it belongs to the app, not to the
 * desktop, but it should not have to fight the stage for a column either.
 * It opens near the left edge, which is the empty side of the layout when
 * the chat panel is docked to the right.
 *
 * It is deliberately *not* modal and not part of the overlay stack. Nothing
 * here has to be answered, so it must never take focus or trap it — you drag
 * it out of the way and keep typing.
 *
 * Where it sits is remembered per machine in localStorage, kept inside the
 * window on every resize, and clamped again on load: a box restored to
 * coordinates from a larger monitor would otherwise open off screen with no
 * way to reach it.
 *
 * Whether it is on screen at all is not this component's business —
 * `nowplaying.ts` polls whether or not the box exists and decides, because a
 * panel that only polls while mounted can never be the thing that mounts
 * itself. This draws whatever that store found.
 *
 * The progress bar is counted forward locally between polls. Asking Spotify
 * every second would burn the rate limit for a number we can work out
 * ourselves.
 */
import { useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import Icon from "./Icon";
import { useStore } from "./store/useStore";
import { nowPlaying, nowPlayingSignal } from "./store/nowplaying";
import "./styles/spotifypanel.css";

interface Props {
    onClose: () => void;
}

/** How often the local progress estimate advances. */
const TICK_MS = 1000;

const POSITION_KEY = "vavis.nowPlaying.position";
const WIDTH = 268;
/** Kept clear of the title strip above and the status bar below. */
const TOP_LIMIT = 44;
const BOTTOM_LIMIT = 32;

export default function SpotifyPanel({ onClose }: Props) {
    const state = useStore(nowPlayingSignal, nowPlaying);
    const now = state.track;

    const [progress, setProgress] = useState(0);
    const [busy, setBusy] = useState(false);

    /** Data URI of the current cover, and the remote URL it came from. */
    const [art, setArt] = useState<string | null>(null);
    const artForRef = useRef<string | null>(null);

    const boxRef = useRef<HTMLElement | null>(null);
    const [x, setX] = useState(24);
    // Replaced on mount with a bottom-left resting place. Anchoring to the top
    // put the box over whatever header the view on the left happens to have —
    // the file tree's path field, the canvas controls — which is the one part
    // of a side rail you cannot afford to cover.
    const [y, setY] = useState(96);
    const [dragging, setDragging] = useState(false);

    // Read fresh inside handlers without re-subscribing every drag frame.
    const xRef = useRef(x);
    const yRef = useRef(y);
    xRef.current = x;
    yRef.current = y;

    /** Pointer offset within the box when the drag started. */
    const grabRef = useRef({ x: 0, y: 0 });

    /** Holds the position inside the window, whatever the window is now. */
    function clamp(nextX: number, nextY: number): [number, number] {
        const height = boxRef.current?.offsetHeight ?? 180;
        const maxX = Math.max(0, window.innerWidth - WIDTH - 8);
        const maxY = Math.max(TOP_LIMIT, window.innerHeight - height - BOTTOM_LIMIT);
        return [
            Math.min(Math.max(nextX, 8), maxX),
            Math.min(Math.max(nextY, TOP_LIMIT), maxY),
        ];
    }

    // Follows the store's track. Progress is reseeded from every poll, and the
    // cover is fetched only when the track actually changes -- the backend
    // caches it on disk, but the round trip is still worth skipping.
    useEffect(() => {
        const fresh = now;
        if (fresh) setProgress(fresh.progressMs);

        if (fresh?.albumArt && fresh.albumArt !== artForRef.current) {
            artForRef.current = fresh.albumArt;
            void api
                .spotifyAlbumArt(fresh.albumArt)
                .then((data) => setArt(data))
                .catch(() => setArt(null));
        } else if (!fresh?.albumArt) {
            setArt(null);
            artForRef.current = null;
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [now]);

    useEffect(() => {
        const saved = localStorage.getItem(POSITION_KEY);
        let restored = false;
        if (saved) {
            try {
                const parsed = JSON.parse(saved) as { x: number; y: number };
                // Clamped on the way in as well as on the way out: a position
                // saved on a larger monitor would open off screen otherwise.
                const [cx, cy] = clamp(parsed.x, parsed.y);
                setX(cx);
                setY(cy);
                restored = true;
            } catch {
                // A corrupt entry is not worth reporting; the default stands.
            }
        }

        // Deferred a frame so the box has been laid out and `clamp` can read
        // its real height rather than guessing.
        if (!restored) {
            queueMicrotask(() => {
                const [cx, cy] = clamp(24, window.innerHeight);
                setX(cx);
                setY(cy);
            });
        }

        const tickTimer = setInterval(() => {
            if (nowPlaying.track?.playing) {
                setProgress((p) => Math.min(p + TICK_MS, nowPlaying.track?.durationMs ?? p));
            }
        }, TICK_MS);

        const onResize = () => {
            const [cx, cy] = clamp(xRef.current, yRef.current);
            setX(cx);
            setY(cy);
        };
        window.addEventListener("resize", onResize);

        // The box changes height when a track starts — the idle line is one
        // sentence, a playing track is art, a bar and transport buttons — and
        // anchored near the bottom it would grow straight off the edge. This
        // re-clamps it against its own height rather than the window's alone.
        const observer = new ResizeObserver(onResize);
        if (boxRef.current) observer.observe(boxRef.current);

        return () => {
            clearInterval(tickTimer);
            window.removeEventListener("resize", onResize);
            observer.disconnect();
        };
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    function startDrag(event: React.PointerEvent<HTMLElement>) {
        // Only the header drags, and only with the primary button. Dragging
        // from anywhere would make the transport buttons unclickable.
        if (event.button !== 0) return;

        // The close button lives in the header, and capturing the pointer
        // retargets the rest of the gesture to whatever captured it -- so the
        // click never arrived and the box could not be shut.
        if ((event.target as HTMLElement).closest("button")) return;

        setDragging(true);
        grabRef.current = { x: event.clientX - x, y: event.clientY - y };
        // Captured so the box keeps following the pointer even when it moves
        // faster than the box and leaves it behind.
        (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    }

    function onDrag(event: React.PointerEvent<HTMLElement>) {
        if (!dragging) return;
        const [cx, cy] = clamp(
            event.clientX - grabRef.current.x,
            event.clientY - grabRef.current.y,
        );
        setX(cx);
        setY(cy);
    }

    function endDrag(event: React.PointerEvent<HTMLElement>) {
        if (!dragging) return;
        setDragging(false);
        localStorage.setItem(POSITION_KEY, JSON.stringify({ x, y }));

        // The handle is focusable so it can be moved with the arrow keys, but
        // a pointer drag focuses it too, and Chromium then treats that as
        // keyboard focus and paints the ring. Dropping focus after a drag
        // leaves the keyboard route intact without the ring following the
        // pointer around.
        (event.currentTarget as HTMLElement | null)?.blur();
    }

    /** Moves the box with the keyboard, for anyone not using a pointer. */
    function nudge(event: React.KeyboardEvent<HTMLElement>) {
        const step = event.shiftKey ? 24 : 8;
        const moves: Record<string, [number, number]> = {
            ArrowLeft: [-step, 0],
            ArrowRight: [step, 0],
            ArrowUp: [0, -step],
            ArrowDown: [0, step],
        };
        const move = moves[event.key];
        if (!move) return;
        event.preventDefault();
        const [cx, cy] = clamp(x + move[0], y + move[1]);
        setX(cx);
        setY(cy);
        localStorage.setItem(POSITION_KEY, JSON.stringify({ x: cx, y: cy }));
    }

    /** Milliseconds as m:ss. */
    function clock(ms: number): string {
        const total = Math.floor(ms / 1000);
        const minutes = Math.floor(total / 60);
        const seconds = total % 60;
        return `${minutes}:${String(seconds).padStart(2, "0")}`;
    }

    const percent = now && now.durationMs > 0 ? (progress / now.durationMs) * 100 : 0;

    /** Runs a transport command, then re-reads rather than guessing the result. */
    async function control(action: "play" | "pause" | "next" | "previous") {
        if (busy) return;
        setBusy(true);
        try {
            await api.spotifyControl(action);
        } catch {
            // Premium-only, or no active device. The box is ambient; the error
            // belongs in a conversation, not blinking here.
        } finally {
            setBusy(false);
            // Spotify needs a moment to settle before it reports the new state.
            nowPlaying.refreshSoon();
        }
    }

    return (
        <section
            className={dragging ? "np dragging" : "np"}
            ref={boxRef as React.RefObject<HTMLElement>}
            style={{ left: `${x}px`, top: `${y}px`, width: `${WIDTH}px` }}
            aria-label="Now playing"
        >
            {/* The drag handle. A header rather than the whole box, so the transport
                 buttons underneath stay clickable. */}
            <header
                onPointerDown={startDrag}
                onPointerMove={onDrag}
                onPointerUp={endDrag}
                onPointerCancel={endDrag}
                onKeyDown={nudge}
                role="toolbar"
                tabIndex={0}
                aria-label="Move the now playing box with the arrow keys"
            >
                <span className="grip" aria-hidden="true"></span>
                <span className="label">Now playing</span>
                <button className="close" onClick={onClose} aria-label="Hide">
                    <Icon name="close" size={13} />
                </button>
            </header>

            {now ? (
                <>
                    <div className="row">
                        {art ? (
                            <img className="art" src={art} alt="" />
                        ) : (
                            <div className="art placeholder">♪</div>
                        )}

                        <div className="meta">
                            <div className="track" title={now.track}>
                                {now.track}
                            </div>
                            <div className="artist" title={now.artist}>
                                {now.artist}
                            </div>
                            {now.device ? (
                                <div className="device" title={now.device}>
                                    {now.device}
                                </div>
                            ) : null}
                        </div>
                    </div>

                    <div
                        className="bar"
                        role="progressbar"
                        aria-valuenow={Math.round(percent)}
                        aria-valuemin={0}
                        aria-valuemax={100}
                        aria-label="Progress"
                    >
                        <div className="fill" style={{ width: `${percent}%` }}></div>
                    </div>

                    <div className="times">
                        <span>{clock(progress)}</span>
                        <span>{clock(now.durationMs)}</span>
                    </div>

                    <div className="controls">
                        <button aria-label="Previous" onClick={() => control("previous")}>
                            ⏮
                        </button>
                        <button
                            className="play"
                            aria-label={now.playing ? "Pause" : "Play"}
                            onClick={() => control(now?.playing ? "pause" : "play")}
                        >
                            {now.playing ? "⏸" : "▶"}
                        </button>
                        <button aria-label="Next" onClick={() => control("next")}>
                            ⏭
                        </button>
                    </div>
                </>
            ) : (
                <p className="idle">
                    Nothing playing. Start something in Spotify, or ask for a track by
                    name.
                </p>
            )}
        </section>
    );
}
