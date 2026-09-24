//! Obsidian vault access.
//!
//! Built in, and deliberately over the file system rather than the community
//! REST plugin: the Markdown on disk is the source of truth, and it is
//! readable whether or not Obsidian happens to be running. The cost is that
//! the vault's conventions have to be understood here — see [`note`].
//!
//! Writing assumes the user has the same files open in Obsidian, because they
//! usually do. Every write re-reads first and refuses if the file moved under
//! it; losing a paragraph someone typed thirty seconds ago is unforgivable.

pub mod note;

use note::Note;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Directories that are never notes.
///
/// `.obsidian` is app config, `.trash` is already-deleted material, `.git` is
/// version control — indexing any of them is noise at best.
const IGNORED_DIRS: [&str; 4] = [".obsidian", ".trash", ".git", "node_modules"];

/// Upper bound on how much of a note is handed back at once.
const MAX_NOTE_CHARS: usize = 12_000;

/// A vault on disk.
#[derive(Debug, Clone, PartialEq)]
pub struct Vault {
    pub root: PathBuf,
}

/// Where a note lives and what it is called — enough to show a choice.
#[derive(Debug, Clone, PartialEq)]
pub struct NoteRef {
    pub path: String,
    pub title: String,
}

impl Vault {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Resolves a vault-relative path, refusing anything that escapes.
    ///
    /// The model supplies these paths, so `../../` has to be impossible
    /// rather than merely discouraged.
    pub fn resolve(&self, relative: &str) -> Result<PathBuf, String> {
        let cleaned = relative.trim().replace('\\', "/");
        if cleaned.is_empty() {
            return Err("path is empty".into());
        }
        if cleaned.starts_with('/') || cleaned.contains(':') {
            return Err("path must be relative to the vault".into());
        }

        let mut out = self.root.clone();
        for segment in cleaned.split('/') {
            match segment {
                "" | "." => continue,
                ".." => return Err("path may not leave the vault".into()),
                s => out.push(s),
            }
        }

        // Notes are Markdown; the extension is added when the model omits it.
        if out.extension().is_none() {
            out.set_extension("md");
        }
        Ok(out)
    }

    /// Turns an absolute path back into a vault-relative one.
    fn relativise(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }

    /// Walks the vault, parsing every note.
    ///
    /// Reads eagerly: a personal vault is thousands of small files, and the
    /// alternative — a persistent index — has to be invalidated correctly on
    /// every external edit, which Obsidian makes constantly.
    pub fn scan(&self) -> Result<Vec<Note>, String> {
        if !self.root.is_dir() {
            return Err(format!("vault not found: {}", self.root.display()));
        }
        let mut notes = Vec::new();
        collect(&self.root, &self.root, &mut notes)?;
        Ok(notes)
    }

