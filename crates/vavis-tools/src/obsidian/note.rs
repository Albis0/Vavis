//! Parsing one Markdown note.
//!
//! Obsidian's conventions are what make a vault more than a folder of text:
//! frontmatter carries structured fields, `[[wikilinks]]` carry the graph,
//! and tags carry the user's own taxonomy. Reading a note as plain prose
//! throws all of that away, so it is parsed here once and reused everywhere.

use std::collections::BTreeSet;

/// A parsed note.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Note {
    /// Vault-relative path, forward-slashed, e.g. `Vavis/integrations/steam.md`.
    pub path: String,
    /// Display title: the frontmatter `title`, else the first H1, else the
    /// file stem. Obsidian itself keys on the file name, so the stem is the
    /// dependable fallback.
    pub title: String,
    /// Raw frontmatter block, without the `---` fences.
    pub frontmatter: String,
    /// Everything after the frontmatter.
    pub body: String,
    /// Tags from both the frontmatter and the body, without the leading `#`.
    pub tags: BTreeSet<String>,
    /// Wikilink targets, without the brackets, alias or heading fragment.
    pub links: BTreeSet<String>,
    /// Embed targets (`![[...]]`) — tracked separately because they are not
    /// expanded when reading.
    pub embeds: BTreeSet<String>,
    /// Heading text in document order.
    pub headings: Vec<String>,
}

/// Splits the frontmatter block from the body.
///
/// Only a block that starts on the very first line counts; a `---` further
/// down is a horizontal rule, not frontmatter.
pub fn split_frontmatter(text: &str) -> (String, String) {
    // A leading BOM would otherwise hide the opening fence.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);

    let Some(rest) = text.strip_prefix("---") else {
        return (String::new(), text.to_string());
    };
    // The opening fence must be alone on its line.
    let rest = match rest.strip_prefix("\r\n") {
        Some(r) => r,
        None => match rest.strip_prefix('\n') {
            Some(r) => r,
            None => return (String::new(), text.to_string()),
        },
    };

    // Find the closing fence at the start of a line.
    let mut offset = 0usize;
    for line in rest.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed == "---" {
            let front = &rest[..offset];
            let body = &rest[offset + line.len()..];
            return (front.to_string(), body.to_string());
        }
        offset += line.len();
    }

    // Unterminated block: treat the whole file as body rather than swallowing
    // it. A malformed note should still be readable.
    (String::new(), text.to_string())
}

/// Reads `key: value` pairs and `- item` lists out of a frontmatter block.
///
/// Deliberately not a YAML parser: a dependency would buy nesting and anchors
/// that vault frontmatter almost never uses, and the failure mode here is a
/// missed tag rather than a crash.
pub fn frontmatter_field(frontmatter: &str, key: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut in_list = false;

    for line in frontmatter.lines() {
        let trimmed = line.trim_end();

        if in_list {
            let item = trimmed.trim_start();
            if let Some(rest) = item.strip_prefix("- ") {
                values.push(clean_scalar(rest));
                continue;
            }
            // A non-indented, non-list line ends the block.
            if !trimmed.starts_with(char::is_whitespace) && !item.is_empty() {
                in_list = false;
            } else {
                continue;
            }
        }

        let Some((name, value)) = trimmed.split_once(':') else {
            continue;
        };
        if !name.trim().eq_ignore_ascii_case(key) {
            continue;
        }

        let value = value.trim();
        if value.is_empty() {
            // The values are on the following lines.
            in_list = true;
        } else if let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
            values.extend(inner.split(',').map(clean_scalar).filter(|s| !s.is_empty()));
        } else {
            values.push(clean_scalar(value));
        }
    }

    values.into_iter().filter(|s| !s.is_empty()).collect()
}

