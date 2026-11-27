import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../lib/api", () => ({
    api: {
        listConversations: vi.fn(async () => [
            { id: 1, title: "Tatil planı", updatedAt: 0, messageCount: 4, current: true },
            { id: 2, title: "", updatedAt: 0, messageCount: 0, current: false },
        ]),
        openConversation: vi.fn(async () => [{ role: "user", content: "eski soru" }]),
        status: vi.fn(async () => null),
        newConversation: vi.fn(async () => 3),
        renameConversation: vi.fn(async () => {}),
        deleteConversation: vi.fn(async () => [{ role: "assistant", content: "kalan cevap" }]),
    },
}));
vi.mock("./store/confirm", () => ({ ask: vi.fn(async () => true) }));
vi.mock("./store/toast", () => ({
    toast: { success: vi.fn(), failure: vi.fn(), info: vi.fn() },
}));

const { api } = await import("../lib/api");
const { chat } = await import("./store/chat");
const { default: ConversationList, when } = await import("./ConversationList");

describe("when", () => {
    const now = 1_000_000_000_000;
    it("counts minutes and hours, then says yesterday", () => {
        expect(when(now / 1000 - 30, now)).toBe("now");
        expect(when(now / 1000 - 5 * 60, now)).toBe("5 min");
        expect(when(now / 1000 - 3 * 3600, now)).toBe("3 h");
        expect(when(now / 1000 - 30 * 3600, now)).toBe("yesterday");
    });
});

describe("ConversationList", () => {
    it("lists conversations, naming the untitled one", async () => {
        render(<ConversationList onClose={() => {}} />);
        expect(await screen.findByText("Tatil planı")).toBeInTheDocument();
        expect(screen.getByText("Untitled")).toBeInTheDocument();
    });

    it("opens another conversation and puts its messages on screen", async () => {
        const onClose = vi.fn();
        const user = userEvent.setup();
        render(<ConversationList onClose={onClose} />);
        await user.click(await screen.findByText("Untitled"));
        expect(api.openConversation).toHaveBeenCalledWith(2);
        expect(chat.messages.map((m) => m.text)).toContain("eski soru");
        expect(onClose).toHaveBeenCalled();
    });

    it("does not reopen the one already on screen", async () => {
        vi.mocked(api.openConversation).mockClear();
        const user = userEvent.setup();
        render(<ConversationList onClose={() => {}} />);
        await user.click(await screen.findByText("Tatil planı"));
        expect(api.openConversation).not.toHaveBeenCalled();
    });

    it("renames a conversation without opening it or closing the list", async () => {
        const onClose = vi.fn();
        const user = userEvent.setup();
        render(<ConversationList onClose={onClose} />);
        const row = (await screen.findByText("Untitled")).closest(".conv-row") as HTMLElement;
        await user.click(within(row).getByRole("button", { name: "Rename conversation" }));
        await user.type(screen.getByRole("textbox", { name: "Conversation title" }), "Yeni ad{Enter}");
        expect(api.renameConversation).toHaveBeenCalledWith(2, "Yeni ad");
        expect(onClose).not.toHaveBeenCalled();
    });

    it("displays the replacement after deleting the active conversation", async () => {
        vi.mocked(api.openConversation).mockClear();
        const user = userEvent.setup();
        render(<ConversationList onClose={() => {}} />);
        const row = (await screen.findByText("Tatil planı")).closest(".conv-row") as HTMLElement;
        await user.click(within(row).getByRole("button", { name: "Delete" }));
        await waitFor(() => expect(chat.messages.map((m) => m.text)).toEqual(["kalan cevap"]));
        expect(api.deleteConversation).toHaveBeenCalledWith(1);
        expect(api.openConversation).not.toHaveBeenCalled();
    });
});
