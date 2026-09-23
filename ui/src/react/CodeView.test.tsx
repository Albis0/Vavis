import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../lib/api", () => ({
    api: {
        currentWorkspace: vi.fn(async () => "/repo"),
        listWorkspace: vi.fn(async () => [
            { path: "a.txt", name: "a.txt", isDir: false },
        ]),
        readWorkspaceFile: vi.fn(async () => "hello\nworld"),
        writeWorkspaceFile: vi.fn(async () => {}),
        openWorkspace: vi.fn(async () => "repo"),
        searchWorkspace: vi.fn(async () => []),
    },
    on: vi.fn(async (event: string, fn: (p: unknown) => void) => {
        handlers.set(event, fn);
        return () => {};
    }),
}));

const handlers = new Map<string, (p: unknown) => void>();

vi.mock("./store/confirm", () => ({ ask: vi.fn(async () => true) }));
vi.mock("./store/toast", () => ({
    toast: { success: vi.fn(), failure: vi.fn(), info: vi.fn() },
}));
vi.mock("./store/chat", () => ({
    chat: { view: "code", input: "" },
}));

const { api } = await import("../lib/api");
const CodeView = (await import("./CodeView")).default;

beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.currentWorkspace).mockResolvedValue("/repo");
    vi.mocked(api.listWorkspace).mockResolvedValue([
        { path: "a.txt", name: "a.txt", isDir: false },
    ]);
    vi.mocked(api.readWorkspaceFile).mockResolvedValue("hello\nworld");
    vi.mocked(api.writeWorkspaceFile).mockResolvedValue(undefined);
});

afterEach(() => {
    vi.restoreAllMocks();
});

describe("CodeView", () => {
    it("shows the no-folder state until a workspace is loaded", async () => {
        vi.mocked(api.currentWorkspace).mockResolvedValueOnce(null);
        render(<CodeView />);
        expect(await screen.findByText("No folder open")).toBeInTheDocument();
    });

    it("lists files in the tree and opens one on click", async () => {
        const user = userEvent.setup();
        render(<CodeView />);

        const fileButton = await screen.findByText("a.txt");
        await user.click(fileButton);

        // The editor is a textarea, so the file arrives as its value -- there
        // is no text node holding it for `findByText` to match.
        await waitFor(() =>
            expect(screen.getByRole("textbox", { name: "a.txt" })).toHaveValue(
                "hello\nworld",
            ),
        );
        expect(api.readWorkspaceFile).toHaveBeenCalledWith("a.txt");
    });

    it("shows the assistant's edit to the open file", async () => {
        const user = userEvent.setup();
        render(<CodeView />);
        await user.click(await screen.findByText("a.txt"));
        await waitFor(() =>
            expect(screen.getByRole("textbox", { name: "a.txt" })).toHaveValue("hello\nworld"),
        );

        vi.mocked(api.readWorkspaceFile).mockResolvedValue("hello\nthere");
        handlers.get("chat:tool-done")!({
            tool: "ws_edit",
            ok: true,
            summary: "a.txt düzenlendi (1 değişiklik)",
            detail: "",
        });
        await waitFor(() =>
            expect(screen.getByRole("textbox", { name: "a.txt" })).toHaveValue("hello\nthere"),
        );
    });

    it("marks the file dirty after an edit and saves on the Save button", async () => {
        const user = userEvent.setup();
        render(<CodeView />);

        await user.click(await screen.findByText("a.txt"));
        const editor = await screen.findByLabelText("a.txt");

        await user.type(editor, "!");
        expect(document.querySelector(".code-dot")).toBeTruthy();

        const saveButton = screen.getByRole("button", { name: /Save/ });
        expect(saveButton).not.toBeDisabled();
        await user.click(saveButton);

        await waitFor(() => expect(api.writeWorkspaceFile).toHaveBeenCalled());
        expect(document.querySelector(".code-dot")).toBeFalsy();
    });

    it("inserts four spaces on Tab instead of moving focus", async () => {
        const user = userEvent.setup();
        render(<CodeView />);

        await user.click(await screen.findByText("a.txt"));
        const editor = (await screen.findByLabelText("a.txt")) as HTMLTextAreaElement;

        editor.focus();
        editor.setSelectionRange(0, 0);
        await user.keyboard("{Tab}");

        expect(editor.value.startsWith("    hello")).toBe(true);
    });

    it("saves on Ctrl+S", async () => {
        const user = userEvent.setup();
        render(<CodeView />);

        await user.click(await screen.findByText("a.txt"));
        const editor = await screen.findByLabelText("a.txt");
        editor.focus();

        await user.keyboard("{Control>}s{/Control}");

        await waitFor(() => expect(api.writeWorkspaceFile).toHaveBeenCalledWith("a.txt", "hello\nworld"));
    });
});
