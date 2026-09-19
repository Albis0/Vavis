/**
 * The toast stack.
 *
 * Mounted once, at the root, and driven entirely by the `toast` store — so
 * anything anywhere can report an outcome without knowing this component
 * exists or having somewhere on screen to put the result.
 *
 * It sits bottom-right, clear of the status bar, and above every layer
 * including modals: a save that fails inside settings has to be visible from
 * inside settings, and a toast that a dialog covers is a toast nobody reads.
 *
 * Newest at the bottom, nearest the corner the eye is drawn to, with older
 * ones pushed up. Hovering holds a toast open, because reading a four-line
 * error should not be a race against its own timer.
 *
 * The Svelte version animated toasts in and out with `fly`, and reordered the
 * stack with `flip`. Neither is available here without pulling in an
 * animation library, which the porting rules for this app rule out -- so the
 * entrance uses a plain CSS keyframe (see `toasts.css`) and reordering /
 * removal happens instantly. The stack itself, the stacking order, the hover
 * hold and every piece of behaviour are unchanged.
 */
import { useStore } from "./store/useStore";
import { toast, toastSignal, type ToastKind } from "./store/toast";
import Icon, { type IconName } from "./Icon";
import "./styles/toasts.css";

const ICONS: Record<ToastKind, IconName> = {
    info: "info",
    success: "check",
    warning: "warning",
    error: "warning",
};

export default function Toasts() {
    const state = useStore(toastSignal, toast);

    return (
        // `aria-live` polite, not assertive: these announce things that already
        // happened, and cutting off whatever the reader is on to say "copied" is
        // worse than waiting for a pause.
        <div className="stack" role="status" aria-live="polite">
            {state.items.map((item) => (
                <div
                    key={item.id}
                    className={`toast ${item.kind}`}
                    onMouseEnter={() => toast.hold(item.id)}
                    onMouseLeave={() => toast.release(item.id)}
                    role="presentation"
                >
                    <span className="mark">
                        <Icon name={ICONS[item.kind]} size={15} />
                    </span>

                    <div className="body">
                        <span className="text">{item.text}</span>
                        {item.detail ? <span className="detail selectable">{item.detail}</span> : null}
                    </div>

                    {item.action ? (
                        <button
                            className="action"
                            onClick={() => {
                                item.action?.run();
                                toast.dismiss(item.id);
                            }}
                        >
                            {item.action.label}
                        </button>
                    ) : null}

                    <button className="dismiss" onClick={() => toast.dismiss(item.id)} aria-label="Dismiss">
                        <Icon name="close" size={13} />
                    </button>
                </div>
            ))}
        </div>
    );
}
