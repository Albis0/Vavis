/**
 * Release checks.
 *
 * Vavis never updates itself. The check reads the project's release page
 * and reports what it found; installing stays a decision someone makes.
 * A check that could not run is reported as exactly that, never folded
 * into "up to date" — silence is not evidence.
 */

import { api, type Status, type UpdateCheck } from "../../../lib/api";
import Section from "../Section";

interface Props {
    status: Status | null;
    update: UpdateCheck | null;
    checking: boolean;
    oncheck: () => void;
}

export default function UpdatesPane({ status, update, checking, oncheck }: Props) {
    return (
        <>
            <h2>Updates</h2>

            <Section
                title="This build"
                blurb="Vavis does not install updates by itself and does not check in the background. Nothing about you is sent with the check."
            >
                <div className="stat-row">
                    <span className="label">Version</span>
                    <span className="value">{status?.version ?? ""}</span>
                </div>

                <div className="actions">
                    <button onClick={oncheck} disabled={checking}>
                        {checking ? "checking…" : "check for updates"}
                    </button>
                </div>
            </Section>

            {update?.status === "available" && (
                <Section title="Available">
                    <div className="update-box">
                        <p className="update-head">
                            Version {update.latest} is out — you have {update.current}.
                        </p>
                        {update.notes && <pre className="snippet selectable">{update.notes}</pre>}
                        <div className="actions">
                            <button className="primary" onClick={() => api.openReleasePage()}>
                                open the download page
                            </button>
                        </div>
                        <p className="blurb">
                            The page has the installer and a checksum. Close Vavis before running it.
                        </p>
                    </div>
                </Section>
            )}
            {update?.status === "upToDate" && (
                <Section title="Result">
                    <p className="blurb">You are on the newest release ({update.current}).</p>
                </Section>
            )}
            {update?.status === "failed" && (
                <Section title="Result">
                    {/* Deliberately not phrased as "up to date": a check that could not
                        run has not established anything. */}
                    <p className="blurb warn-text">
                        Could not check: {update.error}. Your build is {update.current}.
                    </p>
                    <div className="actions">
                        <button onClick={() => api.openReleasePage()}>
                            open the release page anyway
                        </button>
                    </div>
                </Section>
            )}
        </>
    );
}
