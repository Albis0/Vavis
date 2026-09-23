//! Claude through the Claude Code CLI: the user's own Claude subscription
//! as a provider, with no API key.
//!
//! Every other provider here is an HTTP endpoint and a key. This one is a
//! program on the user's machine. `claude -p` answers one prompt and exits,
//! and with `--output-format stream-json` it reports everything it does as
//! one JSON object per line -- including the same text deltas the Messages
//! API streams, so the reply still arrives word by word.
//!
//! ## Why not the API
//!
//! The API bills per token. A Pro or Max subscription already pays for
//! Claude, and the CLI is the sanctioned way to spend it from a program. For
//! someone whose alternatives are a rate-limited free tier, that is the
//! difference between an assistant that works and one that does not.
//!
//! ## Tools
//!
//! Claude Code runs its own agent loop: it calls a tool, reads the result,
//! and carries on until it has an answer, all inside one process. Vavis's
//! tools cannot be handed to it as schemas the way they are to an API; they
//! reach it over MCP instead. The shell runs a small MCP server on
//! localhost (`vavis_tools::mcp::bridge`) and this module points the CLI at
//! it with `--mcp-config`. Every call still goes through Vavis's permission
//! gate, so a destructive tool asks the user exactly as it does with any
//! other provider -- Claude Code just waits for the answer.
//!
//! Its own built-in tools are cut down to web search and fetch. Both are
//! read-only and served by Anthropic, so they need no key of ours. Shell and
//! edit tools are removed outright: they would act on the machine without
//! passing through the gate, which is the one thing that must not happen.
//! A code turn also gets the read-only file tools, confined to the project
//! (see `CODE_READ_TOOLS`); changing it still goes through Vavis.
//!
//! ## History
//!
//! Each turn is a fresh process with no saved session
//! (`--no-session-persistence`). The conversation so far goes into the
//! system prompt as a transcript and the new message is the user turn.
//! Resuming a CLI session instead would be cheaper per turn, but Vavis edits
//! its history -- clears it, forgets the oldest half, trims it to fit -- and
//! a session on disk would quietly disagree with all of that. Stateless is
//! the version that cannot drift. The transcript sits at the end of the
//! system prompt, so the unchanged prefix still hits the prompt cache.

use crate::client::{BrainError, ChatConfig, ChatResponse, Result, StreamEvent};
use crate::message::{Message, Role};
use serde_json::{json, Value};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// The CLI's own default model: whatever the user's plan and settings pick.
pub const DEFAULT_MODEL: &str = "default";

/// Aliases the CLI resolves to the newest model of each tier, so the list
/// never goes stale the way a table of dated ids does.
pub const MODELS: [&str; 4] = [DEFAULT_MODEL, "opus", "sonnet", "haiku"];

/// Stands in for a URL in the provider table, which wants one per provider.
/// Nothing ever connects to it.
pub const PSEUDO_URL: &str = "cli://claude-code";

/// The MCP server name Vavis's tools appear under. Claude Code prefixes
/// each tool with it: `mcp__vavis__take_screenshot`.
pub const SERVER_NAME: &str = "vavis";

/// Claude Code's built-in tools that stay on. Read-only, served remotely.
const BUILTIN_TOOLS: &str = "WebSearch,WebFetch";

/// Added for a code turn: Claude Code's own file reading, which is better
/// at finding its way round a project than anything Vavis offers. Read-only
/// -- every change still goes through Vavis's `ws_edit`/`ws_write`/`ws_run`
/// and the gate -- and confined by the CLI to the working directory, which
/// is the project; reading elsewhere needs a permission print mode cannot
/// grant.
const CODE_READ_TOOLS: &str = "Read,Glob,Grep";

/// How long the CLI may stay silent before the turn is abandoned.
///
/// Long, because silence is normal while a tool runs -- and a destructive
/// tool waits for the user to press a button, which can take as long as it
/// takes. Finite, because a wedged process must not hold the turn forever.
const IDLE_TIMEOUT: Duration = Duration::from_secs(20 * 60);

/// How long one MCP tool call may take, as the CLI counts it. Matched to
/// the idle timeout: an approval dialog left open is the slow case.
const TOOL_TIMEOUT_MS: &str = "1200000";

