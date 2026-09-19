/**
 * The reactor, mounted.
 *
 * This component owns the lifetime of the canvas and translates assistant
 * state into the reactor's drive values. The drawing itself lives in
 * `reactor.ts`, which knows nothing about the app.
 *
 * The mapping below is the whole vocabulary of the visual: hue says what kind
 * of thing is happening, speed says how hard it is working, and the pulse
 * says it is talking. It is deliberately small -- a state you cannot name at
 * a glance is a state that communicates nothing.
 */
import { useEffect, useRef, useState } from "react";
import { createReactor, type Reactor as ReactorHandle } from "../lib/reactor";
import type { CoreState } from "./store/chat";
import "./styles/reactor.css";

interface Props {
    /**
     * What the assistant is doing.
     *
     * Called `mode` rather than `state` because that was the name Svelte
     * needed to avoid clashing with its `$state` rune. React has no such
     * restriction, but the prop name is kept so it still reads the same at
     * every call site.
     */
    mode: CoreState;
    /** Live microphone level, 0-1. Spikes the core while speaking to it. */
    level?: number;
}

/**
 * How each state looks.
 *
 * `spin` is revolutions per second. Thinking runs backwards on purpose:
 * reversal is legible at a glance in a way that a speed change is not,
 * and thinking is the state the user most needs to recognise instantly.
 */
const LOOK: Record<CoreState, { spin: number; glow: number; hue: number }> = {
    idle: { spin: 0.045, glow: 0.72, hue: 205 },
    listening: { spin: 0.16, glow: 1.15, hue: 190 },
    thinking: { spin: -0.34, glow: 1.35, hue: 38 },
    working: { spin: 0.42, glow: 1.25, hue: 265 },
    speaking: { spin: 0.13, glow: 1.4, hue: 210 },
};

export default function Reactor({ mode, level = 0 }: Props) {
    const hostRef = useRef<HTMLDivElement | null>(null);
    const reactorRef = useRef<ReactorHandle | null>(null);
    /** Set when a 2D context cannot be had, so the fallback can take over. */
    const [failed, setFailed] = useState(false);

    useEffect(() => {
        const host = hostRef.current;
        if (!host) return;
        try {
            reactorRef.current = createReactor(host);
        } catch (error) {
            // A 2D context is refused only when the process is out of canvas
            // memory, which is rare and recoverable. The interface must still
            // work, so fall back rather than leaving a hole in the window.
            console.error("reactor failed to start", error);
            setFailed(true);
        }
        return () => {
            reactorRef.current?.dispose();
            reactorRef.current = null;
        };
    }, []);

    // Push state into the reactor. Its loop reads these every frame and eases
    // toward them, so assigning is enough -- there is nothing to animate here.
    useEffect(() => {
        const reactor = reactorRef.current;
        if (!reactor) return;
        const look = LOOK[mode];
        reactor.drive.spin = look.spin;
        reactor.drive.glow = look.glow;
        reactor.drive.hue = look.hue;
        reactor.drive.pulse = mode === "speaking" ? 1 : 0;
        reactor.drive.level = level;
    }, [mode, level]);

    return (
        <div className="stage">
            <div className="host" ref={hostRef}></div>

            {failed ? (
                // No canvas: a plain CSS ring, so the centre of the window is still
                // something rather than nothing.
                <div className="fallback" data-state={mode}></div>
            ) : null}
        </div>
    );
}
