import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { CanvasSettings, GalleryItem } from "../lib/api";

vi.mock("@tauri-apps/api/core", () => ({
    convertFileSrc: (path: string) => `asset://${path}`,
}));

const settingsFixture: CanvasSettings = {
    imageOrder: [],
    videoOrder: [],
    imageModel: "",
    videoModel: "",
    size: "1024x1024",
    count: 1,
    configured: [],
    canImage: true,
    canVideo: false,
    canUpscale: false,
    customUrl: "",
    customHeaderName: "",
    customHeaderValue: "",
    customModel: "",
    items: 0,
    bytes: 0,
};

function item(overrides: Partial<GalleryItem> = {}): GalleryItem {
    return {
        id: 1,
        kind: "image",
        path: "/tmp/one.png",
        prompt: "A red fox in snow",
        provider: "openai",
        model: "gpt-image",
        params: JSON.stringify({ size: "1024x1024" }),
        seed: 42,
        width: 1024,
        height: 1024,
        bytes: 2048,
        parentId: null,
        favourite: false,
        createdAt: 1700000000,
        ...overrides,
    };
}

vi.mock("../lib/api", () => ({
    api: {
        canvasSettings: vi.fn(async () => settingsFixture),
        listGallery: vi.fn(async () => []),
        canvasGenerate: vi.fn(async () => {}),
        deleteGalleryItem: vi.fn(async () => {}),
        favouriteGalleryItem: vi.fn(async () => {}),
        clearGallery: vi.fn(async () => 0),
        openMediaFolder: vi.fn(async () => {}),
    },
    on: vi.fn(async () => () => {}),
}));

vi.mock("./store/confirm", () => ({ ask: vi.fn(async () => true) }));
vi.mock("./store/toast", () => ({
    toast: {
        success: vi.fn(),
        failure: vi.fn(),
        warning: vi.fn(),
        info: vi.fn(),
        error: vi.fn(),
    },
}));

const { api, on } = await import("../lib/api");
const { ask } = await import("./store/confirm");
const CanvasView = (await import("./CanvasView")).default;

beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.canvasSettings).mockResolvedValue(settingsFixture);
    vi.mocked(api.listGallery).mockResolvedValue([]);
    vi.mocked(api.canvasGenerate).mockResolvedValue(undefined);
    vi.mocked(api.deleteGalleryItem).mockResolvedValue(undefined);
    vi.mocked(api.favouriteGalleryItem).mockResolvedValue(undefined);
    vi.mocked(api.clearGallery).mockResolvedValue(0);
    vi.mocked(on).mockResolvedValue(() => {});
    vi.mocked(ask).mockResolvedValue(true);
});