    /// Notes ranked against `query`.
    ///
    /// Ranking follows the vault's own structure: a note **titled** "Spotify
    /// integration" must beat one that mentions Spotify on line 200. BM25
    /// scores one blob of text, so the fields that matter are repeated to
    /// weight them — the standard trick, and it keeps a single index.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<(Note, f64)>, String> {
        let notes = self.scan()?;
        Ok(rank(notes, query, limit))
    }

    /// Reads one note.
    pub fn read(&self, relative: &str) -> Result<(Note, String), String> {
        let path = self.resolve(relative)?;
        let raw = std::fs::read_to_string(&path)
            .map_err(|e| format!("{} could not be read: {e}", self.relativise(&path)))?;
        let note = Note::parse(&self.relativise(&path), &raw);
        Ok((note, raw))
    }

    /// Creates a note, refusing to clobber an existing one.
    pub fn create(&self, relative: &str, content: &str) -> Result<String, String> {
        let path = self.resolve(relative)?;
        if path.exists() {
            return Err(format!(
                "{} already exists — append or edit it instead",
                self.relativise(&path)
            ));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("folder not created: {e}"))?;
        }
        // A note without a trailing newline is a diff hazard in a git-backed
        // vault, and the user's is.
        let body = if content.ends_with('\n') {
            content.to_string()
        } else {
            format!("{content}\n")
        };
        write_atomic(&path, &body)?;
        Ok(self.relativise(&path))
    }

    /// Appends to the end of a note, creating it when absent.
    pub fn append(&self, relative: &str, content: &str) -> Result<String, String> {
        let path = self.resolve(relative)?;
        if !path.exists() {
            return self.create(relative, content);
        }

        let existing = std::fs::read_to_string(&path).map_err(|e| format!("read failed: {e}"))?;
        let newline = dominant_newline(&existing);

        let mut out = existing.clone();
        if !out.is_empty() && !out.ends_with('\n') {
            out.push_str(newline);
        }
        out.push_str(&content.replace("\r\n", "\n").replace('\n', newline));
        if !out.ends_with('\n') {
            out.push_str(newline);
        }

        write_atomic(&path, &out)?;
        Ok(self.relativise(&path))
    }

    /// Replaces an exact snippet inside a note.
    ///
    /// The snippet must still be present and unique at write time — that is
    /// what makes this safe against a concurrent edit in Obsidian, and it
    /// keeps the rest of the file untouched rather than reformatting it.
    pub fn edit(&self, relative: &str, old: &str, new: &str) -> Result<String, String> {
        let path = self.resolve(relative)?;
        // Re-read immediately before writing: the file may have changed since
        // the model decided what to replace.
        let current = std::fs::read_to_string(&path)
            .map_err(|e| format!("{} could not be read: {e}", self.relativise(&path)))?;

        // Compare in normalised newlines so a CRLF file still matches text the
        // model quoted with plain newlines.
        let newline = dominant_newline(&current);
        let normalised = current.replace("\r\n", "\n");
        let old_n = old.replace("\r\n", "\n");
        let new_n = new.replace("\r\n", "\n");

        match normalised.matches(&old_n).count() {
            0 => {
                return Err(format!(
                    "that text is not in {} — it may have changed; read it again",
                    self.relativise(&path)
                ))
            }
            1 => {}
            n => {
                return Err(format!(
                    "that text appears {n} times in {} — include more context to \
                     identify which one",
                    self.relativise(&path)
                ))
            }
        }

        let updated = normalised.replacen(&old_n, &new_n, 1);
        write_atomic(&path, &restore_newlines(&updated, newline))?;
        Ok(self.relativise(&path))
    }

    /// Moves a note into the vault's `.trash/`.
    ///
    /// Never an OS delete: a note is the one thing here that cannot be
    /// recovered from the chat log, and Obsidian's own trash is where the
    /// user already knows to look.
    pub fn delete(&self, relative: &str) -> Result<String, String> {
        let path = self.resolve(relative)?;
        if !path.exists() {
            return Err(format!("{} does not exist", self.relativise(&path)));
        }

        let trash = self.root.join(".trash");
        std::fs::create_dir_all(&trash).map_err(|e| format!("trash not created: {e}"))?;

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "note.md".to_string());

        // Keep an existing trashed note of the same name.
        let mut target = trash.join(&name);
        let mut n = 1;
        while target.exists() {
            let stem = name.trim_end_matches(".md");
            target = trash.join(format!("{stem} ({n}).md"));
            n += 1;
        }

        std::fs::rename(&path, &target).map_err(|e| format!("move to trash failed: {e}"))?;
        Ok(self.relativise(&target))
    }

    /// Notes that link **to** `target`, plus the links found in it.
    pub fn links(&self, relative: &str) -> Result<(Vec<NoteRef>, Vec<NoteRef>), String> {
        let (note, _) = self.read(relative)?;
        let stem = note::stem(&note.path);

        let backlinks = self
            .scan()?
            .into_iter()
            .filter(|other| {
                other.path != note.path
                    && other
                        .links
                        .iter()
                        .chain(other.embeds.iter())
                        .any(|l| link_matches(l, &stem))
            })
            .map(|other| NoteRef {
                path: other.path,
                title: other.title,
            })
            .collect();

        let outgoing = note
            .links
            .iter()
            .map(|l| NoteRef {
                path: l.clone(),
                title: l.clone(),
            })
            .collect();

        Ok((backlinks, outgoing))
    }

    /// Every tag in the vault with how many notes carry it.
    pub fn tags(&self) -> Result<Vec<(String, usize)>, String> {
        let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
        for note in self.scan()? {
            for tag in &note.tags {
                *counts.entry(tag.clone()).or_insert(0) += 1;
            }
        }
        let mut out: Vec<(String, usize)> = counts.into_iter().collect();
        // Most-used first: that is the useful order when listing.
        out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        Ok(out)
    }
}

