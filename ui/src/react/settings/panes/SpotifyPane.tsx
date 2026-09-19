/**
 * Spotify, which is one button in the normal case.
 *
 * The built-in application means nobody has to register anything, so the
 * client id lives behind a disclosure. It used to be the first thing on
 * the screen, which turned a one-click connection into a form.
 */

import { useState } from "react";
import type { ConnectionTest, SpotifySettings } from "../../../lib/api";
import Field from "../Field";
import Section from "../Section";

interface Props {
    spotify: SpotifySettings | null;
    clientId: string;
    result: ConnectionTest | undefined;
    testing: boolean;
    onconnect: () => void;
    ondisconnect: () => void;
    ontest: () => void;
    onsaveid: (id: string) => void;
}

export default function SpotifyPane({
    spotify,
    clientId,
    result,
    testing,
    onconnect,
    ondisconnect,
    ontest,
    onsaveid,
}: Props) {
    // Seeded from the prop rather than mirroring it: `load()` refreshes
    // settings after a save, and a plain mirror (a `useEffect` writing the
    // prop into state on every change) would wipe what is being typed the
    // moment that lands.
    const [typed, setTyped] = useState<string | null>(null);
    const draft = typed ?? clientId;

    const [ownApp, setOwnApp] = useState(false);

    return (
        <>
            <h2>Spotify</h2>

            {spotify?.connected ? (
                <Section title="Connection">
                    <p className="result">✓ Connected.</p>
                    <div className="actions">
                        <button onClick={ontest} disabled={testing}>
                            {testing ? "asking…" : "test"}
                        </button>
                        <button className="danger" onClick={ondisconnect}>
                            disconnect
                        </button>
                    </div>
                    {result && (
                        <p className={result.ok ? "result" : "result bad"}>
                            {result.ok ? "✓" : "✕"} {result.detail}
                        </p>
                    )}
                </Section>
            ) : (
                <>
                    <Section
                        title="Connection"
                        blurb="Opens Spotify in your browser. Approve there and you are done — there is nothing to set up first."
                    >
                        <div className="actions">
                            <button className="primary" onClick={onconnect}>
                                connect
                            </button>
                        </div>
                    </Section>

                    <Section title="Advanced">
                        <button className="disclosure" onClick={() => setOwnApp(!ownApp)}>
                            {ownApp ? "▾" : "▸"} use my own Spotify app
                        </button>

                        {ownApp && (
                            <>
                                <p className="blurb">
                                    Only worth doing if you want your own name on the consent screen.
                                    Register this exact redirect URI on the app, then paste its client id
                                    here.
                                </p>

                                <Field label="Redirect URI" hint="register this on your app">
                                    <code className="path selectable">{spotify?.redirectUri ?? ""}</code>
                                </Field>

                                <Field label="Client id" fallback="the built-in app">
                                    <input
                                        value={draft}
                                        onChange={(e) => setTyped(e.target.value)}
                                        placeholder="client id (optional)"
                                        spellCheck={false}
                                    />
                                </Field>

                                <div className="actions">
                                    <button onClick={() => onsaveid(draft.trim())}>save id</button>
                                </div>
                            </>
                        )}
                    </Section>
                </>
            )}
        </>
    );
}
