/**
 * An ordered fallback chain: tried top to bottom until one answers.
 *
 * This markup existed twice — once for web search, once for image
 * generation — as near-identical copies that had already drifted apart in
 * how they decided a provider counted as configured. One component, so the
 * two cannot disagree again.
 */

export interface ChainItem {
    id: string;
    /** False greys the row and marks it skipped. */
    ready: boolean;
    /** Why it is skipped, or what it does when it is not. */
    note: string;
}

interface Props {
    items: ChainItem[];
    /** Given the new order. The caller persists it. */
    onreorder: (order: string[]) => void;
}

export default function ProviderChain({ items, onreorder }: Props) {
    function move(from: number, to: number) {
        if (to < 0 || to >= items.length) return;
        const order = items.map((i) => i.id);
        const [moved] = order.splice(from, 1);
        order.splice(to, 0, moved);
        onreorder(order);
    }

    return (
        <ol className="chain">
            {items.map((item, i) => (
                <li className={item.ready ? "link" : "link skipped"} key={item.id}>
                    <span className="rank">{i + 1}</span>
                    <span className="main">
                        <span className="name">{item.id}</span>
                        <span className="note">{item.note}</span>
                    </span>
                    <span className="actions">
                        <button
                            className="tiny"
                            onClick={() => move(i, i - 1)}
                            disabled={i === 0}
                            aria-label={`Move ${item.id} up`}
                        >
                            ↑
                        </button>
                        <button
                            className="tiny"
                            onClick={() => move(i, i + 1)}
                            disabled={i === items.length - 1}
                            aria-label={`Move ${item.id} down`}
                        >
                            ↓
                        </button>
                    </span>
                </li>
            ))}
        </ol>
    );
}
