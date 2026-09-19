/**
 * Test environment for the React components.
 *
 * `jest-dom` adds the matchers the component tests read best with
 * (`toBeVisible`, `toHaveTextContent`), and the cleanup below unmounts
 * anything a test rendered -- without it a component's effects keep running
 * into the next test, which shows up as a passing suite that fails when the
 * file order changes.
 */
// The ESM build by path, not the "/vitest" subpath.
//
// That subpath resolves to a CJS bundle here, which registers its matchers
// against a `require`d copy of vitest's expect -- not the ESM one the tests
// actually run against -- so every matcher came back "Invalid Chai property".
// Importing the matchers and extending explicitly keeps it on one module.
import * as matchers from "@testing-library/jest-dom/matchers";
import { expect } from "vitest";

expect.extend(matchers);
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

afterEach(cleanup);

// jsdom implements no layout, so the scroll API the feed uses is simply
// absent and calling it throws. Stubbed rather than guarded in the component:
// the guard would be dead weight in the real WebView, and a component that
// checks whether the DOM supports scrolling is describing the test
// environment rather than the app.
if (!Element.prototype.scrollTo) {
    Element.prototype.scrollTo = function scrollTo() {};
}
if (!Element.prototype.scrollIntoView) {
    Element.prototype.scrollIntoView = function scrollIntoView() {};
}

// Same reason: jsdom has no layout engine, so it ships neither observer.
// The components use them to re-clamp a floating box against its own size,
// which is behaviour there is nothing to observe in a test.
class NoopObserver {
    observe() {}
    unobserve() {}
    disconnect() {}
    takeRecords() {
        return [];
    }
}
globalThis.ResizeObserver ??= NoopObserver as unknown as typeof ResizeObserver;
globalThis.IntersectionObserver ??=
    NoopObserver as unknown as typeof IntersectionObserver;