/// Whether a wikilink target refers to a note with the given stem.
///
/// Links may be bare (`[[Steam integration]]`) or pathed
/// (`[[integrations/Steam integration]]`), and are case-insensitive.
fn link_matches(link: &str, stem: &str) -> bool {
    let link_stem = note::stem(&link.replace('\\', "/"));
    link_stem.eq_ignore_ascii_case(stem)
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<Note>) -> Result<(), String> {
    collect_once(root, dir, out, &mut std::collections::HashSet::new())
}

/// Each real folder once. Linked folders are followed -- people link shared
/// folders into a vault on purpose -- but a link back to a folder above it
/// would otherwise recurse until the stack ran out and took the app down.
fn collect_once(
    root: &Path,
    dir: &Path,
    out: &mut Vec<Note>,
    seen: &mut std::collections::HashSet<PathBuf>,
) -> Result<(), String> {
    if let Ok(real) = dir.canonicalize() {
        if !seen.insert(real) {
            return Ok(());
        }
    }
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("{} unreadable: {e}", dir.display()))?;

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if path.is_dir() {
            if IGNORED_DIRS.contains(&name.as_str()) {
                continue;
            }
            collect_once(root, &path, out, seen)?;
            continue;
        }

        if path.extension().is_none_or(|e| e != "md") {
            continue;
        }
        // An unreadable note should not abort the whole scan.
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        out.push(Note::parse(&relative, &raw));
    }

    Ok(())
}

/// Scores notes against a query with the vault's fields weighted.
pub fn rank(notes: Vec<Note>, query: &str, limit: usize) -> Vec<(Note, f64)> {
    use vavis_core::{Document, SearchIndex};

    if notes.is_empty() {
        return Vec::new();
    }

    let docs: Vec<Document> = notes
        .iter()
        .enumerate()
        .map(|(i, note)| Document {
            id: i as i64,
            text: searchable_text(note),
        })
        .collect();

    let index = SearchIndex::build(docs);
    index
        .search(query, limit)
        .into_iter()
        .filter_map(|hit| {
            notes
                .get(hit.id as usize)
                .map(|note| (note.clone(), hit.score))
        })
        .collect()
}

/// Builds the text BM25 scores, repeating high-value fields to weight them.
///
/// Weights: title ×4, headings ×2, tags ×2, body ×1. Chosen so a title match
/// wins outright while a body match still surfaces.
fn searchable_text(note: &Note) -> String {
    let mut text = String::new();
    for _ in 0..4 {
        text.push_str(&note.title);
        text.push('\n');
    }
    for _ in 0..2 {
        for heading in &note.headings {
            text.push_str(heading);
            text.push('\n');
        }
        for tag in &note.tags {
            text.push_str(tag);
            text.push('\n');
        }
    }
    // The path carries meaning too: `integrations/steam.md`.
    text.push_str(&note.path.replace(['/', '-', '_'], " "));
    text.push('\n');
    text.push_str(&note.body);
    text
}

/// The newline style a file already uses.
///
/// Rewriting a CRLF vault with LF endings turns one edited line into a diff
/// of the whole file — which the user would see in git.
fn dominant_newline(text: &str) -> &'static str {
    if text.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

fn restore_newlines(text: &str, newline: &str) -> String {
    if newline == "\n" {
        text.to_string()
    } else {
        text.replace('\n', newline)
    }
}

/// Writes via a temp file and a rename.
///
/// A half-written note is a corrupt note; rename is atomic on both platforms
/// that matter here.
fn write_atomic(path: &Path, content: &str) -> Result<(), String> {
    let tmp = path.with_extension("md.tmp");
    std::fs::write(&tmp, content).map_err(|e| format!("write failed: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        // Leave no debris if the rename fails.
        let _ = std::fs::remove_file(&tmp);
        format!("save failed: {e}")
    })
}

/// Trims note content handed back to the model.
pub fn clip(text: &str) -> String {
    if text.chars().count() > MAX_NOTE_CHARS {
        let clipped: String = text.chars().take(MAX_NOTE_CHARS).collect();
        format!("{clipped}\n…(truncated)")
    } else {
        text.to_string()
    }
}

// ---------------------------------------------------------------------------
// Vault discovery and the active vault
// ---------------------------------------------------------------------------

