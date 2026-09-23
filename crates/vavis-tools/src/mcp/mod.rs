//! Model Context Protocol client.
//!
//! Built-in integrations are the ones chosen here; MCP is the door to the
//! ones the user chooses. Supporting it means every server anyone has already
//! written becomes available without a line of code in Vavis.
//!
//! Two things make this different from a built-in:
//!
//! * **Tool names are only known at runtime**, so they are interned into
//!   `&'static str` rather than being literals.
//! * **An MCP server runs arbitrary code** on the user's machine, started by
//!   Vavis. Its tools are therefore treated as destructive by default and go
//!   through the same permission gate; the server's own claim that a tool is
//!   safe is not taken at face value.

pub mod bridge;
pub mod rpc;

use crate::tool::{Domain, Param, Registry, Risk, Tool, ToolOutcome};
use rpc::{HttpTransport, RpcError, StdioTransport};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::sync::{Mutex, OnceLock};

/// Interns a runtime string as `&'static str`.
///
/// MCP tool names arrive from a server but the `Tool` trait needs a static
/// name. Interning bounds the cost to the set of distinct names ever seen, so
/// reconnecting a server does not grow memory each time.
fn intern(text: &str) -> &'static str {
    static POOL: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let pool = POOL.get_or_init(|| Mutex::new(HashSet::new()));
    let mut pool = pool.lock().unwrap_or_else(|e| e.into_inner());

    if let Some(existing) = pool.get(text) {
        return existing;
    }
    let leaked: &'static str = Box::leak(text.to_string().into_boxed_str());
    pool.insert(leaked);
    leaked
}

/// How a server is reached.
#[derive(Debug, Clone, PartialEq)]
pub enum Transport {
    /// Vavis starts the server as a child process. The default, and what
    /// local servers use.
    Stdio {
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
    },
    /// A remote server, reached over HTTP.
    Http {
        url: String,
        header_name: String,
        /// Header value; `{key}` is filled from the stored secret.
        header_value: String,
    },
}

/// One configured server.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerConfig {
    /// Short identifier the user gave it — becomes the tool-selection domain.
    pub id: String,
    pub transport: Transport,
    /// Tool names the user switched off. Everything else is offered.
    pub disabled: Vec<String>,
    pub enabled: bool,
}

/// What a server said about one of its tools.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    /// The server's own JSON Schema for the arguments.
    pub schema: Value,
}

/// A live connection.
enum Connection {
    Stdio(StdioTransport),
    Http(HttpTransport),
}

impl Connection {
    fn request(
        &mut self,
        method: &str,
        params: Value,
        timeout: std::time::Duration,
    ) -> Result<Value, RpcError> {
        match self {
            Self::Stdio(t) => t.request(method, params, timeout),
            Self::Http(t) => t.request(method, params, timeout),
        }
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<(), RpcError> {
        match self {
            Self::Stdio(t) => t.notify(method, params),
            // HTTP has no separate notification channel worth maintaining;
            // servers accept the initialized notice as a plain request.
            Self::Http(t) => t
                .request(method, params, rpc::handshake_timeout())
                .map(|_| ()),
        }
    }
}

/// A connected server and the tools it offers.
pub struct Server {
    pub id: String,
    pub tools: Vec<ToolInfo>,
    connection: Mutex<Connection>,
}

impl Server {
    /// Connects and performs the MCP handshake, then lists the tools.
    pub fn connect(config: &ServerConfig, secret: &str) -> Result<Self, String> {
        let connection = match &config.transport {
            Transport::Stdio { command, args, env } => Connection::Stdio(
                StdioTransport::spawn(command, args, env).map_err(|e| e.to_string())?,
            ),
            Transport::Http {
                url,
                header_name,
                header_value,
            } => Connection::Http(HttpTransport {
                url: url.clone(),
                header: (!header_name.trim().is_empty()).then(|| {
                    (
                        header_name.trim().to_string(),
                        header_value.replace("{key}", secret),
                    )
                }),
            }),
        };

        let mut connection = connection;

        // Handshake: announce the protocol, then say we are ready.
        connection
            .request(
                "initialize",
                serde_json::json!({
                    "protocolVersion": rpc::PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": { "name": "Vavis", "version": env!("CARGO_PKG_VERSION") },
                }),
                rpc::handshake_timeout(),
            )
            .map_err(|e| format!("el sıkışma başarısız: {e}"))?;

        // Failing to send this is not fatal — several servers ignore it.
        let _ = connection.notify("notifications/initialized", serde_json::json!({}));

        let listed = connection
            .request(
                "tools/list",
                serde_json::json!({}),
                rpc::handshake_timeout(),
            )
            .map_err(|e| format!("tool listesi alınamadı: {e}"))?;

        let tools = parse_tool_list(&listed);

        Ok(Self {
            id: config.id.clone(),
            tools,
            connection: Mutex::new(connection),
        })
    }

