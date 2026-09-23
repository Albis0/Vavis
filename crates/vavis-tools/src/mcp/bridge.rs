//! Vavis's tools, served over MCP to a local agent.
//!
//! The rest of this module is an MCP *client*: Vavis connecting to servers
//! the user configured. This is the other direction. The Claude Code
//! provider runs Claude as a separate program with its own agent loop, and
//! the only way to hand that program tools is to be an MCP server it can
//! connect to. So Vavis is one, for the length of the app, on localhost.
//!
//! ## Transport
//!
//! Streamable HTTP, the plain-JSON half of it: every request is a POST of
//! one JSON-RPC message and every answer is `application/json`. A server may
//! also offer a GET stream for messages it initiates; this one has nothing
//! to initiate, so GET answers 405, which the spec defines as "no stream
//! here" and clients handle. That keeps the whole server a few hundred
//! lines of blocking I/O rather than an async HTTP stack.
//!
//! ## Who may call
//!
//! A tool here can write files and run commands, so reaching the port must
//! not be enough:
//!
//! - it listens on 127.0.0.1 only;
//! - every request must carry a bearer token generated at startup and
//!   handed only to the CLI Vavis itself spawns;
//! - a request with an `Origin` header is refused outright. Browsers attach
//!   one to every cross-site request and a CLI never does, so this closes
//!   the door on a web page probing localhost even if it guessed the token;
//! - tool calls are only accepted while a turn that attached the bridge is
//!   running. Outside one, a leaked token reaches nothing.
//!
//! Past those checks a call is not trusted either: it goes through the same
//! `Agent::execute_calls` as any provider's, permission gate and all.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// The newest protocol revision this server speaks. A client asking for an
/// older one gets its own version echoed back, as the spec allows; one
/// asking for something newer is offered this.
const PROTOCOL_VERSION: &str = "2025-06-18";

/// Largest request body accepted. Tool arguments are small; anything near
/// this is not a real client.
const MAX_BODY: usize = 8 * 1024 * 1024;

/// How long a connection may sit idle between requests.
const IDLE: Duration = Duration::from_secs(300);

/// What a tool call produced, in the shape MCP wants.
#[derive(Debug, Clone, PartialEq)]
pub struct CallResult {
    pub text: String,
    pub is_error: bool,
    /// Base64 PNG, when the tool captured the screen.
    pub image: Option<String>,
}

/// What the bridge asks of its owner.
///
/// The shell implements this: it holds the agent, the permission gate and
/// the window the approval dialogs appear in. Keeping those out of here is
/// what lets the protocol be tested without a window.
pub trait Handler: Send + Sync + 'static {
    /// Tool schemas in the OpenAI shape the registry already produces:
    /// `{"type":"function","function":{"name","description","parameters"}}`.
    fn tools(&self) -> Vec<Value>;
    /// Runs one call. Blocks for as long as it takes, approval included.
    fn call(&self, id: &str, name: &str, arguments: Value) -> CallResult;
}

/// A running bridge.
pub struct Bridge {
    addr: SocketAddr,
    token: String,
    active: Arc<AtomicBool>,
}

impl Bridge {
    /// Binds a free port on localhost and starts serving.
    pub fn start(handler: Arc<dyn Handler>) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let addr = listener.local_addr()?;
        let token = new_token();
        let active = Arc::new(AtomicBool::new(false));

        let shared = Arc::new(Shared {
            handler,
            token: token.clone(),
            active: active.clone(),
            port: addr.port(),
        });
        std::thread::Builder::new()
            .name("mcp-bridge".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { continue };
                    let shared = shared.clone();
                    let _ = std::thread::Builder::new()
                        .name("mcp-bridge-conn".into())
                        .spawn(move || serve_connection(stream, &shared));
                }
            })?;

        tracing::info!(%addr, "MCP bridge listening");
        Ok(Self {
            addr,
            token,
            active,
        })
    }

    pub fn url(&self) -> String {
        format!("http://{}/mcp", self.addr)
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    /// Opens the bridge for one turn. Tool calls are refused until this is
    /// called and again once the guard it returns is dropped.
    pub fn open(&self) -> OpenGuard {
        self.active.store(true, Ordering::SeqCst);
        OpenGuard {
            active: self.active.clone(),
        }
    }
}

