import { describe, expect, it } from "vitest";
import { CATEGORIES, GROUPS, filterCategories, filterGroups } from "./registry";

describe("settings registry", () => {
    it("gives every category a unique id", () => {
        const ids = CATEGORIES.map((c) => c.id);
        expect(new Set(ids).size).toBe(ids.length);
    });

    it("keeps every category inside exactly one group", () => {
        const grouped = GROUPS.flatMap((g) => g.categories).length;
        expect(grouped).toBe(CATEGORIES.length);
    });

    it("shows everything when nothing is typed", () => {
        expect(filterGroups("")).toEqual(GROUPS);
        expect(filterGroups("   ")).toEqual(GROUPS);
    });

    // The search box is how people find a setting whose category name they
    // cannot guess, so it has to match the words they actually type.
    //
    // `toContain`, not `toEqual`: a provider name legitimately belongs to
    // more than one screen now. Tavily is a search provider *and* a key on
    // the keys screen, and both are places someone typing "tavily" could
    // sensibly want. Asserting an exact list here would fail whenever a
    // setting is reachable from two directions, which is a feature.
    it("matches on keywords, not just the visible label", () => {
        const byKeyword = filterCategories("tavily").map((c) => c.id);
        expect(byKeyword).toContain("search");

        const byLabel = filterCategories("obsidian").map((c) => c.id);
        expect(byLabel).toEqual(["obsidian"]);
    });

    it("ignores case and surrounding space", () => {
        expect(filterCategories("  BRAVE ").map((c) => c.id)).toContain("search");
        // The space and the case are the point: the same query, spelled the
        // way someone actually types it, finds the same thing.
        expect(filterCategories("  BRAVE ").map((c) => c.id)).toEqual(
            filterCategories("brave").map((c) => c.id),
        );
    });

    // An empty heading reads as "nothing on this screen" rather than
    // "nothing in this group", so a group with no matches is dropped.
    it("drops a group once nothing in it matches", () => {
        const groups = filterGroups("tavily");
        // Every group still standing has something in it, and the one that
        // owns web search is among them.
        expect(groups.every((g) => g.categories.length > 0)).toBe(true);
        expect(groups.flatMap((g) => g.categories.map((c) => c.id))).toContain("search");
        expect(groups.length).toBeLessThan(GROUPS.length);
    });

    it("returns nothing when nothing matches", () => {
        expect(filterGroups("zzzzzz")).toEqual([]);
        expect(filterCategories("zzzzzz")).toEqual([]);
    });

    // Someone looking for where to paste a key should land somewhere useful
    // whichever word they reach for.
    it.each(["key", "api", "secret", "token"])("finds a key field by %s", (word) => {
        expect(filterCategories(word).length).toBeGreaterThan(0);
    });

    // The keys screen exists so that "where do I paste this" has one answer
    // that does not depend on knowing which ability the provider serves. A
    // provider whose name is not on it is a provider you can only reach by
    // already knowing where it lives.
    it.each([
        "groq",
        "openai",
        "anthropic",
        "gemini",
        "mistral",
        "deepseek",
        "xai",
        "nvidia",
        "tavily",
        "brave",
        "stability",
        "replicate",
    ])("reaches the keys screen by the provider name %s", (provider) => {
        expect(filterCategories(provider).map((c) => c.id)).toContain("keys");
    });

    it("finds the voice engines by name", () => {
        for (const engine of ["elevenlabs", "kokoro", "edge", "sapi"]) {
            expect(filterCategories(engine).map((c) => c.id)).toContain("voice");
        }
    });
});
