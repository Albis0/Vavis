//! Does the model actually *call* the right tool?
//!
//! `selection_eval` only proves the right tool is on the list. This runs real
//! sentences through a real model (Claude Code, over the MCP bridge) with the
//! tools Vavis would offer for each, and records what it called. Tools are
//! stubs: nothing is closed, written or searched.
//!
//! Needs `claude` installed and logged in, spends subscription tokens:
//! `cargo test -p vavis-tools --test tool_calling_eval -- --ignored --nocapture`
//! Pick the model with `EVAL_MODEL` (default `haiku`); `EVAL_ONLY=sesi` runs
//! just the sentences containing that text.

use serde_json::Value;
use std::sync::{Arc, Mutex};
use vavis_brain::claude_code::ToolBridge;
use vavis_brain::{BrainClient, ChatConfig, Message, Provider};
use vavis_tools::mcp::bridge::{Bridge, CallResult, Handler};
use vavis_tools::{default_registry, selection, Agent};

/// What a sentence should lead to.
enum Want {
    /// The model has to call one of these.
    Any(&'static [&'static str]),
    /// Web question: the bridge's search tools, or Claude Code's own built-in
    /// WebSearch (invisible to the bridge, but its answer carries sources).
    Web,
    /// Plain conversation: calling anything is a mistake.
    Nothing,
}
use Want::{Any, Nothing, Web};

const CASES: &[(&str, Want)] = &[
    // Direct: one obvious tool.
    ("discord'u aç", Any(&["launch_app"])),
    ("excel'i kapat", Any(&["close_app"])),
    ("notepad başlat", Any(&["launch_app"])),
    ("sesi 30 yap", Any(&["set_volume"])),
    ("sesi kapat", Any(&["set_volume"])),
    ("parlaklığı azalt", Any(&["set_brightness"])),
    ("pil yüzde kaç", Any(&["get_battery"])),
    ("ram ne kadar dolu", Any(&["get_system_status"])),
    ("hangi programlar çalışıyor", Any(&["list_processes"])),
    ("ekran görüntüsü al", Any(&["take_screenshot"])),
    ("panodaki metni oku", Any(&["read_clipboard"])),
    ("müziği duraklat", Any(&["media_control"])),
    ("spotify'da ne çalıyor", Any(&["get_now_playing"])),
    ("masaüstündeki dosyaları listele", Any(&["list_directory"])),
    ("masaüstüne deneme.txt diye bir dosya yaz, içinde merhaba olsun", Any(&["write_file"])),
    ("beni hatırla: kahveyi sade içerim", Any(&["remember"])),
    ("hakkımda ne biliyorsun", Any(&["search_memory"])),
    ("10 dakika sonra bana su içmeyi hatırlat", Any(&["create_automation"])),
    ("kurulu otomasyonlarım neler", Any(&["list_automations"])),
    ("bu linkin güvenli olduğundan emin misin: http://example.com", Any(&["virus_scan_url"])),
    // Direct, but phrased the way people really type.
    ("dolar kaç tl", Web),
    ("yarın istanbul'da yağmur var mı", Web),
    ("galatasaray maçı kaç kaç bitti", Web),
    ("ip adresim ne", Any(&["run_command", "get_system_status"])),
    ("ekranı kilitle", Any(&["run_command"])),
    ("wifi'yi kapat", Any(&["run_command"])),
    ("notlarıma bak, alışveriş listesi var mı", Any(&["search_notes", "read_note"])),
    ("kaç gb boş yerim var", Any(&["get_system_status", "run_command"])),
    ("open spotify and play something", Any(&["launch_app", "media_control"])),
    ("şarkıyı değiştir", Any(&["media_control"])),
    // Broad: several steps, the first call is what counts.
    ("bilgisayar yavaş, neden olabilir", Any(&["get_system_status", "list_processes"])),
    ("masaüstümü topla", Any(&["list_directory", "run_command"])),
    // Negatives: no tool at all.
    ("merhaba nasılsın", Nothing),
    ("python'da liste nasıl tersine çevrilir", Nothing),
    ("bana kısa bir şiir yaz", Nothing),
    ("rust ile go arasındaki fark ne", Nothing),
    ("teşekkürler", Nothing),
    ("hangi araçları kullanabilirsin", Nothing),
];

