/**
 * Small actions that more than one screen offers.
 *
 * These exist to keep a failure from being silent. A button wired straight
 * to `api.something()` looks fine until the call rejects: the promise is
 * unhandled, nothing appears, and the user is left pressing a button that
 * does nothing with no idea why.
 *
 * Mirrors `src/lib/actions.ts`, which cannot be imported as-is: it reaches
 * into the Svelte `toast.svelte` store, which only works in files the
 * Svelte compiler processes. This copy reaches into the React store instead.
 */

import { api } from "../lib/api";
import { toast } from "./store/toast";

/**
 * Opens the folder holding generated images and video.
 *
 * Offered from both the canvas and settings. It fails for ordinary reasons
 * -- nothing has been generated yet, so the folder does not exist -- which
 * is exactly the case that has to say something rather than nothing.
 */
export async function openFolder() {
    try {
        await api.openMediaFolder();
    } catch (e) {
        toast.failure("Could not open the folder.", e);
    }
}