/// Where Vavis's tools can be reached for this turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolBridge {
    /// `http://127.0.0.1:<port>/mcp`
    pub url: String,
    /// Sent as a bearer token. Without it the bridge answers nothing, so
    /// another program on the machine cannot drive Vavis's tools.
    pub token: String,
}

// ── Finding the program ─────────────────────────────────────────────────────

/// The Claude Code executable, if one is installed.
///
/// `VAVIS_CLAUDE_PATH` wins when set, for an install somewhere unusual.
/// Otherwise `PATH`, then the places the two installers put it: the native
/// installer's `~/.local/bin` and npm's global folder. A GUI app does not
/// always inherit the `PATH` a terminal has, which is why the fixed places
/// are checked at all.
pub fn find_cli() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("VAVIS_CLAUDE_PATH") {
        let path = PathBuf::from(explicit);
        return path.is_file().then_some(path);
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    let home = home_dir();
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
    candidates(&path, home.as_deref(), appdata.as_deref())
        .into_iter()
        .find(|p| p.is_file())
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// Every place worth looking, in order. Separate from [`find_cli`] so the
/// search order is testable without an install.
fn candidates(path: &std::ffi::OsStr, home: Option<&Path>, appdata: Option<&Path>) -> Vec<PathBuf> {
    // On Windows the npm install is a `.cmd` shim and the native one an
    // `.exe`; a bare `claude` there is neither. Elsewhere the bare name is
    // the whole story.
    let names: &[&str] = if cfg!(windows) {
        &["claude.exe", "claude.cmd"]
    } else {
        &["claude"]
    };

    let mut out = Vec::new();
    for dir in std::env::split_paths(path) {
        for name in names {
            out.push(dir.join(name));
        }
    }
    if let Some(home) = home {
        for name in names {
            out.push(home.join(".local").join("bin").join(name));
            out.push(home.join(".claude").join("local").join(name));
        }
    }
    if let Some(appdata) = appdata {
        out.push(appdata.join("npm").join("claude.cmd"));
    }
    out
}

/// The installed version, e.g. `2.1.281 (Claude Code)`.
pub async fn version() -> Result<String> {
    let cli = find_cli().ok_or(BrainError::CliMissing)?;
    let mut cmd = tokio::process::Command::new(&cli);
    cmd.arg("--version").kill_on_drop(true);
    hide_window(&mut cmd);
    let out = tokio::time::timeout(Duration::from_secs(20), cmd.output())
        .await
        .map_err(|_| BrainError::Cli("`claude --version` did not answer".into()))?
        .map_err(|e| BrainError::Cli(format!("could not start {}: {e}", cli.display())))?;
    if !out.status.success() {
        return Err(BrainError::Cli(tail(&String::from_utf8_lossy(&out.stderr))));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// No console window flashing up for every message on Windows.
fn hide_window(cmd: &mut tokio::process::Command) {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

// ── The prompt ──────────────────────────────────────────────────────────────

/// What one CLI run is given.
#[derive(Debug, Clone, PartialEq)]
pub struct Prompt {
    /// Vavis's system prompt, with the conversation so far appended.
    pub system: String,
    /// The user turn as a stream-json input line.
    pub message: Value,
}

/// Longest a single earlier tool result may be in the transcript. The model
/// already acted on it; what it needs now is the gist.
const MAX_TRANSCRIPT_TOOL_RESULT: usize = 2_000;

/// Turns a conversation into a system prompt and one user turn.
///
/// The last message is the question when it is the user's. Anything else
/// in last place (a council seat's framing, say) goes into the transcript
/// too, and the model is asked to continue.
pub fn build_prompt(messages: &[Message]) -> Prompt {
    let mut system = String::new();
    let mut rest: Vec<&Message> = Vec::new();
    for m in messages {
        if m.role == Role::System {
            if !system.is_empty() {
                system.push_str("\n\n");
            }
            system.push_str(&m.content);
        } else {
            rest.push(m);
        }
    }

    let last_is_user = rest.last().is_some_and(|m| m.role == Role::User);
    let (history, current) = if last_is_user {
        let (current, history) = rest.split_last().expect("checked non-empty");
        (history.to_vec(), Some(*current))
    } else {
        (rest, None)
    };

    if !history.is_empty() {
        system.push_str(
            "\n\n# Conversation so far\n\n\
             The messages below were exchanged before the current one, oldest \
             first. They are context: reply only to the latest message, which \
             arrives as the user turn.\n\n<history>\n",
        );
        for m in &history {
            system.push_str(&transcript_line(m));
        }
        system.push_str("</history>");
    }

    let mut content = Vec::new();
    match current {
        Some(m) => {
            let text = if m.content.trim().is_empty() && m.image.is_some() {
                "(image attached)"
            } else {
                m.content.as_str()
            };
            content.push(json!({"type": "text", "text": text}));
            if let Some(image) = &m.image {
                content.push(json!({
                    "type": "image",
                    "source": {"type": "base64", "media_type": "image/png", "data": image}
                }));
            }
        }
        None => content.push(json!({"type": "text", "text": "Continue."})),
    }

    Prompt {
        system,
        message: json!({
            "type": "user",
            "message": {"role": "user", "content": content}
        }),
    }
}

fn transcript_line(m: &Message) -> String {
    match m.role {
        Role::User => {
            let image = if m.image.is_some() { " [image]" } else { "" };
            format!("<user>{image}\n{}\n</user>\n", m.content.trim())
        }
        Role::Assistant => {
            let calls = m
                .tool_calls
                .as_ref()
                .filter(|c| !c.is_empty())
                .map(|c| {
                    let names: Vec<&str> = c.iter().map(|c| c.function.name.as_str()).collect();
                    format!(" [used: {}]", names.join(", "))
                })
                .unwrap_or_default();
            format!("<assistant>{calls}\n{}\n</assistant>\n", m.content.trim())
        }
        Role::Tool => {
            let mut text: String = m.content.chars().take(MAX_TRANSCRIPT_TOOL_RESULT).collect();
            if m.content.chars().count() > MAX_TRANSCRIPT_TOOL_RESULT {
                text.push_str(" …");
            }
            format!("<tool_result>\n{}\n</tool_result>\n", text.trim())
        }
        Role::System => String::new(),
    }
}

/// The `--mcp-config` document pointing the CLI at Vavis's tools.
pub fn mcp_config(bridge: &ToolBridge) -> Value {
    json!({
        "mcpServers": {
            SERVER_NAME: {
                "type": "http",
                "url": bridge.url,
                "headers": {"Authorization": format!("Bearer {}", bridge.token)}
            }
        }
    })
}

/// The command line, minus the program.
///
/// Nothing the user or the model wrote goes on it: the prompt travels on
/// stdin and the system prompt in a file. Windows caps a command line, and
/// a `.cmd` shim runs through `cmd.exe`, which reinterprets `%` and `&` --
/// text on the command line is a problem twice over.
pub fn args(model: &str, system_file: &Path, mcp_file: Option<&Path>) -> Vec<OsString> {
    args_for(model, system_file, mcp_file, false)
}

/// As [`args`]; `code` switches on the read-only file tools for a turn
/// running inside a project.
pub fn args_for(
    model: &str,
    system_file: &Path,
    mcp_file: Option<&Path>,
    code: bool,
) -> Vec<OsString> {
    let builtin = if code {
        format!("{BUILTIN_TOOLS},{CODE_READ_TOOLS}")
    } else {
        BUILTIN_TOOLS.to_string()
    };
    let mut args: Vec<OsString> = [
        "-p",
        "--verbose",
        "--output-format",
        "stream-json",
        "--input-format",
        "stream-json",
        "--include-partial-messages",
        "--no-session-persistence",
        // The user's own MCP servers stay out of it: they were configured
        // for coding sessions and would bypass the gate.
        "--strict-mcp-config",
        "--tools",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    args.push(builtin.clone().into());

    args.push("--system-prompt-file".into());
    args.push(system_file.into());

    // One comma-separated value: `--allowedTools` is variadic and would
    // swallow whatever came after a space-separated list.
    let mut allowed = builtin;
    if let Some(mcp) = mcp_file {
        args.push("--mcp-config".into());
        args.push(mcp.into());
        // Allowed here because Vavis's gate is the one that asks. Letting
        // the CLI ask as well is not possible in print mode -- it would
        // simply refuse every call.
        allowed.push_str(",mcp__");
        allowed.push_str(SERVER_NAME);
    }
    args.push("--allowedTools".into());
    args.push(allowed.into());

    let model = model.trim();
    if !model.is_empty() && model != DEFAULT_MODEL {
        args.push("--model".into());
        args.push(model.into());
    }
    args
}

// ── Reading the stream ──────────────────────────────────────────────────────

/// What one line of output meant.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Text(String),
    /// The run ended normally. Carries the CLI's own copy of the final
    /// answer, used when nothing was streamed.
    Finished(String),
    Failed(BrainErrorKind),
}

/// A run that ended badly, classified so the user can be told what to do.
#[derive(Debug, Clone, PartialEq)]
pub enum BrainErrorKind {
    Login(String),
    UsageLimit { resets_at: Option<i64> },
    Other(String),
}

impl From<BrainErrorKind> for BrainError {
    fn from(kind: BrainErrorKind) -> Self {
        match kind {
            BrainErrorKind::Login(detail) => BrainError::CliLogin(detail),
            BrainErrorKind::UsageLimit { resets_at } => BrainError::UsageLimit { resets_at },
            BrainErrorKind::Other(detail) => BrainError::Cli(detail),
        }
    }
}

/// Folds output lines into events.
#[derive(Debug, Default)]
pub struct StreamState {
    /// Text has been streamed in an earlier model message of this run.
    wrote_text: bool,
    /// A new model message began after text was already written, so the
    /// next text needs a paragraph break before it: "Let me look." and the
    /// answer that follows the look are two paragraphs, not one run-on.
    need_break: bool,
    /// When the plan's limit resets, if the CLI said so.
    resets_at: Option<i64>,
}

impl StreamState {
    pub fn feed(&mut self, line: &str) -> Vec<Event> {
        let Ok(v) = serde_json::from_str::<Value>(line.trim()) else {
            return Vec::new();
        };
        match v["type"].as_str().unwrap_or_default() {
            "stream_event" => self.stream_event(&v),
            "rate_limit_event" => {
                let info = &v["rate_limit_info"];
                if info["status"] == "rejected" {
                    self.resets_at = info["resetsAt"].as_i64();
                }
                Vec::new()
            }
            "system" if v["subtype"] == "init" => {
                // Not fatal -- the model can still answer without tools --
                // but it is the first thing to look at when a tool "does
                // nothing".
                for server in v["mcp_servers"].as_array().into_iter().flatten() {
                    if server["name"] == SERVER_NAME && server["status"] != "connected" {
                        tracing::warn!(status = %server["status"], "Claude Code could not reach Vavis's tools");
                    }
                }
                Vec::new()
            }
            "result" => vec![self.result(&v)],
            _ => Vec::new(),
        }
    }

    fn stream_event(&mut self, v: &Value) -> Vec<Event> {
        // A sub-agent's text is its working, not the answer.
        if !v["parent_tool_use_id"].is_null() {
            return Vec::new();
        }
        let event = &v["event"];
        match event["type"].as_str().unwrap_or_default() {
            "message_start" => {
                if self.wrote_text {
                    self.need_break = true;
                }
                Vec::new()
            }
            "content_block_delta" if event["delta"]["type"] == "text_delta" => {
                let text = event["delta"]["text"].as_str().unwrap_or_default();
                if text.is_empty() {
                    return Vec::new();
                }
                let mut out = Vec::new();
                if self.need_break {
                    self.need_break = false;
                    out.push(Event::Text("\n\n".into()));
                }
                self.wrote_text = true;
                out.push(Event::Text(text.to_string()));
                out
            }
            _ => Vec::new(),
        }
    }

    fn result(&self, v: &Value) -> Event {
        let text = v["result"].as_str().unwrap_or_default().to_string();
        let failed = v["is_error"].as_bool().unwrap_or(false)
            || v["subtype"]
                .as_str()
                .is_some_and(|s| s.starts_with("error"));
        if !failed {
            return Event::Finished(text);
        }
        let detail = if text.trim().is_empty() {
            v["subtype"].as_str().unwrap_or("unknown error").to_string()
        } else {
            text
        };
        Event::Failed(classify(&detail, self.resets_at))
    }
}

/// Sorts a failure message into something actionable.
fn classify(detail: &str, resets_at: Option<i64>) -> BrainErrorKind {
    let lower = detail.to_ascii_lowercase();
    if lower.contains("/login")
        || lower.contains("not logged in")
        || lower.contains("invalid api key")
        || lower.contains("oauth")
        || lower.contains("authentication")
        || lower.contains("credit balance")
    {
        return BrainErrorKind::Login(tail(detail));
    }
    if lower.contains("usage limit")
        || lower.contains("limit reached")
        || lower.contains("rate limit")
    {
        // The CLI writes the reset time after a `|` as a unix timestamp on
        // some versions: "Claude AI usage limit reached|1790208000".
        let from_text = detail
            .rsplit_once('|')
            .and_then(|(_, t)| t.trim().parse::<i64>().ok());
        return BrainErrorKind::UsageLimit {
            resets_at: from_text.or(resets_at),
        };
    }
    BrainErrorKind::Other(tail(detail))
}

/// The last few hundred characters: where a CLI puts the part that matters.
fn tail(text: &str) -> String {
    let text = text.trim();
    let count = text.chars().count();
    if count <= 400 {
        return text.to_string();
    }
    let skip = count - 400;
    format!("…{}", text.chars().skip(skip).collect::<String>())
}

// ── Running it ──────────────────────────────────────────────────────────────

/// Files a run needs, removed when it ends however it ends.
struct Scratch {
    files: Vec<PathBuf>,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        for f in &self.files {
            let _ = std::fs::remove_file(f);
        }
    }
}

/// An empty folder to run in, so no project's `CLAUDE.md` or settings are
/// picked up from wherever Vavis happened to start.
fn work_dir() -> std::io::Result<PathBuf> {
    let dir = std::env::temp_dir().join("vavis-claude");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn scratch_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    dir.join(format!("{stem}-{}-{n}.{ext}", std::process::id()))
}

/// One turn through the CLI.
///
/// `bridge` is where Vavis's tools live for this turn; `None` runs without
/// them (a council seat, a router question).
pub async fn run<F>(
    cfg: &ChatConfig,
    messages: Vec<Message>,
    bridge: Option<&ToolBridge>,
    mut on_event: F,
) -> Result<ChatResponse>
where
    F: FnMut(StreamEvent),
{
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

    let cli = find_cli().ok_or(BrainError::CliMissing)?;

    // The window is large, but the transcript is still bounded: a pasted
    // novel from a week ago should not ride along on every turn.
    let fitted = crate::budget::fit_request(
        messages,
        Vec::new(),
        crate::budget::ModelCaps::for_model("claude"),
    );
    let prompt = build_prompt(&fitted.messages);

    let dir = work_dir().map_err(|e| BrainError::Cli(format!("temp folder: {e}")))?;
    let mut scratch = Scratch { files: Vec::new() };

    let system_file = scratch_path(&dir, "system", "md");
    std::fs::write(&system_file, &prompt.system)
        .map_err(|e| BrainError::Cli(format!("could not write the system prompt: {e}")))?;
    scratch.files.push(system_file.clone());

    let mcp_file = match bridge {
        Some(bridge) => {
            let path = scratch_path(&dir, "mcp", "json");
            std::fs::write(&path, mcp_config(bridge).to_string())
                .map_err(|e| BrainError::Cli(format!("could not write the MCP config: {e}")))?;
            scratch.files.push(path.clone());
            Some(path)
        }
        None => None,
    };

    let project = cfg.workdir.as_deref().filter(|p| p.is_dir());
    let mut cmd = tokio::process::Command::new(&cli);
    cmd.args(args_for(
        &cfg.model,
        &system_file,
        mcp_file.as_deref(),
        project.is_some(),
    ))
    .current_dir(project.unwrap_or(&dir))
    .env("MCP_TOOL_TIMEOUT", TOOL_TIMEOUT_MS)
    .env("MCP_TIMEOUT", "30000")
    // One process per message: an update check on each would be waste,
    // and the user's interactive sessions still update themselves.
    .env("DISABLE_AUTOUPDATER", "1")
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    // Dropping the turn (the user closed the app, the thread ended)
    // must not leave a process running on their subscription.
    .kill_on_drop(true);
    hide_window(&mut cmd);

    let mut child = cmd
        .spawn()
        .map_err(|e| BrainError::Cli(format!("could not start {}: {e}", cli.display())))?;

    // The prompt, then EOF: the CLI answers what it was given and exits.
    let mut stdin = child.stdin.take().expect("stdin was piped");
    let mut line = prompt.message.to_string();
    line.push('\n');
    stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|e| BrainError::Cli(format!("could not send the prompt: {e}")))?;
    drop(stdin);

    // Drained alongside stdout so a chatty stderr cannot fill its pipe and
    // stall the process; kept for the error message if the run fails.
    let mut stderr = child.stderr.take().expect("stderr was piped");
    let stderr_task = tokio::spawn(async move {
        let mut buf = String::new();
        let _ = stderr.read_to_string(&mut buf).await;
        buf
    });

    let stdout = child.stdout.take().expect("stdout was piped");
    let mut lines = BufReader::new(stdout).lines();
    let mut state = StreamState::default();
    let mut streamed = String::new();
    let mut outcome: Option<std::result::Result<String, BrainError>> = None;

    loop {
        let next = tokio::time::timeout(IDLE_TIMEOUT, lines.next_line()).await;
        let line = match next {
            Err(_) => {
                outcome = Some(Err(BrainError::Cli(
                    "Claude Code stopped responding".into(),
                )));
                break;
            }
            Ok(Err(e)) => {
                outcome = Some(Err(BrainError::Cli(format!("reading output: {e}"))));
                break;
            }
            Ok(Ok(None)) => break,
            Ok(Ok(Some(line))) => line,
        };
        for event in state.feed(&line) {
            match event {
                Event::Text(text) => {
                    streamed.push_str(&text);
                    on_event(StreamEvent::Delta(text));
                }
                Event::Finished(text) => outcome = Some(Ok(text)),
                Event::Failed(kind) => outcome = Some(Err(kind.into())),
            }
        }
    }

    let _ = child.wait().await;
    let stderr = stderr_task.await.unwrap_or_default();
    drop(scratch);

    match outcome {
        Some(Ok(final_text)) => {
            on_event(StreamEvent::Done);
            // What was streamed is what the user watched arrive; prefer it
            // so the saved reply matches the screen. The CLI's own copy is
            // the fallback for a run that streamed nothing.
            let text = if streamed.trim().is_empty() {
                final_text
            } else {
                streamed
            };
            Ok(ChatResponse {
                text,
                tool_calls: Vec::new(),
            })
        }
        Some(Err(e)) => Err(e),
        // Ended without a result line: it died before it could say why on
        // stdout, so stderr has the reason.
        None => {
            let detail = if stderr.trim().is_empty() {
                "Claude Code exited without an answer".to_string()
            } else {
                stderr
            };
            Err(classify(&detail, state.resets_at).into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_all(lines: &[&str]) -> Vec<Event> {
        let mut s = StreamState::default();
        lines.iter().flat_map(|l| s.feed(l)).collect()
    }

    fn delta(text: &str) -> String {
        json!({
            "type": "stream_event",
            "parent_tool_use_id": null,
            "event": {"type": "content_block_delta", "index": 1,
                      "delta": {"type": "text_delta", "text": text}}
        })
        .to_string()
    }

    fn message_start() -> String {
        json!({"type": "stream_event", "parent_tool_use_id": null,
               "event": {"type": "message_start", "message": {}}})
        .to_string()
    }

    // Shapes below were captured from `claude -p --output-format stream-json
    // --verbose --include-partial-messages`, Claude Code 2.1.281.

    #[test]
    fn text_deltas_stream_through() {
        let events = feed_all(&[&message_start(), &delta("mer"), &delta("haba")]);
        assert_eq!(
            events,
            vec![Event::Text("mer".into()), Event::Text("haba".into())]
        );
    }

    #[test]
    fn thinking_and_signature_deltas_are_not_text() {
        let thinking = json!({"type": "stream_event", "parent_tool_use_id": null,
            "event": {"type": "content_block_delta", "index": 0,
                      "delta": {"type": "thinking_delta", "thinking": "hmm"}}})
        .to_string();
        let signature = json!({"type": "stream_event", "parent_tool_use_id": null,
            "event": {"type": "content_block_delta", "index": 0,
                      "delta": {"type": "signature_delta", "signature": "Eo..."}}})
        .to_string();
        assert!(feed_all(&[&thinking, &signature]).is_empty());
    }

    #[test]
    fn a_second_model_message_starts_a_new_paragraph() {
        // Text, a tool call, then the answer: two model messages.
        let events = feed_all(&[
            &message_start(),
            &delta("Bakıyorum."),
            &message_start(),
            &delta("Saat 13:37."),
        ]);
        assert_eq!(
            events,
            vec![
                Event::Text("Bakıyorum.".into()),
                Event::Text("\n\n".into()),
                Event::Text("Saat 13:37.".into()),
            ]
        );
    }

    #[test]
    fn a_tool_only_first_message_adds_no_stray_break() {
        let events = feed_all(&[&message_start(), &message_start(), &delta("Cevap")]);
        assert_eq!(events, vec![Event::Text("Cevap".into())]);
    }

    #[test]
    fn a_sub_agents_text_is_not_the_answer() {
        let line = json!({"type": "stream_event", "parent_tool_use_id": "toolu_1",
            "event": {"type": "content_block_delta",
                      "delta": {"type": "text_delta", "text": "inner"}}})
        .to_string();
        assert!(feed_all(&[&line]).is_empty());
    }

    #[test]
    fn a_successful_result_finishes() {
        let line = r#"{"type":"result","subtype":"success","is_error":false,"result":"merhaba"}"#;
        assert_eq!(feed_all(&[line]), vec![Event::Finished("merhaba".into())]);
    }

    #[test]
    fn a_login_failure_says_so() {
        let line = r#"{"type":"result","subtype":"success","is_error":true,"result":"Invalid API key · Please run /login"}"#;
        assert!(matches!(
            feed_all(&[line]).as_slice(),
            [Event::Failed(BrainErrorKind::Login(_))]
        ));
    }

    #[test]
    fn a_usage_limit_carries_its_reset_time() {
        let line = r#"{"type":"result","subtype":"success","is_error":true,"result":"Claude AI usage limit reached|1790208000"}"#;
        assert_eq!(
            feed_all(&[line]),
            vec![Event::Failed(BrainErrorKind::UsageLimit {
                resets_at: Some(1_790_208_000)
            })]
        );
    }

    #[test]
    fn a_rejected_rate_limit_event_supplies_the_reset_time() {
        let limit = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","resetsAt":1790218200}}"#;
        let result = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"result":"You've hit your usage limit"}"#;
        assert_eq!(
            feed_all(&[limit, result]),
            vec![Event::Failed(BrainErrorKind::UsageLimit {
                resets_at: Some(1_790_218_200)
            })]
        );
    }

    #[test]
    fn noise_lines_are_ignored() {
        let events = feed_all(&[
            r#"{"type":"active_goal","value":null}"#,
            r#"{"type":"system","subtype":"commands_changed","commands":[]}"#,
            "not json at all",
            "",
        ]);
        assert!(events.is_empty());
    }

    #[test]
    fn a_single_question_needs_no_transcript() {
        let p = build_prompt(&[Message::system("Sen Vavis'sin."), Message::user("selam")]);
        assert_eq!(p.system, "Sen Vavis'sin.");
        assert_eq!(p.message["message"]["content"][0]["text"], "selam");
        assert_eq!(p.message["type"], "user");
    }

    #[test]
    fn earlier_turns_go_into_the_system_prompt_in_order() {
        let p = build_prompt(&[
            Message::system("Sen Vavis'sin."),
            Message::user("adım Ali"),
            Message::assistant("Memnun oldum Ali."),
            Message::user("adım neydi?"),
        ]);
        let first = p.system.find("adım Ali").unwrap();
        let second = p.system.find("Memnun oldum").unwrap();
        assert!(first < second);
        assert!(p.system.starts_with("Sen Vavis'sin."));
        assert!(
            !p.system.contains("adım neydi"),
            "the question is not history"
        );
        assert_eq!(p.message["message"]["content"][0]["text"], "adım neydi?");
    }

    #[test]
    fn an_attached_image_travels_as_an_image_block() {
        let p = build_prompt(&[Message::user_with_image("bu ne?", "iVBORw0KGgo=")]);
        let content = &p.message["message"]["content"];
        assert_eq!(content[1]["type"], "image");
        assert_eq!(content[1]["source"]["data"], "iVBORw0KGgo=");
        assert_eq!(content[1]["source"]["media_type"], "image/png");
    }

    #[test]
    fn a_long_tool_result_is_cut_in_the_transcript() {
        let long = "x".repeat(MAX_TRANSCRIPT_TOOL_RESULT * 3);
        let p = build_prompt(&[
            Message::user("a"),
            Message::tool_result("c1", long),
            Message::user("b"),
        ]);
        assert!(p.system.len() < MAX_TRANSCRIPT_TOOL_RESULT * 2);
        assert!(p.system.contains('…'));
    }

    #[test]
    fn a_conversation_not_ending_on_the_user_is_continued() {
        let p = build_prompt(&[Message::user("a"), Message::assistant("b")]);
        assert_eq!(p.message["message"]["content"][0]["text"], "Continue.");
        assert!(p.system.contains("<assistant>"));
    }

    #[test]
    fn the_mcp_config_carries_the_token_as_a_header() {
        let cfg = mcp_config(&ToolBridge {
            url: "http://127.0.0.1:5000/mcp".into(),
            token: "abc".into(),
        });
        let server = &cfg["mcpServers"][SERVER_NAME];
        assert_eq!(server["type"], "http");
        assert_eq!(server["url"], "http://127.0.0.1:5000/mcp");
        assert_eq!(server["headers"]["Authorization"], "Bearer abc");
    }

    fn arg_strings(args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn no_tool_that_touches_the_machine_is_left_on() {
        let line = arg_strings(&args(
            "sonnet",
            Path::new("s.md"),
            Some(Path::new("m.json")),
        ));
        let tools = &line[line.iter().position(|a| a == "--tools").unwrap() + 1];
        for dangerous in [
            "Bash",
            "Edit",
            "Write",
            "Read",
            "Glob",
            "Grep",
            "NotebookEdit",
        ] {
            assert!(!tools.contains(dangerous), "{dangerous} is on: {tools}");
        }
        assert!(line.contains(&"--strict-mcp-config".to_string()));
    }

    #[test]
    fn vavis_tools_are_allowed_only_when_the_bridge_is_attached() {
        let with = arg_strings(&args("", Path::new("s"), Some(Path::new("m"))));
        let without = arg_strings(&args("", Path::new("s"), None));
        let allowed =
            |a: &[String]| a[a.iter().position(|x| x == "--allowedTools").unwrap() + 1].clone();
        assert!(allowed(&with).contains("mcp__vavis"));
        assert!(!allowed(&without).contains("mcp__"));
        assert!(!without.contains(&"--mcp-config".to_string()));
    }

    #[test]
    fn a_code_turn_can_read_the_project_but_not_change_it() {
        let line = arg_strings(&args_for("", Path::new("s"), Some(Path::new("m")), true));
        let tools = &line[line.iter().position(|a| a == "--tools").unwrap() + 1];
        assert!(tools.contains("Read") && tools.contains("Grep") && tools.contains("Glob"));
        for writer in ["Edit", "Write", "Bash", "NotebookEdit"] {
            assert!(!tools.contains(writer), "{writer} is on: {tools}");
        }
        let allowed = &line[line.iter().position(|a| a == "--allowedTools").unwrap() + 1];
        assert!(allowed.contains("Read") && allowed.contains("mcp__vavis"));
    }

    #[test]
    fn the_default_model_passes_no_flag() {
        let plain = arg_strings(&args(DEFAULT_MODEL, Path::new("s"), None));
        assert!(!plain.contains(&"--model".to_string()));
        let opus = arg_strings(&args("opus", Path::new("s"), None));
        let i = opus.iter().position(|a| a == "--model").unwrap();
        assert_eq!(opus[i + 1], "opus");
    }

    #[test]
    fn the_prompt_never_goes_on_the_command_line() {
        let line = arg_strings(&args("sonnet", Path::new("s.md"), None));
        // Everything is a flag, a flag value we chose, or a path we made.
        assert!(line.iter().all(|a| !a.contains(' ')), "{line:?}");
    }

    #[test]
    fn path_comes_before_the_installer_folders() {
        let path = std::env::join_paths(["/opt/bin"]).unwrap();
        let found = candidates(&path, Some(Path::new("/home/u")), None);
        assert!(found[0].starts_with("/opt/bin"));
        assert!(found.iter().any(|p| p.starts_with("/home/u/.local/bin")));
    }

    #[test]
    fn a_long_error_keeps_its_end() {
        let text = format!("{}the reason", "x".repeat(1000));
        let t = tail(&text);
        assert!(t.ends_with("the reason"));
        assert!(t.chars().count() <= 401);
    }
}
