//! The code agent's hands: the folder open in the code screen.
//!
//! Before these, asking about code from the code screen got one answer to
//! one question: the model could not look at the project, could not change
//! it, could not run its tests. These make it an agent -- read and search
//! to understand, edit to change, run to check, and go round again.
//!
//! Everything is confined to the workspace root (see `crate::workspace`),
//! and every change goes through the permission gate:
//!
//! - `ws_list`, `ws_read`, `ws_search` -- look, no approval;
//! - `ws_edit` -- replace one exact piece of text, returns the diff;
//! - `ws_write` -- create or overwrite a whole file;
//! - `ws_run` -- a shell command in the workspace (tests, build, lint),
//!   with a time limit, returning the tail of its output.
//!
//! Edits are exact-match replacements rather than line numbers or patches:
//! a model is reliable at quoting the text it wants changed and unreliable
//! at counting lines, and a replacement that does not match exactly once
//! fails loudly instead of landing in the wrong place.

use crate::tool::{arg_num, arg_str, Domain, Param, Risk, Tool, ToolOutcome};
use crate::workspace;
use serde_json::Value;
use std::time::Duration;

/// Most lines one `ws_read` returns.
const MAX_READ_LINES: usize = 600;

/// How long `ws_run` lets a command take.
const RUN_TIMEOUT: Duration = Duration::from_secs(180);

/// How much command output comes back: the end, where results and errors
/// are.
const RUN_TAIL: usize = 6_000;

fn framed(source: &str, text: &str) -> String {
    // File contents and command output are written by whoever wrote the
    // project; a README can carry instructions aimed at the model.
    crate::untrusted::wrap(source, text).text
}

pub struct WsList;

impl Tool for WsList {
    fn name(&self) -> &'static str {
        "ws_list"
    }
    fn description(&self) -> &'static str {
        "Lists a folder of the open project (folders first). Empty path = project root."
    }
    fn domain(&self) -> Domain {
        Domain::Code
    }
    fn params(&self) -> Vec<Param> {
        vec![Param::optional(
            "path",
            "Folder, relative to the project root",
        )]
    }
    fn run(&self, args: &Value) -> ToolOutcome {
        let path = arg_str(args, "path").unwrap_or("");
        match workspace::list(path) {
            Ok(entries) if entries.is_empty() => ToolOutcome::ok("(boş klasör)"),
            Ok(entries) => ToolOutcome::ok(
                entries
                    .iter()
                    .map(|e| {
                        if e.is_dir {
                            format!("{}/", e.path)
                        } else {
                            e.path.clone()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            Err(e) => ToolOutcome::err(e),
        }
    }
}

pub struct WsRead;

impl Tool for WsRead {
    fn name(&self) -> &'static str {
        "ws_read"
    }
    fn description(&self) -> &'static str {
        "Reads a file of the open project with line numbers. For a long file, read \
         the part you need with start/end."
    }
    fn domain(&self) -> Domain {
        Domain::Code
    }
    fn params(&self) -> Vec<Param> {
        vec![
            Param::required("path", "File, relative to the project root"),
            Param::optional("start", "First line (1-based)"),
            Param::optional("end", "Last line"),
        ]
    }
    fn run(&self, args: &Value) -> ToolOutcome {
        let Some(path) = arg_str(args, "path") else {
            return ToolOutcome::err("path gerekli");
        };
        let text = match workspace::read(path) {
            Ok(t) => t,
            Err(e) => return ToolOutcome::err(e),
        };
        ToolOutcome::ok(framed(
            path,
            &numbered(&text, arg_num(args, "start"), arg_num(args, "end")),
        ))
    }
}

/// A slice of `text` with line numbers, capped, saying what was left out.
pub fn numbered(text: &str, start: Option<f64>, end: Option<f64>) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let total = lines.len();
    if total == 0 {
        return "(boş dosya)".into();
    }
    let first = start.map(|s| (s.max(1.0) as usize).min(total)).unwrap_or(1);
    let last = end
        .map(|e| (e as usize).clamp(first, total))
        .unwrap_or(total)
        .min(first + MAX_READ_LINES - 1);
    let width = last.to_string().len();
    let mut out: String = (first..=last)
        .map(|n| format!("{n:>width$}  {}\n", lines[n - 1]))
        .collect();
    if first > 1 || last < total {
        out.push_str(&format!("({first}-{last} / {total} satır)"));
    }
    out
}

pub struct WsSearch;

