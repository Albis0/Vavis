/**
 * The overlay primitive.
 *
 * Every floating layer in the app goes through this: the command palette,
 * the memory and automation sheets, settings, confirmations. Before it, each
 * one hand-rolled its own scrim and its own z-index, and none of them trapped
 * focus — so Tab walked straight out of an open dialog into the window
 * behind it, where the reactor and the chat box were still tabbable but no
 * longer visible.
 *
 * What it guarantees, so no caller has to think about it again:
 *
 * **Focus goes in and cannot leave.** The first thing worth focusing gets
 * focus on open (mark it `data-autofocus` to choose which), Tab cycles
 * inside, and on close focus returns to whatever the user was on before — so
 * dismissing a dialog puts the caret back where they were typing.
 *
 * **Escape closes the top layer only.** Layers are tracked in a stack and a
 * keystroke is acted on by the last one opened. With a confirmation open over
 * settings, one Escape answers the question and leaves settings up. The event
 * is then stopped, so the window-level handler does not unwind a second layer
 * on the same press.
 *
 * **Layers stack in the order they opened.** Two z-index values per layer,
 * handed out from the stack depth, which is what makes a dialog above a
 * dialog possible at all.
 */
import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import Icon from "./Icon";
import "./styles/modal.css";

/**
 * Open layers, oldest first.
 *
 * A plain array rather than state: it is only ever consulted from inside an
 * event handler, so nothing needs to re-render when it changes.
 */
const stack: symbol[] = [];

/** First layer sits here; each one after it two above — scrim, then panel. */
const BASE_Z = 60;

/**
 * What counts as focusable.
 *
 * `[tabindex="-1"]` is excluded deliberately: it means "focusable by
 * script, not by Tab", which is exactly the distinction the trap needs.
 */
const FOCUSABLE = [
    "a[href]",
    "button:not([disabled])",
    "input:not([disabled])",
    "textarea:not([disabled])",
    "select:not([disabled])",
    '[tabindex]:not([tabindex="-1"])',
].join(",");

interface Props {
    /** Drawn as a heading, and used as the accessible name. */
    title?: string;
    /** Accessible name when there is no visible title, as on the palette. */
    label?: string;
    /** Sub-heading under the title, for a dialog that needs a sentence. */
    description?: string;
    size?: "sm" | "md" | "lg" | "xl" | "full";
    /** `top` sits the panel in the upper third, where the eye rests. */
    align?: "center" | "top";
    /**
     * Whether clicking the backdrop closes it. Off for a dialog that has
     * to be answered rather than avoided by a stray click.
     *
     * This governs the backdrop only. Escape is a separate question — a
     * confirmation should refuse a click that lands beside it and still
     * cancel on Escape, which is the one gesture that cannot be
     * accidental.
     */
    dismissable?: boolean;
    /**
     * Whether Escape closes it. Escape is *consumed* either way while this
     * is the top layer: a press aimed at this dialog must never be acted
     * on by a layer underneath.
     */
    escapable?: boolean;
    /** Hides the corner close button without making the layer sticky. */
    showClose?: boolean;
    /** Removes the panel's own padding, for content that lays itself out. */
    bare?: boolean;
    onClose: () => void;
    children: ReactNode;
    /** Pinned to the bottom of the panel, outside the scroll area. */
    footer?: ReactNode;
    /** Extra controls in the header, beside the close button. */
    actions?: ReactNode;
}

