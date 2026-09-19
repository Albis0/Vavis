/**
 * The handful of settings that belong to no integration.
 *
 * Each applies on change rather than on a save button, so the reader sees
 * the effect of a choice while still looking at the control that made it.
 *
 * Svelte's `onchange` on a text input fires on blur or Enter, not per
 * keystroke; React's `onChange` fires per keystroke. The text and number
 * fields below use `onBlur` to keep that behaviour -- a controlled
 * `onChange` here would save on every keystroke, which is not what the
 * Svelte version did.
 */

import type { Status } from "../../../lib/api";
import Field from "../Field";
import Section from "../Section";

interface Props {
    status: Status | null;
    languages: readonly (readonly [string, string])[];
    windowModes: readonly string[];
    onchange: (key: string, value: string) => void;
}

export default function GeneralPane({ status, languages, windowModes, onchange }: Props) {
    return (
        <>
            <h2>General</h2>

            <Section title="Identity">
                <Field label="Assistant name" hint="spoken aloud, so pick something sayable">
                    <input
                        defaultValue={status?.assistantName ?? ""}
                        key={status?.assistantName ?? ""}
                        onBlur={(e) => onchange("name", e.target.value)}
                    />
                </Field>

                <Field label="Language" inline>
                    {/* A key tied to the current value, not a controlled `value`: the
                        select renders before its options exist in the Svelte version,
                        so `selected` on the option was used instead of `value` on the
                        select to avoid a value naming an option that is not there yet
                        being dropped. React's `defaultValue` on the select is the
                        equivalent -- it is read once, at mount, the same as `selected`
                        was. */}
                    <select
                        defaultValue={status?.language ?? "en"}
                        key={status?.language ?? "en"}
                        onChange={(e) => onchange("language", e.target.value)}
                    >
                        {languages.map(([code, name]) => (
                            <option key={code} value={code}>
                                {name}
                            </option>
                        ))}
                    </select>
                </Field>
            </Section>

            <Section title="Window">
                <Field label="Mode" inline>
                    <select
                        defaultValue={status?.windowMode ?? "windowed"}
                        key={status?.windowMode ?? "windowed"}
                        onChange={(e) => onchange("windowMode", e.target.value)}
                    >
                        {windowModes.map((mode) => (
                            <option key={mode} value={mode}>
                                {mode}
                            </option>
                        ))}
                    </select>
                </Field>

                <Field label="Font size" inline hint="8–32">
                    <input
                        type="number"
                        min="8"
                        max="32"
                        defaultValue={status?.fontSize ?? 14}
                        key={status?.fontSize ?? 14}
                        onBlur={(e) => onchange("fontSize", e.target.value)}
                    />
                </Field>
            </Section>

            <Section title="Build">
                <div className="stat-row">
                    <span className="label">Version</span>
                    <span className="value">{status?.version ?? "—"}</span>
                </div>
            </Section>
        </>
    );
}
