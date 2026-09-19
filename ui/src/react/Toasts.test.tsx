/**
 * The toast stack: stacking order, the cap on how many show at once, and
 * dismissal (by button and by the action running).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const { toast } = await import("./store/toast");
const Toasts = (await import("./Toasts")).default;

beforeEach(() => {
    toast.items = [];
});

afterEach(() => {
    vi.restoreAllMocks();
});

describe("stacking", () => {
    it("shows every toast raised, oldest first", () => {
        toast.info("birinci");
        toast.success("ikinci");

        render(<Toasts />);

        const texts = screen.getAllByText(/^(birinci|ikinci)$/).map((el) => el.textContent);
        expect(texts).toEqual(["birinci", "ikinci"]);
    });

    it("caps the stack at four, dropping the oldest", () => {
        toast.info("bir");
        toast.info("iki");
        toast.info("üç");
        toast.info("dört");
        toast.info("beş");

        render(<Toasts />);

        expect(screen.queryByText("bir")).not.toBeInTheDocument();
        expect(screen.getByText("beş")).toBeInTheDocument();
        expect(screen.getAllByRole("presentation").length).toBe(4);
    });

    it("shows the detail line under the main text when there is one", () => {
        toast.error("Could not save", { detail: "disk full" });
        render(<Toasts />);

        expect(screen.getByText("Could not save")).toBeInTheDocument();
        expect(screen.getByText("disk full")).toBeInTheDocument();
    });
});

describe("dismissal", () => {
    it("removes a toast when its dismiss button is clicked", async () => {
        const user = userEvent.setup();
        toast.info("gidecek");
        render(<Toasts />);

        await user.click(screen.getByLabelText("Dismiss"));

        expect(screen.queryByText("gidecek")).not.toBeInTheDocument();
    });

    it("runs the action and dismisses the toast when its button is clicked", async () => {
        const run = vi.fn();
        const user = userEvent.setup();
        toast.info("geri al", { action: { label: "Undo", run } });
        render(<Toasts />);

        await user.click(screen.getByText("Undo"));

        expect(run).toHaveBeenCalledTimes(1);
        expect(screen.queryByText("geri al")).not.toBeInTheDocument();
    });

    it("holds a toast open on hover and lets it go again on leave", async () => {
        const user = userEvent.setup();
        toast.info("hover me", { duration: 50 });
        render(<Toasts />);

        const item = screen.getByText("hover me").closest(".toast") as HTMLElement;
        await user.hover(item);
        // Held: the original timer was cleared. Waiting past the original
        // duration proves it did not fire while hovered.
        await new Promise((r) => setTimeout(r, 80));
        expect(screen.getByText("hover me")).toBeInTheDocument();

        await user.unhover(item);
        expect(screen.getByText("hover me")).toBeInTheDocument();
    });
});