/// Closes the bridge when dropped, however the turn ended.
pub struct OpenGuard {
    active: Arc<AtomicBool>,
}

impl Drop for OpenGuard {
    fn drop(&mut self) {
        self.active.store(false, Ordering::SeqCst);
    }
}

struct Shared {
    handler: Arc<dyn Handler>,
    token: String,
    active: Arc<AtomicBool>,
    port: u16,
}

/// 256 bits from the OS generator, hex-encoded.
fn new_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Equal-length comparison that does not stop at the first difference.
fn same_secret(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

// ── HTTP ────────────────────────────────────────────────────────────────────

struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

struct Response {
    status: u16,
    body: Option<Value>,
    extra: Vec<(&'static str, String)>,
}

impl Response {
    fn json(body: Value) -> Self {
        Self {
            status: 200,
            body: Some(body),
            extra: Vec::new(),
        }
    }
    fn empty(status: u16) -> Self {
        Self {
            status,
            body: None,
            extra: Vec::new(),
        }
    }
}

fn serve_connection(stream: TcpStream, shared: &Shared) {
    let _ = stream.set_read_timeout(Some(IDLE));
    let Ok(write_half) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(stream);
    let mut writer = write_half;

    // Keep-alive: the CLI reuses one connection for the whole session.
    loop {
        let request = match read_request(&mut reader) {
            Ok(Some(r)) => r,
            Ok(None) => return,
            Err(status) => {
                let _ = write_response(&mut writer, &Response::empty(status));
                return;
            }
        };
        let response = route(&request, shared);
        if write_response(&mut writer, &response).is_err() {
            return;
        }
    }
}

/// Reads one request. `Ok(None)` is a clean close; `Err` is the status to
/// answer a malformed one with before hanging up.
fn read_request(reader: &mut impl BufRead) -> Result<Option<Request>, u16> {
    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(0) => return Ok(None),
        Ok(_) => {}
        Err(_) => return Ok(None),
    }
    let mut parts = line.split_whitespace();
    let (Some(method), Some(path)) = (parts.next(), parts.next()) else {
        return Err(400);
    };
    let (method, path) = (method.to_string(), path.to_string());

    let mut headers = Vec::new();
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).map_err(|_| 400u16)? == 0 {
            return Err(400);
        }
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if headers.len() > 100 {
            return Err(431);
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }

    let length = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .map(|(_, v)| v.parse::<usize>().map_err(|_| 400u16))
        .transpose()?
        .unwrap_or(0);
    if length > MAX_BODY {
        return Err(413);
    }
    if headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("transfer-encoding") && v != "identity")
    {
        // Chunked uploads are legal HTTP but no MCP client sends them for
        // a JSON-RPC message; refusing is simpler than parsing them.
        return Err(411);
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).map_err(|_| 400u16)?;

    Ok(Some(Request {
        method,
        path,
        headers,
        body,
    }))
}

fn write_response(w: &mut impl Write, r: &Response) -> std::io::Result<()> {
    let reason = match r.status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        411 => "Length Required",
        413 => "Payload Too Large",
        _ => "Error",
    };
    let body = r
        .body
        .as_ref()
        .map(|b| b.to_string().into_bytes())
        .unwrap_or_default();
    let mut head = format!(
        "HTTP/1.1 {} {reason}\r\nContent-Length: {}\r\nConnection: keep-alive\r\n",
        r.status,
        body.len()
    );
    if r.body.is_some() {
        head.push_str("Content-Type: application/json\r\n");
    }
    for (k, v) in &r.extra {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    w.write_all(head.as_bytes())?;
    w.write_all(&body)?;
    w.flush()
}

