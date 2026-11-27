/**
 * An automation that fires mid-reply waits its turn instead of vanishing.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

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

beforeEach(() => {
    busy = false;
    handlers.clear();
    vi.clearAllMocks();
});

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

    it("retries a queued prompt when a phone reply ends without a chat event", async () => {
        vi.useFakeTimers();
        const store = new ChatStore();
        try {
            busy = true;
            await store.start();
            handlers.get("automation")!({ id: 1, prompt: "first", trigger: "x" });

            busy = false;
            await vi.advanceTimersByTimeAsync(1000);
            expect(api.send).toHaveBeenCalledExactlyOnceWith("first", false);
            expect(store.queued).toEqual([]);
        } finally {
            store.stop();
            vi.useRealTimers();
        }
    });

    it("keeps a queued prompt and the draft if the backend is already busy", async () => {
        const store = new ChatStore();
        await store.start();
        try {
            store.input = "unfinished draft";
            store.codeContext = true;
            vi.mocked(api.send).mockImplementationOnce(async () => {
                busy = true;
                throw "a reply is already in progress";
            });

            handlers.get("automation")!({ id: 1, prompt: "first", trigger: "x" });
            await vi.waitFor(() => expect(store.status?.busy).toBe(true));
            expect(store.input).toBe("unfinished draft");
            expect(store.codeContext).toBe(true);
            expect(store.queued).toEqual(["first"]);
            expect(store.messages.some((m) => m.speaker === "user")).toBe(false);

            busy = false;
            handlers.get("chat:done")!({ text: "done" });
            await vi.waitFor(() => expect(store.queued).toEqual([]));
            expect(vi.mocked(api.send).mock.calls).toEqual([["first", false], ["first", false]]);
        } finally {
            store.stop();
        }
    });
});
