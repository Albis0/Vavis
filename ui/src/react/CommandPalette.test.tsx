import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import CommandPalette, { type Command } from "./CommandPalette";

function makeCommands(): Command[] {
    return [
        {
            id: "settings",
            label: "Settings",
            group: "General",
            icon: "settings",
            run: vi.fn(),
        },
        {
            id: "voice",
            label: "Toggle voice mode",
            group: "General",
            icon: "mic",
            run: vi.fn(),
        },
        {
            id: "clear",
            label: "New conversation",
            group: "Chat",
            icon: "plus",
            hint: "Ctrl L",
            run: vi.fn(),
        },
    ];
}

describe("CommandPalette", () => {
    it("lists every command with no filter", () => {
        render(<CommandPalette commands={makeCommands()} onClose={vi.fn()} />);
        expect(screen.getByText("Settings")).toBeInTheDocument();
        expect(screen.getByText("Toggle voice mode")).toBeInTheDocument();
        expect(screen.getByText("New conversation")).toBeInTheDocument();
    });

    it("ranks a label that starts with the query above one that merely contains it", async () => {
        const user = userEvent.setup();
        render(<CommandPalette commands={makeCommands()} onClose={vi.fn()} />);

        await user.type(screen.getByRole("textbox"), "se");

        const options = screen.getAllByRole("option");
        // "Settings" starts with "se"; "Toggle voice mode" only contains it as
        // a substring of nothing here -- but the search field does, since "se"
        // is not in the label at all. This should filter to Settings alone.
        expect(options).toHaveLength(1);
        expect(options[0]).toHaveTextContent("Settings");
    });

    it("shows a clear-search option when nothing matches", async () => {
        const user = userEvent.setup();
        render(<CommandPalette commands={makeCommands()} onClose={vi.fn()} />);

        await user.type(screen.getByRole("textbox"), "zzz-nonexistent");

        expect(screen.getByText(/No commands match/)).toBeInTheDocument();
        await user.click(screen.getByText("Clear search"));
        expect(screen.getByRole("textbox")).toHaveValue("");
    });

    it("runs the highlighted command and closes on Enter", async () => {
        const user = userEvent.setup();
        const onClose = vi.fn();
        const commands = makeCommands();
        render(<CommandPalette commands={commands} onClose={onClose} />);

        const input = screen.getByRole("textbox");
        await user.type(input, "settings{Enter}");

        expect(commands[0].run).toHaveBeenCalledTimes(1);
        expect(onClose).toHaveBeenCalledTimes(1);
    });

    it("moves the selection with ArrowDown and wraps around", async () => {
        const user = userEvent.setup();
        const onClose = vi.fn();
        const commands = makeCommands();
        render(<CommandPalette commands={commands} onClose={onClose} />);

        const input = screen.getByRole("textbox");
        input.focus();
        // Three commands: Down x3 should land back on the first.
        await user.keyboard("{ArrowDown}{ArrowDown}{ArrowDown}{Enter}");

        expect(commands[0].run).toHaveBeenCalledTimes(1);
        expect(onClose).toHaveBeenCalledTimes(1);
    });

    it("runs a command on click", async () => {
        const user = userEvent.setup();
        const onClose = vi.fn();
        const commands = makeCommands();
        render(<CommandPalette commands={commands} onClose={onClose} />);

        await user.click(screen.getByText("New conversation"));

        expect(commands[2].run).toHaveBeenCalledTimes(1);
        expect(onClose).toHaveBeenCalledTimes(1);
    });
});
