/**
 * The code interface.
 *
 * A file tree, an editor, and search — the parts of an editor you actually
 * need while working with an assistant. It is not trying to be VS Code; it is
 * trying to be the thing you keep open next to the conversation, so a change
 * the assistant suggests can be read, edited and saved without leaving.
 *
 * The editor is a plain textarea with a gutter rather than a syntax-
 * highlighting component: highlighting means shipping a tokenizer per
 * language, and the point here is editing a file the assistant just touched,
 * not replacing your editor.
 *
 * The gutter and the textarea take their size and line height from the same
 * two custom properties, and scroll in step. They have to be two elements —
 * nothing can be drawn inside a textarea's scroll box — so any disagreement
 * between them shows up immediately as numbers drifting off their lines.
 */
import { useEffect, useRef, useState } from "react";
import { api, type SearchHit, type WorkspaceEntry } from "../lib/api";
import { ask } from "./store/confirm";
import Icon from "./Icon";
import { chat } from "./store/chat";
import { toast } from "./store/toast";
import "./styles/codeview.css";

export default function CodeView() {
    const [root, setRoot] = useState<string | null>(null);
    const [pathDraft, setPathDraft] = useState("");

    /** Expanded folders, by path. */
    const [open, setOpen] = useState<Record<string, WorkspaceEntry[]>>({});
    const [expanded, setExpanded] = useState<Set<string>>(new Set());

    const [file, setFile] = useState<string | null>(null);
    const [text, setText] = useState("");
    /** What was last read or saved, to know whether there are changes. */
    const [saved, setSaved] = useState("");
    const dirty = text !== saved;

    const [query, setQuery] = useState("");
    const [hits, setHits] = useState<SearchHit[]>([]);
    const [searching, setSearching] = useState(false);
    /** True once a search has run and come back with nothing. */
    const [searchedEmpty, setSearchedEmpty] = useState(false);

    const editorRef = useRef<HTMLTextAreaElement | null>(null);
    const gutterRef = useRef<HTMLDivElement | null>(null);

    // Read fresh inside callbacks without pulling every field into every
    // effect's dependency list.
    const fileRef = useRef(file);
    fileRef.current = file;
    const dirtyRef = useRef(dirty);
    dirtyRef.current = dirty;
    const textRef = useRef(text);
    textRef.current = text;

    useEffect(() => {
        void (async () => {
            const current = await api.currentWorkspace();
            setRoot(current);
            if (current) await loadFolder("");
        })();
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    async function openFolder() {
        const path = pathDraft.trim();
        if (!path) return;
        try {
            const name = await api.openWorkspace(path);
            setRoot(path);
            setOpen({});
            setExpanded(new Set());
            setFile(null);
            clearSearch();
            await loadFolder("");
            toast.success(`Opened ${name}.`);
        } catch (e) {
            toast.failure(`Could not open ${path}.`, e);
        }
    }

    async function loadFolder(path: string) {
        try {
            const entries = await api.listWorkspace(path);
            setOpen((prev) => ({ ...prev, [path]: entries }));
        } catch (e) {
            toast.failure(`Could not read ${path || "the workspace root"}.`, e);
        }
    }

    async function toggle(entry: WorkspaceEntry) {
        if (!entry.isDir) return void openFile(entry.path);

        const next = new Set(expanded);
        if (next.has(entry.path)) {
            next.delete(entry.path);
        } else {
            next.add(entry.path);
            if (!open[entry.path]) await loadFolder(entry.path);
        }
        setExpanded(next);
    }

    async function openFile(path: string) {
        // Losing unsaved edits by clicking another file would be unforgivable.
        if (dirtyRef.current) {
            const discard = await ask({
                title: "Discard unsaved changes?",
                body: `${fileRef.current} has edits that have not been written to disk.`,
                confirmLabel: "Discard",
                cancelLabel: "Keep editing",
                danger: true,
            });
            if (!discard) return;
        }

        try {
            const contents = await api.readWorkspaceFile(path);
            setText(contents);
            setSaved(contents);
            setFile(path);
        } catch (e) {
            toast.failure(`Could not open ${path}.`, e);
        }
    }

    async function save() {
        if (!fileRef.current) return;
        try {
            await api.writeWorkspaceFile(fileRef.current, textRef.current);
            setSaved(textRef.current);
            toast.success(`Saved ${fileRef.current}.`);
        } catch (e) {
            toast.failure(`Could not save ${fileRef.current}.`, e);
        }
    }

    async function runSearch() {
        if (query.trim().length < 2) return;
        setSearching(true);
        setSearchedEmpty(false);
        try {
            const found = await api.searchWorkspace(query.trim());
            setHits(found);
            setSearchedEmpty(found.length === 0);
        } catch (e) {
            toast.failure("Search failed.", e);
        } finally {
            setSearching(false);
        }
    }

    function clearSearch() {
        setQuery("");
        setHits([]);
        setSearchedEmpty(false);
    }

    /** Asks the assistant about the open file, with the file as context. */
    function askAboutFile() {
        if (!file) return;
        chat.view = "chat";
        chat.input = `In ${file}, `;
    }

    function onKeyDown(event: React.KeyboardEvent<HTMLTextAreaElement>) {
        // Ctrl+S saves, as it does everywhere else.
        if (event.ctrlKey && event.key.toLowerCase() === "s") {
            event.preventDefault();
            void save();
        }
        // Tab indents rather than leaving the editor. Four spaces, matching
        // the rest of this repository.
        if (event.key === "Tab" && editorRef.current) {
            event.preventDefault();
            const editor = editorRef.current;
            const { selectionStart: start, selectionEnd: end } = editor;
            setText(`${textRef.current.slice(0, start)}    ${textRef.current.slice(end)}`);
            queueMicrotask(() => {
                if (editorRef.current) {
                    editorRef.current.selectionStart = editorRef.current.selectionEnd = start + 4;
                }
            });
        }
    }

    /** Keeps the gutter level with the text as the editor scrolls. */
    function syncScroll() {
        if (gutterRef.current && editorRef.current) {
            gutterRef.current.scrollTop = editorRef.current.scrollTop;
        }
    }

    const lineCount = text.split("\n").length;
    /** Just the file name: the bar would otherwise show a whole path. */
    const fileName = file?.split(/[\\/]/).pop() ?? "";

    function node(entry: WorkspaceEntry, depth: number): React.ReactNode {
        return (
            <div key={entry.path}>
                <button
                    className={`code-entry${file === entry.path ? " active" : ""}${entry.isDir ? " folder" : ""}`}
                    style={{ paddingLeft: `${8 + depth * 14}px` }}
                    onClick={() => toggle(entry)}
                >
                    <span className={`twist${expanded.has(entry.path) ? " open" : ""}`}>
                        {entry.isDir ? <Icon name="chevronRight" size={12} /> : null}
                    </span>
                    <span className="entry-name">{entry.name}</span>
                </button>

                {entry.isDir && expanded.has(entry.path)
                    ? (open[entry.path] ?? []).map((child) => node(child, depth + 1))
                    : null}
            </div>
        );
    }

    return (
        <div className="code">
            <aside className="tree">
                <div className="tree-head">
                    <input
                        value={pathDraft}
                        onChange={(e) => setPathDraft(e.target.value)}
                        placeholder={root ?? "Folder path…"}
                        spellCheck={false}
                        aria-label="Workspace folder path"
                        onKeyDown={(e) => e.key === "Enter" && openFolder()}
                    />
                    <button className="outline" onClick={openFolder} disabled={!pathDraft.trim()}>
                        Open
                    </button>
                </div>

                {root ? (
                    <>
                        <div className="code-search">
                            <input
                                value={query}
                                onChange={(e) => setQuery(e.target.value)}
                                disabled={searching}
                                placeholder={searching ? "Searching…" : "Search in files…"}
                                spellCheck={false}
                                aria-label="Search in files"
                                onKeyDown={(e) => e.key === "Enter" && runSearch()}
                            />
                            {query ? (
                                <button className="code-clear" onClick={clearSearch} aria-label="Clear search">
                                    <Icon name="close" size={13} />
                                </button>
                            ) : null}
                        </div>

                        {searching ? (
                            <div className="code-skeletons" aria-hidden="true">
                                {Array.from({ length: 6 }, (_, row) => (
                                    <div key={row} className="code-skeleton" style={{ width: `${82 - row * 7}%` }}></div>
                                ))}
                            </div>
                        ) : hits.length ? (
                            <div className="hits">
                                <div className="code-section">
                                    <span>{hits.length} matches</span>
                                    <button onClick={clearSearch}>Clear</button>
                                </div>
                                {hits.map((hit) => (
                                    <button
                                        key={hit.path + hit.line}
                                        className="hit"
                                        onClick={() => openFile(hit.path)}
                                    >
                                        <span className="hit-path">
                                            {hit.path}:{hit.line}
                                        </span>
                                        <span className="hit-text">{hit.text}</span>
                                    </button>
                                ))}
                            </div>
                        ) : searchedEmpty ? (
                            <div className="code-state">
                                <p className="code-state-title">Nothing matches "{query}"</p>
                                <button className="outline" onClick={clearSearch}>
                                    Back to the tree
                                </button>
                            </div>
                        ) : (
                            <div className="entries">
                                {(open[""] ?? []).map((entry) => node(entry, 0))}
                            </div>
                        )}
                    </>
                ) : (
                    <div className="code-state">
                        <Icon name="code" size={22} />
                        <p className="code-state-title">No folder open</p>
                        <p className="code-state-body">
                            Put a folder path in the box above and press Enter. The
                            assistant reads and writes inside it, and nowhere else.
                        </p>
                    </div>
                )}
            </aside>

            <main className="editor">
                <div className="code-bar">
                    <span className="filename" title={file ?? ""}>
                        {file ? (
                            <>
                                {fileName}
                                {dirty ? <span className="code-dot" title="Unsaved changes"></span> : null}
                            </>
                        ) : (
                            <span className="muted">No file open</span>
                        )}
                    </span>

                    {file ? (
                        <div className="bar-actions">
                            <button onClick={askAboutFile}>
                                <Icon name="chat" size={14} />
                                Ask about this
                            </button>
                            <button className="primary" disabled={!dirty} onClick={save}>
                                Save
                                <kbd>Ctrl S</kbd>
                            </button>
                        </div>
                    ) : null}
                </div>

                {file ? (
                    <div className="code-pane">
                        {/* A gutter rather than nothing: line numbers are how people
                             talk about code, and the assistant quotes them back. */}
                        <div className="gutter" ref={gutterRef} aria-hidden="true">
                            {Array.from({ length: lineCount }, (_, i) => (
                                <div key={i}>{i + 1}</div>
                            ))}
                        </div>
                        <textarea
                            ref={editorRef}
                            value={text}
                            onChange={(e) => setText(e.target.value)}
                            onKeyDown={onKeyDown}
                            onScroll={syncScroll}
                            spellCheck={false}
                            wrap="off"
                            aria-label={file}
                        ></textarea>
                    </div>
                ) : (
                    <div className="code-state blank">
                        <Icon name="code" size={26} />
                        <p className="code-state-title">Nothing open</p>
                        <p className="code-state-body">
                            Pick a file from the tree, or search the workspace to jump
                            straight to a line.
                        </p>
                    </div>
                )}
            </main>
        </div>
    );
}
