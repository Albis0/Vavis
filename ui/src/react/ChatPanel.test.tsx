/**
 * The composer's keyboard handling, and the feed it drives.
 *
 * The store itself is proven in `store/chat.test.ts`; this is about the DOM
 * wiring on top of it -- that Enter reaches `submit`, that Shift+Enter does
 * not, and that the arrow keys only take over at the edges of the text,
 * exactly where the Svelte version drew that line.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../lib/api", () => ({
    api: {
        send: vi.fn(async () => {}),
        status: vi.fn(async () => null),
        loadHistory: vi.fn(async () => []),
    },
    on: vi.fn(async () => () => {}),
}));

vi.mock("./store/toast", () => ({
    toast: { failure: vi.fn(), success: vi.fn(), warning: vi.fn(), info: vi.fn() },
}));

vi.mock("./store/confirm", () => ({ ask: vi.fn(async () => true) }));

const { chat } = await import("./store/chat");
const ChatPanel = (await import("./ChatPanel")).default;

beforeEach(() => {
    chat.messages = [];
    chat.input = "";
    chat.history = [];
    chat.historyAt = null;
    chat.status = null;
    chat.runningTool = null;
});

afterEach(() => {
    vi.restoreAllMocks();
});

function textarea() {
    return screen.getByPlaceholderText("Message Vavis…") as HTMLTextAreaElement;
}

describe("the composer", () => {
    it("sends on Enter", async () => {
        const user = userEvent.setup();
        render(<ChatPanel onClose={() => {}} />);

        await user.type(textarea(), "merhaba{Enter}");

        expect(chat.messages.at(-1)?.text).toBe("merhaba");
        expect(chat.messages.at(-1)?.speaker).toBe("user");
    });

    it("writes a newline on Shift+Enter instead of sending", async () => {
        const user = userEvent.setup();
        render(<ChatPanel onClose={() => {}} />);

        await user.type(textarea(), "birinci satır{Shift>}{Enter}{/Shift}ikinci satır");

        expect(chat.messages.length).toBe(0);
        expect(textarea().value).toBe("birinci satır\nikinci satır");
    });

    it("does not send an empty or whitespace-only line", async () => {
        const user = userEvent.setup();
        render(<ChatPanel onClose={() => {}} />);

        await user.type(textarea(), "   {Enter}");

        expect(chat.messages.length).toBe(0);
    });
});

describe("history recall with the arrow keys", () => {
    async function seedHistory() {
        chat.history = ["ilk", "ikinci", "üçüncü"];
    }

    it("recalls the previous line with ArrowUp when the caret is at the start", async () => {
        await seedHistory();
        const user = userEvent.setup();
        render(<ChatPanel onClose={() => {}} />);

        const el = textarea();
        el.focus();
        el.setSelectionRange(0, 0);
        await user.keyboard("{ArrowUp}");

        expect(chat.input).toBe("üçüncü");
    });

    it("steps further back on a second ArrowUp", async () => {
        await seedHistory();
        const user = userEvent.setup();
        render(<ChatPanel onClose={() => {}} />);

        const el = textarea();
        el.focus();
        el.setSelectionRange(0, 0);
        await user.keyboard("{ArrowUp}");
        // The caret is moved to the end after a recall, so the next ArrowUp
        // at the end of the text would normally be a plain cursor move --
        // history keeps taking it because it is still ArrowUp with nothing
        // typed since. Re-home the caret to the start as a real ArrowUp
        // press from that position would.
        el.setSelectionRange(0, 0);
        await user.keyboard("{ArrowUp}");

        expect(chat.input).toBe("ikinci");
    });

    it("comes back down with ArrowDown once history was entered", async () => {
        await seedHistory();
        const user = userEvent.setup();
        render(<ChatPanel onClose={() => {}} />);

        const el = textarea();
        el.focus();
        el.setSelectionRange(0, 0);
        await user.keyboard("{ArrowUp}");
        el.setSelectionRange(0, 0);
        await user.keyboard("{ArrowUp}");

        // The caret sits at the end of the recalled text, which is where
        // ArrowDown must be pressed for it to be treated as "at the edge".
        el.setSelectionRange(el.value.length, el.value.length);
        await user.keyboard("{ArrowDown}");

        expect(chat.input).toBe("üçüncü");
    });

    it("lets ArrowUp move the caret normally when not at the start of the text", async () => {
        await seedHistory();
        const user = userEvent.setup();
        render(<ChatPanel onClose={() => {}} />);

        const el = textarea();
        await user.type(el, "satır bir\nsatır iki");
        // Caret is at the very end, on the second line -- not the start of
        // the whole text -- so ArrowUp must be left to move the cursor
        // rather than being taken for history.
        await user.keyboard("{ArrowUp}");

        expect(chat.input).toBe("satır bir\nsatır iki");
    });

    it("leaves the arrow alone when a modifier is held", async () => {
        await seedHistory();
        const user = userEvent.setup();
        render(<ChatPanel onClose={() => {}} />);

        const el = textarea();
        el.focus();
        el.setSelectionRange(0, 0);
        await user.keyboard("{Shift>}{ArrowUp}{/Shift}");

        expect(chat.input).toBe("");
    });

    it("editing a recalled line leaves history browsing", async () => {
        await seedHistory();
        const user = userEvent.setup();
        render(<ChatPanel onClose={() => {}} />);

        const el = textarea();
        el.focus();
        el.setSelectionRange(0, 0);
        await user.keyboard("{ArrowUp}");
        expect(chat.historyAt).not.toBeNull();

        await user.type(el, "!");
        expect(chat.historyAt).toBeNull();
    });
});
