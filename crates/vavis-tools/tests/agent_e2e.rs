//! Ajan döngüsünün uçtan uca testi.
//!
//! Birim testler parçaları doğruluyor; bu test **tam akışı** doğruluyor:
//! mesaj → tool seçimi → model tool ister → izin kapısı → çalıştırma →
//! sonuç modele döner.
//!
//! Ağ yok, LLM yok — modelin isteyeceği tool çağrıları elle üretiliyor.
//! Böylece mantık, sağlayıcı davranışından bağımsız doğrulanıyor.

use vavis_brain::{FunctionCall, ToolCall};
use vavis_tools::{
    default_registry, selection, Agent, AgentHost, Approval, ApprovalReason, ToolOutcome,
};

/// Kararı önceden belirlenmiş sahte kullanıcı.
struct ScriptedHost {
    decision: Approval,
    asked: Vec<String>,
    started: Vec<String>,
    finished: Vec<(String, bool)>,
}

impl ScriptedHost {
    fn new(decision: Approval) -> Self {
        Self {
            decision,
            asked: Vec::new(),
            started: Vec::new(),
            finished: Vec::new(),
        }
    }
}

impl AgentHost for ScriptedHost {
    fn ask_approval(&mut self, tool: &str, _args: &str, _r: ApprovalReason) -> Approval {
        self.asked.push(tool.to_string());
        self.decision
    }
    fn on_tool_start(&mut self, tool: &str, _args: &str) {
        self.started.push(tool.to_string());
    }
    fn on_tool_result(&mut self, tool: &str, outcome: &ToolOutcome) {
        self.finished.push((tool.to_string(), outcome.ok));
    }
}

fn call(name: &str, args: &str) -> ToolCall {
    ToolCall {
        provider_state: None,
        id: format!("call_{name}"),
        kind: "function".into(),
        function: FunctionCall {
            name: name.into(),
            arguments: args.into(),
        },
    }
}

fn agent() -> Agent {
    Agent::new(default_registry())
}

#[test]
fn full_flow_from_message_to_tool_result() {
    let mut agent = agent();
    let mut host = ScriptedHost::new(Approval::Allow);

    // 1) Kullanıcı mesajı → tool seçimi
    let message = "cpu kullanımı ne durumda";
    agent.start_run();
    let offered = agent.tools_for(message, selection::DEFAULT_TOOL_BUDGET);

    assert!(!offered.is_empty(), "sistem sorusuna tool sunulmalı");
    assert!(offered.len() <= selection::DEFAULT_TOOL_BUDGET);

    // 2) Model get_system_status'nu çağırmak istiyor
    let results = agent.execute_calls(&[call("get_system_status", "{}")], &mut host);

    // 3) Sonuç modele dönmeli
    assert_eq!(results.len(), 1);
    assert!(
        !results[0].content.starts_with("HATA"),
        "{}",
        results[0].content
    );
    assert!(
        results[0].content.contains("CPU"),
        "gerçek telemetri dönmeli: {}",
        results[0].content
    );

    assert_eq!(host.started, vec!["get_system_status"]);
    assert_eq!(host.finished, vec![("get_system_status".to_string(), true)]);
    assert!(host.asked.is_empty(), "güvenli tool onay sormamalı");
}

