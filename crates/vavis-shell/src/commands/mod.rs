//! Commands the interface can invoke.
//!
//! Each `#[tauri::command]` is one call the frontend can make. They are
//! deliberately thin: validate, delegate to a layer, return a plain type.
//! Business logic belongs in `vavis-brain` / `vavis-tools`, not here.
//!
//! Long-running work (a model request) emits events instead of blocking,
//! so the interface stays responsive and can stream tokens as they arrive.

pub mod canvas;
pub mod chat;
pub mod connection;
pub mod conversations;
pub mod council;
pub mod errors;
pub mod llm;
pub mod mcp;
pub mod memory;
pub mod obsidian;
pub mod phone;
pub mod search;
pub mod settings;
pub mod speech;
pub mod spotify;
pub mod status;
pub mod steam;
pub mod updates;
pub mod virustotal;
pub mod workspace;

use crate::state::AppState;
use serde::Serialize;
use std::sync::atomic::Ordering;
use tauri::{Emitter, Manager, State};
use vavis_brain::{ChatConfig, Message, Provider, StreamEvent};
use vavis_tools::{AgentHost, Approval, ApprovalReason, ToolOutcome, MAX_STEPS};

use chat::MAX_RATE_LIMIT_WAIT_SECS;
use errors::{error_is_too_long, friendly_error, wait_before_retry};
use llm::{chat_config, fallbacks_for, is_usable, llm_for, model_for};

/// Opens a URL in the user's default browser.
///
/// Never through `cmd /C start`, which is the obvious way and silently
/// mangles exactly the URLs this is for. `&` separates commands in `cmd`, and
/// an argument is only quoted on the way out if it contains a space, so an
/// authorisation URL arrives at the browser truncated at its first parameter
/// separator. Spotify then answers with a bare `client_id: Not present` page
/// and nothing anywhere reports an error: the process spawned fine, the
/// browser opened fine, and the request was simply not the one that was
/// built. Connecting could not work on Windows at all.
///
/// `url.dll,FileProtocolHandler` is the protocol handler itself, so the URL
/// reaches the browser exactly as built, query string and all. `explorer` is
/// the other obvious candidate and is worse: given a URL it does not
/// recognise as one it opens Documents instead, with no error either.
fn open_in_browser(url: &str) -> Result<(), String> {
    use std::process::Command;

    #[cfg(windows)]
    let result = Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn();

    #[cfg(not(windows))]
    let result = Command::new("xdg-open").arg(url).spawn();

    result
        .map(|_| ())
        .map_err(|e| format!("could not open the browser: {e}"))
}
