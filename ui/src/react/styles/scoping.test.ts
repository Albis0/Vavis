/**
 * No two component stylesheets may define the same bare class.
 *
 * Svelte scoped every `<style>` block to its own component, so two files
 * could both call something `.panel` and never meet. Plain CSS has no such
 * boundary: once the blocks became ordinary stylesheets, whichever imported
 * last won.
 *
 * That was not theoretical. `.panel` was ChatPanel's docked column
 * (`position: relative`), Modal's floating window (`position: fixed`) and
 * CouncilView's seat, all at once -- and the settings window opened halfway
 * off the bottom of the screen because it inherited the chat panel's
 * positioning. Every test passed while it was broken.
 *
 * A first version of this check compared property values and reported it
 * clean, because it only looked for the SAME property set differently. It
 * missed CodeView's `.section` adding `justify-content: space-between` to
 * the settings column, which has no `justify-content` of its own to
 * disagree with. So the rule here is the blunt one: a bare class belongs to
 * exactly one stylesheet. Anything shared belongs in `styles.css`, which is
 * global on purpose.
 */

import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

const DIR = path.resolve(__dirname);

/** Bare single-class selectors (`.thing {`), which are the ones that collide. */
function bareClasses(css: string): Set<string> {
    const stripped = css.replace(/\/\*[\s\S]*?\*\//g, "");
    const found = new Set<string>();
    for (const match of stripped.matchAll(/(?:^|\})\s*([^{}@]+?)\s*\{/g)) {
        for (const part of match[1].split(",")) {
            const bare = /^\.([A-Za-z0-9_-]+)\s*$/.exec(part.trim());
            if (bare) found.add(bare[1]);
        }
    }
    return found;
}

describe("component stylesheets", () => {
    it("never define the same bare class in two files", () => {
        const owners = new Map<string, string[]>();

        for (const file of fs.readdirSync(DIR).filter((f) => f.endsWith(".css"))) {
            const css = fs.readFileSync(path.join(DIR, file), "utf8");
            for (const cls of bareClasses(css)) {
                owners.set(cls, [...(owners.get(cls) ?? []), file]);
            }
        }

        const shared = [...owners.entries()]
            .filter(([, files]) => files.length > 1)
            .map(([cls, files]) => `.${cls} -> ${files.sort().join(", ")}`)
            .sort();

        expect(shared).toEqual([]);
    });

    it("finds the classes it is meant to be checking", () => {
        // Guards the parser itself: a regex that silently matched nothing
        // would make the check above pass for the wrong reason.
        const settings = fs.readFileSync(path.join(DIR, "settings.css"), "utf8");
        const classes = bareClasses(settings);

        expect(classes.size).toBeGreaterThan(10);
        expect(classes).toContain("pane");
    });
});