fn clean_scalar(s: &str) -> String {
    s.trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

/// Collects `#tag` occurrences from note body text.
///
/// Skips fenced code blocks and anything that looks like a URL fragment or a
/// Markdown heading, all of which use `#` for something else.
fn body_tags(body: &str) -> BTreeSet<String> {
    let mut tags = BTreeSet::new();
    let mut in_fence = false;

    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence || trimmed.starts_with('#') && trimmed.contains(' ') {
            // `# Heading` — the hash is structure, not a tag.
            if !in_fence && is_heading(trimmed) {
                continue;
            }
            if in_fence {
                continue;
            }
        }

        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] != '#' {
                i += 1;
                continue;
            }
            // A tag starts at the beginning of the line or after whitespace,
            // which rules out `https://x#frag` and `C#` in prose.
            let preceded_ok = i == 0 || chars[i - 1].is_whitespace();
            if !preceded_ok {
                i += 1;
                continue;
            }
            let mut j = i + 1;
            while j < chars.len() && is_tag_char(chars[j]) {
                j += 1;
            }
            let tag: String = chars[i + 1..j].iter().collect();
            // A bare `#` or a numeric tag is not a tag.
            if !tag.is_empty() && tag.chars().any(|c| !c.is_ascii_digit()) {
                tags.insert(tag.trim_end_matches('/').to_string());
            }
            i = j.max(i + 1);
        }
    }

    tags
}

fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_' || c == '/'
}

fn is_heading(trimmed_line: &str) -> bool {
    let hashes = trimmed_line.chars().take_while(|c| *c == '#').count();
    (1..=6).contains(&hashes) && trimmed_line.chars().nth(hashes) == Some(' ')
}

/// Extracts wikilink and embed targets.
///
/// Handles `[[Note]]`, `[[Note|shown]]`, `[[Note#Heading]]` and — inside a
/// markdown table — `[[Note\|shown]]`, where Obsidian requires the alias pipe
/// to be escaped so the table parser does not read it as a column break.
///
/// The escaped form is not an edge case: it is what Obsidian itself writes
/// when you drop a link into a table. Missing it left a trailing backslash on
/// the target, so `[[00 - Start Here\|context]]` resolved to
/// `00 - Start Here\` and matched no note — the link looked broken in every
/// tool that reported it, while Obsidian resolved it fine.
fn extract_links(body: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut links = BTreeSet::new();
    let mut embeds = BTreeSet::new();

    let chars: Vec<char> = body.chars().collect();
    let mut i = 0;
    while i + 1 < chars.len() {
        if chars[i] == '[' && chars[i + 1] == '[' {
            let is_embed = i > 0 && chars[i - 1] == '!';
            let Some(end) = find_close(&chars, i + 2) else {
                // Unclosed bracket: step over it and keep scanning, so one
                // stray `[[` does not hide every later link in the note.
                i += 2;
                continue;
            };
            let inner: String = chars[i + 2..end].iter().collect();
            let target = inner
                .split(['|', '#'])
                .next()
                .unwrap_or_default()
                .trim()
                // The backslash of an escaped `\|` stays on the target after
                // the split; it belongs to the table syntax, not to the name.
                .trim_end_matches('\\')
                .trim()
                .to_string();
            if !target.is_empty() {
                if is_embed {
                    embeds.insert(target);
                } else {
                    links.insert(target);
                }
            }
            i = end + 2;
            continue;
        }
        i += 1;
    }

    (links, embeds)
}

fn find_close(chars: &[char], from: usize) -> Option<usize> {
    let mut i = from;
    while i + 1 < chars.len() {
        if chars[i] == ']' && chars[i + 1] == ']' {
            return Some(i);
        }
        // A link never spans a blank line; bail out rather than swallowing
        // the rest of the note on an unclosed bracket.
        if chars[i] == '\n' && i + 1 < chars.len() && chars[i + 1] == '\n' {
            return None;
        }
        i += 1;
    }
    None
}

