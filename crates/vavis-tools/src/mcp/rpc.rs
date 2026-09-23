//! JSON-RPC over the two MCP transports.
//!
//! stdio is the default: the server runs as a child process and messages are
//! newline-delimited JSON on its stdin/stdout. Reading happens on a dedicated
//! thread feeding a channel, so a server that stops answering times out
//! instead of hanging the tool call forever.

use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

/// How long to wait for one response.
///
/// Generous, because an MCP tool may do real work (a query, a web call), but
/// finite: a wedged server must not freeze the assistant.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

/// Shorter budget for the handshake — a server that cannot introduce itself
/// promptly is broken, and the user is waiting on a settings screen.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// The protocol revision announced during initialisation.
pub const PROTOCOL_VERSION: &str = "2024-11-05";

#[derive(Debug)]
pub enum RpcError {
    /// Could not start or reach the server.
    Transport(String),
    /// The server answered with an error object.
    Server(String),
    Timeout,
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(e) => write!(f, "{e}"),
            Self::Server(e) => write!(f, "sunucu hatası: {e}"),
            Self::Timeout => write!(f, "sunucu zamanında cevap vermedi"),
        }
    }
}

fn next_id() -> i64 {
    static NEXT: AtomicI64 = AtomicI64::new(1);
    NEXT.fetch_add(1, Ordering::SeqCst)
}

/// A server speaking JSON-RPC over a child process's stdio.
pub struct StdioTransport {
    child: Child,
    stdin: ChildStdin,
    /// Lines the reader thread has seen. Notifications and log lines arrive
    /// here too and are skipped while waiting for a specific id.
    lines: Receiver<String>,
}

impl StdioTransport {
    /// Starts the server process.
    pub fn spawn(
        command: &str,
        args: &[String],
        env: &[(String, String)],
    ) -> Result<Self, RpcError> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // The server's own logs go to stderr; letting them inherit would
            // scribble over our console in debug builds.
            .stderr(Stdio::null());

        for (key, value) in env {
            cmd.env(key, value);
        }

        // Without this every stdio server flashes a console window.
        vavis_core::process::hidden(&mut cmd);

        let mut child = cmd
            .spawn()
            .map_err(|e| RpcError::Transport(format!("'{command}' başlatılamadı: {e}")))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| RpcError::Transport("stdin alınamadı".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| RpcError::Transport("stdout alınamadı".into()))?;

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                // A closed receiver means the client was dropped; stop.
                if tx.send(line).is_err() {
                    return;
                }
            }
        });

        Ok(Self {
            child,
            stdin,
            lines: rx,
        })
    }

    /// Sends a request and waits for the matching response.
    pub fn request(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, RpcError> {
        let id = next_id();
        let message = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        writeln!(self.stdin, "{message}")
            .and_then(|()| self.stdin.flush())
            .map_err(|e| RpcError::Transport(format!("istek yazılamadı: {e}")))?;

        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(RpcError::Timeout);
            }

            let line = match self.lines.recv_timeout(remaining) {
                Ok(line) => line,
                Err(mpsc::RecvTimeoutError::Timeout) => return Err(RpcError::Timeout),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(RpcError::Transport("sunucu kapandı".into()))
                }
            };

            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                // Not JSON: some servers print banners. Ignore it.
                continue;
            };
            // Notifications carry no id; responses to other requests carry a
            // different one. Only ours ends the wait.
            if value.get("id").and_then(Value::as_i64) != Some(id) {
                continue;
            }

            return parse_response(&value);
        }
    }

    /// Sends a notification, which has no reply.
    pub fn notify(&mut self, method: &str, params: Value) -> Result<(), RpcError> {
        let message = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        writeln!(self.stdin, "{message}")
            .and_then(|()| self.stdin.flush())
            .map_err(|e| RpcError::Transport(format!("bildirim yazılamadı: {e}")))
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        // The server is our child; leaving it running would leak a process
        // every time a connection is replaced.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Pulls `result` out of a JSON-RPC response, or reports its error.
pub fn parse_response(value: &Value) -> Result<Value, RpcError> {
    if let Some(error) = value.get("error") {
        let message = error["message"]
            .as_str()
            .unwrap_or("bilinmeyen hata")
            .to_string();
        return Err(RpcError::Server(message));
    }
    Ok(value.get("result").cloned().unwrap_or(Value::Null))
}

/// A server reached over HTTP.
///
/// Simpler than stdio: every request is one POST. Used for hosted servers,
/// where there is no child process to manage.
pub struct HttpTransport {
    pub url: String,
    pub header: Option<(String, String)>,
}

impl HttpTransport {
    pub fn request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, RpcError> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": next_id(),
            "method": method,
            "params": params,
        });

        crate::run_async(async {
            let client = reqwest::Client::builder()
                .timeout(timeout)
                .build()
                .map_err(|e| RpcError::Transport(e.to_string()))?;

            let mut req = client
                .post(&self.url)
                .header("Content-Type", "application/json")
                // Servers that speak the streamable transport answer JSON
                // when asked for it.
                .header("Accept", "application/json, text/event-stream");
            if let Some((name, value)) = &self.header {
                req = req.header(name.as_str(), value);
            }

            let resp = req.json(&body).send().await.map_err(|e| {
                if e.is_timeout() {
                    RpcError::Timeout
                } else {
                    RpcError::Transport(e.to_string())
                }
            })?;

            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            if !status.is_success() {
                return Err(RpcError::Transport(format!("HTTP {status}")));
            }

            let value = parse_maybe_sse(&text)
                .ok_or_else(|| RpcError::Transport("yanıt çözümlenemedi".into()))?;
            parse_response(&value)
        })
        .map_err(RpcError::Transport)?
    }
}

