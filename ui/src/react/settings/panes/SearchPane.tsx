/**
 * Web search: the chain, the keys, and the custom endpoint.
 *
 * Three things were wrong with the version this replaces. The key entry was
 * an unlabelled select-plus-password that a layout bug squeezed to a few
 * pixels, so it read as absent. The custom endpoint showed seven equal
 * inputs when only the address is required. And `custom` was marked
 * "no key — skipped" whenever its key was unset, even though the chain runs
 * it on its address alone.
 */

import { useEffect, useState } from "react";
import { api, type SearchSettings } from "../../../lib/api";
import { toast } from "../../store/toast";
import CustomEndpoint, { type OptionalField } from "../CustomEndpoint";
import KeyInput from "../KeyInput";
import ProviderChain, { type ChainItem } from "../ProviderChain";
import Section from "../Section";

interface Props {
    search: SearchSettings | null;
    /** Re-reads settings after a change that the backend may normalise. */
    reload: () => Promise<void>;
}

/** The placeholder the backend insists the address contains. */
const QUERY_TOKEN = "{query}";

/** Providers that take a key. `duckduckgo` is absent: it needs none, and
    that is what makes it the floor of the chain. */
const KEYED = [
    { id: "tavily", note: "written answer + sources, free tier" },
    { id: "brave", note: "independent index, 2000 queries/month free" },
    { id: "custom", note: "only fills {key} in your header" },
];

const BLURB: Record<string, string> = {
    tavily: "written answer with sources",
    brave: "independent index",
    custom: "your own JSON endpoint",
    duckduckgo: "no key needed — works out of the box, rate-limited when busy",
};

const OPTIONAL: OptionalField[] = [
    { key: "resultsPath", label: "Results path", fallback: "results" },
    { key: "titleKey", label: "Title field", fallback: "title" },
    { key: "urlKey", label: "URL field", fallback: "url" },
    { key: "snippetKey", label: "Snippet field", fallback: "content" },
    { key: "headerName", label: "Auth header", hint: "optional" },
    {
        key: "headerValue",
        label: "Header value",
        hint: "{key} is filled from the stored key",
    },
];

export default function SearchPane({ search, reload }: Props) {
    const [testing, setTesting] = useState(false);
    const [result, setResult] = useState<{ ok: boolean; detail: string } | null>(null);

    const items: ChainItem[] = (search?.order ?? []).map((id) => ({
        id,
        // duckduckgo is always ready; the rest depend on what the
        // backend reported as configured.
        ready: id === "duckduckgo" || (search?.configured ?? []).includes(id),
        note:
            id === "duckduckgo" || (search?.configured ?? []).includes(id)
                ? (BLURB[id] ?? "")
                : id === "custom"
                  ? "no address — skipped"
                  : "no key — skipped",
    }));

    /** The optional custom fields, as the flat map CustomEndpoint wants. */
    const [values, setValues] = useState<Record<string, string>>({});
    const [url, setUrl] = useState("");

    // Seeded from the backend each time the settings object is replaced --
    // unlike SpotifyPane and SteamPane, this pane mirrors the prop rather
    // than seeding a draft once, because there is no free-typing risk here:
    // the fields commit on blur, and `reload()` only replaces `search` after
    // a save this same pane triggered.
    useEffect(() => {
        if (!search) return;
        setUrl(search.custom.url);
        setValues({
            resultsPath: search.custom.resultsPath,
            titleKey: search.custom.titleKey,
            urlKey: search.custom.urlKey,
            snippetKey: search.custom.snippetKey,
            headerName: search.custom.headerName,
            headerValue: search.custom.headerValue,
        });
    }, [search]);

    async function saveCustom() {
        try {
            await api.setCustomSearch({
                url,
                resultsPath: values.resultsPath ?? "",
                titleKey: values.titleKey ?? "",
                urlKey: values.urlKey ?? "",
                snippetKey: values.snippetKey ?? "",
                headerName: values.headerName ?? "",
                headerValue: values.headerValue ?? "",
            });
            toast.success("Endpoint saved.");
            await reload();
        } catch (e) {
            toast.failure("Could not save the endpoint.", e);
        }
    }

    async function saveKey(provider: string, key: string) {
        await api.setSearchKey(provider, key);
        toast.success("Key saved, encrypted.");
        await reload();
    }

    async function reorder(order: string[]) {
        try {
            await api.setSearchOrder(order);
            await reload();
        } catch (e) {
            toast.failure("Could not save the order.", e);
        }
    }

    async function test() {
        setTesting(true);
        try {
            setResult(await api.testConnection("search"));
        } catch (e) {
            setResult({ ok: false, detail: String(e) });
        } finally {
            setTesting(false);
        }
    }

    if (!search) return <h2>Web search</h2>;

    return (
        <>
            <h2>Web search</h2>

            <Section
                title="Order"
                blurb="Tried top to bottom until one answers. A provider that is not set up is skipped, so the chain always ends somewhere that works."
            >
                <ProviderChain items={items} onreorder={reorder} />
            </Section>

            <Section
                title="API keys"
                blurb="Stored encrypted on this machine and never shown again once saved."
            >
                <KeyInput
                    providers={[...KEYED, { id: "duckduckgo", keyless: true, note: BLURB.duckduckgo }]}
                    configured={search.configured}
                    onsave={saveKey}
                />
            </Section>

            <Section title="Test">
                <div className="actions">
                    <button onClick={() => void test()} disabled={testing}>
                        {testing ? "searching…" : "run a search"}
                    </button>
                    {result && (
                        <span className={result.ok ? "result" : "result bad"}>
                            {result.ok ? "✓" : "✕"} {result.detail}
                        </span>
                    )}
                </div>
            </Section>

            <Section title="Custom endpoint">
                <CustomEndpoint
                    url={url}
                    onUrlChange={setUrl}
                    values={values}
                    onValuesChange={setValues}
                    requires={QUERY_TOKEN}
                    urlPlaceholder={`https://…?q=${QUERY_TOKEN}&format=json`}
                    blurb="Any JSON search API — a self-hosted SearxNG, a company service. Only the address is required; everything below has a default."
                    fields={OPTIONAL}
                    onchange={saveCustom}
                />
            </Section>
        </>
    );
}
