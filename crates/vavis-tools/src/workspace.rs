//! Workspace access for the code interface.
//!
//! The code view needs to browse a project, open files and save them. That
//! is deliberately kept out of the general file tools: those answer the
//! model's questions anywhere on disk, while this is scoped to one folder
//! the user opened and is driven by clicks, not by the model.
//!
//! Everything here refuses to leave the opened root. The interface sends
//! paths back that it got from us, but a bug there must not turn into a way
//! to write outside the project.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Directories never worth showing in a project tree.
const SKIP_DIRS: [&str; 8] = [
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".venv",
    "__pycache__",
    ".svelte-kit",
];

/// Largest file the editor will open.
///
/// Past this the editor stops being useful and the read starts costing real
/// memory; a 5 MB minified bundle is not something anyone edits by hand.
const MAX_EDIT_BYTES: u64 = 2 * 1024 * 1024;

/// One entry in the tree.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// Path relative to the workspace root, forward-slashed.
    pub path: String,
    pub name: String,
    pub is_dir: bool,
}

/// The folder the code interface is looking at.
fn root() -> &'static Mutex<Option<PathBuf>> {
    static ROOT: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
    ROOT.get_or_init(|| Mutex::new(None))
}

pub fn set_root(path: Option<PathBuf>) {
    *root().lock().unwrap_or_else(|e| e.into_inner()) = path;
}

pub fn current_root() -> Option<PathBuf> {
    root().lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Resolves a workspace-relative path, refusing anything that escapes.
pub fn resolve(relative: &str) -> Result<PathBuf, String> {
    let root = current_root().ok_or("no folder is open")?;
    let cleaned = relative.trim().replace('\\', "/");

    if cleaned.starts_with('/') || cleaned.contains(':') {
        return Err("path must be relative to the workspace".into());
    }

    let mut out = root.clone();
    for segment in cleaned.split('/') {
        match segment {
            "" | "." => continue,
            ".." => return Err("path may not leave the workspace".into()),
            s => out.push(s),
        }
    }

    // Belt and braces: symlinks could still point outside, so the resolved
    // path is checked against the root.
    if let (Ok(canonical), Ok(canonical_root)) = (out.canonicalize(), root.canonicalize()) {
        if !canonical.starts_with(&canonical_root) {
            return Err("path may not leave the workspace".into());
        }
    }

    Ok(out)
}

/// Lists one directory, folders first then files, both alphabetical.
///
/// One level at a time rather than the whole tree: a large project would
/// take seconds to walk and the user only ever looks at a few folders.
pub fn list(relative: &str) -> Result<Vec<Entry>, String> {
    let dir = if relative.trim().is_empty() {
        current_root().ok_or("no folder is open")?
    } else {
        resolve(relative)?
    };

    let entries =
        std::fs::read_dir(&dir).map_err(|e| format!("{} unreadable: {e}", dir.display()))?;
    let root = current_root().ok_or("no folder is open")?;

    let mut out: Vec<Entry> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();
            let is_dir = path.is_dir();

            if is_dir && SKIP_DIRS.contains(&name.as_str()) {
                return None;
            }

            Some(Entry {
                path: path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
                name,
                is_dir,
            })
        })
        .collect();

    out.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(out)
}

/// Reads a file for the editor.
pub fn read(relative: &str) -> Result<String, String> {
    let path = resolve(relative)?;

    let size = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
    if size > MAX_EDIT_BYTES {
        return Err(format!(
            "{relative} is {} MB — too large to edit here",
            size / 1_048_576
        ));
    }

    std::fs::read_to_string(&path).map_err(|e| {
        // A binary file is the usual reason this fails; say so plainly.
        if e.kind() == std::io::ErrorKind::InvalidData {
            format!("{relative} is not a text file")
        } else {
            format!("{relative} could not be read: {e}")
        }
    })
}

/// Writes a file, atomically.
pub fn write(relative: &str, content: &str) -> Result<(), String> {
    let path = resolve(relative)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("folder not created: {e}"))?;
    }

    let tmp = path.with_extension("vavis-tmp");
    std::fs::write(&tmp, content).map_err(|e| format!("write failed: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("save failed: {e}")
    })
}

/// Searches file contents across the workspace.
///
/// Plain substring, capped: this is "find where this string appears", not a
/// code index, and the cap keeps a stray one-letter query from returning the
/// whole project.
pub fn grep(query: &str, limit: usize) -> Result<Vec<(String, usize, String)>, String> {
    let root = current_root().ok_or("no folder is open")?;
    let needle = query.trim().to_lowercase();
    if needle.len() < 2 {
        return Err("search needs at least two characters".into());
    }

    let mut hits = Vec::new();
    walk(&root, &root, &mut |path, relative| {
        if hits.len() >= limit {
            return;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            return; // binary or unreadable
        };
        for (number, line) in text.lines().enumerate() {
            if hits.len() >= limit {
                return;
            }
            if line.to_lowercase().contains(&needle) {
                hits.push((
                    relative.to_string(),
                    number + 1,
                    line.trim().chars().take(200).collect(),
                ));
            }
        }
    });

    Ok(hits)
}