fn extract_headings(body: &str) -> Vec<String> {
    let mut headings = Vec::new();
    let mut in_fence = false;

    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence || !is_heading(trimmed) {
            continue;
        }
        let text = trimmed.trim_start_matches('#').trim();
        if !text.is_empty() {
            headings.push(text.to_string());
        }
    }

    headings
}

/// The file stem of a vault-relative path.
pub fn stem(path: &str) -> String {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .trim_end_matches(".md")
        .to_string()
}

impl Note {
    /// Parses raw file contents.
    pub fn parse(path: &str, raw: &str) -> Self {
        let (frontmatter, body) = split_frontmatter(raw);
        let (links, embeds) = extract_links(&body);
        let headings = extract_headings(&body);

        let mut tags = body_tags(&body);
        for key in ["tags", "tag"] {
            for value in frontmatter_field(&frontmatter, key) {
                // Frontmatter tags may or may not carry the hash.
                let cleaned = value.trim_start_matches('#').trim().to_string();
                if !cleaned.is_empty() {
                    tags.insert(cleaned);
                }
            }
        }

        let title = frontmatter_field(&frontmatter, "title")
            .into_iter()
            .next()
            .filter(|t| !t.is_empty())
            .or_else(|| headings.first().cloned())
            .unwrap_or_else(|| stem(path));

        Self {
            path: path.to_string(),
            title,
            frontmatter,
            body,
            tags,
            links,
            embeds,
            headings,
        }
    }

