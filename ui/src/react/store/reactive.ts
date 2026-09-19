/**
 * The smallest reactive primitive the app needs, with no framework in it.
 *
 * The stores used to be Svelte classes with `$state` fields, which only work
 * inside files the Svelte compiler processes. React cannot read them, so the
 * state layer moves here: plain objects that announce their own changes, and
 * a `useStore` hook on the React side that subscribes.
 *
 * This is deliberately tiny. It is not a state library -- it is the seam that
 * lets both frameworks read the same state while the interface is rewritten,
 * and the thing React keeps afterwards.
 */

export type Unsubscribe = () => void;

/**
 * A value that tells its readers when it changes.
 *
 * `version` is what subscribers actually compare. React's
 * `useSyncExternalStore` needs a snapshot that is `Object.is`-stable between
 * renders, and the state itself is mutated in place (the stores are classes
 * with mutable fields, and rewriting all of them to be immutable is a much
 * larger change than this migration needs). A monotonic counter is stable,
 * cheap, and changes exactly when something did.
 */
export class Signal {
    private listeners = new Set<() => void>();
    private version = 0;

    /** Registers a listener, and hands back the way to remove it. */
    subscribe = (fn: () => void): Unsubscribe => {
        this.listeners.add(fn);
        return () => {
            this.listeners.delete(fn);
        };
    };

    /** The current version. Stable until something calls `notify`. */
    snapshot = (): number => this.version;

    /**
     * Announces a change.
     *
     * Listeners are copied before being called: a listener that unsubscribes
     * during the walk (React does this when a component unmounts mid-update)
     * would otherwise mutate the set being iterated.
     */
    notify = (): void => {
        this.version++;
        for (const fn of [...this.listeners]) fn();
    };
}

/**
 * Wraps an object so that writing any field announces the change.
 *
 * Used by the stores to keep their existing shape -- `chat.messages = [...]`
 * still reads the same at every call site -- while becoming observable to
 * React. Nested mutation (`chat.messages.push(x)`) is NOT observed, matching
 * how the Svelte version behaved: the stores already reassign rather than
 * mutate in place, because `$state` had the same requirement for arrays
 * crossing a component boundary.
 */
export function observable<T extends object>(target: T, signal: Signal): T {
    return new Proxy(target, {
        set(obj, prop, value, receiver) {
            const had = Reflect.get(obj, prop, receiver);
            const ok = Reflect.set(obj, prop, value, receiver);
            // Skip the notify when nothing moved: a status poll that returns
            // an identical primitive should not re-render the whole tree.
            if (ok && !Object.is(had, value)) signal.notify();
            return ok;
        },
        deleteProperty(obj, prop) {
            const ok = Reflect.deleteProperty(obj, prop);
            if (ok) signal.notify();
            return ok;
        },
    });
}
