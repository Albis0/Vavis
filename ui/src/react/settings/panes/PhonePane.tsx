/**
 * The phone: a Telegram bot as a second way into the assistant.
 *
 * Three steps, shown in order and only as far as the user has got: paste a
 * bot token, pair the account with a one-time code, then use it. Pairing is
 * by code rather than by "first person to message the bot", because a bot's
 * name can be found and messaged by anyone.
 */

import { useEffect, useState } from "react";
import { api, type PhoneSettings } from "../../../lib/api";
import { toast } from "../../store/toast";
import Section from "../Section";

export default function PhonePane() {
    const [phone, setPhone] = useState<PhoneSettings | null>(null);
    const [token, setToken] = useState("");
    const [busy, setBusy] = useState(false);

    async function reload() {
        try {
            setPhone(await api.phoneSettings());
        } catch (e) {
            toast.failure("Could not read the phone settings.", e);
        }
    }

    // Poll while a pairing code is showing: pairing completes on the phone,
    // and the screen should notice without being reopened.
    useEffect(() => {
        void reload();
        const t = setInterval(() => void reload(), 3000);
        return () => clearInterval(t);
    }, []);

    async function saveToken() {
        if (!token.trim()) return;
        setBusy(true);
        try {
            const name = await api.setTelegramToken(token.trim());
            setToken("");
            await api.telegramPairingCode();
            toast.success(`Connected to @${name}.`);
            await reload();
        } catch (e) {
            toast.failure("That token did not work.", e);
        } finally {
            setBusy(false);
        }
    }

    return (
        <>
            <h2>Phone</h2>

            <Section
                title="1 · Bot"
                blurb="Talk to Vavis from anywhere through your own Telegram bot. In Telegram, message @BotFather, send /newbot, and paste the token it gives you here. The token is encrypted like every other key."
            >
                {phone?.hasToken && phone.botName && (
                    <p className="hint">
                        Connected to <strong>@{phone.botName}</strong>
                        {phone.enabled ? "" : " — switched off"}.
                    </p>
                )}
                <div className="actions">
                    <input
                        className="key-input"
                        type="password"
                        value={token}
                        placeholder={phone?.hasToken ? "paste a new token to replace it…" : "123456789:AA…"}
                        onChange={(e) => setToken(e.target.value)}
                        onKeyDown={(e) => e.key === "Enter" && void saveToken()}
                        aria-label="Telegram bot token"
                    />
                    <button onClick={saveToken} disabled={busy || !token.trim()}>
                        {busy ? "checking…" : "save"}
                    </button>
                </div>
                {phone?.error && <p className="result bad">✕ {phone.error}</p>}
            </Section>

            {phone?.hasToken && (
                <Section
                    title="2 · Pair your account"
                    blurb="The bot answers exactly one Telegram account. Send it the code below from yours; anyone else who finds the bot gets no reply at all."
                >
                    {phone.paired ? (
                        <>
                            <p className="hint">
                                Paired with <strong>{phone.ownerName || "your account"}</strong>.
                            </p>
                            <div className="actions">
                                <button
                                    className="danger"
                                    onClick={async () => {
                                        await api.telegramUnpair();
                                        await reload();
                                    }}
                                >
                                    unpair
                                </button>
                            </div>
                        </>
                    ) : phone.pairingCode ? (
                        <>
                            <p className="hint">Send this to @{phone.botName} within ten minutes:</p>
                            <p className="pairing-code">/pair {phone.pairingCode}</p>
                        </>
                    ) : (
                        <div className="actions">
                            <button
                                onClick={async () => {
                                    await api.telegramPairingCode();
                                    await reload();
                                }}
                            >
                                show a pairing code
                            </button>
                        </div>
                    )}
                </Section>
            )}

            {phone?.hasToken && (
                <Section
                    title="3 · Use"
                    blurb="Write to the bot as you would in this window. Anything destructive — writing a file, running a command, clicking — is sent to your phone as a question with buttons, every time, even with full authority on: a phone can be in someone else's hand. Ask Vavis to “tell me on my phone” and it will; automations can too. /yeni starts a new conversation."
                >
                    <label className="switch">
                        <input
                            type="checkbox"
                            checked={phone.enabled}
                            onChange={async (e) => {
                                await api.setTelegramEnabled(e.target.checked);
                                await reload();
                            }}
                        />
                        <span>Bot on</span>
                    </label>
                </Section>
            )}
        </>
    );
}
