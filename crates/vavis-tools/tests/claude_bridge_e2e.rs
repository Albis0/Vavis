//! Claude Code calling a Vavis tool through the MCP bridge, for real.
//!
//! Needs `claude` installed and logged in, and spends a few tokens of the
//! subscription, so it only runs when asked:
//! `cargo test -p vavis-tools --test claude_bridge_e2e -- --ignored`

use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use vavis_brain::claude_code::ToolBridge;
use vavis_brain::{BrainClient, ChatConfig, Message, Provider};
use vavis_tools::mcp::bridge::{Bridge, CallResult, Handler};

struct Clock {
    calls: Mutex<Vec<Value>>,
}

impl Handler for Clock {
    fn tools(&self) -> Vec<Value> {
        vec![json!({"type": "function", "function": {
            "name": "get_time",
            "description": "Returns the current time in a city",
            "parameters": {"type": "object",
                           "properties": {"city": {"type": "string"}},
                           "required": ["city"]}
        }})]
    }
    fn call(&self, _id: &str, _name: &str, arguments: Value) -> CallResult {
        self.calls.lock().unwrap().push(arguments);
        CallResult {
            text: "It is exactly 13:37 there.".into(),
            is_error: false,
            image: None,
        }
    }
}

#[tokio::test]
#[ignore]
async fn claude_uses_a_vavis_tool_and_reports_its_result() {
    let clock = Arc::new(Clock {
        calls: Mutex::new(Vec::new()),
    });
    let bridge = Bridge::start(clock.clone()).unwrap();
    let _open = bridge.open();

    let mut cfg = ChatConfig::new(Provider::ClaudeCode, "haiku", "");
    cfg.tool_bridge = Some(ToolBridge {
        url: bridge.url(),
        token: bridge.token().to_string(),
    });
    // Any non-empty list attaches the bridge; the schemas themselves come
    // from the bridge's own tools/list.
    let tools = clock.tools();

    let reply = BrainClient::new()
        .chat_stream_with_tools(
            &cfg,
            vec![Message::user(
                "What time is it in Istanbul? Use the get_time tool, then answer in one sentence.",
            )],
            &tools,
            |_| {},
        )
        .await
        .unwrap();

    let calls = clock.calls.lock().unwrap();
    assert_eq!(calls.len(), 1, "the tool should run exactly once");
    assert!(calls[0]["city"].as_str().unwrap().contains("Istanbul"));
    assert!(reply.text.contains("13:37"), "{}", reply.text);
}
