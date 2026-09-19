/**
 * The same suite the Svelte store passed, run against the React one.
 *
 * Kept as a near-copy on purpose: it is the evidence that moving off runes
 * did not change behaviour. The store now announces writes through a proxy,
 * and the six places that mutated an array in place had to be rewritten to
 * reassign -- exactly the kind of change that looks harmless and breaks the
 * feed. These tests cover those paths.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";

// The store talks to Tauri, which does not exist under node. Only the calls
// the tests below actually reach are stubbed; anything else throwing would be
// a useful signal rather than a gap.
vi.mock("../../lib/api", () => ({
    api: {
        send: vi.fn(async () => {}),
        status: vi.fn(async () => null),
    },
    on: vi.fn(async () => () => {}),
}));

vi.mock("./toast", () => ({
    toast: { failure: vi.fn(), success: vi.fn() },
}));

vi.mock("./confirm", () => ({ ask: vi.fn(async () => true) }));

const { ChatStore } = await import("./chat");

const { observable, Signal } = await import("./reactive");

let chat: InstanceType<typeof ChatStore>;
let signal: InstanceType<typeof Signal>;
beforeEach(() => {
    // Through the proxy, not beside it: a test on the bare class would pass
    // while the app -- which only ever sees the wrapped one -- was broken.
    signal = new Signal();
    chat = observable(new ChatStore(), signal);
});

/** Sends without going near the network, so history fills up the real way. */
async function sendAll(...lines: string[]) {
    for (const line of lines) await chat.send(line);
}

describe("shell-style history", () => {
    it("walks back through what was sent, newest first", async () => {
        await sendAll("bir", "iki", "üç");

        expect(chat.recallOlder()).toBe(true);
        expect(chat.input).toBe("üç");

        chat.recallOlder();
        expect(chat.input).toBe("iki");

        chat.recallOlder();
        expect(chat.input).toBe("bir");
    });

    it("stops at the oldest instead of wrapping round", async () => {
        await sendAll("bir", "iki");

        chat.recallOlder();
        chat.recallOlder();
        expect(chat.input).toBe("bir");

        // Still handled -- the key must not fall through and move the caret
        // out of the recalled line -- but the position does not change.
        expect(chat.recallOlder()).toBe(true);
        expect(chat.input).toBe("bir");
    });

    it("comes back down the way it went up", async () => {
        await sendAll("bir", "iki", "üç");

        chat.recallOlder();
        chat.recallOlder();
        expect(chat.input).toBe("iki");

        chat.recallNewer();
        expect(chat.input).toBe("üç");
    });

    /**
     * The half-typed line is the thing people lose in a badly built history,
     * and they lose it silently -- one arrow key and the sentence is gone.
     */
    it("gives back the draft that was interrupted", async () => {
        await sendAll("gönderilmiş");

        chat.input = "yarım kalan cümle";
        chat.recallOlder();
        expect(chat.input).toBe("gönderilmiş");

        chat.recallNewer();
        expect(chat.input).toBe("yarım kalan cümle");
    });

    it("does nothing when there is no history to walk", () => {
        expect(chat.recallOlder()).toBe(false);
        expect(chat.recallNewer()).toBe(false);
        expect(chat.input).toBe("");
    });

    it("declines ↓ when it never went up, so the key keeps its normal job", async () => {
        await sendAll("bir");
        expect(chat.recallNewer()).toBe(false);
    });

    it("does not stack repeats of the same line", async () => {
        await sendAll("devam", "devam", "devam");
        expect(chat.history).toEqual(["devam"]);
    });

    it("keeps repeats that are not consecutive", async () => {
        await sendAll("a", "b", "a");
        expect(chat.history).toEqual(["a", "b", "a"]);
    });

    it("holds twenty lines and drops the oldest", async () => {
        await sendAll(...Array.from({ length: 25 }, (_, i) => `satır ${i}`));

        expect(chat.history.length).toBe(20);
        expect(chat.history[0]).toBe("satır 5");
        expect(chat.history[19]).toBe("satır 24");
    });

    it("starts from the newest again after sending", async () => {
        await sendAll("bir", "iki");

        chat.recallOlder();
        chat.recallOlder();
        expect(chat.input).toBe("bir");

        await chat.send("üç");

        chat.recallOlder();
        expect(chat.input).toBe("üç");
    });

    it("leaves browsing when typing resumes", async () => {
        await sendAll("bir", "iki");

        chat.recallOlder();
        expect(chat.historyAt).not.toBeNull();

        chat.leaveHistory();
        expect(chat.historyAt).toBeNull();

        // Having left, ↓ has nothing to return to.
        expect(chat.recallNewer()).toBe(false);
    });

    it("ignores blank sends", async () => {
        await sendAll("", "   ", "\n");
        expect(chat.history).toEqual([]);
    });

    it("stores the trimmed line, which is what was sent", async () => {
        await sendAll("  boşluklu  ");
        expect(chat.history).toEqual(["boşluklu"]);
    });
});

describe("the feed", () => {
    it("records what the user said", async () => {
        await sendAll("merhaba");
        expect(chat.messages.at(-1)?.text).toBe("merhaba");
        expect(chat.messages.at(-1)?.speaker).toBe("user");
    });

    it("finds the last user line past replies and tool output", async () => {
        await sendAll("ilk soru");
        chat.add("assistant", "cevap");
        chat.add("tool", "list_directory — 3 dosya");

        expect(chat.lastUserText()).toBe("ilk soru");
    });

    it("returns an empty string when the user has said nothing", () => {
        chat.add("system", "No API key yet.");
        expect(chat.lastUserText()).toBe("");
    });
});

describe("streaming a reply", () => {
    it("grows one bubble rather than adding a message per chunk", () => {
        chat.appendDelta("Mer");
        chat.appendDelta("haba");
        chat.appendDelta(" dünya");

        const assistant = chat.messages.filter((m) => m.speaker === "assistant");
        expect(assistant.length).toBe(1);
        expect(assistant[0].text).toBe("Merhaba dünya");
    });

    it("starts a new bubble after the previous one is closed", () => {
        chat.appendDelta("ilk");
        chat.finishStreaming();
        chat.appendDelta("ikinci");

        const assistant = chat.messages.filter((m) => m.speaker === "assistant");
        expect(assistant.map((m) => m.text)).toEqual(["ilk", "ikinci"]);
    });

    it("marks the bubble finished", () => {
        chat.appendDelta("bitti");
        chat.finishStreaming();
        expect(chat.messages.at(-1)?.streaming).toBe(false);
    });

    /**
     * A turn that only called tools produces no text of its own, and an empty
     * bubble above the tool line reads as the model having said nothing when
     * in fact it acted.
     */
    it("drops a bubble that never got any text", () => {
        chat.appendDelta("");
        chat.finishStreaming();
        expect(chat.messages.filter((m) => m.speaker === "assistant")).toEqual([]);
    });

    it("does nothing when there is no open bubble", () => {
        expect(() => chat.finishStreaming()).not.toThrow();
        chat.add("user", "soru");
        chat.finishStreaming();
        expect(chat.messages.at(-1)?.text).toBe("soru");
    });

    /** A tool line has to land between the model's text and what it says next. */
    it("does not fold text after a tool line into the earlier bubble", () => {
        chat.appendDelta("Bakıyorum");
        chat.finishStreaming();
        chat.add("tool", "list_directory — 3 dosya");
        chat.appendDelta("Üç dosya var");

        expect(chat.messages.map((m) => m.speaker)).toEqual([
            "assistant",
            "tool",
            "assistant",
        ]);
    });
});
