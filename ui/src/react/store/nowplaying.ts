/**
 * What Spotify is playing, and whether to show it.
 *
 * The poll lives here rather than inside `SpotifyPanel.svelte` for one
 * reason: a panel that only polls while it is mounted can never be the thing
 * that decides to mount itself. Detection has to outlive the box.
 *
 * The rule is that music appearing is what puts the box on screen, and music
 * stopping is what takes it away again. Nobody should have to go and ask for a
 * now-playing panel while a track is already playing — by the time you have
 * opened the command palette to find out what is playing, you could have
 * looked at Spotify.
 *
 * Two things stop that becoming annoying:
 *
 * **Closing it means "not now", not "never".** A manual close suppresses the
 * automatic show, but only until playback actually stops. Start something an
 * hour later and the box comes back, which is what you wanted the first time.
 *
 * **Silence has to last to count.** A track change, a seek, or a device
 * handover all report nothing playing for a moment. Hiding on the first empty
 * poll would make the box flicker away between songs, so it takes a few in a
 * row.
 */

import { observable, Signal } from "./reactive";

import { api, type SpotifyNowPlaying } from "../../lib/api";

/** How often Spotify is actually asked. */
const POLL_MS = 4000;

/**
 * Empty polls before the box is taken away.
 *
 * Three at four seconds is about twelve, which comfortably outlasts a track
 * change and is still quick enough that the box is gone by the time you have
 * noticed the music stopped.
 */
const SILENCE_BEFORE_HIDING = 3;

class NowPlayingStore {
    /** The current track, or null when nothing is playing or reachable. */
    track: SpotifyNowPlaying | null = null;
    /** Whether the box is on screen. */
    visible = false;

    /** Set by the close button: suppresses the automatic show until silence. */
    private dismissed = false;
    /**
     * Set when the user asked for the box themselves.
     *
     * Without it, asking for the box while nothing is playing showed it for
     * one poll and then took it away again: silence had already accumulated,
     * so the very next tick hid the thing that had just been requested. Only
     * a box that appeared on its own is allowed to leave on its own.
     */
    private pinned = false;
    /** Consecutive polls that reported nothing. */
    private silence = 0;
    private timer: ReturnType<typeof setInterval> | null = null;

    async poll() {
        let fresh: SpotifyNowPlaying | null = null;
        try {
            fresh = await api.spotifyNowPlaying();
        } catch {
            // Not connected, or Spotify is unreachable. Indistinguishable from
            // silence as far as this is concerned, and equally not worth
            // reporting: the box is ambient, not a task.
            fresh = null;
        }

        this.track = fresh;

        if (fresh) {
            this.silence = 0;
            if (!this.dismissed) this.visible = true;
            return;
        }

        this.silence += 1;
        if (this.silence >= SILENCE_BEFORE_HIDING) {
            // Silence is also what clears a manual dismissal, so closing the
            // box mutes it for this listening session rather than for good.
            this.dismissed = false;
            if (!this.pinned) this.visible = false;
        }
    }

    start() {
        if (this.timer !== null) return;
        void this.poll();
        this.timer = setInterval(() => void this.poll(), POLL_MS);
    }

    stop() {
        if (this.timer !== null) clearInterval(this.timer);
        this.timer = null;
    }

    /** The close button. Hides it now, without meaning it forever. */
    dismiss() {
        this.pinned = false;
        this.dismissed = true;
        this.visible = false;
    }

    /**
     * The palette command, for looking at the box when nothing is playing.
     *
     * A box asked for by hand is pinned: it stays until it is closed by hand,
     * where one that appeared on its own leaves once the music stops.
     */
    toggle() {
        if (this.visible) {
            this.dismiss();
        } else {
            this.pinned = true;
            this.dismissed = false;
            this.visible = true;
        }
    }

    /** Re-reads straight after a transport command, which needs a moment. */
    refreshSoon() {
        setTimeout(() => void this.poll(), 400);
    }
}

export const nowPlayingSignal = new Signal();

export const nowPlaying = observable(new NowPlayingStore(), nowPlayingSignal);
