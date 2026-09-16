//! Anthropic (Claude) sağlayıcısı.
//!
//! Neden ayrı modül: Anthropic'in Messages API'si OpenAI-uyumlu **değil**.
//! Farklar:
//!
//! | | OpenAI-uyumlu | Anthropic |
//! |---|---|---|
//! | Kimlik | `Authorization: Bearer` | `x-api-key` + `anthropic-version` |
//! | Sistem istemi | `messages[0]` (role=system) | **top-level `system` alanı** |
//! | Tool şeması | `{type, function:{name, parameters}}` | `{name, description, input_schema}` |
//! | Tool çağrısı | `tool_calls` dizisi | içerik bloğu (`type: "tool_use"`) |
//! | Tool sonucu | `role: "tool"` mesajı | user mesajı içinde `tool_result` bloğu |
//! | Akış | `data: {choices:[{delta}]}` | isimli SSE olayları |
//! | `max_tokens` | isteğe bağlı | **zorunlu** |
//!
//! Bu modül dönüşümü tek yerde yapar; `client.rs` sadece yönlendirir.

use crate::message::{FunctionCall, Message, Role, ToolCall};
use serde::Deserialize;
use serde_json::{json, Value};

/// API sürümü — Anthropic bunu zorunlu tutuyor.
pub const API_VERSION: &str = "2023-06-01";
pub const CHAT_URL: &str = "https://api.anthropic.com/v1/messages";
pub const MODELS_URL: &str = "https://api.anthropic.com/v1/models";

/// Varsayılan model.
pub const DEFAULT_MODEL: &str = "claude-sonnet-5";

/// Sohbet geçmişini Anthropic gövdesine çevirir.
///
/// Dönen: `(system_prompt, messages_dizisi)`. Sistem istemi ayrı çıkar
/// çünkü Anthropic onu mesaj dizisinde değil, top-level alanda bekler.
pub fn build_messages(messages: &[Message]) -> (String, Vec<Value>) {
    let mut system = String::new();
    let mut out: Vec<Value> = Vec::new();

    for m in messages {
        match m.role {
            Role::System => {
                // Birden çok sistem mesajı varsa birleştir.
                if !system.is_empty() {
                    system.push_str("\n\n");
                }
                system.push_str(&m.content);
            }

            Role::Tool => {
                // Tool sonucu, user mesajı içinde bir blok olarak gider.
                let block = json!({
                    "type": "tool_result",
                    "tool_use_id": m.tool_call_id.clone().unwrap_or_default(),
                    "content": m.content,
                });

                // Ardışık tool sonuçları TEK user mesajında birleşmeli —
                // ayrı mesajlara bölmek modeli paralel tool kullanmaktan caydırır.
                match out.last_mut() {
                    Some(last)
                        if last["role"] == "user"
                            && last["content"].is_array()
                            && last["content"][0]["type"] == "tool_result" =>
                    {
                        if let Some(arr) = last["content"].as_array_mut() {
                            arr.push(block);
                        }
                    }
                    _ => out.push(json!({ "role": "user", "content": [block] })),
                }
            }

            Role::Assistant => {
                let mut blocks: Vec<Value> = Vec::new();
                if !m.content.trim().is_empty() {
                    blocks.push(json!({"type": "text", "text": m.content}));
                }
                // Modelin tool isteği de içerik bloğu olarak geri gönderilir.
                if let Some(calls) = &m.tool_calls {
                    for c in calls {
                        let input: Value = serde_json::from_str(&c.function.arguments)
                            .unwrap_or_else(|_| json!({}));
                        blocks.push(json!({
                            "type": "tool_use",
                            "id": c.id,
                            "name": c.function.name,
                            "input": input,
                        }));
                    }
                }
                // Tamamen boş asistan mesajı API tarafından reddedilir.
                if !blocks.is_empty() {
                    out.push(json!({"role": "assistant", "content": blocks}));
                }
            }

            Role::User => {
                // Görüntülü mesaj çok parçalı gider. Anthropic'in şekli
                // OpenAI'dan farklı: `source` nesnesi, `image_url` değil.
                if let Some(image) = &m.image {
                    let mut blocks = vec![json!({
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": "image/png",
                            "data": image,
                        }
                    })];
                    // Metin görüntüden SONRA gelmeli — Anthropic bunu öneriyor.
                    if !m.content.trim().is_empty() {
                        blocks.push(json!({"type": "text", "text": m.content}));
                    }
                    out.push(json!({"role": "user", "content": blocks}));
                } else if !m.content.trim().is_empty() {
                    out.push(json!({"role": "user", "content": m.content}));
                }
            }
        }
    }

    // Anthropic ilk mesajın "user" olmasını ister.
    if out
        .first()
        .map(|m| m["role"] == "assistant")
        .unwrap_or(false)
    {
        out.insert(0, json!({"role": "user", "content": "(devam)"}));
    }

    (system, out)
}

