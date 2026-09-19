/**
 * The command palette.
 *
 * One place to reach everything, opened with Ctrl+K. The brief was explicit
 * that the user should not have to hunt through menus or remember a command
 * to type -- so every action the interface can perform is listed here, found
 * by typing part of its name, and run with Enter.
 *
 * This is also what keeps the rest of the window empty. Actions do not need
 * a permanent button on screen if they are all one keystroke away, which is
 * what lets the stage be a reactor and nothing else.
 */
import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import Icon, { type IconName } from "./Icon";
import Modal from "./Modal";
import "./styles/commandpalette.css";

export interface Command {
    id: string;
    label: string;
    /** Grouping heading. Commands are listed under it in insertion order. */
    group: string;
    icon: IconName;
    /** Shown right-aligned, when the action also has a shortcut. */
    hint?: string;
    /** Extra words to match on, for things the label does not say. */
    keywords?: string;
    run: () => void;
}

interface Props {
    commands: Command[];
    onClose: () => void;
}

export default function CommandPalette({ commands, onClose }: Props) {
    const [query, setQuery] = useState("");
    const [selected, setSelected] = useState(0);
    const listRef = useRef<HTMLDivElement | null>(null);

    /**
     * Filter and rank.
     *
     * A label that *starts* with what was typed outranks one that merely
     * contains it, so typing "se" puts "Settings" above "Toggle voice mode".
     * Anything past that is ordering by relevance nobody asked for.
     */
    const matches = useMemo(() => {
        const q = query.trim().toLowerCase();
        if (!q) return commands;

        return commands
            .map((command) => {
                const label = command.label.toLowerCase();
                const haystack = `${label} ${command.keywords ?? ""} ${command.group.toLowerCase()}`;
                if (!haystack.includes(q)) return null;
                return { command, rank: label.startsWith(q) ? 0 : label.includes(q) ? 1 : 2 };
            })
            .filter((hit): hit is { command: Command; rank: number } => hit !== null)
            .sort((a, b) => a.rank - b.rank)
            .map((hit) => hit.command);
    }, [commands, query]);

    /** Reset the cursor whenever the result set changes under it. */
    useEffect(() => {
        setSelected(0);
    }, [matches]);

    /** Keeps the highlighted row in view during keyboard navigation. */
    useEffect(() => {
        queueMicrotask(() => {
            listRef.current
                ?.querySelector<HTMLElement>('[data-selected="true"]')
                ?.scrollIntoView({ block: "nearest" });
        });
    }, [selected]);

    function run(command: Command) {
        onClose();
        command.run();
    }

    function onKeyDown(event: React.KeyboardEvent<HTMLInputElement>) {
        if (event.key === "ArrowDown") {
            event.preventDefault();
            // Wraps, so holding Down never dead-ends at the bottom.
            setSelected((s) => (s + 1) % Math.max(matches.length, 1));
        } else if (event.key === "ArrowUp") {
            event.preventDefault();
            setSelected((s) => (s - 1 + matches.length) % Math.max(matches.length, 1));
        } else if (event.key === "Enter") {
            event.preventDefault();
            const command = matches[selected];
            if (command) run(command);
        }
        // Escape is not handled here: Modal owns it, so that with a dialog
        // open over the palette one press closes the dialog alone.
    }

    /** Group heading, emitted only when it changes down the list. */
    function headingFor(index: number): string | null {
        const group = matches[index]?.group;
        return index === 0 || matches[index - 1]?.group !== group
            ? (group ?? null)
            : null;
    }

    return (
        <Modal label="Commands" align="top" bare showClose={false} onClose={onClose}>
            <div className="palette-search">
                <Icon name="chevronRight" size={16} />
                <input
                    data-autofocus
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                    onKeyDown={onKeyDown}
                    placeholder="Search commands…"
                    spellCheck={false}
                    aria-label="Search commands"
                />
                <kbd>Esc</kbd>
            </div>

            <div className="palette-list" ref={listRef} role="listbox" tabIndex={-1}>
                {matches.map((command, index) => {
                    const heading = headingFor(index);
                    return (
                        <Fragment key={command.id}>
                            {heading ? <div className="group">{heading}</div> : null}

                            <button
                                className="palette-item"
                                role="option"
                                aria-selected={index === selected}
                                data-selected={index === selected}
                                onClick={() => run(command)}
                                onMouseEnter={() => setSelected(index)}
                            >
                                <Icon name={command.icon} size={16} />
                                <span className="palette-label">{command.label}</span>
                                {command.hint ? <kbd className="hint">{command.hint}</kbd> : null}
                            </button>
                        </Fragment>
                    );
                })}

                {matches.length === 0 ? (
                    <div className="none">
                        <p>No commands match "{query}"</p>
                        <button onClick={() => setQuery("")}>Clear search</button>
                    </div>
                ) : null}
            </div>
        </Modal>
    );
}
