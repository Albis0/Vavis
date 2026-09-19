import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../lib/api", () => ({
    api: {
        spotifyAlbumArt: vi.fn(async () => "data:image/png;base64,x"),
        spotifyControl: vi.fn(async () => {}),
    },
}));

const { api } = await import("../lib/api");
const { nowPlaying } = await import("./store/nowplaying");
const SpotifyPanel = (await import("./SpotifyPanel")).default;

// Real timers on purpose. `userEvent` waits on its own scheduler between
// the events that make up a click, so under fake timers it waits for a clock
// nothing is advancing and the test hangs until it is killed. Nothing here
// depends on time passing -- the panel is rendered from store state that the
// test sets directly.
beforeEach(() => {
    nowPlaying.track = null;
});

describe("SpotifyPanel", () => {
    it("shows the idle copy when nothing is playing", () => {
        render(<SpotifyPanel onClose={vi.fn()} />);
        expect(
            screen.getByText(/Nothing playing\. Start something in Spotify/),
        ).toBeInTheDocument();
    });

    it("shows the track, artist and progress when something is playing", () => {
        nowPlaying.track = {
            track: "Are You Bored Yet?",
            artist: "Wallows",
            albumArt: null,
            durationMs: 180000,
            progressMs: 60000,
            playing: true,
            device: "Living room",
        };

        render(<SpotifyPanel onClose={vi.fn()} />);

        expect(screen.getByText("Are You Bored Yet?")).toBeInTheDocument();
        expect(screen.getByText("Wallows")).toBeInTheDocument();
        expect(screen.getByText("Living room")).toBeInTheDocument();
        expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "33");
    });

    it("calls onClose when the close button is pressed", async () => {
        const user = userEvent.setup();
        const onClose = vi.fn();
        render(<SpotifyPanel onClose={onClose} />);

        await user.click(screen.getByLabelText("Hide"));
        expect(onClose).toHaveBeenCalledTimes(1);
    });

    it("toggles play/pause through the transport control", async () => {
        nowPlaying.track = {
            track: "Song",
            artist: "Artist",
            albumArt: null,
            durationMs: 100000,
            progressMs: 0,
            playing: false,
            device: null,
        };
        const user = userEvent.setup();

        render(<SpotifyPanel onClose={vi.fn()} />);

        await user.click(screen.getByLabelText("Play"));
        expect(api.spotifyControl).toHaveBeenCalledWith("play");
    });
});
