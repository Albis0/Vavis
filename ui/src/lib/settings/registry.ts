/**
 * What the settings screen contains, and how it is grouped.
 *
 * Kept out of the component so the filtering can be tested without rendering
 * anything, and so adding a pane is one entry here plus one file under
 * `panes/` rather than an edit in the middle of a thousand-line template.
 *
 * The grouping exists because fourteen flat entries is a list you read rather
 * than scan. Four headings turn "where would that be?" into one guess.
 */

export interface Category {
    id: string;
    label: string;
    icon: string;
    /** Extra words the search box should match — what people actually type. */
    keywords: string;
}

export interface Group {
    /** Shown above the group. Hidden while a search is narrowing the list. */
    title: string;
    categories: Category[];
}

export const GROUPS: Group[] = [
    {
        title: "Basics",
        categories: [
            {
                id: "general",
                label: "General",
                icon: "◈",
                keywords: "name language font window interface theme size assistant",
            },
            // Model and API keys were two categories, and the split was
            // arbitrary: picking a provider marked "no key" sent you to a
            // second screen to do the one thing that pick implied.
            {
                id: "provider",
                label: "Model & keys",
                icon: "◉",
                keywords:
                    "provider model llm groq openai anthropic claude gemini local ollama temperature key token secret credential api auth",
            },
            // Every key in the app, on one screen. The provider cards under
            // "Model & keys" keep their own key button -- there the key is
            // part of choosing a provider -- but someone who has just
            // collected four keys and wants to paste them in should not have
            // to know which ability each one belongs to first.
            {
                id: "keys",
                label: "API keys",
                icon: "⚿",
                keywords:
                    "key api token secret credential auth groq openai anthropic claude gemini mistral deepseek xai grok nvidia tavily brave stability replicate virustotal virus malware scan security paste add replace",
            },
            {
                id: "voice",
                label: "Voice",
                icon: "◍",
                keywords:
                    "microphone speech tts stt wake word listen speak elevenlabs kokoro edge sapi voice engine",
            },
        ],
    },
    {
        title: "Abilities",
        categories: [
            {
                id: "search",
                label: "Web search",
                icon: "⌕",
                keywords: "tavily brave duckduckgo searx internet browse search key endpoint",
            },
            {
                id: "canvas",
                label: "Image & video",
                icon: "◧",
                keywords:
                    "canvas image video generate openai stability replicate dalle gallery disk key endpoint",
            },
            {
                id: "memory",
                label: "Memory",
                icon: "◎",
                keywords: "facts remember forget knowledge history",
            },
            {
                id: "tools",
                label: "Tools",
                icon: "⚙",
                keywords: "tool registry risk domain permission approval authority",
            },
        ],
    },
    {
        title: "Connections",
        categories: [
            {
                id: "phone",
                label: "Phone",
                icon: "✆",
                keywords: "telegram bot phone mobile remote pair notify message away",
            },
            {
                id: "obsidian",
                label: "Obsidian",
                icon: "❒",
                keywords: "vault notes markdown wiki",
            },
            {
                id: "spotify",
                label: "Spotify",
                icon: "♪",
                keywords: "music playback player oauth",
            },
            {
                id: "steam",
                label: "Steam",
                icon: "▤",
                keywords: "games library achievements wishlist",
            },
            {
                id: "mcp",
                label: "MCP servers",
                icon: "⁘",
                keywords: "model context protocol custom server stdio http tools",
            },
        ],
    },
    {
        title: "System",
        categories: [
            {
                id: "shortcuts",
                label: "Shortcuts",
                icon: "⌘",
                keywords: "keyboard keys hotkey binding",
            },
            {
                id: "data",
                label: "Data",
                icon: "▦",
                keywords: "folder disk database storage conversation clear export",
            },
            {
                id: "updates",
                label: "Updates",
                icon: "⇡",
                keywords: "update version release upgrade download new latest changelog",
            },
        ],
    },
];

/** Every category, flattened, in display order. */
export const CATEGORIES: Category[] = GROUPS.flatMap((g) => g.categories);

/**
 * The groups a query leaves standing, with non-matching entries removed.
 *
 * A group with nothing left is dropped rather than shown empty — an empty
 * heading reads as "nothing here" about the whole screen rather than about
 * that one group.
 */
export function filterGroups(query: string): Group[] {
    const q = query.trim().toLowerCase();
    if (!q) return GROUPS;

    return GROUPS.map((g) => ({
        title: g.title,
        categories: g.categories.filter((c) =>
            `${c.label} ${c.keywords}`.toLowerCase().includes(q),
        ),
    })).filter((g) => g.categories.length > 0);
}

/** The categories a query leaves standing, flattened. */
export function filterCategories(query: string): Category[] {
    return filterGroups(query).flatMap((g) => g.categories);
}
