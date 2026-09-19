/**
 * One labelled setting: a name, an optional explanation, and the control.
 *
 * Why a component rather than a class: the old screen relied on
 * `.pane > * { width: 100% }` to lay rows out, which also stretched every
 * flex child. A select beside an input ate the whole row and squeezed the
 * input to nothing — that is why the web-search key box looked like it did
 * not exist, and why the "full authority" checkbox pushed its own label off
 * the side. Layout belongs to the row that owns it, not to a rule applied to
 * everything in the pane.
 */

import type { ReactNode } from "react";

interface Props {
    label: string;
    /** Sits under the label, in quieter type. */
    hint?: string;
    /** Marks the field as the one that must be filled. */
    required?: boolean;
    /** What an empty value falls back to, named rather than implied. */
    fallback?: string;
    /** Shown in place of the hint when something is wrong. */
    error?: string;
    /** Puts the control beside the label instead of under it. */
    inline?: boolean;
    children: ReactNode;
}

export default function Field({
    label,
    hint = "",
    required = false,
    fallback = "",
    error = "",
    inline = false,
    children,
}: Props) {
    return (
        <div className={inline ? "setting inline" : "setting"}>
            <div className="head">
                <span className="label">
                    {label}
                    {required && (
                        <span className="req" title="Required">
                            *
                        </span>
                    )}
                </span>
                {error ? (
                    <span className="error">{error}</span>
                ) : fallback ? (
                    <span className="fallback">default: {fallback}</span>
                ) : hint ? (
                    <span className="hint">{hint}</span>
                ) : null}
            </div>
            <div className="control">{children}</div>
        </div>
    );
}
