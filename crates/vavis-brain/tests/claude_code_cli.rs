//! The Claude Code provider against a stand-in CLI.
//!
//! A shell script plays `claude`: it checks the flags it was given, reads
//! the prompt from stdin, and prints stream-json the way the real CLI does.
//! That exercises the process plumbing -- spawning, stdin, line reading,
//! temp files -- without a network or a login. Unix-only, since the
//! stand-in is a shell script; the parsing itself is covered on every
//! platform by the unit tests in `claude_code`.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use vavis_brain::{BrainClient, BrainError, ChatConfig, Message, Provider, StreamEvent};

/// `VAVIS_CLAUDE_PATH` is process-wide, so the tests take turns.
static ENV: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn fake_cli(dir: &std::path::Path, body: &str) -> std::path::PathBuf {
    let path = dir.join("claude");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

const HAPPY: &str = r#"
# The prompt must arrive on stdin, as one stream-json user line.
read line
case "$line" in
  *'"type":"user"'*) ;;
  *) echo "no user line on stdin" >&2; exit 3 ;;
esac
# No shell or file tools may be switched on.
case "$*" in
  *Bash*|*Edit*|*Write*) echo "dangerous tools on: $*" >&2; exit 4 ;;
esac
echo '{"type":"system","subtype":"init","mcp_servers":[]}'
echo '{"type":"stream_event","parent_tool_use_id":null,"event":{"type":"message_start","message":{}}}'
echo '{"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"mer"}}}'
echo '{"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"haba"}}}'
echo '{"type":"result","subtype":"success","is_error":false,"result":"merhaba"}'
"#;

#[tokio::test]
async fn a_reply_streams_from_the_cli() {
    let _env = ENV.lock().await;
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("VAVIS_CLAUDE_PATH", fake_cli(dir.path(), HAPPY));

    let cfg = ChatConfig::new(Provider::ClaudeCode, "haiku", "");
    let mut deltas = Vec::new();
    let reply = BrainClient::new()
        .chat_stream(
            &cfg,
            vec![Message::system("sys"), Message::user("selam")],
            |e| {
                if let StreamEvent::Delta(t) = e {
                    deltas.push(t);
                }
            },
        )
        .await;

    std::env::remove_var("VAVIS_CLAUDE_PATH");
    assert_eq!(reply.unwrap(), "merhaba");
    assert_eq!(deltas, vec!["mer", "haba"]);
}

#[tokio::test]
async fn a_logged_out_cli_is_reported_as_such() {
    let _env = ENV.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let script = r#"read line
echo '{"type":"result","subtype":"success","is_error":true,"result":"Invalid API key · Please run /login"}'"#;
    std::env::set_var("VAVIS_CLAUDE_PATH", fake_cli(dir.path(), script));

    let cfg = ChatConfig::new(Provider::ClaudeCode, "default", "");
    let err = BrainClient::new()
        .chat_stream(&cfg, vec![Message::user("selam")], |_| {})
        .await
        .unwrap_err();

    std::env::remove_var("VAVIS_CLAUDE_PATH");
    assert!(matches!(err, BrainError::CliLogin(_)), "{err:?}");
}

#[tokio::test]
async fn a_cli_that_dies_reports_its_stderr() {
    let _env = ENV.lock().await;
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var(
        "VAVIS_CLAUDE_PATH",
        fake_cli(
            dir.path(),
            "read line\necho 'something broke badly' >&2\nexit 1",
        ),
    );

    let cfg = ChatConfig::new(Provider::ClaudeCode, "default", "");
    let err = BrainClient::new()
        .chat_stream(&cfg, vec![Message::user("selam")], |_| {})
        .await
        .unwrap_err();

    std::env::remove_var("VAVIS_CLAUDE_PATH");
    assert!(err.to_string().contains("something broke badly"), "{err}");
}

#[tokio::test]
async fn a_missing_cli_is_its_own_error() {
    let _env = ENV.lock().await;
    std::env::set_var("VAVIS_CLAUDE_PATH", "/nonexistent/claude");
    let cfg = ChatConfig::new(Provider::ClaudeCode, "default", "");
    let err = BrainClient::new()
        .chat_stream(&cfg, vec![Message::user("selam")], |_| {})
        .await
        .unwrap_err();
    std::env::remove_var("VAVIS_CLAUDE_PATH");
    assert!(matches!(err, BrainError::CliMissing), "{err:?}");
}

/// Against the real CLI. Needs `claude` installed and logged in, and spends
/// a few tokens of the subscription, so it only runs when asked:
/// `cargo test -p vavis-brain --test claude_code_cli -- --ignored`
#[tokio::test]
#[ignore]
async fn the_real_cli_answers() {
    let cfg = ChatConfig::new(Provider::ClaudeCode, "haiku", "");
    let reply = BrainClient::new()
        .chat_stream(
            &cfg,
            vec![Message::user("Reply with the single word: ready")],
            |_| {},
        )
        .await
        .unwrap();
    assert!(reply.to_lowercase().contains("ready"), "{reply}");
}