/// OpenAI tool şemasını Anthropic biçimine çevirir.
pub fn convert_tools(tools: &[Value]) -> Vec<Value> {
    tools
        .iter()
        .filter_map(|t| {
            let f = t.get("function")?;
            Some(json!({
                "name": f.get("name")?,
                "description": f.get("description").cloned().unwrap_or(json!("")),
                "input_schema": f.get("parameters").cloned().unwrap_or(json!({
                    "type": "object", "properties": {}
                })),
            }))
        })
        .collect()
}

/// Tam istek gövdesi.
pub fn build_body(
    model: &str,
    messages: &[Message],
    tools: &[Value],
    max_tokens: usize,
    temperature: f32,
) -> Value {
    let (system, msgs) = build_messages(messages);

    let mut body = json!({
        "model": model,
        // Anthropic'te max_tokens ZORUNLU — atlanırsa 400 döner.
        "max_tokens": max_tokens,
        "messages": msgs,
        "stream": true,
    });

    if !system.is_empty() {
        body["system"] = json!(system);
    }
    // Bazı yeni modeller temperature'ı reddediyor; sadece varsayılandan
    // farklıysa gönder.
    if (temperature - 1.0).abs() > f32::EPSILON {
        body["temperature"] = json!(temperature);
    }
    if !tools.is_empty() {
        body["tools"] = json!(convert_tools(tools));
    }

    body
}

// ── Akış çözümleme ──────────────────────────────────────────────────────────

/// Akış boyunca biriken durum.
///
/// Anthropic'te tool argümanları `input_json_delta` parçaları hâlinde gelir;
/// blok indeksine göre biriktirilip sonunda çözülür.
#[derive(Default)]
pub struct StreamState {
    /// Blok indeksi → (tool id, ad, biriken JSON metni)
    tool_blocks: Vec<(usize, String, String, String)>,
}

/// Bir SSE veri satırının sonucu.
#[derive(Debug, PartialEq)]
pub enum Chunk {
    /// Metin parçası.
    Text(String),
    /// Bu olayda işlenecek bir şey yok.
    Nothing,
    /// Akış bitti.
    Done,
}

impl StreamState {
    /// Tek bir `data:` satırını işler.
    pub fn feed(&mut self, data: &str) -> Chunk {
        let Ok(event) = serde_json::from_str::<Value>(data) else {
            return Chunk::Nothing;
        };

        match event["type"].as_str().unwrap_or_default() {
            // Yeni içerik bloğu başladı — tool ise kaydını aç.
            "content_block_start" => {
                let index = event["index"].as_u64().unwrap_or(0) as usize;
                let block = &event["content_block"];
                if block["type"] == "tool_use" {
                    self.tool_blocks.push((
                        index,
                        block["id"].as_str().unwrap_or_default().to_string(),
                        block["name"].as_str().unwrap_or_default().to_string(),
                        String::new(),
                    ));
                }
                Chunk::Nothing
            }

            "content_block_delta" => {
                let delta = &event["delta"];
                match delta["type"].as_str().unwrap_or_default() {
                    "text_delta" => {
                        Chunk::Text(delta["text"].as_str().unwrap_or_default().to_string())
                    }
                    "input_json_delta" => {
                        let index = event["index"].as_u64().unwrap_or(0) as usize;
                        let partial = delta["partial_json"].as_str().unwrap_or_default();
                        if let Some(slot) = self.tool_blocks.iter_mut().find(|(i, ..)| *i == index)
                        {
                            slot.3.push_str(partial);
                        }
                        Chunk::Nothing
                    }
                    // "thinking_delta" vb. — gösterilmiyor.
                    _ => Chunk::Nothing,
                }
            }

            "message_stop" => Chunk::Done,

            // Sunucu hata olayı gönderdiyse akışı bitir.
            "error" => {
                tracing::warn!(?event, "anthropic akış hatası");
                Chunk::Done
            }

            _ => Chunk::Nothing,
        }
    }