/// Vaults Obsidian knows about, newest first.
///
/// Read from Obsidian's own `obsidian.json`, so the user does not have to
/// paste a path. Obsidian need not be running — this is just a file.
pub fn discover() -> Vec<PathBuf> {
    let Some(config) = obsidian_config_path() else {
        return Vec::new();
    };
    let Ok(raw) = std::fs::read_to_string(&config) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Vec::new();
    };

    let Some(vaults) = json["vaults"].as_object() else {
        return Vec::new();
    };

    let mut found: Vec<(i64, PathBuf)> = vaults
        .values()
        .filter_map(|v| {
            let path = v["path"].as_str()?;
            // `ts` is the last-opened timestamp; the most recent is the one
            // the user is most likely to mean.
            Some((v["ts"].as_i64().unwrap_or(0), PathBuf::from(path)))
        })
        .filter(|(_, p)| p.is_dir())
        .collect();

    found.sort_by_key(|(ts, _)| std::cmp::Reverse(*ts));
    found.into_iter().map(|(_, p)| p).collect()
}

#[cfg(windows)]
fn obsidian_config_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(
        PathBuf::from(appdata)
            .join("obsidian")
            .join("obsidian.json"),
    )
}

#[cfg(not(windows))]
fn obsidian_config_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join(".config")
            .join("obsidian")
            .join("obsidian.json"),
    )
}

/// The vault tools operate on. One at a time, on purpose: "which vault?" on
/// every call would be a worse experience than picking one in settings.
fn active() -> &'static Mutex<Option<Vault>> {
    static ACTIVE: OnceLock<Mutex<Option<Vault>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(None))
}

/// Sets the active vault. `None` disables the tools.
pub fn set_active(vault: Option<Vault>) {
    *active().lock().unwrap_or_else(|e| e.into_inner()) = vault;
}

