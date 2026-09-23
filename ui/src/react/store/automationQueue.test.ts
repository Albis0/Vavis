/**
 * An automation that fires mid-reply waits its turn instead of vanishing.
 */
import { describe, expect, it, vi } from "vitest";

const handlers = new Map<string, (payload: unknown) => void>();
let busy = false;

vi.mock("../../lib/api", () => ({
    api: {
        loadHistory: vi.fn(async () => []),
        status: vi.fn(async () => ({ keys: [], providers: [], busy })),
        micLevel: vi.fn(async () => 0),
        send: vi.fn(async () => {}),
    },
    on: vi.fn(async (event: string, fn: (payload: unknown) => void) => {
        handlers.set(event, fn);
        return () => {};
    }),
}));
vi.mock("./toast", () => ({
    toast: { failure: vi.fn(), success: vi.fn(), info: vi.fn() },
}));
vi.mock("./confirm", () => ({ ask: vi.fn(async () => true) }));

const { api } = await import("../../lib/api");
const { ChatStore } = await import("./chat");

describe("automations during a reply", () => {
    it("are queued and sent when the reply finishes", async () => {
        const store = new ChatStore();
        await store.start();

        busy = true;
        await store.refresh();
        handlers.get("automation")!({ id: 1, prompt: "tara: setup.exe", trigger: "x" });
        expect(api.send).not.toHaveBeenCalled();
        expect(store.queued).toEqual(["tara: setup.exe"]);

        busy = false;
        handlers.get("chat:done")!({ text: "bitti" });
        await vi.waitFor(() => expect(api.send).toHaveBeenCalledWith("tara: setup.exe", false));
        expect(store.queued).toEqual([]);
        store.stop();
    });
});