    /// Runs one tool on the server.
    pub fn call(&self, tool: &str, args: &Value) -> Result<String, String> {
        let mut connection = self.connection.lock().unwrap_or_else(|e| e.into_inner());
        let result = connection
            .request(
                "tools/call",
                serde_json::json!({ "name": tool, "arguments": args }),
                rpc::response_timeout(),
            )
            .map_err(|e| e.to_string())?;

        // A server can report a failure inside a successful response.
        if result["isError"].as_bool() == Some(true) {
            return Err(flatten_content(&result));
        }
        Ok(flatten_content(&result))
    }
}

/// Reads the `tools/list` payload.
pub fn parse_tool_list(result: &Value) -> Vec<ToolInfo> {
    result["tools"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|t| {
                    Some(ToolInfo {
                        name: t["name"].as_str()?.to_string(),
                        description: t["description"].as_str().unwrap_or("").to_string(),
                        schema: t["inputSchema"].clone(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Turns MCP's content blocks into plain text for the model.
///
/// Images and embedded resources are named rather than inlined: dropping a
/// base64 image into the transcript would blow the context budget without
/// the model having asked to look at anything.
pub fn flatten_content(result: &Value) -> String {
    let Some(blocks) = result["content"].as_array() else {
        // Some servers answer with a bare value.
        return match &result["content"] {
            Value::Null => String::new(),
            other => other.to_string(),
        };
    };

    let mut parts = Vec::new();
    for block in blocks {
        match block["type"].as_str() {
            Some("text") => {
                if let Some(text) = block["text"].as_str() {
                    parts.push(text.to_string());
                }
            }
            Some("image") => parts.push("(görsel döndü)".to_string()),
            Some("resource") => {
                let uri = block["resource"]["uri"].as_str().unwrap_or("kaynak");
                parts.push(format!("(kaynak: {uri})"));
            }
            _ => {}
        }
    }
    parts.join("\n")
}

// ---------------------------------------------------------------------------
// The tool wrapper
// ---------------------------------------------------------------------------

/// One MCP tool, presented to the model like any other.
pub struct McpTool {
    name: &'static str,
    description: String,
    domain: Domain,
    server: std::sync::Arc<Server>,
    /// The name the server knows it by, without the prefix.
    remote_name: String,
    schema: Value,
}

impl Tool for McpTool {
    fn name(&self) -> &'static str {
        self.name
    }

    fn description(&self) -> &'static str {
        // The trait wants a static description; leak it the same way as the
        // name so the two stay consistent.
        intern(&self.description)
    }

    fn domain(&self) -> Domain {
        self.domain
    }

    fn risk(&self) -> Risk {
        // An MCP server is arbitrary code running on the user's machine. Its
        // own claim that a tool is harmless is not something to rely on, so
        // every call goes through the permission gate.
        Risk::Destructive
    }

    fn params(&self) -> Vec<Param> {
        // Unused: `schema` below is overridden with the server's own.
        Vec::new()
    }

    fn schema(&self) -> Value {
        // The server already published a JSON Schema; passing it through is
        // both more accurate and less work than re-describing it.
        let parameters = if self.schema.is_object() {
            self.schema.clone()
        } else {
            serde_json::json!({ "type": "object", "properties": {} })
        };

        serde_json::json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": parameters,
            }
        })
    }

    fn run(&self, args: &Value) -> ToolOutcome {
        match self.server.call(&self.remote_name, args) {
            Ok(text) if text.trim().is_empty() => ToolOutcome::ok("(boş yanıt)"),
            Ok(text) => ToolOutcome::ok(text),
            Err(e) => ToolOutcome::err(e),
        }
    }
}

/// Prefixes a server's tool so two servers can both offer `search`.
pub fn qualified_name(server_id: &str, tool: &str) -> String {
    format!("{server_id}_{tool}")
}

// ---------------------------------------------------------------------------
// Connected servers
// ---------------------------------------------------------------------------

/// Everything currently connected, by server id.
type Servers = BTreeMap<String, std::sync::Arc<Server>>;

fn servers() -> &'static Mutex<Servers> {
    static SERVERS: OnceLock<Mutex<Servers>> = OnceLock::new();
    SERVERS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// What the settings panel shows about a server.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerStatus {
    pub id: String,
    pub connected: bool,
    /// Tool names the server published.
    pub tools: Vec<String>,
    pub error: Option<String>,
}

/// Connects one server and registers its tools.
///
/// Returns what happened so the settings panel can show it. A server that
/// fails is reported, not retried — the others carry on.
pub fn connect(registry: &mut Registry, config: &ServerConfig, secret: &str) -> ServerStatus {
    if !config.enabled {
        return ServerStatus {
            id: config.id.clone(),
            connected: false,
            tools: Vec::new(),
            error: Some("kapalı".into()),
        };
    }

    let server = match Server::connect(config, secret) {
        Ok(s) => std::sync::Arc::new(s),
        Err(error) => {
            tracing::warn!(server = %config.id, %error, "mcp server did not connect");
            return ServerStatus {
                id: config.id.clone(),
                connected: false,
                tools: Vec::new(),
                error: Some(error),
            };
        }
    };

    let domain = Domain::Mcp(intern(&config.id));
    let mut published = Vec::new();

    for info in &server.tools {
        published.push(info.name.clone());
        if config.disabled.iter().any(|d| d == &info.name) {
            continue;
        }

        let qualified = qualified_name(&config.id, &info.name);
        let tool = McpTool {
            name: intern(&qualified),
            description: if info.description.trim().is_empty() {
                format!("{} ({})", info.name, config.id)
            } else {
                info.description.clone()
            },
            domain,
            server: server.clone(),
            remote_name: info.name.clone(),
            schema: info.schema.clone(),
        };

        // A name clash would panic the registry; skip instead. Two servers
        // with the same id is the only way this happens, and the settings
        // panel prevents it.
        if registry.get(tool.name()).is_none() {
            registry.register(Box::new(tool));
        }
    }

    servers()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(config.id.clone(), server);

    ServerStatus {
        id: config.id.clone(),
        connected: true,
        tools: published,
        error: None,
    }
}

/// Drops every connection. Child processes are killed on drop.
pub fn disconnect_all() {
    servers().lock().unwrap_or_else(|e| e.into_inner()).clear();
}

/// Ids of the servers currently connected.
pub fn connected_ids() -> Vec<String> {
    servers()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .keys()
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interning_returns_the_same_pointer_for_the_same_name() {
        let a = intern("some_tool");
        let b = intern("some_tool");
        assert_eq!(a, b);
        assert!(
            std::ptr::eq(a.as_ptr(), b.as_ptr()),
            "reconnecting must not leak a second copy"
        );
    }

    #[test]
    fn tool_names_are_prefixed_by_server() {
        // Two servers may both publish "search"; without this they collide.
        assert_eq!(qualified_name("github", "search"), "github_search");
        assert_ne!(
            qualified_name("github", "search"),
            qualified_name("notion", "search")
        );
    }

    #[test]
    fn a_tool_list_is_parsed() {
        let result = serde_json::json!({
            "tools": [
                {
                    "name": "search",
                    "description": "Search things",
                    "inputSchema": {"type": "object", "properties": {"q": {"type": "string"}}}
                },
                { "name": "bare" }
            ]
        });

        let tools = parse_tool_list(&result);
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "search");
        assert_eq!(tools[0].description, "Search things");
        assert_eq!(
            tools[1].description, "",
            "a missing description is not fatal"
        );
    }

