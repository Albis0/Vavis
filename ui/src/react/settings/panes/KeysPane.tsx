/**
 * Every API key in the app, on one screen.
 *
 * They used to sit in three places: the chat providers inside their own
 * cards under Model & keys, the search keys under Web search, the image
 * keys under Image & video. Each pane was coherent on its own, and the
 * result was still that "where do I put a key" had three answers and you
 * had to already know which ability a provider belonged to before you
 * could find its row.
 *
 * The keys themselves genuinely are three different stores with three
 * different commands behind them -- a chat key is not a search key -- so
 * this screen does not merge them. It groups them, under the ability they
 * unlock, and sends each save to the command that owns it.
 *
 * Two things deliberately stay where they are:
 *
 *   - The provider cards under Model & keys keep their own key button. The
 *     key is part of picking a provider there; sending someone to another
 *     screen to finish the thing they just started would be the same bug
 *     this screen exists to fix, pointed the other way.
 *   - The custom endpoints keep their addresses under their own abilities.
 *     An address is not a credential, and it only means something next to
 *     the field mapping it belongs to.
 *
 * Keys travel one way. Nothing here ever reads a stored key back; each
 * group is handed a list of ids that have one, and a saved key is replaced,
 * never revealed.
 */

import {
    api,
    type CanvasSettings,
    type SearchSettings,
    type Status,
    type VirusTotalSettings,
} from "../../../lib/api";
import { toast } from "../../store/toast";
import KeyInput, { type Provider } from "../KeyInput";
import Section from "../Section";

interface Props {
    status: Status | null;
    search: SearchSettings | null;
    canvas: CanvasSettings | null;
    virustotal: VirusTotalSettings | null;
    /** Re-reads settings so a saved key shows as stored straight away. */
    reload: () => Promise<void>;
}

/** Chat providers, as the model screen names them. `local` and
    `claude-code` are absent: one talks to a server on this machine, the
    other to a CLI that holds its own login. Neither has a key to store. */
const CHAT: Provider[] = [
    { id: "gemini", label: "gemini (google)", note: "free tier" },
    { id: "groq", note: "fast, generous free tier" },
    { id: "cerebras", note: "free tier, about a million tokens a day" },
    { id: "openrouter", note: "one key for hundreds of models, many free" },
    { id: "github", label: "github models", note: "free with a GitHub token (models: read)" },
    { id: "mistral", note: "free experiment plan" },
    { id: "nvidia", note: "free credits" },
    { id: "openai", note: "also used by image generation" },
    { id: "anthropic", label: "anthropic (claude api)", note: "pay per token — Claude Code uses your plan instead" },
    { id: "deepseek", note: "" },
    { id: "xai", label: "xai (grok)", note: "" },
    // Labelled for the same reason as the search and canvas `custom` rows
    // below: three rows with one bare name cannot be told apart.
    { id: "custom", label: "custom (chat)", note: "optional — only if your endpoint wants one" },
];

/** Search providers. `duckduckgo` is absent from the keyed list for a
    reason worth keeping visible: it is the floor of the chain precisely
    because it needs nothing. */
const SEARCH: Provider[] = [
    { id: "duckduckgo", keyless: true, note: "the fallback — always available" },
    { id: "tavily", note: "written answer + sources, free tier" },
    { id: "brave", note: "independent index, 2000 queries/month free" },
    // Labelled, not bare `custom`: there is a second `custom` further down
    // under image generation, and on a screen that lists every key at once
    // two identical names are two rows you cannot tell apart. The id sent
    // to the backend is still `custom`.
    { id: "custom", label: "custom (search)", note: "only fills {key} in your header" },
];

/** Image and video providers. */
const CANVAS: Provider[] = [
    // `openai` appears under Chat too, and this row is a separate slot
    // rather than the same key shown twice -- but it is optional: the
    // backend falls back to the chat key when this one is unset, and only
    // prefers this one when it is set. The note says so, because a row
    // reading "not set" beside a provider that demonstrably works is the
    // kind of thing that sends someone hunting for a bug that is not there.
    {
        id: "openai",
        label: "openai (images)",
        note: "gpt-image-1 — uses your chat key unless you set a separate one",
    },
    { id: "stability", note: "reports the seed it used, so results repeat" },
    { id: "replicate", note: "the only one here that also does video" },
    { id: "custom", label: "custom (images)", note: "only fills {key} in your header" },
];

/** File and link reputation. One provider, so the list is one row -- but it
    belongs on this screen rather than in a corner of its own, because
    "where do I put a key" has to keep having one answer. */
const SECURITY: Provider[] = [
    {
        id: "virustotal",
        note: "free account is enough — 4 lookups a minute, 500 a day",
    },
];

export default function KeysPane({ status, search, canvas, virustotal, reload }: Props) {
    /** One saver per store. They look alike and are not interchangeable:
        the same provider id means a different credential in each. */
    async function save(
        kind: "chat" | "search" | "canvas" | "security",
        provider: string,
        key: string,
    ): Promise<void> {
        try {
            if (kind === "chat") await api.setKey(provider, key);
            else if (kind === "search") await api.setSearchKey(provider, key);
            else if (kind === "canvas") await api.setCanvasKey(provider, key);
            else {
                // This one checks the key against the service and says what
                // came back, so a key with a stray character is caught here
                // rather than at the first scan, when nobody connects the two.
                const said = await api.setVirusTotalKey(key);
                await reload();
                toast.success(said);
                return;
            }
            await reload();
            toast.success("Key saved, encrypted.");
        } catch (e) {
            // Rethrown so the row keeps the box open rather than reporting
            // a save that did not happen.
            toast.failure(`Could not save the ${provider} key.`, e);
            throw e;
        }
    }

    return (
        <>
            <h2>API keys</h2>

            <Section
                title="Chat"
                blurb="The provider that answers you. Picking one lives under Model & keys — this is the same key, in the place you look when you are collecting keys rather than choosing a model."
            >
                <KeyInput
                    providers={CHAT}
                    configured={status?.keys ?? []}
                    onsave={(p, k) => save("chat", p, k)}
                />
            </Section>

            <Section
                title="Web search"
                blurb="Tried in the order set under Web search. One that has no key is skipped rather than failing the turn."
            >
                <KeyInput
                    providers={SEARCH}
                    configured={search?.configured ?? []}
                    onsave={(p, k) => save("search", p, k)}
                />
            </Section>

            <Section
                title="Image & video"
                blurb="A slot of its own for each, so drawing can bill a different account than talking. OpenAI is the exception worth knowing: leave it unset and it borrows your chat key."
            >
                <KeyInput
                    providers={CANVAS}
                    configured={canvas?.configured ?? []}
                    onsave={(p, k) => save("canvas", p, k)}
                />
            </Section>

            <Section
                title="File & link checks"
                blurb="Lets you ask whether a file or a link is known to be malicious. Files are never uploaded — only a fingerprint calculated on this machine is sent, so nothing leaves your disk."
            >
                <KeyInput
                    providers={SECURITY}
                    configured={virustotal?.hasKey ? ["virustotal"] : []}
                    onsave={(p, k) => save("security", p, k)}
                />
            </Section>

            <Section title="Where these are kept">
                <p className="blurb">
                    Not in the settings file. Keys live in a file of their own,
                    encrypted by Windows against your account, so a copy taken to
                    another machine or another user cannot be read. Nothing here
                    reads one back either: a row can say a key is saved and offer to
                    replace it, which is all it can do.
                </p>
            </Section>
        </>
    );
}
