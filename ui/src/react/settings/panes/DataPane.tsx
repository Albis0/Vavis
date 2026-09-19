/**
 * What Vavis stores, where, and how to get rid of it.
 *
 * Read-only except for one destructive button. The counts come from the
 * status poll rather than their own call, so this pane costs nothing to
 * open.
 */

import type { CanvasSettings, Status } from "../../../lib/api";
import { openFolder } from "../../actions";
import Section from "../Section";

interface Props {
    status: Status | null;
    canvas: CanvasSettings | null;
    onclear: () => void;
}

function bytes(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
    if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
    return `${(n / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

export default function DataPane({ status, canvas, onclear }: Props) {
    /** Rows shown under "Stored". Built as data so the markup stays one loop. */
    const rows = [
        { label: "Conversation", value: `${status?.messageCount ?? 0} messages` },
        { label: "Remembered", value: `${status?.factCount ?? 0} facts` },
        { label: "Scheduled", value: `${status?.automationCount ?? 0} automations` },
        ...(canvas
            ? [
                  {
                      label: "Generated",
                      value: `${canvas.items} files · ${bytes(canvas.bytes)}`,
                  },
              ]
            : []),
    ];

    return (
        <>
            <h2>Data</h2>

            <Section title="Location" blurb="Everything Vavis stores lives in this folder.">
                <div className="path-row">
                    <code className="path selectable">{status?.dataDir ?? ""}</code>
                    <button className="tiny" onClick={() => openFolder()}>
                        open
                    </button>
                </div>
            </Section>

            <Section title="Stored">
                <div className="rows">
                    {rows.map((row) => (
                        <div className="stat-row" key={row.label}>
                            <span className="label">{row.label}</span>
                            <span className="value">{row.value}</span>
                        </div>
                    ))}
                </div>
            </Section>

            <Section
                title="Clear"
                blurb="Remembered facts and generated files survive this — forget facts in Memory, clear files in Image & video."
            >
                <div className="actions">
                    <button className="danger" onClick={onclear}>
                        Clear the conversation
                    </button>
                </div>
            </Section>
        </>
    );
}