/// The active vault, if one is selected.
pub fn current() -> Option<Vault> {
    active().lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Picks a vault at startup: the configured one, else the most recent that
/// Obsidian knows about, so the tools work without any setup.
pub fn autoselect(configured: &str) -> Option<Vault> {
    if !configured.trim().is_empty() {
        let path = PathBuf::from(configured.trim());
        if path.is_dir() {
            return Some(Vault::new(path));
        }
    }
    discover().into_iter().next().map(Vault::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault_with(files: &[(&str, &str)]) -> (tempfile::TempDir, Vault) {
        let tmp = tempfile::tempdir().unwrap();
        for (name, content) in files {
            let path = tmp.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }
        let vault = Vault::new(tmp.path());
        (tmp, vault)
    }

    #[test]
    fn scan_finds_markdown_and_skips_app_folders() {
        let (_tmp, vault) = vault_with(&[
            ("a.md", "# A\n"),
            ("sub/b.md", "# B\n"),
            (".obsidian/workspace.json", "{}"),
            (".trash/old.md", "# Old\n"),
            ("image.png", "binary"),
        ]);

        let notes = vault.scan().unwrap();
        let paths: Vec<&str> = notes.iter().map(|n| n.path.as_str()).collect();
        assert!(paths.contains(&"a.md"));
        assert!(paths.contains(&"sub/b.md"));
        assert_eq!(paths.len(), 2, "got {paths:?}");
    }

    #[test]
    fn paths_may_not_escape_the_vault() {
        let (_tmp, vault) = vault_with(&[]);
        assert!(vault.resolve("../secrets.md").is_err());
        assert!(vault.resolve("sub/../../out.md").is_err());
        assert!(vault.resolve("/etc/passwd").is_err());
        assert!(vault.resolve("").is_err());
        assert!(vault.resolve("fine/note.md").is_ok());
    }

    #[test]
    fn a_missing_extension_is_assumed_to_be_markdown() {
        let (_tmp, vault) = vault_with(&[]);
        let path = vault.resolve("Daily/2026-08-28").unwrap();
        assert_eq!(path.extension().unwrap(), "md");
    }

    #[test]
    fn title_matches_outrank_body_mentions() {
        let (_tmp, vault) = vault_with(&[
            (
                "spotify integration.md",
                "# Spotify integration\nsome text\n",
            ),
            (
                "other.md",
                &format!("# Other\n{}\nspotify\n", "filler ".repeat(80)),
            ),
        ]);

        let results = vault.search("spotify integration", 5).unwrap();
        assert_eq!(
            results[0].0.path, "spotify integration.md",
            "the titled note must win"
        );
    }

    #[test]
    fn create_refuses_to_overwrite() {
        let (_tmp, vault) = vault_with(&[("a.md", "original\n")]);
        assert!(vault.create("a.md", "new").is_err());
        assert_eq!(
            std::fs::read_to_string(vault.root.join("a.md")).unwrap(),
            "original\n",
            "the original must be untouched"
        );
    }

    #[test]
    fn create_adds_a_trailing_newline() {
        let (_tmp, vault) = vault_with(&[]);
        vault.create("new.md", "no newline").unwrap();
        let written = std::fs::read_to_string(vault.root.join("new.md")).unwrap();
        assert!(written.ends_with('\n'));
    }

    #[test]
    fn append_adds_to_the_end_and_keeps_existing_content() {
        let (_tmp, vault) = vault_with(&[("a.md", "first\n")]);
        vault.append("a.md", "second").unwrap();
        let written = std::fs::read_to_string(vault.root.join("a.md")).unwrap();
        assert_eq!(written, "first\nsecond\n");
    }

    #[test]
    fn append_creates_the_note_when_missing() {
        let (_tmp, vault) = vault_with(&[]);
        vault.append("fresh.md", "content").unwrap();
        assert!(vault.root.join("fresh.md").exists());
    }

    #[test]
    fn append_preserves_crlf_line_endings() {
        let (_tmp, vault) = vault_with(&[("a.md", "first\r\n")]);
        vault.append("a.md", "second").unwrap();
        let written = std::fs::read_to_string(vault.root.join("a.md")).unwrap();
        assert!(written.ends_with("second\r\n"), "got {written:?}");
        assert!(!written.contains("first\n\r"), "endings got mangled");
    }

    #[test]
    fn edit_replaces_only_the_named_snippet() {
        let (_tmp, vault) = vault_with(&[("a.md", "keep\nchange me\nkeep too\n")]);
        vault.edit("a.md", "change me", "changed").unwrap();
        assert_eq!(
            std::fs::read_to_string(vault.root.join("a.md")).unwrap(),
            "keep\nchanged\nkeep too\n"
        );
    }

    #[test]
    fn edit_refuses_when_the_text_is_gone() {
        // Stands in for the file having changed in Obsidian since it was read.
        let (_tmp, vault) = vault_with(&[("a.md", "the text moved on\n")]);
        let err = vault.edit("a.md", "old text", "new").unwrap_err();
        assert!(err.contains("read it again"), "got {err}");
    }

    #[test]
    fn edit_refuses_an_ambiguous_snippet() {
        let (_tmp, vault) = vault_with(&[("a.md", "dup\ndup\n")]);
        let err = vault.edit("a.md", "dup", "x").unwrap_err();
        assert!(err.contains("2 times"), "got {err}");
        assert_eq!(
            std::fs::read_to_string(vault.root.join("a.md")).unwrap(),
            "dup\ndup\n",
            "nothing may be written on an ambiguous edit"
        );
    }

    #[test]
    fn edit_keeps_crlf_endings() {
        let (_tmp, vault) = vault_with(&[("a.md", "one\r\ntwo\r\n")]);
        vault.edit("a.md", "two", "three").unwrap();
        let written = std::fs::read_to_string(vault.root.join("a.md")).unwrap();
        assert_eq!(written, "one\r\nthree\r\n");
    }

    #[test]
    fn delete_moves_to_the_vault_trash_not_the_os() {
        let (_tmp, vault) = vault_with(&[("gone.md", "bye\n")]);
        let trashed = vault.delete("gone.md").unwrap();

        assert!(!vault.root.join("gone.md").exists());
        assert!(trashed.starts_with(".trash/"), "got {trashed}");
        assert_eq!(
            std::fs::read_to_string(vault.root.join(".trash/gone.md")).unwrap(),
            "bye\n",
            "content must survive in the trash"
        );
    }

    #[test]
    fn delete_does_not_clobber_an_earlier_trashed_note() {
        let (_tmp, vault) = vault_with(&[(".trash/gone.md", "older\n"), ("gone.md", "newer\n")]);
        vault.delete("gone.md").unwrap();
        assert_eq!(
            std::fs::read_to_string(vault.root.join(".trash/gone.md")).unwrap(),
            "older\n",
            "the earlier trashed note must survive"
        );
        assert!(vault.root.join(".trash/gone (1).md").exists());
    }

    #[test]
    fn delete_reports_a_missing_note() {
        let (_tmp, vault) = vault_with(&[]);
        assert!(vault.delete("nope.md").is_err());
    }

    #[test]
    fn backlinks_find_notes_pointing_here() {
        let (_tmp, vault) = vault_with(&[
            ("target.md", "# Target\n"),
            ("a.md", "see [[target]]\n"),
            ("b.md", "see [[Target|aliased]]\n"),
            ("c.md", "unrelated\n"),
        ]);

        let (backlinks, _) = vault.links("target.md").unwrap();
        let paths: Vec<&str> = backlinks.iter().map(|n| n.path.as_str()).collect();
        assert!(paths.contains(&"a.md"));
        assert!(paths.contains(&"b.md"), "alias links still count");
        assert!(!paths.contains(&"c.md"));
    }

    #[test]
    fn backlinks_match_pathed_links() {
        let (_tmp, vault) = vault_with(&[
            ("sub/target.md", "# Target\n"),
            ("a.md", "see [[sub/target]]\n"),
        ]);
        let (backlinks, _) = vault.links("sub/target.md").unwrap();
        assert_eq!(backlinks.len(), 1);
    }

    #[test]
    fn tags_are_counted_across_the_vault() {
        let (_tmp, vault) = vault_with(&[("a.md", "#project #idea\n"), ("b.md", "#project\n")]);
        let tags = vault.tags().unwrap();
        assert_eq!(tags[0], ("project".to_string(), 2), "most used comes first");
        assert!(tags.contains(&("idea".to_string(), 1)));
    }

    #[test]
    fn search_on_an_empty_vault_returns_nothing() {
        let (_tmp, vault) = vault_with(&[]);
        assert!(vault.search("anything", 5).unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_link_loop_in_the_vault_is_walked_once() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/a.md"), "# A").unwrap();
        std::os::unix::fs::symlink(dir.path(), dir.path().join("sub/up")).unwrap();

        let notes = Vault::new(dir.path().to_path_buf()).scan().unwrap();
        assert_eq!(
            notes.len(),
            1,
            "{:?}",
            notes.iter().map(|n| &n.path).collect::<Vec<_>>()
        );
    }

    #[test]
    fn scanning_a_missing_vault_is_an_error_not_a_panic() {
        let vault = Vault::new("C:/definitely/not/here");
        assert!(vault.scan().is_err());
    }

    #[test]
    fn clip_truncates_only_when_needed() {
        assert_eq!(clip("short"), "short");
        let long = "x".repeat(MAX_NOTE_CHARS + 10);
        assert!(clip(&long).contains("truncated"));
    }

    /// Gerçek bir kasaya karşı okuma denemesi.
    ///
    /// Normalde atlanır — makineye bağlı. Çalıştırmak için:
    /// `VAVIS_TEST_VAULT="C:/.../Vault" cargo test -p vavis-tools real_vault -- --ignored`
    #[test]
    #[ignore = "needs a real vault via VAVIS_TEST_VAULT"]
    fn real_vault_scans_and_searches() {
        let Ok(root) = std::env::var("VAVIS_TEST_VAULT") else {
            panic!("set VAVIS_TEST_VAULT to a vault path");
        };
        let vault = Vault::new(&root);

        let notes = vault.scan().expect("vault should scan");
        println!("{} notes found", notes.len());
        assert!(!notes.is_empty(), "a real vault should have notes");

        for note in notes.iter().take(5) {
            println!(
                "  {} — {} ({} tags)",
                note.title,
                note.path,
                note.tags.len()
            );
        }

        // Reading must never mutate; the scan is repeatable.
        let again = vault.scan().unwrap();
        assert_eq!(
            notes.len(),
            again.len(),
            "scanning must not change the vault"
        );
    }

    #[test]
    fn newline_style_is_detected() {
        assert_eq!(dominant_newline("a\r\nb"), "\r\n");
        assert_eq!(dominant_newline("a\nb"), "\n");
        assert_eq!(dominant_newline(""), "\n");
    }
}
