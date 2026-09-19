/**
 * The canvas interface — image and video generation.
 *
 * A separate interface rather than a chat feature, for one reason: twenty
 * variations in a message feed destroy the feed. Here the results stay in a
 * grid, the prompt stays in one place, and nothing scrolls away.
 *
 * Everything a result was made with is kept next to it. A picture you liked
 * is worth nothing if you cannot make it again, so seed, model, size and
 * prompt all come back with the tile — and when a provider gave no seed, the
 * detail panel says the result cannot be repeated exactly rather than
 * pretending.
 *
 * The detail opens as a modal rather than a third column. As a column it was
 * three hundred pixels wide, which is not enough to look at a picture, and it
 * took that width away from the grid whether or not anything was open.
 */
import { useEffect, useRef, useState } from "react";
import {
    api,
    on,
    type CanvasDoneEvent,
    type CanvasSettings,
    type GalleryItem,
} from "../lib/api";
import { convertFileSrc } from "@tauri-apps/api/core";
import { ask } from "./store/confirm";
import Icon from "./Icon";
import Modal from "./Modal";
import { toast } from "./store/toast";
import { openFolder } from "./actions";
import "./styles/canvasview.css";

type Kind = "image" | "video";

/** Named sizes, so nobody has to remember what 1536×640 is for. */
const SIZES: [string, number, number][] = [
    ["Square", 1024, 1024],
    ["Landscape", 1536, 1024],
    ["Portrait", 1024, 1536],
    ["Wide", 1920, 1080],
    ["Tall", 1080, 1920],
];