    #[test]
    fn an_empty_tool_list_is_not_an_error() {
        assert!(parse_tool_list(&serde_json::json!({})).is_empty());
        assert!(parse_tool_list(&serde_json::json!({"tools": []})).is_empty());
    }

    #[test]
    fn text_content_blocks_are_joined() {
        let result = serde_json::json!({
            "content": [
                {"type": "text", "text": "first"},
                {"type": "text", "text": "second"}
            ]
        });
        assert_eq!(flatten_content(&result), "first\nsecond");
    }

    #[test]
    fn images_are_named_not_inlined() {
        // Inlining base64 would silently consume the whole context budget.
        let result = serde_json::json!({
            "content": [
                {"type": "text", "text": "here"},
                {"type": "image", "data": "AAAA...", "mimeType": "image/png"}
            ]
        });
        let text = flatten_content(&result);
        assert!(text.contains("görsel"), "{text}");
        assert!(
            !text.contains("AAAA"),
            "image data must not reach the model"
        );
    }

    #[test]
    fn resources_are_reported_by_uri() {
        let result = serde_json::json!({
            "content": [{"type": "resource", "resource": {"uri": "file:///x.txt"}}]
        });
        assert!(flatten_content(&result).contains("file:///x.txt"));
    }

    #[test]
    fn a_response_with_no_content_is_empty_not_a_panic() {
        assert_eq!(flatten_content(&serde_json::json!({})), "");
    }

    #[test]
    fn a_disabled_server_is_not_connected() {
        let mut registry = Registry::new();
        let config = ServerConfig {
            id: "off".into(),
            transport: Transport::Stdio {
                command: "should-never-run".into(),
                args: vec![],
                env: vec![],
            },
            disabled: vec![],
            enabled: false,
        };

        let status = connect(&mut registry, &config, "");
        assert!(!status.connected);
        assert!(registry.is_empty(), "nothing should have been registered");
    }

    #[test]
    fn a_server_that_fails_to_start_is_reported_not_fatal() {
        let mut registry = Registry::new();
        let config = ServerConfig {
            id: "broken".into(),
            transport: Transport::Stdio {
                command: "definitely-not-a-real-command-xyz".into(),
                args: vec![],
                env: vec![],
            },
            disabled: vec![],
            enabled: true,
        };

        let status = connect(&mut registry, &config, "");
        assert!(!status.connected);
        assert!(status.error.is_some(), "the reason must be reported");
        assert!(registry.is_empty());
    }
}