    /// Biriken tool çağrılarını tamamlanmış hâle getirir.
    pub fn finish(self) -> Vec<ToolCall> {
        self.tool_blocks
            .into_iter()
            .filter(|(_, _, name, _)| !name.is_empty())
            .map(|(index, id, name, args)| ToolCall {
                // Anthropic sends no such state.
                provider_state: None,
                id: if id.is_empty() {
                    format!("call_{index}")
                } else {
                    id
                },
                kind: "function".to_string(),
                function: FunctionCall {
                    name,
                    arguments: if args.trim().is_empty() {
                        "{}".to_string()
                    } else {
                        args
                    },
                },
            })
            .collect()
    }
}

/// Model listesi cevabı.
#[derive(Deserialize)]
pub struct ModelList {
    #[serde(default)]
    pub data: Vec<ModelEntry>,
}

#[derive(Deserialize)]
pub struct ModelEntry {
    pub id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_message_becomes_top_level_field() {
        let msgs = vec![Message::system("sen vavis'sin"), Message::user("selam")];
        let (system, out) = build_messages(&msgs);

        assert_eq!(system, "sen vavis'sin");
        assert_eq!(out.len(), 1, "sistem mesajı dizide olmamalı");
        assert_eq!(out[0]["role"], "user");
    }

    #[test]
    fn multiple_system_messages_are_merged() {
        let msgs = vec![
            Message::system("birinci"),
            Message::system("ikinci"),
            Message::user("x"),
        ];
        let (system, _) = build_messages(&msgs);
        assert!(system.contains("birinci") && system.contains("ikinci"));
    }

    #[test]
    fn tool_result_becomes_a_user_content_block() {
        let msgs = vec![
            Message::user("saat kaç"),
            Message::tool_result("call_1", "14:30"),
        ];
        let (_, out) = build_messages(&msgs);

        let last = out.last().unwrap();
        assert_eq!(last["role"], "user");
        assert_eq!(last["content"][0]["type"], "tool_result");
        assert_eq!(last["content"][0]["tool_use_id"], "call_1");
    }