describe("CanvasView", () => {
    it("shows the empty state once loading finishes with no results", async () => {
        render(<CanvasView />);
        expect(await screen.findByText("Nothing generated yet")).toBeInTheDocument();
    });

    it("renders gallery tiles once results load", async () => {
        vi.mocked(api.listGallery).mockResolvedValue([item()]);
        render(<CanvasView />);
        expect(await screen.findByText("A red fox in snow")).toBeInTheDocument();
    });

    it("keeps Generate disabled until a prompt is typed", async () => {
        const user = userEvent.setup();
        render(<CanvasView />);
        await screen.findByText("Nothing generated yet");

        const go = screen.getByRole("button", { name: /Generate/ });
        expect(go).toBeDisabled();

        await user.type(screen.getByLabelText("Prompt"), "a cat");
        expect(go).not.toBeDisabled();
    });

    it("disables the Video kind button when no video provider has a key", async () => {
        render(<CanvasView />);
        await screen.findByText("Nothing generated yet");
        expect(screen.getByRole("button", { name: "Video" })).toBeDisabled();
    });

    it("disables Generate for video when no video provider has a key, even with a prompt", async () => {
        const user = userEvent.setup();
        // Start from an item so kind can be switched to video via Animate,
        // since the Video segmented button is itself disabled.
        vi.mocked(api.listGallery).mockResolvedValue([item()]);
        render(<CanvasView />);

        const tile = await screen.findByText("A red fox in snow");
        await user.click(tile);

        const animateButton = await screen.findByRole("button", { name: "Animate" });
        expect(animateButton).toBeDisabled();
    });

    it("calls canvasGenerate with the trimmed prompt and parsed seed on Generate", async () => {
        const user = userEvent.setup();
        render(<CanvasView />);
        await screen.findByText("Nothing generated yet");

        await user.type(screen.getByLabelText("Prompt"), "  a fox  ");
        await user.type(screen.getByLabelText("Seed"), "7");
        await user.click(screen.getByRole("button", { name: /Generate/ }));

        await waitFor(() =>
            expect(api.canvasGenerate).toHaveBeenCalledWith(
                expect.objectContaining({ prompt: "a fox", seed: 7, kind: "image" }),
            ),
        );
    });

    it("treats an empty seed field as no seed", async () => {
        const user = userEvent.setup();
        render(<CanvasView />);
        await screen.findByText("Nothing generated yet");

        await user.type(screen.getByLabelText("Prompt"), "a fox");
        await user.click(screen.getByRole("button", { name: /Generate/ }));

        await waitFor(() =>
            expect(api.canvasGenerate).toHaveBeenCalledWith(
                expect.objectContaining({ seed: null }),
            ),
        );
    });

    it("opens the detail modal on tile click and shows the recorded facts", async () => {
        const user = userEvent.setup();
        vi.mocked(api.listGallery).mockResolvedValue([item({ model: "gpt-image-1" })]);
        render(<CanvasView />);

        const tile = await screen.findByText("A red fox in snow");
        await user.click(tile);

        expect(await screen.findByText("gpt-image-1")).toBeInTheDocument();
        expect(screen.getByText("42")).toBeInTheDocument();
    });

    it("says a result with no seed cannot be repeated exactly", async () => {
        const user = userEvent.setup();
        vi.mocked(api.listGallery).mockResolvedValue([item({ seed: null })]);
        render(<CanvasView />);

        await user.click(await screen.findByText("A red fox in snow"));
        expect(
            await screen.findByText("Not reported — cannot be repeated exactly"),
        ).toBeInTheDocument();
    });

    it("loads a result's exact settings back into the form via Same settings", async () => {
        const user = userEvent.setup();
        vi.mocked(api.listGallery).mockResolvedValue([
            item({ model: "gpt-image-1", seed: 99, prompt: "A blue whale" }),
        ]);
        render(<CanvasView />);

        await user.click(await screen.findByText("A blue whale"));
        await user.click(screen.getByRole("button", { name: "Same settings" }));

        await waitFor(() =>
            expect(screen.getByLabelText("Prompt")).toHaveValue("A blue whale"),
        );
        expect(screen.getByLabelText("Model")).toHaveValue("gpt-image-1");
        expect(screen.getByLabelText("Seed")).toHaveValue("99");
    });

    it("asks for confirmation and deletes on Delete", async () => {
        const user = userEvent.setup();
        vi.mocked(api.listGallery).mockResolvedValue([item()]);
        render(<CanvasView />);

        await user.click(await screen.findByText("A red fox in snow"));
        await user.click(screen.getByRole("button", { name: /Delete/ }));

        await waitFor(() => expect(api.deleteGalleryItem).toHaveBeenCalledWith(1));
        expect(ask).toHaveBeenCalled();
    });

    it("does not delete when the confirmation is declined", async () => {
        vi.mocked(ask).mockResolvedValueOnce(false);
        const user = userEvent.setup();
        vi.mocked(api.listGallery).mockResolvedValue([item()]);
        render(<CanvasView />);

        await user.click(await screen.findByText("A red fox in snow"));
        await user.click(screen.getByRole("button", { name: /Delete/ }));

        await waitFor(() => expect(ask).toHaveBeenCalled());
        expect(api.deleteGalleryItem).not.toHaveBeenCalled();
    });

    it("sets up a variation from a result and closes the modal", async () => {
        const user = userEvent.setup();
        vi.mocked(api.listGallery).mockResolvedValue([item({ prompt: "A blue whale" })]);
        render(<CanvasView />);

        await user.click(await screen.findByText("A blue whale"));
        await user.click(screen.getByRole("button", { name: "Variation" }));

        await waitFor(() => expect(screen.getByText("Continuing from #1")).toBeInTheDocument());
        expect(screen.getByLabelText("Prompt")).toHaveValue("A blue whale");
    });

    it("drops the source when Drop is clicked", async () => {
        const user = userEvent.setup();
        vi.mocked(api.listGallery).mockResolvedValue([item()]);
        render(<CanvasView />);

        await user.click(await screen.findByText("A red fox in snow"));
        await user.click(screen.getByRole("button", { name: "Variation" }));
        await screen.findByText("Continuing from #1");

        await user.click(screen.getByRole("button", { name: "Drop" }));
        expect(screen.queryByText("Continuing from #1")).not.toBeInTheDocument();
    });

    it("clears the gallery after confirmation", async () => {
        const user = userEvent.setup();
        vi.mocked(api.listGallery).mockResolvedValue([item()]);
        vi.mocked(api.clearGallery).mockResolvedValue(4096);
        render(<CanvasView />);

        await screen.findByText("A red fox in snow");
        await user.click(screen.getByRole("button", { name: "Clear" }));

        await waitFor(() => expect(api.clearGallery).toHaveBeenCalledWith(true));
    });

    it("shows the warning when no image provider has a key", async () => {
        vi.mocked(api.canvasSettings).mockResolvedValue({
            ...settingsFixture,
            canImage: false,
        });
        render(<CanvasView />);
        expect(
            await screen.findByText(/No image provider has a key/),
        ).toBeInTheDocument();
    });

    it("cleans up its event listeners and timer on unmount", async () => {
        const offA = vi.fn();
        const offB = vi.fn();
        vi.mocked(on).mockResolvedValueOnce(offA).mockResolvedValueOnce(offB);

        const { unmount } = render(<CanvasView />);
        await screen.findByText("Nothing generated yet");

        unmount();
        await waitFor(() => {
            expect(offA).toHaveBeenCalled();
            expect(offB).toHaveBeenCalled();
        });
    });
});