    /// True when this note carries `tag` or a sub-tag of it.
    ///
    /// `#project` is expected to find `#project/vavis`, which is how Obsidian
    /// treats nested tags.
    pub fn has_tag(&self, tag: &str) -> bool {
        let wanted = tag.trim_start_matches('#').to_lowercase();
        self.tags.iter().any(|t| {
            let t = t.to_lowercase();
            t == wanted || t.starts_with(&format!("{wanted}/"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_is_split_from_body() {
        let raw = "---\ntitle: Hello\n---\nBody text\n";
        let (front, body) = split_frontmatter(raw);
        assert_eq!(front.trim(), "title: Hello");
        assert_eq!(body, "Body text\n");
    }

    #[test]
    fn a_horizontal_rule_is_not_frontmatter() {
        let raw = "Some text\n\n---\n\nMore text\n";
        let (front, body) = split_frontmatter(raw);
        assert!(front.is_empty(), "rule mid-document must not count");
        assert_eq!(body, raw);
    }

    #[test]
    fn unterminated_frontmatter_keeps_the_whole_file_as_body() {
        let raw = "---\ntitle: Hello\nnever closed\n";
        let (front, body) = split_frontmatter(raw);
        assert!(front.is_empty());
        assert_eq!(body, raw, "content must not be swallowed");
    }

    #[test]
    fn crlf_frontmatter_is_handled() {
        let raw = "---\r\ntitle: Hello\r\n---\r\nBody\r\n";
        let (front, body) = split_frontmatter(raw);
        assert!(front.contains("title: Hello"));
        assert_eq!(body, "Body\r\n");
    }

    #[test]
    fn frontmatter_reads_scalars_inline_lists_and_block_lists() {
        assert_eq!(frontmatter_field("title: Hello", "title"), vec!["Hello"]);
        assert_eq!(
            frontmatter_field("tags: [a, b, c]", "tags"),
            vec!["a", "b", "c"]
        );
        assert_eq!(
            frontmatter_field("tags:\n  - one\n  - two\n", "tags"),
            vec!["one", "two"]
        );
    }

    #[test]
    fn frontmatter_strips_quotes() {
        assert_eq!(
            frontmatter_field("title: \"Quoted Title\"", "title"),
            vec!["Quoted Title"]
        );
    }

    #[test]
    fn inline_tags_are_found_in_the_body() {
        let note = Note::parse("n.md", "text #alpha and #beta/sub here\n");
        assert!(note.tags.contains("alpha"));
        assert!(note.tags.contains("beta/sub"));
    }

    #[test]
    fn headings_are_not_mistaken_for_tags() {
        let note = Note::parse("n.md", "# Real Heading\n\nbody\n");
        assert!(note.tags.is_empty(), "got {:?}", note.tags);
        assert_eq!(note.headings, vec!["Real Heading"]);
    }

    #[test]
    fn url_fragments_are_not_tags() {
        let note = Note::parse("n.md", "see https://example.com/page#section\n");
        assert!(note.tags.is_empty(), "got {:?}", note.tags);
    }

    #[test]
    fn tags_inside_code_fences_are_ignored() {
        let note = Note::parse("n.md", "```\n#notatag\n```\n#real\n");
        assert!(note.tags.contains("real"));
        assert!(!note.tags.contains("notatag"), "got {:?}", note.tags);
    }

    #[test]
    fn frontmatter_tags_join_body_tags() {
        let note = Note::parse("n.md", "---\ntags: [from-front]\n---\n#from-body\n");
        assert!(note.tags.contains("from-front"));
        assert!(note.tags.contains("from-body"));
    }

    #[test]
    fn nested_tags_match_their_parent() {
        let note = Note::parse("n.md", "#project/vavis\n");
        assert!(note.has_tag("project"), "parent tag must match");
        assert!(note.has_tag("#project"), "leading hash is optional");
        assert!(note.has_tag("project/vavis"));
        assert!(!note.has_tag("proj"), "prefix alone must not match");
    }

    /// Obsidian escapes the alias pipe inside tables, and writes that form
    /// itself. Leaving the backslash on made the target match no note.
    #[test]
    fn an_escaped_alias_pipe_inside_a_table_still_resolves() {
        let note = Note::parse(
            "n.md",
            "| a | b |\n|---|---|\n| [[00 - Start Here\\|context]] | x |\n",
        );
        assert!(
            note.links.contains("00 - Start Here"),
            "ters bölü hedefte kalmamalı: {:?}",
            note.links
        );
    }

    #[test]
    fn wikilinks_are_extracted_with_alias_and_heading_stripped() {
        let note = Note::parse(
            "n.md",
            "see [[Plain]], [[Target|shown]] and [[Other#Heading]]\n",
        );
        assert!(note.links.contains("Plain"));
        assert!(note.links.contains("Target"));
        assert!(note.links.contains("Other"));
    }

    #[test]
    fn embeds_are_tracked_separately_from_links() {
        let note = Note::parse("n.md", "![[Embedded]] and [[Linked]]\n");
        assert!(note.embeds.contains("Embedded"));
        assert!(note.links.contains("Linked"));
        assert!(
            !note.links.contains("Embedded"),
            "an embed is not a plain link"
        );
    }

    #[test]
    fn title_prefers_frontmatter_then_h1_then_filename() {
        let front = Note::parse("file.md", "---\ntitle: From Front\n---\n# From H1\n");
        assert_eq!(front.title, "From Front");

        let h1 = Note::parse("file.md", "# From H1\n");
        assert_eq!(h1.title, "From H1");

        let stem = Note::parse("dir/File Name.md", "no heading\n");
        assert_eq!(stem.title, "File Name");
    }

    #[test]
    fn headings_inside_code_fences_are_ignored() {
        let note = Note::parse("n.md", "```\n# not a heading\n```\n## Real\n");
        assert_eq!(note.headings, vec!["Real"]);
    }

    #[test]
    fn an_unclosed_bracket_does_not_swallow_the_note() {
        let note = Note::parse("n.md", "[[unclosed\n\nnext paragraph [[Good]]\n");
        assert!(note.links.contains("Good"));
    }

    #[test]
    fn a_note_with_no_markup_parses_to_plain_body() {
        let note = Note::parse("n.md", "just text\n");
        assert_eq!(note.body, "just text\n");
        assert!(note.tags.is_empty());
        assert!(note.links.is_empty());
        assert_eq!(note.title, "n");
    }
}