/// Walks the tree, skipping the usual noise directories.
fn walk(root: &Path, dir: &Path, visit: &mut impl FnMut(&Path, &str)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if path.is_dir() {
            if !SKIP_DIRS.contains(&name.as_str()) {
                walk(root, &path, visit);
            }
            continue;
        }

        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        visit(&path, &relative);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The open workspace is process-global, so these tests are serialised.
    fn test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    fn workspace(files: &[(&str, &str)], f: impl FnOnce()) {
        let _guard = test_lock();
        let tmp = tempfile::tempdir().unwrap();
        for (name, content) in files {
            let path = tmp.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }
        set_root(Some(tmp.path().to_path_buf()));
        f();
        set_root(None);
    }

    #[test]
    fn nothing_works_without_an_open_folder() {
        let _guard = test_lock();
        set_root(None);
        assert!(list("").is_err());
        assert!(read("a.rs").is_err());
        assert!(write("a.rs", "x").is_err());
    }

    #[test]
    fn paths_may_not_escape_the_workspace() {
        workspace(&[("a.rs", "fn main() {}")], || {
            assert!(resolve("../outside.rs").is_err());
            assert!(resolve("sub/../../outside.rs").is_err());
            assert!(resolve("/etc/passwd").is_err());
            assert!(resolve("C:/Windows/system.ini").is_err());
            assert!(resolve("a.rs").is_ok());
        });
    }

    #[test]
    fn listing_puts_folders_first_and_skips_noise() {
        workspace(
            &[
                ("src/main.rs", "fn main() {}"),
                ("README.md", "# hi"),
                ("node_modules/pkg/index.js", "module.exports = 1"),
                ("target/debug/thing", "binary"),
            ],
            || {
                let entries = list("").unwrap();
                let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();

                assert_eq!(names[0], "src", "folders come first");
                assert!(names.contains(&"README.md"));
                assert!(!names.contains(&"node_modules"), "got {names:?}");
                assert!(!names.contains(&"target"), "got {names:?}");
            },
        );
    }

    #[test]
    fn reading_and_writing_round_trips() {
        workspace(&[("a.rs", "original")], || {
            assert_eq!(read("a.rs").unwrap(), "original");
            write("a.rs", "changed").unwrap();
            assert_eq!(read("a.rs").unwrap(), "changed");
        });
    }

    #[test]
    fn writing_creates_missing_folders() {
        workspace(&[], || {
            write("deep/new/file.txt", "x").unwrap();
            assert_eq!(read("deep/new/file.txt").unwrap(), "x");
        });
    }

    #[test]
    fn a_binary_file_is_reported_not_garbled() {
        workspace(&[], || {
            let path = current_root().unwrap().join("blob.bin");
            std::fs::write(path, [0xff, 0xfe, 0x00, 0x01]).unwrap();

            let err = read("blob.bin").unwrap_err();
            assert!(err.contains("not a text file"), "got {err}");
        });
    }

    #[test]
    fn search_finds_matches_with_line_numbers() {
        workspace(
            &[
                ("src/a.rs", "fn one() {}\nfn target() {}\n"),
                ("src/b.rs", "nothing here\n"),
            ],
            || {
                let hits = grep("target", 20).unwrap();
                assert_eq!(hits.len(), 1);
                assert_eq!(hits[0].0, "src/a.rs");
                assert_eq!(hits[0].1, 2, "line numbers are 1-based");
                assert!(hits[0].2.contains("fn target"));
            },
        );
    }

    #[test]
    fn search_is_capped() {
        workspace(&[("big.txt", &"match\n".repeat(500))], || {
            assert_eq!(grep("match", 10).unwrap().len(), 10);
        });
    }

    #[test]
    fn a_one_character_search_is_refused() {
        workspace(&[("a.rs", "x")], || {
            // Otherwise it returns essentially the whole project.
            assert!(grep("x", 10).is_err());
        });
    }

    #[test]
    fn search_skips_noise_directories() {
        workspace(
            &[
                ("src/a.rs", "needle here\n"),
                ("node_modules/p/i.js", "needle here\n"),
            ],
            || {
                let hits = grep("needle", 20).unwrap();
                assert_eq!(hits.len(), 1, "node_modules should not be searched");
                assert_eq!(hits[0].0, "src/a.rs");
            },
        );
    }
}
