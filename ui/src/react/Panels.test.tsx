import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../lib/api", () => ({
    api: {
        listFacts: vi.fn(),
        listAutomations: vi.fn(),
        listTools: vi.fn(),
        forgetFact: vi.fn(async () => true),
        toggleAutomation: vi.fn(async () => true),
        deleteAutomation: vi.fn(async () => true),
    },
}));

vi.mock("./store/confirm", () => ({ ask: vi.fn(async () => true) }));
vi.mock("./store/toast", () => ({
    toast: { success: vi.fn(), failure: vi.fn() },
}));

const { api } = await import("../lib/api");
const { chat } = await import("./store/chat");
const Panels = (await import("./Panels")).default;

beforeEach(() => {
    chat.panel = "none";
    vi.mocked(api.listFacts).mockReset();
    vi.mocked(api.listAutomations).mockReset();
    vi.mocked(api.listTools).mockReset();
});

afterEach(() => {
    chat.panel = "none";
});

describe("Panels", () => {
    it("shows the empty state copy for memory when there are no facts", async () => {
        vi.mocked(api.listFacts).mockResolvedValue([]);
        chat.panel = "memory";

        render(<Panels />);

        expect(await screen.findByText("Nothing remembered yet")).toBeInTheDocument();
    });

    it("lists facts once loaded", async () => {
        vi.mocked(api.listFacts).mockResolvedValue([
            { id: 1, text: "Prefers metric units", source: "user" },
            { id: 2, text: "Lives in Istanbul", source: "auto" },
        ]);
        chat.panel = "memory";

        render(<Panels />);

        expect(await screen.findByText("Prefers metric units")).toBeInTheDocument();
        expect(screen.getByText("Lives in Istanbul")).toBeInTheDocument();
    });

    it("filters the list by the query once there are enough rows for a filter field", async () => {
        vi.mocked(api.listFacts).mockResolvedValue(
            Array.from({ length: 9 }, (_, i) => ({ id: i, text: `fact number ${i}`, source: "user" })),
        );
        chat.panel = "memory";
        const user = userEvent.setup();

        render(<Panels />);

        await screen.findByText("fact number 0");
        const filter = screen.getByLabelText("Filter remembered facts");
        await user.type(filter, "number 3");

        expect(screen.getByText("fact number 3")).toBeInTheDocument();
        expect(screen.queryByText("fact number 0")).not.toBeInTheDocument();
    });

    it("shows a failure state with a retry button when loading throws", async () => {
        vi.mocked(api.listTools).mockRejectedValue(new Error("network down"));
        chat.panel = "tools";

        render(<Panels />);

        expect(await screen.findByText("That did not load.")).toBeInTheDocument();
        expect(screen.getByText("network down")).toBeInTheDocument();
    });

    it("shows the automation trigger and prompt", async () => {
        vi.mocked(api.listAutomations).mockResolvedValue([
            { id: 1, prompt: "tell me the weather", trigger: "every morning at 09:00", enabled: true },
        ]);
        chat.panel = "automations";

        render(<Panels />);

        expect(await screen.findByText("every morning at 09:00")).toBeInTheDocument();
        expect(screen.getByText("tell me the weather")).toBeInTheDocument();
    });
});