struct Recorder {
    tools: Mutex<Vec<Value>>,
    calls: Mutex<Vec<(String, Value)>>,
}

impl Handler for Recorder {
    fn tools(&self) -> Vec<Value> {
        self.tools.lock().unwrap().clone()
    }
    fn call(&self, _id: &str, name: &str, arguments: Value) -> CallResult {
        self.calls.lock().unwrap().push((name.to_string(), arguments));
        CallResult {
            text: "Tamam, yapıldı. (test: gerçek bir işlem yapılmadı)".into(),
            is_error: false,
            image: None,
        }
    }
}

const SYSTEM: &str = "Sen Vavis'sin, Windows için kişisel bir asistan. Kısa ve Türkçe cevap ver. \
Bilgisayarda bir şey yapman ya da bir bilgiyi öğrenmen gerekiyorsa sana verilen araçları kullan; \
sohbet ya da genel bilgi sorularında araç çağırma.";

#[tokio::test]
#[ignore = "gerçek model çağırır — cargo test -- --ignored"]
async fn the_model_calls_the_right_tool_for_real_sentences() {
    let model = std::env::var("EVAL_MODEL").unwrap_or_else(|_| "haiku".into());
    let agent = Agent::new(default_registry());
    let rec = Arc::new(Recorder {
        tools: Mutex::new(Vec::new()),
        calls: Mutex::new(Vec::new()),
    });
    let bridge = Bridge::start(rec.clone()).unwrap();
    let _open = bridge.open();

    let mut cfg = ChatConfig::new(Provider::ClaudeCode, &model, "");
    cfg.tool_bridge = Some(ToolBridge {
        url: bridge.url(),
        token: bridge.token().to_string(),
    });

    let (mut direct_hit, mut direct_total, mut stray, mut neg_total) = (0, 0, 0, 0);
    let mut rows = Vec::new();

    let only = std::env::var("EVAL_ONLY").ok();
    for (msg, want) in CASES {
        if only.as_deref().is_some_and(|o| !msg.contains(o)) {
            continue;
        }
        let names = selection::select_broad(&agent.registry, msg, &[], selection::BROAD_BUDGET);
        let offered = agent.schemas_for(&names);
        *rec.tools.lock().unwrap() = offered.clone();
        rec.calls.lock().unwrap().clear();

        let history = vec![Message::system(SYSTEM), Message::user(*msg)];
        let outcome = BrainClient::new()
            .chat_stream_with_tools(&cfg, history, &offered, |_| {})
            .await;
        let called: Vec<String> = rec
            .calls
            .lock()
            .unwrap()
            .iter()
            .map(|(n, _)| n.rsplit("__").next().unwrap_or(n).to_string())
            .collect();

        let (reply, err) = match outcome {
            Ok(r) => (r.text, String::new()),
            Err(e) => (String::new(), format!(" ERR {e}")),
        };

        let (ok, expect) = match want {
            Any(tools) => {
                direct_total += 1;
                let ok = called.iter().any(|c| tools.contains(&c.as_str()));
                direct_hit += ok as usize;
                (ok, tools.join("|"))
            }
            Web => {
                direct_total += 1;
                let ok = called.iter().any(|c| c == "web_search" || c == "fetch_url")
                    || reply.contains("http");
                direct_hit += ok as usize;
                (ok, "web".to_string())
            }
            Nothing => {
                neg_total += 1;
                let ok = called.is_empty();
                stray += (!ok) as usize;
                (ok, "-".to_string())
            }
        };
        let offered_ok = match want {
            Any(tools) => tools.iter().any(|t| names.contains(t)),
            Web | Nothing => true,
        };
        let said = if ok {
            String::new()
        } else {
            format!("\n    model dedi ki: {}", reply.trim().replace('\n', " "))
        };
        rows.push(format!(
            "{} {:<58} bekl={:<28} çağrı={:?}{}{}",
            if ok { "✓" } else { "✗" },
            msg,
            expect,
            called,
            if offered_ok { "" } else { " [araç sunulmadı!]" },
            err
        ) + &said);
    }

    println!("\n{}\n", rows.join("\n"));
    println!(
        "model={model}  doğrudan: {direct_hit}/{direct_total}  gereksiz çağrı: {stray}/{neg_total}"
    );
}
