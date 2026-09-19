/**
 * Steam: a Web API key and a SteamID64.
 *
 * Both are required together — the key alone returns an empty library,
 * and so does a private profile, which is the failure people spend the
 * longest on because Steam reports neither as an error.
 */

import { useState } from "react";
import type { ConnectionTest, SteamSettings } from "../../../lib/api";
import Field from "../Field";
import Section from "../Section";

interface Props {
    steam: SteamSettings | null;
    steamId: string;
    result: ConnectionTest | undefined;
    testing: boolean;
    onsave: (steamId: string, key: string) => void;
    ontest: () => void;
}

export default function SteamPane({ steam, steamId, result, testing, onsave, ontest }: Props) {
    // Seeded from the prop rather than mirroring it: the settings reload
    // after a save, and a plain mirror (a `useEffect` writing the prop into
    // state on every change) would wipe what is being typed the moment that
    // lands.
    const [typedId, setTypedId] = useState<string | null>(null);
    const idDraft = typedId ?? steamId;

    const [keyDraft, setKeyDraft] = useState("");

    /** Steam ids are exactly 17 digits; anything else is a copy-paste slip. */
    const idError =
        idDraft.trim() === "" || /^\d{17}$/.test(idDraft.trim()) ? "" : "should be 17 digits";

    function save() {
        onsave(idDraft.trim(), keyDraft);
        setKeyDraft("");
    }

    return (
        <>
            <h2>Steam</h2>

            <Section
                title="Account"
                blurb="Game details must be public, or Steam returns an empty library without saying why."
            >
                <Field label="SteamID64" required error={idError} hint="17 digits">
                    <input
                        value={idDraft}
                        onChange={(e) => setTypedId(e.target.value)}
                        placeholder="76561198000000000"
                        spellCheck={false}
                    />
                </Field>

                <Field
                    label="Web API key"
                    required
                    hint={
                        steam?.hasKey ? "stored — paste to replace" : "from steamcommunity.com/dev/apikey"
                    }
                >
                    <input
                        type="password"
                        value={keyDraft}
                        onChange={(e) => setKeyDraft(e.target.value)}
                        placeholder={steam?.hasKey ? "••••••••" : "paste key…"}
                        onKeyDown={(e) => e.key === "Enter" && save()}
                    />
                </Field>

                <div className="actions">
                    <button className="primary" onClick={save}>
                        save and check
                    </button>
                    <button onClick={ontest} disabled={testing || !steam?.hasKey}>
                        {testing ? "asking…" : "test"}
                    </button>
                </div>

                {result && (
                    <p className={result.ok ? "result" : "result bad"}>
                        {result.ok ? "✓" : "✕"} {result.detail}
                    </p>
                )}
            </Section>

            <Section title="Without a key">
                <p className="blurb">
                    Which game is running is detected locally, so that part works even on a
                    private profile. The key is only needed for the library and playtime.
                </p>
            </Section>
        </>
    );
}
