/**
 * Which vault Vavis reads and writes.
 *
 * Vaults are discovered rather than typed in, because Obsidian already
 * keeps a list of them and a hand-typed path is one typo away from a
 * vault that silently contains nothing.
 */

import type { ConnectionTest, VaultInfo } from "../../../lib/api";
import Section from "../Section";

interface Props {
    vaults: VaultInfo[];
    result: ConnectionTest | undefined;
    testing: boolean;
    onpick: (path: string) => void;
    ontest: () => void;
}

export default function ObsidianPane({ vaults, result, testing, onpick, ontest }: Props) {
    return (
        <>
            <h2>Obsidian</h2>

            {vaults.length === 0 ? (
                <Section
                    title="No vault found"
                    blurb="Obsidian does not have to be running — Vavis reads the Markdown files directly — but it needs to know where the vault is."
                >
                    <p className="empty">
                        Open a vault in Obsidian once, then come back and it will show up here.
                    </p>
                </Section>
            ) : (
                <>
                    <Section
                        title="Vault"
                        blurb="Notes are read and written on disk, so this works whether or not Obsidian is open."
                    >
                        <div className="list">
                            {vaults.map((vault) => (
                                <button
                                    className={vault.active ? "row active" : "row"}
                                    title={vault.path}
                                    onClick={() => onpick(vault.path)}
                                    key={vault.path}
                                >
                                    {vault.active ? "● " : "○ "}
                                    {vault.name}
                                </button>
                            ))}
                        </div>
                    </Section>

                    <Section title="Check">
                        <div className="actions">
                            <button onClick={ontest} disabled={testing}>
                                {testing ? "reading…" : "test"}
                            </button>
                        </div>
                        {result && (
                            <p className={result.ok ? "result" : "result bad"}>
                                {result.ok ? "✓" : "✕"} {result.detail}
                            </p>
                        )}
                    </Section>
                </>
            )}
        </>
    );
}
