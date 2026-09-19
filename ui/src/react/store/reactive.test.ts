/**
 * The reactive seam, tested on its own.
 *
 * This is the piece every rewritten component depends on, so it is worth
 * proving before anything is built on it -- particularly the parts that are
 * easy to get subtly wrong: a version that does not change, a listener that
 * unsubscribes mid-notify, and a write of an identical value that should not
 * wake the whole tree.
 */

import { describe, expect, it, vi } from "vitest";
import { observable, Signal } from "./reactive";

describe("Signal", () => {
    it("hands every subscriber the same version until something changes", () => {
        const s = new Signal();
        const first = s.snapshot();
        expect(s.snapshot()).toBe(first);

        s.notify();
        expect(s.snapshot()).not.toBe(first);
    });

    it("calls listeners on notify", () => {
        const s = new Signal();
        const seen = vi.fn();
        s.subscribe(seen);

        s.notify();
        s.notify();

        expect(seen).toHaveBeenCalledTimes(2);
    });

    it("stops calling a listener once it unsubscribes", () => {
        const s = new Signal();
        const seen = vi.fn();
        const off = s.subscribe(seen);

        s.notify();
        off();
        s.notify();

        expect(seen).toHaveBeenCalledTimes(1);
    });

    // React unsubscribes while a notify is in flight when a component
    // unmounts in response to that very change. Iterating the live set would
    // throw or skip a listener.
    it("survives a listener that unsubscribes during the notify", () => {
        const s = new Signal();
        const order: string[] = [];
        const off = s.subscribe(() => {
            order.push("first");
            off();
        });
        s.subscribe(() => order.push("second"));

        expect(() => s.notify()).not.toThrow();
        expect(order).toEqual(["first", "second"]);
    });
});

describe("observable", () => {
    it("notifies when a field is written", () => {
        const s = new Signal();
        const store = observable({ count: 0 }, s);
        const seen = vi.fn();
        s.subscribe(seen);

        store.count = 1;

        expect(seen).toHaveBeenCalledTimes(1);
        expect(store.count).toBe(1);
    });

    // A status poll runs every second and usually returns the same numbers.
    // Waking the tree for those would make the whole app re-render on a timer.
    it("stays quiet when the value written is the one already there", () => {
        const s = new Signal();
        const store = observable({ count: 7 }, s);
        const seen = vi.fn();
        s.subscribe(seen);

        store.count = 7;

        expect(seen).not.toHaveBeenCalled();
    });

    it("notifies on delete", () => {
        const s = new Signal();
        const store = observable<{ gone?: string }>({ gone: "x" }, s);
        const seen = vi.fn();
        s.subscribe(seen);

        delete store.gone;

        expect(seen).toHaveBeenCalledTimes(1);
        expect(store.gone).toBeUndefined();
    });

    it("reads through to the underlying object", () => {
        const s = new Signal();
        const store = observable({ nested: { deep: 1 } }, s);
        expect(store.nested.deep).toBe(1);
    });

    // Reassignment is the contract, matching what `$state` required of arrays
    // crossing a component boundary. Worth pinning so nobody "fixes" a
    // component by pushing instead.
    it("does not observe mutation inside a nested value", () => {
        const s = new Signal();
        const store = observable({ list: [1] }, s);
        const seen = vi.fn();
        s.subscribe(seen);

        store.list.push(2);
        expect(seen).not.toHaveBeenCalled();

        store.list = [...store.list, 3];
        expect(seen).toHaveBeenCalledTimes(1);
    });
});