fn route(req: &Request, shared: &Shared) -> Response {
    if req.path.split('?').next() != Some("/mcp") {
        return Response::empty(404);
    }
    // A browser page, whatever it claims to be.
    if req.header("origin").is_some() {
        return Response::empty(403);
    }
    // DNS rebinding: a hostile name resolving to 127.0.0.1 would arrive
    // with that name here.
    if let Some(host) = req.header("host") {
        let ok = [
            format!("127.0.0.1:{}", shared.port),
            format!("localhost:{}", shared.port),
        ];
        if !ok.iter().any(|h| h.eq_ignore_ascii_case(host)) {
            return Response::empty(403);
        }
    }
    let token_ok = req
        .header("authorization")
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| same_secret(t.trim(), &shared.token));
    if !token_ok {
        return Response::empty(401);
    }

    match req.method.as_str() {
        "POST" => {}
        // No server-initiated stream, and no session to end.
        "GET" | "DELETE" => {
            let mut r = Response::empty(405);
            r.extra.push(("Allow", "POST".into()));
            return r;
        }
        _ => return Response::empty(405),
    }

    let Ok(message) = serde_json::from_slice::<Value>(&req.body) else {
        return Response::json(rpc_error(Value::Null, -32700, "parse error"));
    };
    // A batch is legal in older revisions; answer each.
    if let Some(batch) = message.as_array() {
        let answers: Vec<Value> = batch.iter().filter_map(|m| handle(m, shared)).collect();
        return if answers.is_empty() {
            Response::empty(202)
        } else {
            Response::json(Value::Array(answers))
        };
    }
    match handle(&message, shared) {
        Some(answer) => Response::json(answer),
        // A notification or a response: nothing to say back.
        None => Response::empty(202),
    }
}

// ── JSON-RPC ────────────────────────────────────────────────────────────────

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn rpc_result(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

/// Answers one message, or `None` for a notification.
fn handle(message: &Value, shared: &Shared) -> Option<Value> {
    let id = message.get("id").cloned()?;
    let Some(method) = message["method"].as_str() else {
        // A response to something we never asked.
        return None;
    };
    let params = &message["params"];

    Some(match method {
        "initialize" => {
            let asked = params["protocolVersion"]
                .as_str()
                .unwrap_or(PROTOCOL_VERSION);
            // Dated revisions compare as strings.
            let version = if asked <= PROTOCOL_VERSION {
                asked
            } else {
                PROTOCOL_VERSION
            };
            rpc_result(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": "vavis", "version": env!("CARGO_PKG_VERSION")},
                    "instructions": "Vavis's own tools: the user's computer, files, \
                        apps, media, memory and integrations. Destructive calls ask \
                        the user first and may be refused."
                }),
            )
        }
        "ping" => rpc_result(id, json!({})),
        "tools/list" => rpc_result(id, json!({"tools": mcp_tools(&shared.handler.tools())})),
        "tools/call" => {
            if !shared.active.load(Ordering::SeqCst) {
                return Some(rpc_error(id, -32000, "no turn is running"));
            }
            let Some(name) = params["name"].as_str() else {
                return Some(rpc_error(id, -32602, "missing tool name"));
            };
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let call_id = params["_meta"]["claudecode/toolUseId"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| id.to_string());
            let result = shared.handler.call(&call_id, name, arguments);
            rpc_result(id, call_result(result))
        }
        // `resources` and `prompts` are not offered; say so the standard way.
        _ => rpc_error(id, -32601, "method not found"),
    })
}

/// Registry schemas → MCP tool descriptors.
fn mcp_tools(schemas: &[Value]) -> Vec<Value> {
    schemas
        .iter()
        .filter_map(|s| {
            let f = &s["function"];
            let name = f["name"].as_str()?;
            let mut input = f["parameters"].clone();
            if !input.is_object() {
                input = json!({"type": "object", "properties": {}});
            }
            Some(json!({
                "name": name,
                "description": f["description"].as_str().unwrap_or_default(),
                "inputSchema": input,
            }))
        })
        .collect()
}