    #[test]
    fn consecutive_tool_results_merge_into_one_message() {
        // Ayrı mesajlara bölmek modeli paralel tool kullanmaktan caydırır.
        let msgs = vec![
            Message::user("x"),
            Message::tool_result("call_1", "bir"),
            Message::tool_result("call_2", "iki"),
        ];
        let (_, out) = build_messages(&msgs);

        assert_eq!(out.len(), 2, "tool sonuçları tek mesajda birleşmeli");
        assert_eq!(out[1]["content"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn assistant_tool_calls_become_tool_use_blocks() {
        let msgs = vec![
            Message::user("saat"),
            Message {
                role: Role::Assistant,
                content: "Bakıyorum".into(),
                tool_call_id: None,
                image: None,
                tool_calls: Some(vec![ToolCall {
                    provider_state: None,
                    id: "call_x".into(),
                    kind: "function".into(),
                    function: FunctionCall {
                        name: "get_current_time".into(),
                        arguments: r#"{"a":1}"#.into(),
                    },
                }]),
            },
        ];
        let (_, out) = build_messages(&msgs);

        let assistant = &out[1];
        let blocks = assistant["content"].as_array().unwrap();
        assert_eq!(blocks[0]["type"], "text");
        assert_eq!(blocks[1]["type"], "tool_use");
        assert_eq!(blocks[1]["name"], "get_current_time");
        assert_eq!(blocks[1]["input"]["a"], 1);
    }

    #[test]
    fn empty_assistant_message_is_dropped() {
        // Boş içerik + tool yok → API bunu reddeder.
        let msgs = vec![Message::user("x"), Message::assistant("")];
        let (_, out) = build_messages(&msgs);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn conversation_always_starts_with_user() {
        let msgs = vec![Message::assistant("ben başlıyorum")];
        let (_, out) = build_messages(&msgs);
        assert_eq!(out[0]["role"], "user", "ilk mesaj user olmalı");
    }

    #[test]
    fn tools_are_converted_to_anthropic_shape() {
        let openai = vec![json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "dosya okur",
                "parameters": {"type": "object", "properties": {"path": {"type": "string"}}}
            }
        })];

        let converted = convert_tools(&openai);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0]["name"], "read_file");
        assert_eq!(converted[0]["description"], "dosya okur");
        assert!(converted[0]["input_schema"]["properties"]["path"].is_object());
        assert!(
            converted[0].get("function").is_none(),
            "sarmalayıcı kalmamalı"
        );
    }

    #[test]
    fn body_always_includes_max_tokens() {
        // Anthropic'te zorunlu — eksikse 400.
        let body = build_body("claude-sonnet-5", &[Message::user("x")], &[], 1024, 0.7);
        assert_eq!(body["max_tokens"], 1024);
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn body_omits_tools_when_none_given() {
        let body = build_body("claude-sonnet-5", &[Message::user("x")], &[], 1024, 0.7);
        assert!(body.get("tools").is_none());
    }

    // ── Akış ─────────────────────────────────────────────────────────────

    #[test]
    fn text_delta_yields_text() {
        let mut state = StreamState::default();
        let chunk = state.feed(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Mer"}}"#,
        );
        assert_eq!(chunk, Chunk::Text("Mer".into()));
    }

    #[test]
    fn message_stop_ends_the_stream() {
        let mut state = StreamState::default();
        assert_eq!(state.feed(r#"{"type":"message_stop"}"#), Chunk::Done);
    }

    #[test]
    fn unknown_events_are_ignored() {
        let mut state = StreamState::default();
        assert_eq!(state.feed(r#"{"type":"ping"}"#), Chunk::Nothing);
        assert_eq!(state.feed(r#"{"type":"message_start"}"#), Chunk::Nothing);
        assert_eq!(state.feed("bozuk json"), Chunk::Nothing);
    }

    #[test]
    fn tool_use_is_assembled_from_json_deltas() {
        let mut state = StreamState::default();

        state.feed(
            r#"{"type":"content_block_start","index":0,
                "content_block":{"type":"tool_use","id":"toolu_1","name":"read_file"}}"#,
        );
        state.feed(
            r#"{"type":"content_block_delta","index":0,
                "delta":{"type":"input_json_delta","partial_json":"{\"path\":"}}"#,
        );
        state.feed(
            r#"{"type":"content_block_delta","index":0,
                "delta":{"type":"input_json_delta","partial_json":"\"~/a.txt\"}"}}"#,
        );

        let calls = state.finish();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "toolu_1");
        assert_eq!(calls[0].function.name, "read_file");
        assert_eq!(calls[0].function.arguments, r#"{"path":"~/a.txt"}"#);
    }

    #[test]
    fn parallel_tool_blocks_stay_separate() {
        let mut state = StreamState::default();

        state.feed(
            r#"{"type":"content_block_start","index":0,
                "content_block":{"type":"tool_use","id":"a","name":"birinci"}}"#,
        );
        state.feed(
            r#"{"type":"content_block_start","index":1,
                "content_block":{"type":"tool_use","id":"b","name":"ikinci"}}"#,
        );
        state.feed(
            r#"{"type":"content_block_delta","index":1,
                "delta":{"type":"input_json_delta","partial_json":"{}"}}"#,
        );

        let calls = state.finish();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].function.name, "birinci");
        assert_eq!(calls[1].function.name, "ikinci");
    }

    #[test]
    fn tool_without_arguments_gets_empty_object() {
        let mut state = StreamState::default();
        state.feed(
            r#"{"type":"content_block_start","index":0,
                "content_block":{"type":"tool_use","id":"x","name":"get_current_time"}}"#,
        );

        let calls = state.finish();
        assert_eq!(
            calls[0].function.arguments, "{}",
            "boş argüman geçerli JSON olmalı"
        );
    }

    #[test]
    fn text_blocks_do_not_become_tool_calls() {
        let mut state = StreamState::default();
        state.feed(
            r#"{"type":"content_block_start","index":0,
                "content_block":{"type":"text","text":""}}"#,
        );
        assert!(state.finish().is_empty());
    }
}
