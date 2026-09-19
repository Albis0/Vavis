/**
 * A titled block inside a pane: heading, optional explanation, contents.
 *
 * Exists so panes stop hand-rolling `<h3>` plus a `<p class="hint">` plus a
 * wrapper div in four slightly different ways.
 */

import type { ReactNode } from "react";

interface Props {
    title?: string;
    blurb?: string;
    children: ReactNode;
}

export default function Section({ title = "", blurb = "", children }: Props) {
    return (
        <section className="section">
            {title && <h3>{title}</h3>}
            {blurb && <p className="blurb">{blurb}</p>}
            <div className="body">{children}</div>
        </section>
    );
}
