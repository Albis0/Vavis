/**
 * The message variants: user bubble, assistant markdown, tool/error notes,
 * and the approval card with its inline decision buttons.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
    writeText: vi.fn(async () => {}),
}));

vi.mock("./store/toast", () => ({
    toast: { failure: vi.fn(), success: vi.fn(), warning: vi.fn(), info: vi.fn() },
}));

vi.mock("../lib/api", () => ({
    api: { send: vi.fn(async () => {}), status: vi.fn(async () => null) },
    on: vi.fn(async () => () => {}),
}));

vi.mock("./store/confirm", () => ({ ask: vi.fn(async () => true) }));

const { chat } = await import("./store/chat");
const Message = (await import("./Message")).default;

function baseMessage(overrides: Partial<Parameters<typeof Message>[0]["message"]> = {}) {
    return {
        id: 1,
        speaker: "user" as const,
        text: "merhaba",
        at: Date.now(),
        ...overrides,
    };
}

beforeEach(() => {
    chat.approval = null;
});

afterEach(() => {
    vi.restoreAllMocks();
});

describe("the user bubble", () => {
    it("shows the raw text, not rendered as markdown", () => {
        render(<Message message={baseMessage({ speaker: "user", text: "**bold**" })} />);
        expect(screen.getByText("**bold**")).toBeInTheDocument();
    });
});

describe("the assistant reply", () => {
    it("renders markdown", () => {
        render(
            <Message
                message={baseMessage({ speaker: "assistant", text: "**bold** text" })}
            />,
        );
        const strong = document.querySelector(".msg-md strong");
        expect(strong).not.toBeNull();
        expect(strong?.textContent).toBe("bold");
    });

    it("shows a streaming caret while the reply is still coming in", () => {
        const { container } = render(
            <Message
                message={baseMessage({ speaker: "assistant", text: "yazıyor", streaming: true })}
            />,
        );
        expect(container.querySelector(".caret")).not.toBeNull();
        // No copy action while it is still streaming -- there is nothing
        // finished to copy yet.
        expect(screen.queryByTitle("Copy")).not.toBeInTheDocument();
    });

    it("offers a copy action once the reply has finished", () => {
        render(
            <Message
                message={baseMessage({ speaker: "assistant", text: "bitti", streaming: false })}
            />,
        );
        expect(screen.getByTitle("Copy")).toBeInTheDocument();
    });
});

describe("a tool note", () => {
    it("is collapsed by default and opens to show what it was called with", async () => {
        const user = userEvent.setup();
        render(
            <Message
                message={baseMessage({
                    speaker: "tool",
                    text: "list_directory — 3 dosya",
                    args: '{"path":"."}',
                    detail: "3 files found",
                })}
            />,
        );

        expect(screen.queryByText("Called with")).not.toBeInTheDocument();

        await user.click(screen.getByText("list_directory — 3 dosya"));

        expect(screen.getByText("Called with")).toBeInTheDocument();
        expect(screen.getByText("Returned")).toBeInTheDocument();
    });

    it("is not clickable when there is nothing behind it to open", () => {
        render(<Message message={baseMessage({ speaker: "tool", text: "no detail here" })} />);
        expect(screen.queryByRole("button")).not.toBeInTheDocument();
    });
});

describe("an error note", () => {
    it("shows the message and a recovery action when one is offered", async () => {
        const run = vi.fn(async () => {});
        const user = userEvent.setup();
        render(
            <Message
                message={baseMessage({
                    speaker: "error",
                    text: "Request too large",
                    recovery: { label: "Forget the oldest half and retry", run },
                })}
            />,
        );

        expect(screen.getByText("Request too large")).toBeInTheDocument();
        const button = screen.getByText("Forget the oldest half and retry");
        await user.click(button);

        expect(run).toHaveBeenCalledTimes(1);
    });
});

describe("an approval card", () => {
    it("shows the allow/always/deny actions while it is the pending request", () => {
        chat.approval = { tool: "delete_file", args: "{}", reason: "risk", messageId: 7 };
        render(
            <Message
                message={baseMessage({ id: 7, speaker: "approval", text: "delete_file", reason: "risk" })}
            />,
        );

        expect(screen.getByText("Allow")).toBeInTheDocument();
        expect(screen.getByText("Always allow")).toBeInTheDocument();
        expect(screen.getByText("Deny")).toBeInTheDocument();
    });

    it("hides the actions and shows the decision once answered", () => {
        chat.approval = null;
        render(
            <Message
                message={baseMessage({
                    id: 8,
                    speaker: "approval",
                    text: "delete_file",
                    reason: "risk",
                    decision: "deny",
                })}
            />,
        );

        expect(screen.queryByText("Allow")).not.toBeInTheDocument();
        expect(screen.getByText("Denied")).toBeInTheDocument();
    });

    it("explains a tainted-context request differently from a plain destructive one", () => {
        render(
            <Message
                message={baseMessage({
                    id: 9,
                    speaker: "approval",
                    text: "run_shell",
                    reason: "tainted",
                })}
            />,
        );

        expect(
            screen.getByText(/tried to give the assistant/i),
        ).toBeInTheDocument();
    });
});
