/**
 * Confirmation questions.
 *
 * Four places used the browser's `confirm()` to guard a delete. In a
 * frameless Tauri window that is the wrong control in every respect: it draws
 * an OS dialog with the app's own title on it in the system font, ignores the
 * theme, cannot say which of the two answers is destructive, and blocks the
 * whole webview while it is up.
 *
 * This keeps the shape that made `confirm()` convenient — one `await` at the
 * call site, a boolean back — while the dialog itself is ours. `Confirm.svelte`
 * is mounted once at the root and draws whatever is pending here.
 */

import { observable, Signal } from "./reactive";

export interface ConfirmRequest {
    /** The question, as a statement of what is about to happen. */
    title: string;
    /** What the user cannot undo, spelled out. Optional but usually wanted. */
    body?: string;
    /**
     * The affirmative button.
     *
     * Name the action — "Delete", "Discard" — rather than saying "OK". A
     * button that says what it does can be read on its own, which is how it
     * is actually read.
     */
    confirmLabel?: string;
    cancelLabel?: string;
    /** Colours the affirmative button as destructive. */
    danger?: boolean;
}

interface Pending extends ConfirmRequest {
    resolve: (answer: boolean) => void;
}

class ConfirmStore {
    pending: Pending | null = null;

    ask(request: ConfirmRequest): Promise<boolean> {
        // A second question can only arrive from code, never from the user:
        // the first one is modal. Whatever raised it is no longer the thing on
        // screen, so its answer is taken as a cancel rather than left hanging.
        this.pending?.resolve(false);

        return new Promise<boolean>((resolve) => {
            this.pending = { ...request, resolve };
        });
    }

    answer(confirmed: boolean) {
        const pending = this.pending;
        // Cleared first, so a double click on the affirmative button cannot
        // resolve the same promise twice.
        this.pending = null;
        pending?.resolve(confirmed);
    }
}

export const confirmSignal = new Signal();

export const confirmStore = observable(new ConfirmStore(), confirmSignal);

/**
 * Asks the user to confirm, resolving to their answer.
 *
 * Named `ask` rather than `confirm` on purpose: a module-level `confirm`
 * shadows the global one, and a call site that forgot to import it would
 * silently fall back to the OS dialog this exists to replace.
 */
export function ask(request: ConfirmRequest): Promise<boolean> {
    return confirmStore.ask(request);
}
