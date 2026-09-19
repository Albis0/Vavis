/**
 * Every class a stylesheet defines is used by some component.
 *
 * A rule nothing references is usually harmless dead weight. Here it is the
 * fingerprint of a specific bug: a rename that edited the stylesheet and
 * missed the markup, leaving the element with a class that now has no rule.
 *
 * That happened twice while the class collisions were being untangled, and
 * neither showed up in the typechecker or in 149 passing tests:
 *
 *   - `.content` became `.modal-content` in modal.css, but the markup wrote
 *     it inside a conditional (`bare ? "content" : "content padded"`) that
 *     the rename did not reach. The panel body lost its `flex: 1`, so the
 *     settings window stopped filling its own frame and left a 300px hole
 *     at the bottom with the reactor showing through it.
 *   - `.chip` became `.settings-chip`, and ToolsPane's risk filters kept the
 *     old name.
 *
 * Both are invisible to every other check: the class is still a valid
 * string, the component still renders, only the styling is silently gone.
 * A missing rule is the cheap thing to detect, so this looks for that.
 */

import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

const STYLES = path.resolve(__dirname);
const REACT = path.resolve(__dirname, "..");

/** Every `.tsx` under src/react, concatenated. */
function allMarkup(): string {
    const out: string[] = [];
    const walk = (dir: string) => {
        for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
            const full = path.join(dir, entry.name);
            if (entry.isDirectory()) walk(full);
            else if (entry.name.endsWith(".tsx")) out.push(fs.readFileSync(full, "utf8"));
        }
    };
    walk(REACT);
    return out.join("\n");
}

/** The leading class of every selector in a stylesheet. */
function definedClasses(css: string): Set<string> {
    const stripped = css.replace(/\/\*[\s\S]*?\*\//g, "");
    const found = new Set<string>();
    for (const rule of stripped.matchAll(/(?:^|\})\s*([^{}@]+?)\s*\{/g)) {
        for (const part of rule[1].split(",")) {
            const lead = /^\.([A-Za-z0-9_-]+)/.exec(part.trim());
            if (lead) found.add(lead[1]);
        }
    }
    return found;
}

describe("component stylesheets", () => {
    it("define no class that the markup never uses", () => {
        const markup = allMarkup();
        const orphans: string[] = [];

        for (const file of fs.readdirSync(STYLES).filter((f) => f.endsWith(".css"))) {
            const css = fs.readFileSync(path.join(STYLES, file), "utf8");
            for (const cls of definedClasses(css)) {
                const used = new RegExp(`(?<![A-Za-z0-9_-])${cls}(?![A-Za-z0-9_-])`);
                if (!used.test(markup)) orphans.push(`${file}: .${cls}`);
            }
        }

        expect(orphans.sort()).toEqual([]);
    });

    it("finds the classes it is meant to be checking", () => {
        // Guards the parser: a regex matching nothing would make the check
        // above pass for the wrong reason.
        const modal = fs.readFileSync(path.join(STYLES, "modal.css"), "utf8");
        const classes = definedClasses(modal);

        expect(classes.size).toBeGreaterThan(5);
        expect(classes).toContain("modal-content");
    });
});
