/**
 * The icon set.
 *
 * One component, one path table. The old interface drew its icons with
 * Unicode glyphs -- a lot of them dingbats picked for vague resemblance,
 * which rendered at whatever weight and baseline the system font felt like
 * and could not be aligned with anything.
 *
 * These are drawn on a 24-unit grid and stroked, never filled, so weight
 * stays even at every size and the whole set optically matches the text
 * beside it. `currentColor` throughout: an icon takes the colour of what
 * it sits in, and is never separately themed.
 *
 * `IconName` is exported from this module (Svelte kept it in a `module`
 * script block because that was the only scope it allowed a type export
 * from; a plain `.tsx` file has no such restriction). Typing `PATHS` as
 * `Record<IconName, ...>` ties the two together, so adding a name without
 * drawing it -- or asking for one that was never drawn -- is a compile
 * error rather than a blank space.
 */
import "./styles/icon.css";

export type IconName =
    | "chat"
    | "code"
    | "canvas"
    | "council"
    | "send"
    | "mic"
    | "micOff"
    | "stop"
    | "settings"
    | "plus"
    | "close"
    | "minimise"
    | "maximise"
    | "copy"
    | "check"
    | "trash"
    | "panelRight"
    | "chevronDown"
    | "chevronRight"
    | "arrowDown"
    | "sun"
    | "moon"
    | "tool"
    | "warning"
    | "info"
    | "memory"
    | "clock";

interface Props {
    name: IconName;
    /** Rendered size in pixels. Stroke scales with it, so weight is even. */
    size?: number;
    /** Overrides the computed stroke width, for optical corrections. */
    stroke?: number;
    /** Named `className`, React's spelling of Svelte's `class` prop here. */
    className?: string;
}

/**
 * Paths on a 24x24 grid.
 *
 * Each entry is an array of `d` strings; multi-path icons keep their parts
 * separate so joins and caps stay clean rather than being welded into one
 * self-intersecting outline.
 */
