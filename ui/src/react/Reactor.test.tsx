/**
 * Reactor is a thin wrapper around a canvas-drawn instrument face
 * (`../lib/reactor.ts`), which needs a real 2D canvas context. jsdom has no
 * such thing -- `getContext("2d")` comes back `null`, so `createReactor`
 * throws exactly the way it would on a real machine that is out of canvas
 * memory. That is the one path safe to exercise here: it proves the
 * component catches the failure, falls back to the CSS ring instead of
 * crashing, and tears down cleanly on unmount without ever touching WebGL.
 */
import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import Reactor from "./Reactor";

describe("Reactor", () => {
    it("falls back to the CSS ring when no 2D context is available, and unmounts cleanly", () => {
        const { container, unmount } = render(<Reactor mode="idle" />);

        expect(container.querySelector(".reactor-fallback")).toBeTruthy();
        expect(container.querySelector(".reactor-fallback")?.getAttribute("data-state")).toBe(
            "idle",
        );

        expect(() => unmount()).not.toThrow();
    });
});
