/**
 * The status bar.
 *
 * A single quiet line along the bottom. It replaces a 190-pixel telemetry
 * rail that ran the full height of the window showing CPU meters, uptime and
 * a shortcut list -- none of which is worth a column of permanent real estate
 * in an interface whose whole point is an empty stage.
 *
 * What survives is only what answers a question the user might actually have
 * mid-conversation: which model is about to answer, how many tools it can
 * reach, and whether anything is wrong. Each one is a button that opens the
 * thing it describes, so reading a value and changing it are the same
 * gesture.
 *
 * Load is shown only when it is high enough to explain something feeling
 * slow. A meter that reads 4% all day is decoration.
 */
import { useStore } from "./store/useStore";
import { chat, chatSignal } from "./store/chat";
import Icon from "./Icon";
import "./styles/statusBar.css";

interface Props {
    onOpenSettings: () => void;
}

/** Above this, load is worth mentioning; below it, it is noise. */
const BUSY_CPU = 55;

/** Shortens a model id so it fits without wrapping the bar. */
function short(name: string, max = 26): string {
    return name.length <= max ? name : `${name.slice(0, max - 1)}…`;
}

export default function StatusBar({ onOpenSettings }: Props) {
    const state = useStore(chatSignal, chat);
    const status = state.status;

    return (
        <footer className="status-bar">
            <div className="left">
                {!status?.keys.length ? (
                    // The one genuine error state: with no key nothing can work, so it
                    // gets colour and says what to do about it.
                    <button className="status-item status-warn" onClick={onOpenSettings}>
                        <Icon name="warning" size={13} />
                        <span>No API key — open settings</span>
                    </button>
                ) : (
                    <button className="status-item" onClick={onOpenSettings} title="Change model">
                        <span className="provider">{status.provider}</span>
                        <span className="sep">/</span>
                        <span className="model" title={status.model}>
                            {short(status.model)}
                        </span>
                    </button>
                )}
            </div>

            <div className="right">
                {status?.fullAuthority ? (
                    // The one indicator that has to be here rather than in settings.
                    // Full authority's whole effect is that nothing appears -- no prompts,
                    // no budget warnings -- so without a standing marker the mode is
                    // indistinguishable from a quiet session. Clicking goes to the switch
                    // that turns it off.
                    <button
                        className="status-item status-warn"
                        onClick={onOpenSettings}
                        title="Every approval is off. Click to change."
                    >
                        <Icon name="warning" size={12} />
                        <span>Full authority</span>
                    </button>
                ) : null}

                {status?.steamGame ? (
                    // Context, not a control: you do not steer a game from a chat
                    // window. One line, and it disappears when the process does.
                    <span className="status-item quiet" title="Running now">
                        <span className="playing-dot"></span>
                        {status.steamGame}
                    </span>
                ) : null}

                {(status?.cpu ?? 0) >= BUSY_CPU ? (
                    <span className="status-item quiet" title="System load">
                        {status?.cpu}% CPU
                    </span>
                ) : null}

                {status?.battery != null && status.battery < 20 ? (
                    <span className="status-item status-warn" title="Battery low">
                        {status.battery}%
                    </span>
                ) : null}

                <button
                    className="status-item"
                    onClick={() => (chat.panel = "tools")}
                    title="Tools available to the model"
                >
                    <Icon name="tool" size={12} />
                    {status?.toolCount ?? 0}
                </button>

                {(status?.factCount ?? 0) > 0 ? (
                    <button
                        className="status-item"
                        onClick={() => (chat.panel = "memory")}
                        title="Facts remembered about you"
                    >
                        <Icon name="memory" size={12} />
                        {status?.factCount}
                    </button>
                ) : null}

                <span className="status-item version">v{status?.version ?? "—"}</span>
            </div>
        </footer>
    );
}
