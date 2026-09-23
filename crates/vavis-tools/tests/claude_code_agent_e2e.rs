//! The code agent end to end: Claude Code, in a real project, finds a bug
//! from a failing test, fixes it through Vavis's gated ws_* tools, and
//! runs the test again.
//!
//! Needs `claude` installed and logged in, and `python3`; spends some of the
//! subscription, so it only runs when asked:
//! `cargo test -p vavis-tools --test claude_code_agent_e2e -- --ignored --nocapture`

use serde_json::Value;
use std::sync::{Arc, Mutex};
use vavis_brain::claude_code::ToolBridge;
use vavis_brain::{BrainClient, ChatConfig, FunctionCall, Message, Provider, ToolCall};
use vavis_tools::mcp::bridge::{Bridge, CallResult, Handler};
use vavis_tools::{Agent, AgentHost, Approval, ApprovalReason, Domain};

/// Approves everything and records what was asked.
struct Yes(Arc<Mutex<Vec<String>>>);

impl AgentHost for Yes {
    fn ask_approval(&mut self, tool: &str, _args: &str, _r: ApprovalReason) -> Approval {
        self.0.lock().unwrap().push(tool.to_string());
        Approval::Allow
    }
}

struct Real {
    agent: Mutex<Agent>,
    asked: Arc<Mutex<Vec<String>>>,
    ran: Mutex<Vec<String>>,
}

impl Handler for Real {
    fn tools(&self) -> Vec<Value> {
        let agent = self.agent.lock().unwrap();
        let names: Vec<&str> = agent
            .registry
            .in_domain(Domain::Code)
            .map(|t| t.name())
            .collect();
        agent.schemas_for(&names)
    }
    fn call(&self, id: &str, name: &str, arguments: Value) -> CallResult {
        self.ran.lock().unwrap().push(name.to_string());
        let call = ToolCall {
            id: id.into(),
            kind: "function".into(),
            function: FunctionCall {
                name: name.into(),
                arguments: arguments.to_string(),
            },
            provider_state: None,
        };
        let mut host = Yes(self.asked.clone());
        let out = self.agent.lock().unwrap().execute_calls(&[call], &mut host);
        let text = out
            .into_iter()
            .next()
            .map(|m| m.content)
            .unwrap_or_default();
        CallResult {
            is_error: text.starts_with("HATA:"),
            text,
            image: None,
        }
    }
}

#[tokio::test]
#[ignore]
async fn claude_fixes_a_failing_test_through_the_gate() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("calc.py"),
        "def add(a, b):\n    return a - b\n",
    )
    .unwrap();
    std::fs::write(
        project.path().join("test_calc.py"),
        "from calc import add\nassert add(2, 3) == 5, 'add is wrong'\nprint('OK')\n",
    )
    .unwrap();
    vavis_tools::workspace::set_root(Some(project.path().to_path_buf()));

    let asked = Arc::new(Mutex::new(Vec::new()));
    let handler = Arc::new(Real {
        agent: Mutex::new(Agent::new(vavis_tools::default_registry())),
        asked: asked.clone(),
        ran: Mutex::new(Vec::new()),
    });
    handler.agent.lock().unwrap().start_run();
    let bridge = Bridge::start(handler.clone()).unwrap();
    let _open = bridge.open();

    let mut cfg = ChatConfig::new(Provider::ClaudeCode, "sonnet", "");
    cfg.tool_bridge = Some(ToolBridge {
        url: bridge.url(),
        token: bridge.token().to_string(),
    });
    cfg.workdir = Some(project.path().to_path_buf());
    let tools = handler.tools();

    let reply = BrainClient::new()
        .chat_stream_with_tools(
            &cfg,
            vec![
                Message::system(
                    "You work in the open project. Change files only with \
                     mcp__vavis__ws_edit / ws_write, run commands only with \
                     mcp__vavis__ws_run.",
                ),
                Message::user(
                    "Run `python3 test_calc.py`, find why it fails, fix the bug, \
                     and run the test again to confirm. Reply in one sentence.",
                ),
            ],
            &tools,
            |_| {},
        )
        .await
        .unwrap();

    let fixed = std::fs::read_to_string(project.path().join("calc.py")).unwrap();
    let ran = handler.ran.lock().unwrap().clone();
    let asked = asked.lock().unwrap().clone();
    println!(
        "tools run: {ran:?}\napprovals asked: {asked:?}\nreply: {}",
        reply.text
    );

    assert!(fixed.contains("a + b"), "not fixed:\n{fixed}");
    assert!(
        ran.iter().filter(|t| *t == "ws_run").count() >= 2,
        "test not re-run: {ran:?}"
    );
    assert!(
        asked.contains(&"ws_edit".to_string()),
        "the edit went around the gate"
    );
    vavis_tools::workspace::set_root(None);
}
