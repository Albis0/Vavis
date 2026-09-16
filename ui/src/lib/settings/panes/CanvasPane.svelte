<!--
    Image & video: two chains, the keys, defaults, custom endpoint, storage.

    The chain and key markup here used to be a copy of the web-search pane's,
    and the two had already drifted: one decided `custom` was configured by
    its key, the other by its address. Both now use the same components, so
    they cannot disagree again.
-->
<script lang="ts">
    import { api, type CanvasSettings } from "../../api";
    import { openFolder } from "../../actions";
    import { ask } from "../../confirm.svelte";
    import { toast } from "../../toast.svelte";
    import CustomEndpoint, { type OptionalField } from "../CustomEndpoint.svelte";
    import Field from "../Field.svelte";
    import KeyInput from "../KeyInput.svelte";
    import ProviderChain, { type ChainItem } from "../ProviderChain.svelte";
    import Section from "../Section.svelte";

    interface Props {
        canvas: CanvasSettings | null;
        reload: () => Promise<void>;
    }

    let { canvas, reload }: Props = $props();

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

    let testing = $state(false);
    let result = $state<{ ok: boolean; detail: string } | null>(null);

    function chain(order: string[]): ChainItem[] {
        const ready = canvas?.configured ?? [];
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

    const imageChain = $derived(chain(canvas?.imageOrder ?? []));
    const videoChain = $derived(chain(canvas?.videoOrder ?? []));

    let url = $state("");
    let values = $state<Record<string, string>>({});

    $effect(() => {
        if (!canvas) return;
        url = canvas.customUrl;
        values = {
            customModel: canvas.customModel,
            customHeaderName: canvas.customHeaderName,
            customHeaderValue: canvas.customHeaderValue,
        };
    });

    /** Every defaults write goes through one call, so partial saves cannot
        leave the two models and the endpoint disagreeing. */
    async function saveDefaults() {
        if (!canvas) return;
        try {
            await api.setCanvasDefaults({
                imageModel: canvas.imageModel,
                videoModel: canvas.videoModel,
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
        testing = true;
        try {
            result = await api.testConnection("canvas");
        } catch (e) {
            result = { ok: false, detail: String(e) };
        } finally {
            testing = false;
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

    function bytes(n: number): string {
        if (n < 1024) return `${n} B`;
        if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
        if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
        return `${(n / 1024 / 1024 / 1024).toFixed(1)} GB`;
    }
</script>

<h2>Image &amp; video</h2>

{#if canvas}
    <Section
        title="Image providers"
        blurb="Tried top to bottom until one answers. Results land in the canvas interface, not in the conversation."
    >
        <ProviderChain items={imageChain} onreorder={(o) => reorder("image", o)} />
    </Section>

    <Section
        title="Video providers"
        blurb="A shorter list: most of these only make images."
    >
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
        <div class="actions">
            <button onclick={test} disabled={testing}>
                {testing ? "checking…" : "test"}
            </button>
            {#if result}
                <span class="result" class:bad={!result.ok}>
                    {result.ok ? "✓" : "✕"}
                    {result.detail}
                </span>
            {/if}
        </div>
    </Section>

    <Section
        title="Models"
        blurb="Empty means the provider's own default, so a new model upstream needs no update here."
    >
        <Field label="Image model" hint="optional">
            <input
                bind:value={canvas.imageModel}
                onchange={saveDefaults}
                placeholder="the provider's default"
                spellcheck="false"
            />
        </Field>
        <Field label="Video model" hint="optional">
            <input
                bind:value={canvas.videoModel}
                onchange={saveDefaults}
                placeholder="the provider's default"
                spellcheck="false"
            />
        </Field>
    </Section>

    <Section title="Custom endpoint">
        <CustomEndpoint
            bind:url
            bind:values
            urlPlaceholder="http://localhost:8080/v1/images/generations"
            blurb="Must speak the OpenAI /images/generations shape. Only the address is required."
            fields={OPTIONAL}
            onchange={saveDefaults}
        />
    </Section>

    <Section title="Storage">
        <div class="storage">
            <span class="hint">
                {canvas.items} results · {bytes(canvas.bytes)} on disk
            </span>
            <span class="buttons">
                <button class="tiny" onclick={() => openFolder()}>folder</button>
                <button class="tiny" onclick={clear}>clear</button>
            </span>
        </div>
        <p class="hint">Clearing spares anything you starred.</p>
    </Section>
{/if}

<style>
    .actions,
    .storage {
        display: flex;
        align-items: center;
        gap: var(--sp-3);
        flex-wrap: wrap;
    }

    .storage {
        justify-content: space-between;
    }

    .buttons {
        display: flex;
        gap: var(--sp-1);
    }

    .hint {
        font-size: var(--text-xs);
        color: var(--text-muted);
        margin: 0;
    }

    .result {
        font-size: var(--text-xs);
        color: var(--ok, #9ece6a);
    }
    .result.bad {
        color: var(--danger, #f7768e);
    }
</style>