impl Tool for WsSearch {
    fn name(&self) -> &'static str {
        "ws_search"
    }
    fn description(&self) -> &'static str {
        "Finds where a piece of text appears in the open project: file, line, and the line itself."
    }
    fn domain(&self) -> Domain {
        Domain::Code
    }
    fn params(&self) -> Vec<Param> {
        vec![Param::required(
            "query",
            "Text to look for (case-insensitive)",
        )]
    }
    fn run(&self, args: &Value) -> ToolOutcome {
        let Some(query) = arg_str(args, "query") else {
            return ToolOutcome::err("query gerekli");
        };
        match workspace::grep(query, 80) {
            Ok(hits) if hits.is_empty() => ToolOutcome::ok(format!("'{query}' bulunamadı")),
            Ok(hits) => ToolOutcome::ok(framed(
                "workspace search",
                &hits
                    .iter()
                    .map(|(f, n, l)| format!("{f}:{n}: {l}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )),
            Err(e) => ToolOutcome::err(e),
        }
    }
}

pub struct WsEdit;

impl Tool for WsEdit {
    fn name(&self) -> &'static str {
        "ws_edit"
    }
    fn description(&self) -> &'static str {
        "Changes a file of the open project by replacing an exact piece of its text. \
         'old' must be copied exactly from the file (read it first) and appear once — \
         include enough surrounding lines to make it unique. Returns the diff."
    }
    fn domain(&self) -> Domain {
        Domain::Code
    }
    fn risk(&self) -> Risk {
        Risk::Destructive
    }
    fn params(&self) -> Vec<Param> {
        vec![
            Param::required("path", "File, relative to the project root"),
            Param::required("old", "The exact text to replace"),
            Param::required("new", "What to put in its place"),
            Param::optional("all", "'yes' to replace every occurrence"),
        ]
    }
    fn run(&self, args: &Value) -> ToolOutcome {
        let (Some(path), Some(old), Some(new)) = (
            arg_str(args, "path"),
            arg_str(args, "old"),
            arg_str(args, "new"),
        ) else {
            return ToolOutcome::err("path, old ve new gerekli");
        };
        let all = arg_str(args, "all").is_some_and(|v| matches!(v, "yes" | "true" | "evet"));
        let text = match workspace::read(path) {
            Ok(t) => t,
            Err(e) => return ToolOutcome::err(e),
        };
        match replace(&text, old, new, all) {
            Ok((changed, count)) => match workspace::write(path, &changed) {
                Ok(()) => ToolOutcome::ok(format!(
                    "{path} düzenlendi ({count} değişiklik)\n{}",
                    diff(&text, &changed)
                )),
                Err(e) => ToolOutcome::err(e),
            },
            Err(e) => ToolOutcome::err(format!("{path}: {e}")),
        }
    }
}

/// Replaces `old` with `new`: exactly once, or everywhere when `all`.
///
/// Line endings are matched loosely -- a model quoting a CRLF file writes
/// LF -- and the file's own ending is kept in what is written back.
pub fn replace(text: &str, old: &str, new: &str, all: bool) -> Result<(String, usize), String> {
    if old.is_empty() {
        return Err("'old' boş olamaz — yeni dosya için ws_write kullan".into());
    }
    let crlf = text.contains("\r\n");
    let (text_n, old_n, new_n) = if crlf {
        (
            text.replace("\r\n", "\n"),
            old.replace("\r\n", "\n"),
            new.replace("\r\n", "\n"),
        )
    } else {
        (text.to_string(), old.to_string(), new.to_string())
    };
    let count = text_n.matches(old_n.as_str()).count();
    let replaced = match (count, all) {
        (0, _) => {
            return Err(
                "'old' dosyada bulunamadı — metni ws_read ile okuyup birebir kopyala".into(),
            )
        }
        (1, _) | (_, true) => text_n.replace(old_n.as_str(), &new_n),
        (n, false) => {
            return Err(format!(
                "'old' dosyada {n} kez geçiyor — benzersiz olacak kadar çevresini de ekle, \
                 ya da hepsi için all='yes'"
            ))
        }
    };
    let out = if crlf {
        replaced.replace('\n', "\r\n")
    } else {
        replaced
    };
    Ok((out, count))
}

/// A compact line diff: the changed region with a little context, marked
/// `-` and `+`, the way a reviewer reads a change.
pub fn diff(before: &str, after: &str) -> String {
    let a: Vec<&str> = before.lines().collect();
    let b: Vec<&str> = after.lines().collect();
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let context = 2;
    let start = prefix.saturating_sub(context);
    let mut out = format!("@@ satır {}\n", prefix + 1);
    for line in &a[start..prefix] {
        out.push_str(&format!("  {line}\n"));
    }
    let removed = &a[prefix..a.len() - suffix];
    let added = &b[prefix..b.len() - suffix];
    // A very long change is summarised; the file itself has the rest.
    for line in removed.iter().take(40) {
        out.push_str(&format!("- {line}\n"));
    }
    if removed.len() > 40 {
        out.push_str(&format!("- … ({} satır daha)\n", removed.len() - 40));
    }
    for line in added.iter().take(40) {
        out.push_str(&format!("+ {line}\n"));
    }
    if added.len() > 40 {
        out.push_str(&format!("+ … ({} satır daha)\n", added.len() - 40));
    }
    for line in a[a.len() - suffix..].iter().take(context) {
        out.push_str(&format!("  {line}\n"));
    }
    out
}

pub struct WsWrite;

impl Tool for WsWrite {
    fn name(&self) -> &'static str {
        "ws_write"
    }
    fn description(&self) -> &'static str {
        "Creates a file in the open project, or replaces one entirely. For changing \
         part of an existing file, ws_edit is safer."
    }
    fn domain(&self) -> Domain {
        Domain::Code
    }
    fn risk(&self) -> Risk {
        Risk::Destructive
    }
    fn params(&self) -> Vec<Param> {
        vec![
            Param::required("path", "File, relative to the project root"),
            Param::required("content", "The whole file"),
        ]
    }
    fn run(&self, args: &Value) -> ToolOutcome {
        let (Some(path), Some(content)) = (arg_str(args, "path"), arg_str(args, "content")) else {
            return ToolOutcome::err("path ve content gerekli");
        };
        let existed = workspace::resolve(path).is_ok_and(|p| p.exists());
        match workspace::write(path, content) {
            Ok(()) => ToolOutcome::ok(format!(
                "{path} {} ({} satır)",
                if existed {
                    "üzerine yazıldı"
                } else {
                    "oluşturuldu"
                },
                content.lines().count()
            )),
            Err(e) => ToolOutcome::err(e),
        }
    }
}