export default function Modal({
    title,
    label,
    description,
    size = "md",
    align = "center",
    dismissable = true,
    escapable = true,
    showClose = true,
    bare = false,
    onClose,
    children,
    footer,
    actions,
}: Props) {
    const panelRef = useRef<HTMLDivElement | null>(null);
    const idRef = useRef<symbol>(Symbol("modal"));
    const [depth, setDepth] = useState(0);

    function focusables(): HTMLElement[] {
        const panel = panelRef.current;
        if (!panel) return [];
        return Array.from(panel.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
            // `offsetParent` is null for anything display:none or inside a
            // collapsed section, which should not be a Tab stop.
            (element) => element.offsetParent !== null,
        );
    }

    /** True when this is the layer a keystroke belongs to. */
    function topmost(): boolean {
        return stack[stack.length - 1] === idRef.current;
    }

    useEffect(() => {
        const id = idRef.current;
        stack.push(id);
        setDepth(stack.length - 1);

        // Captured before focus moves, so it can be given back on close. If the
        // trigger has since left the page, focusing it is a harmless no-op.
        const opener = document.activeElement as HTMLElement | null;

        // Deferred a tick: children with their own mount-time focus, such as the
        // search field in the palette, have not run yet on this one.
        //
        // The fallback is the panel itself, not its first focusable. Reaching
        // for the first one put the ring on the close button of every sheet
        // that opens to be read, which both looks like the primary action and
        // is the one control nobody arrived wanting. Focusing the panel names
        // the dialog, makes Escape work, and leaves Tab to enter the content
        // in reading order.
        queueMicrotask(() => {
            const chosen =
                panelRef.current?.querySelector<HTMLElement>("[data-autofocus]") ??
                panelRef.current;
            chosen?.focus();
        });

        return () => {
            const at = stack.indexOf(id);
            if (at !== -1) stack.splice(at, 1);
            opener?.focus?.();
        };
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    function dismiss() {
        if (dismissable) onClose();
    }

    function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
        if (!topmost()) return;

        if (event.key === "Escape") {
            // Consumed whether or not it closes anything. A window-level
            // listener elsewhere would otherwise have a *lower* layer act on
            // an Escape aimed at this one — which is how a confirmation over
            // settings used to close settings and leave the question up.
            event.preventDefault();
            event.stopPropagation();
            if (escapable) onClose();
            return;
        }

        if (event.key !== "Tab") return;

        const items = focusables();
        if (items.length === 0) {
            // Nothing to move to, so Tab must not escape to the page behind.
            event.preventDefault();
            return;
        }

        const first = items[0];
        const last = items[items.length - 1];
        const current = document.activeElement;

        if (event.shiftKey && (current === first || current === panelRef.current)) {
            event.preventDefault();
            last.focus();
        } else if (!event.shiftKey && current === last) {
            event.preventDefault();
            first.focus();
        }
    }

    return (
        <>
            <div
                className={dismissable ? "scrim" : "scrim soft"}
                style={{ zIndex: BASE_Z + depth * 2 }}
                role="presentation"
                onClick={dismiss}
            ></div>

            <div
                className={`panel ${size} ${align}${bare ? " bare" : ""}`}
                ref={panelRef}
                style={{ zIndex: BASE_Z + depth * 2 + 1 }}
                role="dialog"
                aria-modal="true"
                aria-label={title ? undefined : label}
                aria-labelledby={title ? "modal-title" : undefined}
                aria-describedby={description ? "modal-description" : undefined}
                tabIndex={-1}
                onKeyDown={onKeyDown}
            >
                {title || actions || showClose ? (
                    <header className={title ? "titled" : ""}>
                        <div className="heading">
                            {title ? <h2 id="modal-title">{title}</h2> : null}
                            {description ? (
                                <p id="modal-description">{description}</p>
                            ) : null}
                        </div>

                        <div className="modal-header-actions">
                            {actions}
                            {showClose ? (
                                <button
                                    className="modal-close"
                                    onClick={onClose}
                                    title={dismissable ? "Close (Esc)" : "Close"}
                                    aria-label="Close"
                                >
                                    <Icon name="close" size={16} />
                                </button>
                            ) : null}
                        </div>
                    </header>
                ) : null}

                <div className={bare ? "content" : "content padded"}>{children}</div>

                {footer ? <div className="footer">{footer}</div> : null}
            </div>
        </>
    );
}
