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
    it("matches on keywords, not just the visible label", () => {
        const byKeyword = filterCategories("tavily").map((c) => c.id);
        expect(byKeyword).toEqual(["search"]);

        const byLabel = filterCategories("obsidian").map((c) => c.id);
        expect(byLabel).toEqual(["obsidian"]);
    });

    it("ignores case and surrounding space", () => {
        expect(filterCategories("  BRAVE ").map((c) => c.id)).toEqual(["search"]);
    });

    // An empty heading reads as "nothing on this screen" rather than
    // "nothing in this group", so a group with no matches is dropped.
    it("drops a group once nothing in it matches", () => {
        const groups = filterGroups("tavily");
        expect(groups).toHaveLength(1);
        expect(groups[0].categories.map((c) => c.id)).toEqual(["search"]);
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

    it("finds the voice engines by name", () => {
        for (const engine of ["elevenlabs", "kokoro", "edge", "sapi"]) {
            expect(filterCategories(engine).map((c) => c.id)).toContain("voice");
        }
    });
});