pub struct WsRun;

impl Tool for WsRun {
    fn name(&self) -> &'static str {
        "ws_run"
    }
    fn description(&self) -> &'static str {
        "Runs a shell command in the open project's root — tests, a build, a linter, \
         git status — and returns its exit code and the end of its output. Three \
         minutes at most; nothing interactive."
    }
    fn domain(&self) -> Domain {
        Domain::Code
    }
    fn risk(&self) -> Risk {
        Risk::Destructive
    }
    fn params(&self) -> Vec<Param> {
        vec![Param::required(
            "command",
            "The command, e.g. 'cargo test' or 'npm test'",
        )]
    }
    fn run(&self, args: &Value) -> ToolOutcome {
        let Some(command) = arg_str(args, "command").filter(|c| !c.trim().is_empty()) else {
            return ToolOutcome::err("command gerekli");
        };
        let Some(root) = workspace::current_root() else {
            return ToolOutcome::err("kod ekranında açık bir klasör yok");
        };
        match run_in(&root, command, RUN_TIMEOUT) {
            Ok((code, output)) => {
                let body = format!("çıkış kodu: {code}\n{}", tail(&output, RUN_TAIL));
                let text = framed("command output", &body);
                if code == 0 {
                    ToolOutcome::ok(text)
                } else {
                    // Not an error for the agent: a failing test run is the
                    // information it asked for.
                    ToolOutcome::ok(text)
                }
            }
            Err(e) => ToolOutcome::err(e),
        }
    }
}

fn tail(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    format!("…\n{}", text.chars().skip(count - max).collect::<String>())
}