#[test]
fn destructive_tool_blocked_when_user_denies() {
    let mut agent = agent();
    let mut host = ScriptedHost::new(Approval::Deny);

    let results = agent.execute_calls(&[call("forget", r#"{"number":"1"}"#)], &mut host);

    assert_eq!(host.asked, vec!["forget"], "onay sorulmalıydı");
    assert!(host.started.is_empty(), "reddedilen tool ÇALIŞMAMALI");
    assert!(results[0].content.contains("reddetti"));
}

#[test]
fn multiple_tools_in_one_turn_all_execute() {
    let mut agent = agent();
    let mut host = ScriptedHost::new(Approval::Allow);

    let results = agent.execute_calls(
        &[
            call("get_current_time", "{}"),
            call("calculate", r#"{"expression":"6*7"}"#),
        ],
        &mut host,
    );

    assert_eq!(results.len(), 2);
    assert_eq!(host.started.len(), 2);
    assert!(
        results[1].content.contains("42"),
        "hesap sonucu: {}",
        results[1].content
    );
}

#[test]
fn tool_error_is_reported_to_model_not_swallowed() {
    let mut agent = agent();
    let mut host = ScriptedHost::new(Approval::Allow);

    // Var olmayan dosya → tool hata döner, ama akış devam etmeli.
    let results = agent.execute_calls(
        &[call(
            "read_file",
            r#"{"path":"C:/kesinlikle/yok/dosya.txt"}"#,
        )],
        &mut host,
    );

    assert!(
        results[0].content.starts_with("HATA"),
        "{}",
        results[0].content
    );
    assert_eq!(host.finished, vec![("read_file".to_string(), false)]);
}

#[test]
fn destructive_budget_forces_reapproval_within_one_run() {
    let mut agent = agent();

    // "Hep izin ver" seçen kullanıcı.
    struct AlwaysAllow {
        count: usize,
    }
    impl AgentHost for AlwaysAllow {
        fn ask_approval(&mut self, _t: &str, _a: &str, _r: ApprovalReason) -> Approval {
            self.count += 1;
            Approval::AllowAlways
        }
    }
    let mut host = AlwaysAllow { count: 0 };

    agent.start_run();
    // Bütçe 3; farklı argümanlarla 5 yıkıcı çağrı.
    for i in 1..=5 {
        agent.execute_calls(
            &[call("forget", &format!(r#"{{"number":"{i}"}}"#))],
            &mut host,
        );
    }

    assert!(
        host.count >= 2,
        "bütçe dolunca kalıcı izne rağmen tekrar sorulmalı (sorulma: {})",
        host.count
    );
}

#[test]
fn conversational_message_offers_no_tools_at_all() {
    let agent = agent();
    for msg in ["merhaba", "teşekkürler", "bana bir şiir yaz", "nasılsın"] {
        assert!(
            agent
                .tools_for(msg, selection::DEFAULT_TOOL_BUDGET)
                .is_empty(),
            "'{msg}' için tool sunulmamalı"
        );
    }
}

#[test]
fn memory_round_trip_through_the_agent() {
    // Hafıza depoya bağlı değilse tool hata döner — bu da geçerli davranış.
    // Burada asıl doğrulanan: çağrı akışının çökmeden tamamlanması.
    let mut agent = agent();
    let mut host = ScriptedHost::new(Approval::Allow);

    let results = agent.execute_calls(&[call("remember", r#"{"fact":"test bilgisi"}"#)], &mut host);

    assert_eq!(results.len(), 1, "her çağrı bir sonuç üretmeli");
    assert_eq!(host.started.len(), 1);
}

#[test]
fn unknown_tool_does_not_abort_remaining_calls() {
    let mut agent = agent();
    let mut host = ScriptedHost::new(Approval::Allow);

    let results = agent.execute_calls(
        &[call("uydurma_tool", "{}"), call("get_current_time", "{}")],
        &mut host,
    );

    assert_eq!(
        results.len(),
        2,
        "ilk çağrı hatalı olsa da ikincisi çalışmalı"
    );
    assert!(results[0].content.contains("yok"));
    assert!(!results[1].content.starts_with("HATA"));
}

#[test]
fn every_registered_tool_has_a_usable_schema() {
    let registry = default_registry();
    assert!(registry.len() >= 10, "yeterli tool kayıtlı olmalı");

    for tool in registry.iter() {
        let schema = tool.schema();
        let f = &schema["function"];

        assert!(
            f["name"].as_str().is_some_and(|n| !n.is_empty()),
            "adsız tool var"
        );
        assert!(
            f["description"].as_str().is_some_and(|d| d.len() > 10),
            "{} açıklaması yetersiz — model ne zaman çağıracağını bilemez",
            tool.name()
        );

        // Zorunlu parametreler özelliklerde tanımlı olmalı.
        let props = f["parameters"]["properties"].as_object();
        if let Some(required) = f["parameters"]["required"].as_array() {
            for r in required {
                let name = r.as_str().unwrap_or_default();
                assert!(
                    props.is_some_and(|p| p.contains_key(name)),
                    "{}: zorunlu '{name}' tanımsız",
                    tool.name()
                );
            }
        }
    }
}

#[test]
fn tool_names_are_unique_and_lowercase() {
    let registry = default_registry();
    let mut names: Vec<&str> = registry.iter().map(|t| t.name()).collect();
    let before = names.len();

    names.sort_unstable();
    names.dedup();
    assert_eq!(before, names.len(), "tool adı çakışması var");

    for name in names {
        assert_eq!(
            name,
            name.to_lowercase(),
            "tool adları küçük harf olmalı: {name}"
        );
        assert!(!name.contains(' '), "tool adında boşluk olmamalı: {name}");
    }
}
