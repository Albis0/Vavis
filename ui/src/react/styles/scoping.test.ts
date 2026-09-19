/**
 * A component stylesheet may not reach into another component.
 *
 * Svelte scoped every `<style>` block to its own markup, so two files could
 * both call something `.panel` and never meet. Plain CSS has no boundary:
 * once those blocks became ordinary stylesheets, whichever imported last
 * won, silently.
 *
 * This check has been wrong twice, and each time the app was visibly broken
 * while every test passed:
 *
 *   1. First it compared property VALUES and called seventeen collisions
 *      harmless. Wrong: CodeView's `.section` added `justify-content` to the
 *      settings column, which had none of its own to disagree with.
 *   2. Then it only looked at bare single-class rules. Wrong again: the
 *      reactor's `.fallback` is a 220px spinning circle, and the settings
 *      `.setting .fallback` label inherited it -- the user saw a rotating
 *      ring drawn over the provider list.
 *
 * So the rule here is the blunt one, stated positively: a rule whose
 * selector is not anchored to something a single component owns can hit
 * markup anywhere, and therefore each such class must belong to exactly one
 * stylesheet. Anything genuinely shared belongs in `styles.css`, which is
 * global on purpose.
 */

import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

const DIR = path.resolve(__dirname);

interface Usage {
    /** Files whose selector for this class has no ancestor qualifier. */
    loose: Set<string>;
    /** Every file mentioning the class anywhere in a selector. */
    all: Set<string>;
}

function collectUsage(): Map<string, Usage> {
    const usage = new Map<string, Usage>();
    const note = (cls: string, file: string, loose: boolean) => {
        const entry = usage.get(cls) ?? { loose: new Set(), all: new Set() };
        entry.all.add(file);
        if (loose) entry.loose.add(file);
        usage.set(cls, entry);
    };

    for (const file of fs.readdirSync(DIR).filter((f) => f.endsWith(".css"))) {
        const css = fs
            .readFileSync(path.join(DIR, file), "utf8")
            .replace(/\/\*[\s\S]*?\*\//g, "");

        for (const rule of css.matchAll(/(?:^|\})\s*([^{}@]+?)\s*\{/g)) {
            for (const raw of rule[1].split(",")) {
                const selector = raw.trim();
                if (!selector) continue;

                // A descendant or sibling combinator means the rule is
                // anchored: `.setting .fallback` cannot escape a `.setting`.
                // One compound selector (`.code-entry.active`) is anchored
                // too -- its first class is the owner.
                const anchored = /[ >+~]/.test(selector);
                const classes = [...selector.matchAll(/\.([A-Za-z0-9_-]+)/g)].map(
                    (m) => m[1],
                );

                classes.forEach((cls, index) => {
                    // Only the FIRST class of an unanchored compound is the
                    // one doing the reaching; the rest narrow it.
                    note(cls, file, !anchored && index === 0);
                });
            }
        }
    }
    return usage;
}

describe("component stylesheets", () => {
    it("never let one component's rule reach another's markup", () => {
        const offenders: string[] = [];

        for (const [cls, { loose, all }] of collectUsage()) {
            if (loose.size === 0 || all.size < 2) continue;
            // Loose in one file and used in another, or loose in two at once.
            const elsewhere = [...all].filter((f) => !loose.has(f));
            if (elsewhere.length > 0 || loose.size > 1) {
                offenders.push(
                    `.${cls} — unanchored in ${[...loose].sort().join(", ")}; ` +
                        `also styled in ${[...all].sort().join(", ")}`,
                );
            }
        }

        expect(offenders.sort()).toEqual([]);
    });

    it("finds the classes it is meant to be checking", () => {
        // Guards the parser itself: a regex matching nothing would make the
        // check above pass for the wrong reason.
        const usage = collectUsage();

        expect(usage.size).toBeGreaterThan(50);
        expect(usage.has("pane")).toBe(true);
    });
});
