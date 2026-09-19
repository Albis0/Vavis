/**
 * Reading the app's stores from React.
 *
 * The stores are plain observable objects (see `reactive.ts`), not hooks and
 * not context. That is deliberate: they are created once, live for the life
 * of the app, and are read by components at every depth. Threading them
 * through context would add a provider and a re-render boundary for no gain,
 * and the non-React code (event handlers in `api.ts`, the reactor) has to be
 * able to write to them without a hook in scope.
 */

import { useSyncExternalStore } from "react";
import type { Signal } from "./reactive";

/**
 * Subscribes to a store and re-renders when it changes.
 *
 * Returns the store itself rather than a selected slice. Selectors would cut
 * re-renders, but they also require every read to name its dependency up
 * front, and the panels here read a dozen fields each. The whole app is a
 * handful of panels on one screen -- the render cost is not where this app
 * is slow, and a wrong selector is a silently stale interface.
 *
 * ```ts
 * const chat = useStore(chatSignal, chatStore);
 * return <p>{chat.messages.length}</p>;
 * ```
 */
export function useStore<T>(signal: Signal, store: T): T {
    useSyncExternalStore(signal.subscribe, signal.snapshot, signal.snapshot);
    return store;
}