export default function CanvasView() {
    const [settings, setSettings] = useState<CanvasSettings | null>(null);
    const [items, setItems] = useState<GalleryItem[]>([]);
    /** False until the first load finishes, so an empty grid is not asserted. */
    const [loaded, setLoaded] = useState(false);

    const [prompt, setPrompt] = useState("");
    const [negative, setNegative] = useState("");
    const [kind, setKind] = useState<Kind>("image");
    const [sizeIndex, setSizeIndex] = useState(0);
    const [count, setCount] = useState(1);
    const [seedText, setSeedText] = useState("");
    const [model, setModel] = useState("");
    const [duration, setDuration] = useState(5);
    const [strength, setStrength] = useState(0.6);

    /** The result being continued from, if any. */
    const [source, setSource] = useState<GalleryItem | null>(null);
    /** The result open in the detail dialog. */
    const [open, setOpen] = useState<GalleryItem | null>(null);

    const [busy, setBusy] = useState(false);
    const [elapsed, setElapsed] = useState(0);

    // Read fresh inside event handlers and async callbacks without
    // re-subscribing every render.
    const busyRef = useRef(busy);
    busyRef.current = busy;
    const modelRef = useRef(model);
    modelRef.current = model;
    const kindRef = useRef(kind);
    kindRef.current = kind;
    const promptRef = useRef(prompt);
    promptRef.current = prompt;
    const negativeRef = useRef(negative);
    negativeRef.current = negative;
    const seedTextRef = useRef(seedText);
    seedTextRef.current = seedText;
    const durationRef = useRef(duration);
    durationRef.current = duration;
    const strengthRef = useRef(strength);
    strengthRef.current = strength;
    const sourceRef = useRef(source);
    sourceRef.current = source;
    const sizeRef = useRef<[string, number, number]>(SIZES[sizeIndex] ?? SIZES[0]);
    sizeRef.current = SIZES[sizeIndex] ?? SIZES[0];
    const openRef = useRef(open);
    openRef.current = open;

    const canGo =
        !busy &&
        prompt.trim().length > 0 &&
        (kind === "image" ? settings?.canImage : settings?.canVideo);

    async function loadSettings() {
        try {
            const next = await api.canvasSettings();
            setSettings(next);
            if (!modelRef.current && next) {
                setModel(kindRef.current === "video" ? next.videoModel : next.imageModel);
            }
        } catch (e) {
            toast.failure("Could not read the canvas settings.", e);
        }
    }

    async function refresh() {
        await loadSettings();
        try {
            setItems(await api.listGallery(300));
        } catch (e) {
            toast.failure("Could not load the gallery.", e);
        } finally {
            setLoaded(true);
        }
    }

    useEffect(() => {
        void refresh();

        const listeners = Promise.all([
            on<CanvasDoneEvent>("canvas:done", (payload) => {
                setBusy(false);
                // Newest first, matching the order the backend lists them in.
                setItems((prev) => [...payload.items.slice().reverse(), ...prev]);
                toast.success(
                    `${payload.items.length} from ${payload.provider}.`,
                    payload.notes.length
                        ? { detail: payload.notes.join(" · ") }
                        : undefined,
                );
                void loadSettings();
            }),
            on<{ message: string }>("canvas:error", (payload) => {
                setBusy(false);
                toast.error("Generation failed.", { detail: payload.message });
            }),
        ]);

        // A generation has no progress to report, so the elapsed seconds are
        // the only honest signal that something is still happening.
        const timer = setInterval(() => {
            if (busyRef.current) setElapsed((prev) => prev + 1);
        }, 1000);

        return () => {
            void listeners.then((offs) => offs.forEach((off) => off()));
            clearInterval(timer);
        };
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    async function generate(options: { upscale?: boolean } = {}) {
        if (busyRef.current) return;
        setBusy(true);
        setElapsed(0);

        const seedTrim = seedTextRef.current.trim();
        const parsedSeed = seedTrim === "" ? null : Number(seedTrim);
        const [, w, h] = sizeRef.current;

        try {
            await api.canvasGenerate({
                prompt: promptRef.current.trim(),
                kind: kindRef.current,
                model: modelRef.current.trim(),
                width: w,
                height: h,
                count,
                seed: Number.isFinite(parsedSeed) ? parsedSeed : null,
                negative: negativeRef.current.trim(),
                durationSecs: durationRef.current,
                fromId: sourceRef.current?.id ?? null,
                strength: strengthRef.current,
                upscale: options.upscale ?? false,
            });
        } catch (e) {
            setBusy(false);
            toast.failure("Could not start that generation.", e);
        }
    }

    /** Sets up a variation: same prompt and settings, that image as the start. */
    function variationOf(item: GalleryItem) {
        setSource(item);
        setKind("image");
        setPrompt(item.prompt);
        setOpen(null);
    }

    /** Sets up an animation with the image as the first frame. */
    function animate(item: GalleryItem) {
        setSource(item);
        setKind("video");
        setPrompt(item.prompt);
        setOpen(null);
        if (!settings?.canVideo) {
            toast.warning("No video provider has a key yet.", {
                detail: "Add one under Image & video in settings.",
            });
        }
    }

    async function enlarge(item: GalleryItem) {
        setSource(item);
        setOpen(null);
        await generate({ upscale: true });
    }

    /** Loads a result's exact parameters back into the form. */
    function reuse(item: GalleryItem) {
        setPrompt(item.prompt);
        setModel(item.model);
        setSeedText(item.seed === null ? "" : String(item.seed));
        setKind(item.kind);
        setSource(null);

        const params = readParams(item);
        const found = SIZES.findIndex(([, w, h]) => `${w}x${h}` === params.size);
        if (found >= 0) setSizeIndex(found);
        setNegative(params.negative ?? "");

        setOpen(null);
        if (item.seed === null) {
            toast.info("Settings loaded.", {
                detail: "No seed was recorded, so this will not repeat exactly.",
            });
        } else {
            toast.success("Settings loaded — same seed, same result.");
        }
    }

    function readParams(item: GalleryItem): Record<string, string> {
        try {
            return JSON.parse(item.params);
        } catch {
            return {};
        }
    }

    async function remove(item: GalleryItem) {
        const confirmed = await ask({
            title: "Delete this result?",
            body: `"${item.prompt}" — the file is removed from disk and cannot be recovered.`,
            confirmLabel: "Delete",
            danger: true,
        });
        if (!confirmed) return;

        try {
            await api.deleteGalleryItem(item.id);
            setItems((prev) => prev.filter((i) => i.id !== item.id));
            if (openRef.current?.id === item.id) setOpen(null);
            if (sourceRef.current?.id === item.id) setSource(null);
            void loadSettings();
            toast.success("Deleted.");
        } catch (e) {
            toast.failure("Could not delete that.", e);
        }
    }

    async function toggleFavourite(item: GalleryItem) {
        const next = !item.favourite;
        try {
            await api.favouriteGalleryItem(item.id, next);
            setItems((prev) => prev.map((i) => (i.id === item.id ? { ...i, favourite: next } : i)));
            setOpen((prev) => (prev?.id === item.id ? { ...prev, favourite: next } : prev));
        } catch (e) {
            toast.failure("Could not star that.", e);
        }
    }

    async function clearAll() {
        const confirmed = await ask({
            title: "Clear the gallery?",
            body: "Everything goes except the results you starred. The files are removed from disk and cannot be recovered.",
            confirmLabel: "Clear",
            danger: true,
        });
        if (!confirmed) return;

        try {
            const freed = await api.clearGallery(true);
            toast.success(`Freed ${bytes(freed)}.`);
            await refresh();
        } catch (e) {
            toast.failure("Could not clear the gallery.", e);
        }
    }

    function bytes(n: number): string {
        if (n < 1024) return `${n} B`;
        if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
        if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
        return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
    }

    function src(item: GalleryItem): string {
        return convertFileSrc(item.path);
    }

    function when(seconds: number): string {
        return new Date(seconds * 1000).toLocaleString();
    }

    return (
        <div className="canvas">
            <aside className="canvas-controls">
                <div className="kinds" role="group" aria-label="What to generate">
                    <button
                        className={kind === "image" ? "active" : ""}
                        onClick={() => setKind("image")}
                    >
                        Image
                    </button>
                    <button
                        className={kind === "video" ? "active" : ""}
                        disabled={!settings?.canVideo}
                        title={settings?.canVideo ? "" : "Needs a video provider key"}
                        onClick={() => setKind("video")}
                    >
                        Video
                    </button>
                </div>

                {source ? (
                    <div className="source">
                        <img src={src(source)} alt="" />
                        <div className="source-text">
                            <span>Continuing from #{source.id}</span>
                            <button onClick={() => setSource(null)}>Drop</button>
                        </div>
                    </div>
                ) : null}

                <textarea
                    value={prompt}
                    onChange={(e) => setPrompt(e.target.value)}
                    className="prompt"
                    rows={5}
                    placeholder="What should it look like?"
                    aria-label="Prompt"
                    onKeyDown={(e) => {
                        if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
                            e.preventDefault();
                            if (canGo) void generate();
                        }
                    }}
                ></textarea>

                <input
                    value={negative}
                    onChange={(e) => setNegative(e.target.value)}
                    className="field"
                    placeholder="What to avoid (optional)"
                    aria-label="What to avoid"
                />

                <div className="canvas-row">
                    <label htmlFor="canvas-size">Size</label>
                    <select
                        id="canvas-size"
                        value={sizeIndex}
                        onChange={(e) => setSizeIndex(Number(e.target.value))}
                    >
                        {SIZES.map(([label, w, h], i) => (
                            <option key={label} value={i}>
                                {label} · {w}×{h}
                            </option>
                        ))}
                    </select>
                </div>

                {kind === "image" ? (
                    <div className="canvas-row">
                        <label htmlFor="canvas-count">How many</label>
                        <input
                            id="canvas-count"
                            type="number"
                            min={1}
                            max={8}
                            value={count}
                            onChange={(e) => setCount(Number(e.target.value))}
                        />
                    </div>
                ) : (
                    <div className="canvas-row">
                        <label htmlFor="canvas-duration">Seconds</label>
                        <input
                            id="canvas-duration"
                            type="number"
                            min={1}
                            max={60}
                            value={duration}
                            onChange={(e) => setDuration(Number(e.target.value))}
                        />
                    </div>
                )}

                <div className="canvas-row">
                    <label htmlFor="canvas-seed">Seed</label>
                    <input
                        id="canvas-seed"
                        value={seedText}
                        onChange={(e) => setSeedText(e.target.value)}
                        placeholder="Random"
                        title="Leave empty to let the provider choose. The one it used is saved with the result."
                    />
                </div>

                {source && kind === "image" ? (
                    <div className="canvas-row">
                        <label htmlFor="canvas-strength">Drift</label>
                        <input
                            id="canvas-strength"
                            type="range"
                            min={0}
                            max={1}
                            step={0.05}
                            value={strength}
                            onChange={(e) => setStrength(Number(e.target.value))}
                        />
                        <span className="value">{strength.toFixed(2)}</span>
                    </div>
                ) : null}

                <div className="canvas-row">
                    <label htmlFor="canvas-model">Model</label>
                    <input
                        id="canvas-model"
                        value={model}
                        onChange={(e) => setModel(e.target.value)}
                        placeholder="Provider default"
                    />
                </div>

                <button className="primary canvas-go" disabled={!canGo} onClick={() => generate()}>
                    {busy ? (
                        <>
                            <span className="canvas-spinner" aria-hidden="true"></span>
                            Working… {elapsed}s
                        </>
                    ) : (
                        <>
                            Generate
                            <kbd>Ctrl ⏎</kbd>
                        </>
                    )}
                </button>

                {settings && !settings.canImage ? (
                    <p className="canvas-warn">
                        <Icon name="warning" size={14} />
                        No image provider has a key. Add one under Image &amp; video in
                        settings — OpenAI, Stability, Replicate, or your own endpoint.
                    </p>
                ) : null}

                <div className="usage">
                    <span>{settings?.items ?? 0} results · {bytes(settings?.bytes ?? 0)}</span>
                    <div className="usage-actions">
                        <button onClick={() => openFolder()}>Open folder</button>
                        <button className="danger" onClick={clearAll}>Clear</button>
                    </div>
                </div>
            </aside>

            <main className="grid-pane">
                {!loaded ? (
                    <div className="grid" aria-hidden="true">
                        {Array.from({ length: 8 }, (_, i) => (
                            <div key={i} className="tile skeleton"></div>
                        ))}
                    </div>
                ) : items.length === 0 ? (
                    <div className="canvas-state">
                        <Icon name="canvas" size={26} />
                        <p className="canvas-state-title">Nothing generated yet</p>
                        <p className="canvas-state-body">
                            Describe a picture on the left and press Generate. Results
                            stay here with the seed and settings that made them, so you
                            can come back and make the same one again.
                        </p>
                    </div>
                ) : (
                    <div className="grid">
                        {items.map((item) => (
                            <button key={item.id} className="tile" onClick={() => setOpen(item)}>
                                {item.kind === "video" ? (
                                    <>
                                        {/* Muted and loopable: a grid of talking videos is
                                             unusable. */}
                                        <video
                                            src={src(item)}
                                            muted
                                            loop
                                            playsInline
                                            preload="metadata"
                                        ></video>
                                        <span className="badge">Video</span>
                                    </>
                                ) : (
                                    <img src={src(item)} alt={item.prompt} loading="lazy" />
                                )}
                                {item.favourite ? (
                                    <span className="star" title="Starred">★</span>
                                ) : null}
                                <span className="caption">{item.prompt}</span>
                            </button>
                        ))}
                    </div>
                )}
            </main>

            {open
                ? (() => {
                      const current = open;
                      const params = readParams(current);
                      return (
                          <Modal
                              size="xl"
                              title={`#${current.id}`}
                              description={current.prompt}
                              onClose={() => setOpen(null)}
                              footer={
                                  <>
                                      <button className="danger" onClick={() => remove(current)}>
                                          <Icon name="trash" size={14} />
                                          Delete
                                      </button>
                                      <span className="spacer"></span>
                                      <button onClick={() => toggleFavourite(current)}>
                                          {current.favourite ? "Unstar" : "Star"}
                                      </button>
                                      {current.kind === "image" ? (
                                          <>
                                              <button
                                                  disabled={!settings?.canUpscale || busy}
                                                  title={
                                                      settings?.canUpscale
                                                          ? "Four times the size, same picture"
                                                          : "Needs a Stability or Replicate key"
                                                  }
                                                  onClick={() => enlarge(current)}
                                              >
                                                  Enlarge
                                              </button>
                                              <button
                                                  disabled={!settings?.canVideo}
                                                  title={settings?.canVideo ? "" : "Needs a video provider key"}
                                                  onClick={() => animate(current)}
                                              >
                                                  Animate
                                              </button>
                                              <button onClick={() => variationOf(current)}>Variation</button>
                                          </>
                                      ) : null}
                                      <button className="primary" onClick={() => reuse(current)}>
                                          Same settings
                                      </button>
                                  </>
                              }
                          >
                              <div className="canvas-detail">
                                  <div className="preview">
                                      {current.kind === "video" ? (
                                          // No caption track exists: this video was generated
                                          // seconds ago and has no dialogue to caption.
                                          <video src={src(current)} controls loop></video>
                                      ) : (
                                          <img src={src(current)} alt={current.prompt} />
                                      )}
                                  </div>

                                  <dl className="facts">
                                      <dt>Provider</dt>
                                      <dd>{current.provider}</dd>
                                      <dt>Model</dt>
                                      <dd>{current.model || "default"}</dd>
                                      <dt>Seed</dt>
                                      <dd className={current.seed === null ? "missing" : ""}>
                                          {current.seed ?? "Not reported — cannot be repeated exactly"}
                                      </dd>
                                      <dt>Size</dt>
                                      <dd>{current.width}×{current.height}</dd>
                                      <dt>File</dt>
                                      <dd>{bytes(current.bytes)}</dd>
                                      {params.negative ? (
                                          <>
                                              <dt>Avoided</dt>
                                              <dd>{params.negative}</dd>
                                          </>
                                      ) : null}
                                      {current.parentId !== null ? (
                                          <>
                                              <dt>From</dt>
                                              <dd>#{current.parentId}</dd>
                                          </>
                                      ) : null}
                                      <dt>Made</dt>
                                      <dd>{when(current.createdAt)}</dd>
                                  </dl>
                              </div>
                          </Modal>
                      );
                  })()
                : null}
        </div>
    );
}