const PATHS: Record<IconName, readonly string[]> = {
    // -- Views ----------------------------------------------------------
    chat: ["M21 11.5a8.4 8.4 0 0 1-9 8.4 9.5 9.5 0 0 1-2.9-.4L4 21l1.4-3.9A8.2 8.2 0 0 1 3.6 11.5 8.4 8.4 0 0 1 12 3.1a8.4 8.4 0 0 1 9 8.4Z"],
    code: ["M8 17 3.5 12 8 7", "M16 7l4.5 5L16 17", "M13.6 4.5 10.4 19.5"],
    canvas: [
        "M4 5.5A1.5 1.5 0 0 1 5.5 4h13A1.5 1.5 0 0 1 20 5.5v13a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 18.5Z",
        "M4 15.5 8.5 11l4 4",
        "M14 12.5 16 10.5l4 4",
        "M15 8.5h.01",
    ],
    council: [
        "M9 11a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z",
        "M17.5 12.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5Z",
        "M2.5 19.5a6.5 6.5 0 0 1 13 0",
        "M17.5 15a5 5 0 0 1 4 4.5",
    ],

    // -- Actions --------------------------------------------------------
    send: ["M4.5 12h6", "M4.9 5.6 20 12 4.9 18.4l1.7-6.4Z"],
    mic: [
        "M12 3.5a2.7 2.7 0 0 1 2.7 2.7v5.4a2.7 2.7 0 0 1-5.4 0V6.2A2.7 2.7 0 0 1 12 3.5Z",
        "M18 11.2a6 6 0 0 1-12 0",
        "M12 17.2v3.3",
    ],
    micOff: [
        "M9.3 9.3v2.3a2.7 2.7 0 0 0 4.3 2.2",
        "M14.7 11V6.2a2.7 2.7 0 0 0-5.2-1",
        "M18 11.2a6 6 0 0 1-1 3.3",
        "M6 11.2a6 6 0 0 0 9 5.2",
        "M12 17.2v3.3",
        "M3.5 3.5 20.5 20.5",
    ],
    stop: ["M8.5 8.5h7v7h-7z"],
    settings: [
        "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z",
        "M19.1 14.5a1.5 1.5 0 0 0 .3 1.6l.1.1a1.8 1.8 0 1 1-2.6 2.6l-.1-.1a1.5 1.5 0 0 0-2.6 1.1v.2a1.8 1.8 0 1 1-3.6 0v-.1a1.5 1.5 0 0 0-2.6-1.1l-.1.1a1.8 1.8 0 1 1-2.6-2.6l.1-.1a1.5 1.5 0 0 0-1.1-2.6h-.2a1.8 1.8 0 1 1 0-3.6h.1a1.5 1.5 0 0 0 1.1-2.6l-.1-.1A1.8 1.8 0 1 1 7.7 4.7l.1.1a1.5 1.5 0 0 0 1.6.3H9.5a1.5 1.5 0 0 0 .9-1.4v-.2a1.8 1.8 0 1 1 3.6 0v.1a1.5 1.5 0 0 0 2.6 1.1l.1-.1a1.8 1.8 0 1 1 2.6 2.6l-.1.1a1.5 1.5 0 0 0-.3 1.6v.1a1.5 1.5 0 0 0 1.4.9h.2a1.8 1.8 0 1 1 0 3.6h-.1a1.5 1.5 0 0 0-1.4.9Z",
    ],
    plus: ["M12 5.5v13", "M5.5 12h13"],
    close: ["M6 6l12 12", "M18 6 6 18"],
    minimise: ["M5.5 12h13"],
    maximise: ["M5 7.5A2.5 2.5 0 0 1 7.5 5h9A2.5 2.5 0 0 1 19 7.5v9a2.5 2.5 0 0 1-2.5 2.5h-9A2.5 2.5 0 0 1 5 16.5Z"],
    copy: [
        "M9 9.5A2.5 2.5 0 0 1 11.5 7h6A2.5 2.5 0 0 1 20 9.5v6a2.5 2.5 0 0 1-2.5 2.5h-6A2.5 2.5 0 0 1 9 15.5Z",
        "M15 7V6.5A2.5 2.5 0 0 0 12.5 4h-6A2.5 2.5 0 0 0 4 6.5v6A2.5 2.5 0 0 0 6.5 15H7",
    ],
    check: ["M4.5 12.5 9.5 17.5 19.5 7"],
    trash: [
        "M4.5 6.5h15",
        "M9 6.5V5a1.5 1.5 0 0 1 1.5-1.5h3A1.5 1.5 0 0 1 15 5v1.5",
        "M6.5 6.5 7.3 19a1.5 1.5 0 0 0 1.5 1.4h6.4a1.5 1.5 0 0 0 1.5-1.4l.8-12.5",
    ],

    // -- Chrome ---------------------------------------------------------
    panelRight: [
        "M3.5 6.5A2.5 2.5 0 0 1 6 4h12a2.5 2.5 0 0 1 2.5 2.5v11A2.5 2.5 0 0 1 18 20H6a2.5 2.5 0 0 1-2.5-2.5Z",
        "M14.5 4v16",
    ],
    chevronDown: ["M6.5 9.5 12 15l5.5-5.5"],
    chevronRight: ["M9.5 6.5 15 12l-5.5 5.5"],
    arrowDown: ["M12 5v14", "M6 13l6 6 6-6"],
    sun: [
        "M12 16.5a4.5 4.5 0 1 0 0-9 4.5 4.5 0 0 0 0 9Z",
        "M12 2.5v2M12 19.5v2M4.2 4.2l1.4 1.4M18.4 18.4l1.4 1.4M2.5 12h2M19.5 12h2M4.2 19.8l1.4-1.4M18.4 5.6l1.4-1.4",
    ],
    moon: ["M20 14.2A8.2 8.2 0 0 1 9.8 4 8.4 8.4 0 1 0 20 14.2Z"],

    // -- Status ---------------------------------------------------------
    tool: [
        "M14.2 6.3a3.9 3.9 0 0 0 5 5l-8.4 8.4a2.2 2.2 0 0 1-3.1-3.1Z",
        "M14.2 6.3 11 3.1a2.2 2.2 0 0 0-3.1 0L5.6 5.4a2.2 2.2 0 0 0 0 3.1l3.2 3.2",
    ],
    warning: [
        "M10.3 4.3 2.9 17a2 2 0 0 0 1.7 3h14.8a2 2 0 0 0 1.7-3L13.7 4.3a2 2 0 0 0-3.4 0Z",
        "M12 9.5v4",
        "M12 17h.01",
    ],
    info: ["M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Z", "M12 11.5v5", "M12 8h.01"],
    memory: [
        "M12 4.5a3 3 0 0 0-3 3v9a3 3 0 0 0 6 0v-9a3 3 0 0 0-3-3Z",
        "M9 8.5H6.5M9 15.5H6.5M15 8.5h2.5M15 15.5h2.5",
    ],
    clock: ["M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Z", "M12 7.5V12l3 2"],
};

export default function Icon({ name, size = 18, stroke, className = "" }: Props) {
    const paths = PATHS[name];

    // Stroke thins as the icon grows, so a 32px icon does not look bolder
    // than a 16px one sitting next to it.
    const width = stroke ?? (size >= 28 ? 1.4 : 1.7);

    return (
        <svg
            className={className}
            width={size}
            height={size}
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={width}
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
            focusable="false"
        >
            {paths.map((d, i) => (
                <path key={i} d={d} />
            ))}
        </svg>
    );
}
