/**
 * Image & video: two chains, the keys, defaults, custom endpoint, storage.
 *
 * The chain and key markup here used to be a copy of the web-search pane's,
 * and the two had already drifted: one decided `custom` was configured by
 * its key, the other by its address. Both now use the same components, so
 * they cannot disagree again.
 */

import { useEffect, useState } from "react";
import { api, type CanvasSettings } from "../../../lib/api";
import { openFolder } from "../../actions";
import { ask } from "../../store/confirm";
import { toast } from "../../store/toast";
import CustomEndpoint, { type OptionalField } from "../CustomEndpoint";
import Field from "../Field";
import KeyInput from "../KeyInput";
import ProviderChain, { type ChainItem } from "../ProviderChain";
import Section from "../Section";

interface Props {
    canvas: CanvasSettings | null;
    reload: () => Promise<void>;
}

const KEYED = [
    { id: "openai", note: "gpt-image-1 — uses your chat key if you have one" },
    { id: "stability", note: "reports the seed it used, so results repeat" },
    { id: "replicate", note: "the only one here that also does video" },
    { id: "custom", note: "only fills {key} in your header" },
];

const BLURB: Record<string, string> = {
    openai: "gpt-image-1",
    stability: "repeatable, reports its seed",
    replicate: "images and video",
    custom: "your own OpenAI-compatible endpoint",
};

const OPTIONAL: OptionalField[] = [
    { key: "customModel", label: "Model", hint: "optional — the endpoint's own default" },
    { key: "customHeaderName", label: "Auth header", hint: "optional" },
    {
        key: "customHeaderValue",
        label: "Header value",
        hint: "{key} is filled from the stored key",
    },
];