fn call_result(r: CallResult) -> Value {
    let mut content = vec![json!({"type": "text", "text": r.text})];
    if let Some(image) = r.image {
        content.push(json!({"type": "image", "data": image, "mimeType": "image/png"}));
    }
    json!({"content": content, "isError": r.is_error})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::sync::Mutex;

    struct Fake {
        calls: Mutex<Vec<(String, Value)>>,
    }

    impl Handler for Fake {
        fn tools(&self) -> Vec<Value> {
            vec![json!({"type": "function", "function": {
                "name": "get_time",
                "description": "Returns the time",
                "parameters": {"type": "object", "properties": {"zone": {"type": "string"}}}
            }})]
        }
        fn call(&self, _id: &str, name: &str, arguments: Value) -> CallResult {
            self.calls.lock().unwrap().push((name.into(), arguments));
            CallResult {
                text: "13:37".into(),
                is_error: false,
                image: None,
            }
        }
    }

    fn start() -> (Bridge, Arc<Fake>) {
        let fake = Arc::new(Fake {
            calls: Mutex::new(Vec::new()),
        });
        let bridge = Bridge::start(fake.clone()).unwrap();
        (bridge, fake)
    }

    /// A raw HTTP exchange, so the tests see exactly what a client would.
    fn post(bridge: &Bridge, headers: &[(&str, String)], body: &Value) -> (u16, Option<Value>) {
        let mut s = TcpStream::connect(bridge.addr).unwrap();
        let body = body.to_string();
        let mut req = format!(
            "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
            bridge.addr.port(),
            body.len()
        );
        for (k, v) in headers {
            req.push_str(&format!("{k}: {v}\r\n"));
        }
        req.push_str("\r\n");
        req.push_str(&body);
        s.write_all(req.as_bytes()).unwrap();
        read_reply(s)
    }

    fn read_reply(s: TcpStream) -> (u16, Option<Value>) {
        let mut r = BufReader::new(s);
        let mut status = String::new();
        r.read_line(&mut status).unwrap();
        let code: u16 = status.split_whitespace().nth(1).unwrap().parse().unwrap();
        let mut length = 0;
        loop {
            let mut h = String::new();
            r.read_line(&mut h).unwrap();
            if h.trim().is_empty() {
                break;
            }
            if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                length = v.trim().parse().unwrap();
            }
        }
        let mut body = vec![0; length];
        r.read_exact(&mut body).unwrap();
        (code, serde_json::from_slice(&body).ok())
    }

    fn auth(b: &Bridge) -> (&'static str, String) {
        ("Authorization", format!("Bearer {}", b.token()))
    }

    #[test]
    fn a_request_without_the_token_is_refused() {
        let (b, _) = start();
        let (code, _) = post(
            &b,
            &[],
            &json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
        );
        assert_eq!(code, 401);
        let wrong = ("Authorization", "Bearer nope".to_string());
        let (code, _) = post(
            &b,
            &[wrong],
            &json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
        );
        assert_eq!(code, 401);
    }

    #[test]
    fn a_browser_is_refused_even_with_the_token() {
        let (b, _) = start();
        let origin = ("Origin", "https://evil.example".to_string());
        let (code, _) = post(
            &b,
            &[auth(&b), origin],
            &json!({"jsonrpc":"2.0","id":1,"method":"ping"}),
        );
        assert_eq!(code, 403);
    }

    #[test]
    fn initialize_announces_tools() {
        let (b, _) = start();
        let (code, body) = post(
            &b,
            &[auth(&b)],
            &json!({"jsonrpc":"2.0","id":0,"method":"initialize",
                    "params":{"protocolVersion":"2025-03-26","capabilities":{}}}),
        );
        assert_eq!(code, 200);
        let result = &body.unwrap()["result"];
        assert_eq!(result["protocolVersion"], "2025-03-26");
        assert!(result["capabilities"]["tools"].is_object());
    }

    #[test]
    fn a_newer_protocol_request_gets_ours() {
        let (b, _) = start();
        let (_, body) = post(
            &b,
            &[auth(&b)],
            &json!({"jsonrpc":"2.0","id":0,"method":"initialize",
                    "params":{"protocolVersion":"2099-01-01"}}),
        );
        assert_eq!(body.unwrap()["result"]["protocolVersion"], PROTOCOL_VERSION);
    }

    #[test]
    fn a_notification_gets_202_and_no_body() {
        let (b, _) = start();
        let (code, body) = post(
            &b,
            &[auth(&b)],
            &json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        );
        assert_eq!(code, 202);
        assert!(body.is_none());
    }

    #[test]
    fn tools_are_listed_in_mcp_shape() {
        let (b, _) = start();
        let (_, body) = post(
            &b,
            &[auth(&b)],
            &json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
        );
        let tools = &body.unwrap()["result"]["tools"];
        assert_eq!(tools[0]["name"], "get_time");
        assert_eq!(
            tools[0]["inputSchema"]["properties"]["zone"]["type"],
            "string"
        );
    }

    #[test]
    fn a_call_outside_a_turn_is_refused() {
        let (b, fake) = start();
        let (_, body) = post(
            &b,
            &[auth(&b)],
            &json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
                    "params":{"name":"get_time","arguments":{}}}),
        );
        assert!(body.unwrap()["error"].is_object());
        assert!(
            fake.calls.lock().unwrap().is_empty(),
            "the tool must not run"
        );
    }

    #[test]
    fn a_call_during_a_turn_runs_and_answers() {
        let (b, fake) = start();
        let guard = b.open();
        let (_, body) = post(
            &b,
            &[auth(&b)],
            &json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
                    "params":{"name":"get_time","arguments":{"zone":"Istanbul"}}}),
        );
        let result = &body.unwrap()["result"];
        assert_eq!(result["content"][0]["text"], "13:37");
        assert_eq!(result["isError"], false);
        assert_eq!(fake.calls.lock().unwrap()[0].1["zone"], "Istanbul");

        drop(guard);
        let (_, body) = post(
            &b,
            &[auth(&b)],
            &json!({"jsonrpc":"2.0","id":3,"method":"tools/call",
                    "params":{"name":"get_time","arguments":{}}}),
        );
        assert!(
            body.unwrap()["error"].is_object(),
            "closed again after the turn"
        );
    }

    #[test]
    fn an_unknown_method_is_a_standard_error() {
        // Claude Code probes with `server/discover` before initialising.
        let (b, _) = start();
        let (_, body) = post(
            &b,
            &[auth(&b)],
            &json!({"jsonrpc":"2.0","id":"server-discover-probe-1","method":"server/discover"}),
        );
        assert_eq!(body.unwrap()["error"]["code"], -32601);
    }

    #[test]
    fn get_says_there_is_no_stream() {
        let (b, _) = start();
        let mut s = TcpStream::connect(b.addr).unwrap();
        write!(
            s,
            "GET /mcp HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\n\r\n",
            b.addr.port(),
            b.token()
        )
        .unwrap();
        assert_eq!(read_reply(s).0, 405);
    }

    #[test]
    fn a_rebound_host_name_is_refused() {
        let (b, _) = start();
        let mut s = TcpStream::connect(b.addr).unwrap();
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        write!(
            s,
            "POST /mcp HTTP/1.1\r\nHost: attacker.example:{}\r\nAuthorization: Bearer {}\r\nContent-Length: {}\r\n\r\n{body}",
            b.addr.port(),
            b.token(),
            body.len()
        )
        .unwrap();
        assert_eq!(read_reply(s).0, 403);
    }

    #[test]
    fn one_connection_serves_several_requests() {
        let (b, _) = start();
        let s = TcpStream::connect(b.addr).unwrap();
        let mut w = s.try_clone().unwrap();
        let mut r = BufReader::new(s);
        for id in 0..3 {
            let body = format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"ping"}}"#);
            write!(
                w,
                "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\nContent-Length: {}\r\n\r\n{body}",
                b.addr.port(),
                b.token(),
                body.len()
            )
            .unwrap();
            let mut status = String::new();
            r.read_line(&mut status).unwrap();
            assert!(status.contains("200"), "{status}");
            let mut length = 0;
            loop {
                let mut h = String::new();
                r.read_line(&mut h).unwrap();
                if h.trim().is_empty() {
                    break;
                }
                if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap();
                }
            }
            let mut buf = vec![0; length];
            r.read_exact(&mut buf).unwrap();
        }
    }

    #[test]
    fn tokens_are_long_and_unique() {
        let a = new_token();
        assert_eq!(a.len(), 64);
        assert_ne!(a, new_token());
    }

    #[test]
    fn secrets_compare_exactly() {
        assert!(same_secret("abc", "abc"));
        assert!(!same_secret("abc", "abd"));
        assert!(!same_secret("abc", "abcd"));
    }

    #[test]
    fn a_screenshot_rides_along_as_an_image() {
        let v = call_result(CallResult {
            text: "ok".into(),
            is_error: false,
            image: Some("iVBOR".into()),
        });
        assert_eq!(v["content"][1]["type"], "image");
        assert_eq!(v["content"][1]["mimeType"], "image/png");
    }
}
