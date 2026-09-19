/**
 * The confirmation dialog.
 *
 * Mounted once at the root; draws whatever `confirm.ts` has pending. See that
 * store for why the browser's own `confirm()` had to go.
 *
 * Two decisions worth keeping:
 *
 * **Cancel gets the focus, not the affirmative button.** The dialog exists
 * because the action is hard to undo, so the keyboard default has to be the
 * harmless answer. Enter still confirms — it is bound at the window, since
 * the affirmative button is deliberately not the focused control — but it
 * takes a press aimed at this dialog rather than being what happens next if
 * the user was already typing.
 *
 * **It cannot be dismissed by clicking away.** A question that vanishes when
 * the pointer slips leaves the user unsure which answer they gave. Escape
 * still cancels, because that one is unambiguous — which is why `dismissable`
 * and `escapable` are separate props on Modal rather than one.
 */
import { useEffect } from "react";
import { useStore } from "./store/useStore";
import { confirmStore, confirmSignal } from "./store/confirm";
import Modal from "./Modal";
import "./styles/confirmDialog.css";

export default function ConfirmDialog() {
    const state = useStore(confirmSignal, confirmStore);
    const pending = state.pending;

    useEffect(() => {
        function onWindowKey(event: KeyboardEvent) {
            if (!pending || event.key !== "Enter") return;
            event.preventDefault();
            confirmStore.answer(true);
        }
        window.addEventListener("keydown", onWindowKey);
        return () => window.removeEventListener("keydown", onWindowKey);
    }, [pending]);

    if (!pending) return null;

    return (
        <Modal
            size="sm"
            title={pending.title}
            dismissable={false}
            showClose={false}
            onClose={() => confirmStore.answer(false)}
            footer={
                <>
                    <button data-autofocus onClick={() => confirmStore.answer(false)}>
                        {pending.cancelLabel ?? "Cancel"}
                    </button>
                    <button
                        className={pending.danger ? "primary destructive" : "primary"}
                        onClick={() => confirmStore.answer(true)}
                    >
                        {pending.confirmLabel ?? "Confirm"}
                    </button>
                </>
            }
        >
            {pending.body ? <p className="body">{pending.body}</p> : null}
        </Modal>
    );
}
