/**
 * Transient notifications.
 *
 * The interface had no way to say "saved", "copied" or "that key is wrong"
 * without either stealing the view with a dialog or writing a line into the
 * conversation, which is a permanent record of something that was never part
 * of the conversation. Toasts are the third option: visible, brief, and gone.
 *
 * Two rules shape the defaults below.
 *
 * **Success is shorter than failure.** A confirmation only has to be caught
 * out of the corner of the eye; an error has to be read, and possibly acted
 * on, so it stays roughly three times as long.
 *
 * **Nothing that needs a decision goes here.** A toast can be missed — the
 * user may be looking at the other side of the screen — so anything with a
 * consequence belongs in a dialog. What lands here is feedback about
 * something that has already happened.
 */

import { observable, Signal } from "./reactive";

/** How loud a toast is, which picks its colour and its icon. */
export type ToastKind = "info" | "success" | "warning" | "error";

export interface ToastAction {
    label: string;
    run: () => void;
}

export interface Toast {
    id: number;
    kind: ToastKind;
    text: string;
    /** A second line, for the part of an error that is too long for the first. */
    detail?: string;
    /** Milliseconds on screen. Zero means it stays until dismissed. */
    duration: number;
    /** One optional button, for "Undo" and nothing more elaborate. */
    action?: ToastAction;
}

/** Default lifetimes, in milliseconds. */
const LIFETIME: Record<ToastKind, number> = {
    info: 3200,
    success: 2600,
    warning: 6000,
    error: 8000,
};

/**
 * How many are shown at once.
 *
 * Past four the stack covers the corner of the window it sits in, and the
 * oldest are the ones the user has already had a chance to read.
 */
const MAX_VISIBLE = 4;

let nextId = 1;

interface ShowOptions {
    detail?: string;
    duration?: number;
    action?: ToastAction;
}

class ToastStore {
    items: Toast[] = [];

    /** Timers by toast id, so dismissing early does not leave one pending. */
    private timers = new Map<number, ReturnType<typeof setTimeout>>();

    show(kind: ToastKind, text: string, options: ShowOptions = {}): number {
        const toast: Toast = {
            id: nextId++,
            kind,
            text,
            detail: options.detail,
            duration: options.duration ?? LIFETIME[kind],
            action: options.action,
        };

        // Oldest first, so the newest — the one the user is most likely
        // waiting for — is never the one that gets dropped. Built as one new
        // array: the proxy watches the field, not the array, so `push` and
        // `shift` would never reach it.
        const grown = [...this.items, toast];
        for (const dropped of grown.slice(0, Math.max(0, grown.length - MAX_VISIBLE))) {
            this.clearTimer(dropped.id);
        }
        this.items = grown.slice(-MAX_VISIBLE);

        if (toast.duration > 0) {
            this.timers.set(
                toast.id,
                setTimeout(() => this.dismiss(toast.id), toast.duration),
            );
        }

        return toast.id;
    }

    dismiss(id: number) {
        this.clearTimer(id);
        this.items = this.items.filter((toast) => toast.id !== id);
    }

    /**
     * Holds a toast open.
     *
     * Called when the pointer enters one: reading a four-line error should not
     * be a race against its own timer.
     */
    hold(id: number) {
        this.clearTimer(id);
    }

    /** Restarts the countdown when the pointer leaves again. */
    release(id: number) {
        const toast = this.items.find((item) => item.id === id);
        if (!toast || toast.duration === 0 || this.timers.has(id)) return;
        this.timers.set(
            id,
            // A shorter tail than the original: it has already been on screen
            // and read, so it only needs long enough not to vanish under the
            // pointer as it moves away.
            setTimeout(() => this.dismiss(id), 1600),
        );
    }

    private clearTimer(id: number) {
        const timer = this.timers.get(id);
        if (timer !== undefined) {
            clearTimeout(timer);
            this.timers.delete(id);
        }
    }

    info(text: string, options?: ShowOptions) {
        return this.show("info", text, options);
    }

    success(text: string, options?: ShowOptions) {
        return this.show("success", text, options);
    }

    warning(text: string, options?: ShowOptions) {
        return this.show("warning", text, options);
    }

    error(text: string, options?: ShowOptions) {
        return this.show("error", text, options);
    }

    /**
     * Reports a thrown value.
     *
     * Errors reach the frontend from three places — a Tauri command that
     * returned `Err`, a rejected promise, a real `Error` — and only the last
     * has a `message`. Everything else stringifies to something usable, so the
     * cast is done in one place instead of at forty call sites.
     */
    failure(text: string, cause: unknown) {
        const detail = cause instanceof Error ? cause.message : String(cause);
        return this.error(text, { detail });
    }
}

export const toastSignal = new Signal();

export const toast = observable(new ToastStore(), toastSignal);
