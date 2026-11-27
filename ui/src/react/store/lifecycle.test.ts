/**
 * The stores clean up after themselves.
 *
 * `main.tsx` deliberately does not use StrictMode, whose double-mount would
 * run `start()` twice against stores that own pollers and Tauri event
 * subscriptions. That is a real trade, so the property StrictMode would have
 * checked is checked here instead: after `stop()`, nothing the store started
 * is still running.
 *
 * Without this, a leaked interval shows up as the app getting slower the
 * longer it is open -- which is exactly the complaint that started the last
 * round of performance work, and took measuring rather than reading to find.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const listeners: Array<() => void> = [];

vi.mock("../../lib/api", () => ({
    api: {
        loadHistory: vi.fn(async () => []),
        status: vi.fn(async () => ({ keys: ["groq"] })),
        micLevel: vi.fn(async () => 0),
        send: vi.fn(async () => {}),
    },
    // Every subscription hands back its unsubscribe, and the store is
    // expected to call all of them.
    on: vi.fn(async (_event: string, _fn: unknown) => {
        const off = vi.fn();
        listeners.push(off);
        return off;
    }),
}));

vi.mock("./toast", () => ({
    toast: { failure: vi.fn(), success: vi.fn(), info: vi.fn() },
}));
vi.mock("./confirm", () => ({ ask: vi.fn(async () => true) }));

const { ChatStore } = await import("./chat");
const { api, on } = await import("../../lib/api");

beforeEach(() => {
    vi.useFakeTimers();
    listeners.length = 0;
});

afterEach(() => {
    vi.useRealTimers();
});

describe("ChatStore lifecycle", () => {
    it("cleans up subscriptions when stopped while startup is still pending", async () => {
        const chat = new ChatStore();
        const late = vi.fn();
        let resolveListener!: (off: () => void) => void;
        vi.mocked(on).mockImplementationOnce(() => new Promise((resolve) => {
            resolveListener = resolve;
        }));

        const start = chat.start();
        await vi.advanceTimersByTimeAsync(0);
        chat.stop();
        resolveListener(late);
        await start;

        expect(late).toHaveBeenCalledOnce();
        for (const off of listeners) expect(off).toHaveBeenCalledOnce();
        expect(vi.getTimerCount()).toBe(0);
    });

    it("subscribes to replies even if saved history cannot be loaded", async () => {
        const chat = new ChatStore();
        vi.mocked(api.loadHistory).mockRejectedValueOnce(new Error("unreadable history"));
        await chat.start();
        try {
            expect(listeners.length).toBeGreaterThan(0);
            expect(vi.getTimerCount()).toBeGreaterThan(0);
        } finally {
            chat.stop();
        }
    });

    it("leaves no timer running after stop", async () => {
        const chat = new ChatStore();
        await chat.start();

        expect(vi.getTimerCount()).toBeGreaterThan(0);

        chat.stop();

        expect(vi.getTimerCount()).toBe(0);
    });

    it("unsubscribes from every event it subscribed to", async () => {
        const chat = new ChatStore();
        await chat.start();

        const subscribed = listeners.length;
        expect(subscribed).toBeGreaterThan(0);

        chat.stop();

        for (const off of listeners) expect(off).toHaveBeenCalled();
    });

    // The case StrictMode would have caught: a second start without a stop
    // must not leave the first round's timers orphaned. Two stores are used
    // because that is the shape a double-mount produces.
    it("a second store started and stopped leaves nothing behind", async () => {
        const first = new ChatStore();
        await first.start();
        const second = new ChatStore();
        await second.start();

        first.stop();
        second.stop();

        expect(vi.getTimerCount()).toBe(0);
    });
});
