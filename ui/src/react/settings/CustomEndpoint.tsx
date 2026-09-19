/**
 * A custom endpoint: the one field that is required, then the rest folded
 * away.
 *
 * The old screen showed seven equal-looking inputs in a column with the
 * defaults buried in placeholder text, so it read as seven things to fill
 * in. Only the address is actually required — the backend's own check is
 * `!url.is_empty() && url.contains("{query}")` and every other field falls
 * back to a default at request time. Six optional boxes presented as
 * mandatory is a worse lie than hiding them.
 */

import { useState } from "react";
import Field from "./Field";

export interface OptionalField {
    key: string;
    label: string;
    /** What an empty value resolves to. Named, not implied. */
    fallback?: string;
    hint?: string;
}

interface Props {
    /** The required address. */
    url: string;
    urlPlaceholder: string;
    /** A substring the address must contain, if the backend demands one. */
    requires?: string;
    /** Explains the endpoint in a sentence. */
    blurb: string;
    fields: OptionalField[];
    /** Current values for the optional fields, keyed by `key`. */
    values: Record<string, string>;
    onUrlChange: (url: string) => void;
    onValuesChange: (values: Record<string, string>) => void;
    onchange: (patch: { url?: string; values?: Record<string, string> }) => void;
}

export default function CustomEndpoint({
    url,
    urlPlaceholder,
    requires = "",
    blurb,
    fields,
    values,
    onUrlChange,
    onValuesChange,
    onchange,
}: Props) {
    const [expanded, setExpanded] = useState(false);

    /**
     * Checked here as well as in the backend, because the backend only
     * answers after a save: the old flow accepted the address, then raised a
     * toast about it, leaving a saved-but-broken value behind.
     */
    const urlError =
        (url ?? "").trim() && requires && !url.includes(requires)
            ? `must contain ${requires}`
            : "";

    /** How many optional fields the user has actually set. */
    const filled = fields.filter((f) => (values[f.key] ?? "").trim()).length;

    return (
        <div className="custom">
            <p className="blurb">{blurb}</p>

            <Field
                label="Address"
                required
                error={urlError}
                hint={requires ? `must contain ${requires}` : ""}
            >
                <input
                    value={url}
                    onChange={(e) => onUrlChange(e.target.value)}
                    placeholder={urlPlaceholder}
                    spellCheck={false}
                    onBlur={() => !urlError && onchange({ url })}
                />
            </Field>

            <button
                className="disclosure"
                onClick={() => setExpanded(!expanded)}
                aria-expanded={expanded}
            >
                <span className={expanded ? "arrow open" : "arrow"}>›</span>
                Advanced — field mapping
                <span className="count">
                    {filled > 0 ? `${filled} set` : `${fields.length} optional`}
                </span>
            </button>

            {expanded && (
                <div className="advanced">
                    {fields.map((f) => (
                        <Field key={f.key} label={f.label} fallback={f.fallback} hint={f.hint}>
                            <input
                                value={values[f.key] ?? ""}
                                onChange={(e) =>
                                    onValuesChange({ ...values, [f.key]: e.target.value })
                                }
                                placeholder={f.fallback ?? "optional"}
                                spellCheck={false}
                                onBlur={() => onchange({ values })}
                            />
                        </Field>
                    ))}
                </div>
            )}
        </div>
    );
}
