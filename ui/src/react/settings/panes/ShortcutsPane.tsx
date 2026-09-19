/**
 * Keyboard shortcuts. A reference table, nothing to configure.
 *
 * The bindings live in the components that listen for them; this is a
 * written copy, which means it can drift. Keeping it here rather than
 * deriving it from the handlers is deliberate — the handlers are spread
 * across several components, and a wrong list is easier to spot than a
 * missing one.
 */

interface Props {
    /** [key, what it does] pairs. */
    shortcuts: readonly (readonly [string, string])[];
}

export default function ShortcutsPane({ shortcuts }: Props) {
    return (
        <>
            <h2>Shortcuts</h2>

            <div className="list">
                {shortcuts.map(([key, action]) => (
                    <div className="shortcut-row" key={key}>
                        <kbd>{key}</kbd>
                        <span>{action}</span>
                    </div>
                ))}
            </div>
        </>
    );
}