function bytes(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
    if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
    return `${(n / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

function chain(order: string[], ready: string[]): ChainItem[] {
    return order.map((id) => ({
        id,
        ready: ready.includes(id),
        note: ready.includes(id)
            ? (BLURB[id] ?? "")
            : id === "custom"
              ? "no address — skipped"
              : "no key — skipped",
    }));
}

export default function CanvasPane({ canvas, reload }: Props) {
    const [testing, setTesting] = useState(false);
    const [result, setResult] = useState<{ ok: boolean; detail: string } | null>(null);

    // Local drafts for the two model fields, so they can commit on blur the
    // same way the Svelte version's `bind:value` + `onchange` did rather
    // than saving on every keystroke.
    const [imageModel, setImageModel] = useState(canvas?.imageModel ?? "");
    const [videoModel, setVideoModel] = useState(canvas?.videoModel ?? "");

    const imageChain = chain(canvas?.imageOrder ?? [], canvas?.configured ?? []);
    const videoChain = chain(canvas?.videoOrder ?? [], canvas?.configured ?? []);

    const [url, setUrl] = useState("");
    const [values, setValues] = useState<Record<string, string>>({});

    useEffect(() => {
        if (!canvas) return;
        setUrl(canvas.customUrl);
        setValues({
            customModel: canvas.customModel,
            customHeaderName: canvas.customHeaderName,
            customHeaderValue: canvas.customHeaderValue,
        });
        setImageModel(canvas.imageModel);
        setVideoModel(canvas.videoModel);
    }, [canvas]);

    /** Every defaults write goes through one call, so partial saves cannot
        leave the two models and the endpoint disagreeing. */
    async function saveDefaults(overrides: { imageModel?: string; videoModel?: string } = {}) {
        if (!canvas) return;
        try {
            await api.setCanvasDefaults({
                imageModel: overrides.imageModel ?? imageModel,
                videoModel: overrides.videoModel ?? videoModel,
                size: canvas.size,
                count: canvas.count,
                customUrl: url,
                customModel: values.customModel ?? "",
                customHeaderName: values.customHeaderName ?? "",
                customHeaderValue: values.customHeaderValue ?? "",
            });
            toast.success("Saved.");
            await reload();
        } catch (e) {
            toast.failure("Could not save.", e);
        }
    }

    async function saveKey(provider: string, key: string) {
        await api.setCanvasKey(provider, key);
        toast.success("Key saved, encrypted.");
        await reload();
    }

    async function reorder(kind: "image" | "video", order: string[]) {
        try {
            await api.setCanvasOrder(kind, order);
            await reload();
        } catch (e) {
            toast.failure("Could not save the order.", e);
        }
    }

    async function test() {
        setTesting(true);
        try {
            setResult(await api.testConnection("canvas"));
        } catch (e) {
            setResult({ ok: false, detail: String(e) });
        } finally {
            setTesting(false);
        }
    }

    async function clear() {
        const yes = await ask({
            title: "Delete generated files?",
            body: "Everything in the gallery goes except the results you starred. The files are removed from disk and cannot be recovered.",
            confirmLabel: "Delete",
            danger: true,
        });
        if (!yes) return;
        try {
            const freed = await api.clearGallery(true);
            toast.success(`Freed ${bytes(freed)}.`);
            await reload();
        } catch (e) {
            toast.failure("Could not clear the gallery.", e);
        }
    }

    if (!canvas) return <h2>Image & video</h2>;

    return (
        <>
            <h2>Image & video</h2>

            <Section
                title="Image providers"
                blurb="Tried top to bottom until one answers. Results land in the canvas interface, not in the conversation."
            >
                <ProviderChain items={imageChain} onreorder={(o) => reorder("image", o)} />
            </Section>

            <Section title="Video providers" blurb="A shorter list: most of these only make images.">
                <ProviderChain items={videoChain} onreorder={(o) => reorder("video", o)} />
            </Section>

            <Section
                title="API keys"
                blurb="Stored encrypted on this machine and never shown again once saved."
            >
                <KeyInput providers={KEYED} configured={canvas.configured} onsave={saveKey} />
            </Section>

            <Section
                title="Test"
                blurb="Reports what is configured rather than generating something — a test that charged you a few cents per press would not be one."
            >
                <div className="actions">
                    <button onClick={() => void test()} disabled={testing}>
                        {testing ? "checking…" : "test"}
                    </button>
                    {result && (
                        <span className={result.ok ? "result" : "result bad"}>
                            {result.ok ? "✓" : "✕"} {result.detail}
                        </span>
                    )}
                </div>
            </Section>

            <Section
                title="Models"
                blurb="Empty means the provider's own default, so a new model upstream needs no update here."
            >
                <Field label="Image model" hint="optional">
                    <input
                        value={imageModel}
                        onChange={(e) => setImageModel(e.target.value)}
                        onBlur={() => void saveDefaults({ imageModel })}
                        placeholder="the provider's default"
                        spellCheck={false}
                    />
                </Field>
                <Field label="Video model" hint="optional">
                    <input
                        value={videoModel}
                        onChange={(e) => setVideoModel(e.target.value)}
                        onBlur={() => void saveDefaults({ videoModel })}
                        placeholder="the provider's default"
                        spellCheck={false}
                    />
                </Field>
            </Section>

            <Section title="Custom endpoint">
                <CustomEndpoint
                    url={url}
                    onUrlChange={setUrl}
                    values={values}
                    onValuesChange={setValues}
                    urlPlaceholder="http://localhost:8080/v1/images/generations"
                    blurb="Must speak the OpenAI /images/generations shape. Only the address is required."
                    fields={OPTIONAL}
                    onchange={() => void saveDefaults()}
                />
            </Section>

            <Section title="Storage">
                <div className="storage">
                    <span className="hint">
                        {canvas.items} results · {bytes(canvas.bytes)} on disk
                    </span>
                    <span className="buttons">
                        <button className="tiny" onClick={() => openFolder()}>
                            folder
                        </button>
                        <button className="tiny" onClick={() => void clear()}>
                            clear
                        </button>
                    </span>
                </div>
                <p className="hint">Clearing spares anything you starred.</p>
            </Section>
        </>
    );
}