/// Runs `command` through the platform shell in `dir`, stdout and stderr
/// together, killed if it outlives `timeout`.
pub fn run_in(
    dir: &std::path::Path,
    command: &str,
    timeout: Duration,
) -> Result<(i32, String), String> {
    let mut cmd = if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", command]);
        c
    } else {
        let mut c = std::process::Command::new("sh");
        c.args(["-c", command]);
        c
    };
    cmd.current_dir(dir);
    let run = crate::process::run(cmd, timeout)?;
    let mut output = run.stdout;
    if !run.stderr.trim().is_empty() {
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(&run.stderr);
    }
    match run.code {
        Some(code) => Ok((code, output)),
        None => Err(format!(
            "{:.0} saniyede bitmedi, durduruldu. Son çıktı:\n{}",
            timeout.as_secs_f32().max(1.0),
            tail(&output, 2_000)
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exact_unique_match_is_replaced() {
        let (out, n) = replace("a\nb\nc\n", "b\n", "B\n", false).unwrap();
        assert_eq!(out, "a\nB\nc\n");
        assert_eq!(n, 1);
    }

    #[test]
    fn an_ambiguous_match_is_refused_unless_all() {
        let err = replace("x x", "x", "y", false).unwrap_err();
        assert!(err.contains("2 kez"), "{err}");
        assert_eq!(replace("x x", "x", "y", true).unwrap().0, "y y");
    }

    #[test]
    fn a_missing_match_says_to_read_first() {
        assert!(replace("abc", "zzz", "y", false)
            .unwrap_err()
            .contains("ws_read"));
        assert!(replace("abc", "", "y", false).is_err());
    }

    #[test]
    fn crlf_files_match_lf_quotes_and_stay_crlf() {
        let (out, _) = replace("a\r\nb\r\nc\r\n", "a\nb", "A\nB", false).unwrap();
        assert_eq!(out, "A\r\nB\r\nc\r\n");
    }

    #[test]
    fn the_diff_shows_the_change_in_context() {
        let d = diff("1\n2\n3\n4\n5\n", "1\n2\nthree\n4\n5\n");
        assert!(d.contains("- 3"), "{d}");
        assert!(d.contains("+ three"), "{d}");
        assert!(d.contains("  2"), "{d}");
        assert!(d.contains("  4"), "{d}");
        assert!(d.starts_with("@@ satır 3"), "{d}");
    }

    #[test]
    fn numbered_reads_a_slice_and_says_where_it_is() {
        let text = (1..=10)
            .map(|n| format!("line{n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let out = numbered(&text, Some(3.0), Some(4.0));
        assert!(out.contains("3  line3"), "{out}");
        assert!(out.contains("4  line4"), "{out}");
        assert!(!out.contains("line5"));
        assert!(out.contains("(3-4 / 10 satır)"));
        assert_eq!(numbered("", None, None), "(boş dosya)");
    }

    #[test]
    fn a_command_runs_in_the_folder_and_reports_its_exit_code() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("marker.txt"), "hi").unwrap();
        let cmd = if cfg!(windows) {
            "dir /b"
        } else {
            "ls; exit 3"
        };
        let (code, out) = run_in(dir.path(), cmd, Duration::from_secs(20)).unwrap();
        assert!(out.contains("marker.txt"), "{out}");
        if !cfg!(windows) {
            assert_eq!(code, 3);
        }
    }

    #[test]
    #[cfg(unix)]
    fn a_command_that_runs_too_long_is_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let err = run_in(
            dir.path(),
            "echo started; sleep 5",
            Duration::from_millis(300),
        )
        .unwrap_err();
        assert!(err.contains("durduruldu"), "{err}");
        assert!(err.contains("started"), "{err}");
    }

    #[test]
    fn edits_and_reads_go_through_the_workspace() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("main.rs"),
            "fn main() {\n    println!(\"a\");\n}\n",
        )
        .unwrap();
        workspace::set_root(Some(dir.path().to_path_buf()));

        let read = WsRead.run(&serde_json::json!({"path": "main.rs"}));
        assert!(
            read.ok && read.content.contains("println"),
            "{}",
            read.content
        );

        let edit = WsEdit.run(&serde_json::json!({
            "path": "main.rs", "old": "println!(\"a\");", "new": "println!(\"b\");"
        }));
        assert!(edit.ok, "{}", edit.content);
        assert!(
            edit.content.contains("+     println!(\"b\");"),
            "{}",
            edit.content
        );
        let now = std::fs::read_to_string(dir.path().join("main.rs")).unwrap();
        assert!(now.contains("\"b\""));

        let escape = WsWrite.run(&serde_json::json!({"path": "../evil.txt", "content": "x"}));
        assert!(!escape.ok, "must not leave the workspace");
        workspace::set_root(None);
    }

    #[test]
    fn changing_tools_ask_and_looking_tools_do_not() {
        assert_eq!(WsRead.risk(), Risk::Safe);
        assert_eq!(WsSearch.risk(), Risk::Safe);
        assert_eq!(WsList.risk(), Risk::Safe);
        assert_eq!(WsEdit.risk(), Risk::Destructive);
        assert_eq!(WsWrite.risk(), Risk::Destructive);
        assert_eq!(WsRun.risk(), Risk::Destructive);
    }
}