/// Reads a body that may be plain JSON or a single server-sent event.
///
/// Streamable-HTTP servers answer a plain request with one `data:` frame;
/// treating that as JSON would fail for no good reason.
pub fn parse_maybe_sse(text: &str) -> Option<Value> {
    if let Ok(value) = serde_json::from_str::<Value>(text) {
        return Some(value);
    }
    for line in text.lines() {
        if let Some(payload) = line.strip_prefix("data:") {
            if let Ok(value) = serde_json::from_str::<Value>(payload.trim()) {
                return Some(value);
            }
        }
    }
    None
}

/// Default timeouts, exposed so the client reads clearly.
pub fn response_timeout() -> Duration {
    RESPONSE_TIMEOUT
}
pub fn handshake_timeout() -> Duration {
    HANDSHAKE_TIMEOUT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_result_is_unwrapped() {
        let value = serde_json::json!({"jsonrpc": "2.0", "id": 1, "result": {"ok": true}});
        assert_eq!(
            parse_response(&value).unwrap(),
            serde_json::json!({"ok": true})
        );
    }

    #[test]
    fn an_error_object_becomes_an_error() {
        let value = serde_json::json!({
            "jsonrpc": "2.0", "id": 1,
            "error": {"code": -32601, "message": "Method not found"}
        });
        let err = parse_response(&value).unwrap_err();
        assert!(err.to_string().contains("Method not found"), "{err}");
    }

    #[test]
    fn a_missing_result_is_null_not_a_failure() {
        // Notifications-turned-responses and empty acks look like this.
        let value = serde_json::json!({"jsonrpc": "2.0", "id": 1});
        assert_eq!(parse_response(&value).unwrap(), Value::Null);
    }

    #[test]
    fn ids_are_unique_and_increasing() {
        let a = next_id();
        let b = next_id();
        assert!(b > a, "each request needs its own id");
    }

    #[test]
    fn plain_json_bodies_are_parsed() {
        let value = parse_maybe_sse(r#"{"jsonrpc":"2.0","id":1,"result":42}"#).unwrap();
        assert_eq!(value["result"], 42);
    }

    #[test]
    fn a_single_sse_frame_is_parsed() {
        let body = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":7}\n\n";
        let value = parse_maybe_sse(body).expect("should read the data frame");
        assert_eq!(value["result"], 7);
    }

    #[test]
    fn an_unparseable_body_yields_nothing() {
        assert!(parse_maybe_sse("<html>nope</html>").is_none());
        assert!(parse_maybe_sse("").is_none());
    }

    #[test]
    fn spawning_a_missing_command_fails_cleanly() {
        // `StdioTransport` holds a child process and is not `Debug`, so the
        // result is matched rather than unwrapped.
        match StdioTransport::spawn("definitely-not-a-real-command-xyz", &[], &[]) {
            Ok(_) => panic!("a missing command must not start"),
            Err(err) => assert!(
                err.to_string().contains("başlatılamadı"),
                "should name the problem: {err}"
            ),
        }
    }
}
