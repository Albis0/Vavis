/**
 * Markdown rendering.
 *
 * Model answers arrive as markdown: code blocks, lists, tables, emphasis.
 * Shown as flat text they are unreadable.
 *
 * This is a small hand-written renderer rather than a library, for one
 * reason that matters: **it escapes HTML before it does anything else.**
 * A model answer is untrusted text — passing it to `innerHTML` unescaped
 * would be an injection hole, and the app has a strict CSP precisely
 * because that mistake is easy to make.
 *
 * Only the subset models actually emit is handled. Anything unrecognised
 * falls through as plain text, which beats rendering it wrong.
 */

/** Escapes the five characters that matter in HTML. */
function escapeHtml(text: string): string {
    return text
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;")
        .replace(/'/g, "&#39;");
}

/**
 * Inline formatting: code, bold, italic.
 *
 * Runs on already-escaped text, so the tags it inserts are the only HTML
 * in the result.
 */
function renderInline(escaped: string): string {
    return (
        escaped
            // Inline code first: its contents must not be re-processed for
            // emphasis, or `a*b*c` inside code would sprout tags.
            .replace(/`([^`]+)`/g, '<code class="inline">$1</code>')
            // Bold before italic, so ** is not eaten by the single-* rule.
            .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
            .replace(/__([^_]+)__/g, "<strong>$1</strong>")
            // Italic: require a non-space after the marker so "2 * 3 * 4"
            // stays arithmetic rather than becoming emphasis.
            .replace(/\*(\S[^*]*?)\*/g, "<em>$1</em>")
            .replace(/(?<![a-zA-Z0-9])_(\S[^_]*?)_(?![a-zA-Z0-9])/g, "<em>$1</em>")
    );
}

/** Renders markdown to HTML. Input is treated as untrusted. */
export function renderMarkdown(source: string): string {
    const lines = source.split("\n");
    const out: string[] = [];

    let paragraph: string[] = [];
    let listItems: string[] = [];
    let listOrdered = false;
    let tableRows: string[][] = [];
    let tableHasHeader = false;

    const flushParagraph = () => {
        if (paragraph.length === 0) return;
        const text = renderInline(escapeHtml(paragraph.join(" ")));
        out.push(`<p>${text}</p>`);
        paragraph = [];
    };

    const flushList = () => {
        if (listItems.length === 0) return;
        const tag = listOrdered ? "ol" : "ul";
        const items = listItems
            .map((item) => `<li>${renderInline(escapeHtml(item))}</li>`)
            .join("");
        out.push(`<${tag}>${items}</${tag}>`);
        listItems = [];
    };

    const flushTable = () => {
        if (tableRows.length === 0) return;

        let html = '<div class="table-wrap"><table>';
        tableRows.forEach((cells, index) => {
            const isHeader = tableHasHeader && index === 0;
            const tag = isHeader ? "th" : "td";
            const row = cells
                .map((c) => `<${tag}>${renderInline(escapeHtml(c))}</${tag}>`)
                .join("");
            html += `<tr>${row}</tr>`;
        });
        html += "</table></div>";

        out.push(html);
        tableRows = [];
        tableHasHeader = false;
    };

    const flushAll = () => {
        flushParagraph();
        flushList();
        flushTable();
    };

    for (let i = 0; i < lines.length; i++) {
        const line = lines[i];
        const trimmed = line.trim();

        // ── Fenced code ────────────────────────────────────────────────
        if (trimmed.startsWith("```")) {
            flushAll();

            const lang = trimmed.slice(3).trim();
            const code: string[] = [];
            i++;

            // Collect to the closing fence, or to the end if the answer was
            // truncated mid-block — a half-written block must still render.
            while (i < lines.length && !lines[i].trim().startsWith("```")) {
                code.push(lines[i]);
                i++;
            }

            const body = escapeHtml(code.join("\n"));
            const label = lang ? `<span class="lang">${escapeHtml(lang)}</span>` : "";
            out.push(
                `<div class="code-block">` +
                    `<div class="code-head">${label}` +
                    `<button class="copy" data-code="${encodeURIComponent(code.join("\n"))}">copy</button>` +
                    `</div><pre><code>${body}</code></pre></div>`,
            );
            continue;
        }

        // ── Heading ────────────────────────────────────────────────────
        const heading = /^(#{1,6})\s+(.+)$/.exec(trimmed);
        if (heading) {
            flushAll();
            const level = heading[1].length;
            out.push(
                `<h${level}>${renderInline(escapeHtml(heading[2]))}</h${level}>`,
            );
            continue;
        }

        // ── Horizontal rule ────────────────────────────────────────────
        if (/^([-*_])\1{2,}$/.test(trimmed)) {
            flushAll();
            out.push("<hr>");
            continue;
        }

        // ── Table ──────────────────────────────────────────────────────
        if (trimmed.startsWith("|") && trimmed.split("|").length > 2) {
            flushParagraph();
            flushList();

            const cells = trimmed
                .replace(/^\||\|$/g, "")
                .split("|")
                .map((c) => c.trim());

            // The |---|---| separator marks the row above as the header and is
            // not itself rendered.
            const isSeparator = cells.every((c) => /^:?-+:?$/.test(c));
            if (isSeparator) {
                tableHasHeader = tableRows.length > 0;
                continue;
            }

            tableRows.push(cells);
            continue;
        }

        // ── Blockquote ─────────────────────────────────────────────────
        if (trimmed.startsWith("> ")) {
            flushAll();
            out.push(
                `<blockquote>${renderInline(escapeHtml(trimmed.slice(2)))}</blockquote>`,
            );
            continue;
        }

        // ── List ───────────────────────────────────────────────────────
        const bullet = /^[-*+]\s+(.+)$/.exec(trimmed);
        const numbered = /^\d{1,3}[.)]\s+(.+)$/.exec(trimmed);

        if (bullet || numbered) {
            flushParagraph();
            flushTable();

            const ordered = Boolean(numbered);
            // A change of list type starts a new list.
            if (listItems.length > 0 && ordered !== listOrdered) flushList();

            listOrdered = ordered;
            listItems.push((bullet ?? numbered)![1]);
            continue;
        }

        // ── Blank line ends the current block ──────────────────────────
        if (trimmed === "") {
            flushAll();
            continue;
        }

        // ── Ordinary prose ─────────────────────────────────────────────
        flushList();
        flushTable();
        paragraph.push(trimmed);
    }

    flushAll();
    return out.join("");
}
