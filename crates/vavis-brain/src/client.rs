//! LLM istemcisi — akan (streaming) sohbet tamamlama.
//!
//! Akış neden önemli: kullanıcı cevabı harf harf görür, beklemez. Eski projede
//! akış vardı ama TTS gecikmesi yüzünden "akış hissi" kayboluyordu (manuel test
//! notu). Burada akış parçaları kanal üzerinden anında UI'ya gider.
//!
//! Ağ çağrıları tokio üzerinde; UI iş parçacığı **asla bloklanmaz.**

use crate::budget::{estimate_tokens, fit_request, ModelCaps};
use crate::message::{Message, ToolCall};
use crate::provider::Provider;
use futures_util::StreamExt;
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum BrainError {
    #[error("{provider} için API anahtarı yok")]
    MissingKey { provider: Provider },

    #[error("ağ hatası: {0}")]
    Network(#[from] reqwest::Error),

    #[error("sağlayıcı hatası ({status}): {body}")]
    Api { status: u16, body: String },

    #[error("cevap çözümlenemedi: {0}")]
    Parse(String),
}

pub type Result<T> = std::result::Result<T, BrainError>;

/// Akış sırasında üretilen olaylar.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    /// Metin parçası geldi.
    Delta(String),
    /// Model tool çağırmak istedi (F3'te işlenecek).
    ToolCalls(Vec<ToolCall>),
    /// Akış bitti.
    Done,
}

/// Bir sohbet turunun sonucu.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChatResponse {
    /// Modelin ürettiği metin.
    pub text: String,
    /// Modelin çalıştırılmasını istediği tool'lar (boş olabilir).
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Clone)]
pub struct ChatConfig {
    pub provider: Provider,
    pub model: String,
    pub api_key: String,
    pub temperature: f32,
    /// Sağlayıcının varsayılan URL'sini ezer.
    ///
    /// Gerçek kullanımı: yerel sunucu farklı portta koşuyorsa (Ollama 11434
    /// yerine LM Studio 1234). Testlerde de sahte sunucuya yönlendirmek için.
    pub url_override: Option<String>,
}

impl ChatConfig {
    pub fn new(provider: Provider, model: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            provider,
            model: model.into(),
            api_key: api_key.into(),
            temperature: 0.7,
            url_override: None,
        }
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url_override = Some(url.into());
        self
    }

    fn chat_url(&self) -> &str {
        self.url_override
            .as_deref()
            .unwrap_or_else(|| self.provider.chat_url())
    }
}

pub struct BrainClient {
    http: reqwest::Client,
}

impl Default for BrainClient {
    fn default() -> Self {
        Self::new()
    }
}

impl BrainClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            // Akış uzun sürebilir; bağlanma zaman aşımı ayrı tutulur.
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(300))
            .build()
            .expect("http istemcisi kurulamadı");
        Self { http }
    }

    /// Akan sohbet. Her parça için `on_event` çağrılır.
    ///
    /// Bütçe sığdırma **burada** yapılır — çağıran tarafın unutması mümkün değil.
    pub async fn chat_stream<F>(
        &self,
        cfg: &ChatConfig,
        messages: Vec<Message>,
        on_event: F,
    ) -> Result<String>
    where
        F: FnMut(StreamEvent),
    {
        self.chat_stream_with_tools(cfg, messages, &[], on_event)
            .await
            .map(|r| r.text)
    }

    /// Tool'lu akan sohbet.
    ///
    /// Tool çağrıları akışta **parça parça** gelir (ad bir parçada, argümanlar
    /// sonraki parçalarda) — burada indekse göre birleştirilir. Eski projede
    /// bu birleştirme eksikti ve uzun argümanlar bozuluyordu.
    pub async fn chat_stream_with_tools<F>(
        &self,
        cfg: &ChatConfig,
        messages: Vec<Message>,
        tools: &[serde_json::Value],
        mut on_event: F,
    ) -> Result<ChatResponse>
    where
        F: FnMut(StreamEvent),
    {
        if cfg.provider.needs_key() && cfg.api_key.trim().is_empty() {
            return Err(BrainError::MissingKey {
                provider: cfg.provider,
            });
        }

        let caps = ModelCaps::for_model(&cfg.model);

        // Anthropic tamamen farklı gövde/akış şeması kullanıyor — ayrı yol.
        if cfg.provider == Provider::Anthropic {
            return self
                .anthropic_stream(cfg, messages, tools, caps, on_event)
                .await;
        }

        // Tool şemaları da bütçeye sayılır — 413'ün kök nedeni buydu.
        let fitted = fit_request(messages, tools.to_vec(), caps);
        if fitted.history_dropped > 0 || fitted.tools_dropped > 0 {
            tracing::info!(
                dropped = fitted.history_dropped,
                tools_dropped = fitted.tools_dropped,
                est = fitted.est_tokens,
                "geçmiş bütçeye sığdırıldı"
            );
        }

        let mut body = serde_json::json!({
            "model": cfg.model,
            "messages": openai_messages(&fitted.messages),
            "temperature": cfg.temperature,
            "max_tokens": caps.max_output,
            "stream": true,
        });
        // `fitted.tools`, not `tools`: the trimmed list is the one that fits.
        // Sending the full set here is exactly the bug that made the budget
        // module's trimming pointless.
        if !fitted.tools.is_empty() {
            body["tools"] = serde_json::Value::Array(fitted.tools.clone());
            body["tool_choice"] = serde_json::Value::String("auto".into());
        }

        let mut req = self
            .http
            .post(cfg.chat_url())
            .header("content-type", "application/json");
        if cfg.provider.needs_key() {
            req = req.bearer_auth(&cfg.api_key);
        }

        let resp = req.json(&body).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            let body = body.chars().take(500).collect::<String>();
            return Err(BrainError::Api {
                status: status.as_u16(),
                body,
            });
        }

        let mut full = String::new();
        let mut buf = String::new();
        // Tool çağrıları indekse göre biriktirilir: sağlayıcı adı bir parçada,
        // argümanları sonraki parçalarda gönderir.
        let mut pending: Vec<ToolCallBuilder> = Vec::new();
        let mut stream = resp.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let bytes = chunk?;
            buf.push_str(&String::from_utf8_lossy(&bytes));

            // SSE: olaylar boş satırla değil, `data: ` satırlarıyla gelir.
            // Yarım satır kalabilir — tamponda bekletiriz.
            while let Some(nl) = buf.find('\n') {
                let line = buf[..nl].trim().to_string();
                buf.drain(..=nl);

                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                let data = data.trim();

                if data == "[DONE]" {
                    let calls = finish_calls(pending);
                    if !calls.is_empty() {
                        on_event(StreamEvent::ToolCalls(calls.clone()));
                    }
                    on_event(StreamEvent::Done);
                    return Ok(ChatResponse {
                        text: full,
                        tool_calls: calls,
                    });
                }
                if data.is_empty() {
                    continue;
                }

                match serde_json::from_str::<StreamChunk>(data) {
                    Ok(parsed) => {
                        if let Some(choice) = parsed.choices.into_iter().next() {
                            if let Some(text) = choice.delta.content {
                                if !text.is_empty() {
                                    full.push_str(&text);
                                    on_event(StreamEvent::Delta(text));
                                }
                            }
                            if let Some(calls) = choice.delta.tool_calls {
                                merge_tool_calls(&mut pending, calls);
                            }
                        }
                    }
                    // Tek bozuk parça yüzünden akışı öldürme — atla, devam et.
                    Err(e) => tracing::debug!(%e, data, "akış parçası çözümlenemedi"),
                }
            }
        }

        // [DONE] gelmeden akış bitti (bazı sağlayıcılar göndermiyor).
        let calls = finish_calls(pending);
        if !calls.is_empty() {
            on_event(StreamEvent::ToolCalls(calls.clone()));
        }
        on_event(StreamEvent::Done);
        Ok(ChatResponse {
            text: full,
            tool_calls: calls,
        })
    }

    /// Anthropic Messages API akışı.
    ///
    /// OpenAI yolundan ayrı tutuluyor çünkü hemen her şey farklı: kimlik
    /// başlıkları, sistem isteminin yeri, tool şeması, SSE olay tipleri.
    /// Dönüşüm `crate::anthropic` modülünde; burası sadece HTTP.
    async fn anthropic_stream<F>(
        &self,
        cfg: &ChatConfig,
        messages: Vec<Message>,
        tools: &[serde_json::Value],
        caps: ModelCaps,
        mut on_event: F,
    ) -> Result<ChatResponse>
    where
        F: FnMut(StreamEvent),
    {
        use crate::anthropic::{self, Chunk, StreamState};

        let fitted = fit_request(messages, tools.to_vec(), caps);

        // The trimmed list, for the same reason as the OpenAI path above.
        let body = anthropic::build_body(
            &cfg.model,
            &fitted.messages,
            &fitted.tools,
            caps.max_output,
            cfg.temperature,
        );

        let resp = self
            .http
            .post(cfg.chat_url())
            // Anthropic Bearer değil x-api-key kullanıyor.
            .header("x-api-key", &cfg.api_key)
            .header("anthropic-version", anthropic::API_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(BrainError::Api {
                status: status.as_u16(),
                body: body.chars().take(500).collect(),
            });
        }

        let mut full = String::new();
        let mut buf = String::new();
        let mut state = StreamState::default();
        let mut stream = resp.bytes_stream();

        while let Some(chunk) = stream.next().await {
            buf.push_str(&String::from_utf8_lossy(&chunk?));

            while let Some(nl) = buf.find('\n') {
                let line = buf[..nl].trim().to_string();
                buf.drain(..=nl);

                // `event:` satırları yok sayılır — tip zaten veri içinde.
                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                let data = data.trim();
                if data.is_empty() {
                    continue;
                }

                match state.feed(data) {
                    Chunk::Text(text) => {
                        full.push_str(&text);
                        on_event(StreamEvent::Delta(text));
                    }
                    Chunk::Done => {
                        let calls = state.finish();
                        if !calls.is_empty() {
                            on_event(StreamEvent::ToolCalls(calls.clone()));
                        }
                        on_event(StreamEvent::Done);
                        return Ok(ChatResponse {
                            text: full,
                            tool_calls: calls,
                        });
                    }
                    Chunk::Nothing => {}
                }
            }
        }

        // Akış `message_stop` gelmeden bitti.
        let calls = state.finish();
        if !calls.is_empty() {
            on_event(StreamEvent::ToolCalls(calls.clone()));
        }
        on_event(StreamEvent::Done);
        Ok(ChatResponse {
            text: full,
            tool_calls: calls,
        })
    }

    /// Canlı model listesi. Sağlayıcı gürültüsü süzülür.
    pub async fn list_models(&self, provider: Provider, api_key: &str) -> Result<Vec<String>> {
        if provider.needs_key() && api_key.trim().is_empty() {
            return Err(BrainError::MissingKey { provider });
        }

        let mut req = self.http.get(provider.models_url());
        if provider.needs_key() {
            req = req.bearer_auth(api_key);
        }

        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(BrainError::Api {
                status: status.as_u16(),
                body: body.chars().take(300).collect(),
            });
        }

        let list: ModelList = resp
            .json()
            .await
            .map_err(|e| BrainError::Parse(e.to_string()))?;

        let all: Vec<String> = list.data.into_iter().map(|m| m.id).collect();
        let useful: Vec<String> = all
            .iter()
            .filter(|id| crate::provider::is_useful_model(provider, id))
            .cloned()
            .collect();

        // Süzgeç her şeyi elerse süzülmemiş listeye dön — boş liste en kötüsü.
        let mut out = if useful.is_empty() { all } else { useful };
        out.sort_unstable();
        Ok(out)
    }
}

/// Mesajları OpenAI-uyumlu gövdeye çevirir.
///
/// Görüntüsüz mesajlar doğrudan serileştirilir. Görüntülü olanlar
/// **çok parçalı** biçime dönüşür:
/// `content: [{type:"text",...}, {type:"image_url", image_url:{url:"data:..."}}]`
///
/// Neden burada: `Message` yapısı sade kalsın; biçim dönüşümü sağlayıcıya
/// ait bir ayrıntı.
fn openai_messages(messages: &[Message]) -> Vec<serde_json::Value> {
    messages
        .iter()
        .map(|m| {
            let Some(image) = &m.image else {
                return serde_json::to_value(m).unwrap_or(serde_json::json!({}));
            };

            let mut parts = Vec::new();
            if !m.content.trim().is_empty() {
                parts.push(serde_json::json!({"type": "text", "text": m.content}));
            }
            parts.push(serde_json::json!({
                "type": "image_url",
                "image_url": {"url": format!("data:image/png;base64,{image}")}
            }));

            serde_json::json!({
                "role": m.role.as_str(),
                "content": parts,
            })
        })
        .collect()
}

/// Sistem istemi — asistanın kimliği.
pub fn system_prompt(assistant_name: &str, language: &str) -> String {
    let lang = match language {
        "en" => "English",
        _ => "Türkçe",
    };
    format!(
        "Sen {assistant_name} adlı kişisel bir asistansın. Kullanıcının bilgisayarında \
         çalışıyorsun. {lang} konuş. Kısa, net ve doğrudan cevap ver — gereksiz \
         nezaket cümleleri kurma. Bilmediğin bir şeyi uydurma, bilmiyorum de.\n\n\
         Sana her istekte yalnızca o iş için gerekli görünen araçlar veriliyor. \
         İhtiyacın olan bir araç elinde yoksa uydurma: `request_tools` ile ne \
         yapmak istediğini yaz, ilgili araçlar bir sonraki adımda elinde olur."
    )
}

/// Tahmini token — arayüzün bilgi göstermesi için.
pub fn estimate_conversation_tokens(messages: &[Message]) -> usize {
    messages
        .iter()
        .map(|m| estimate_tokens(&m.content) + 4)
        .sum()
}

// ── Sağlayıcı cevap şemaları ────────────────────────────────────────────────

#[derive(Deserialize)]
struct StreamChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
}

#[derive(Deserialize)]
struct StreamChoice {
    #[serde(default)]
    delta: Delta,
}

#[derive(Deserialize, Default)]
struct Delta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<PartialToolCall>>,
}

#[derive(Deserialize)]
struct PartialToolCall {
    /// Hangi tool çağrısına ait olduğu — parçalar bununla birleştirilir.
    #[serde(default)]
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<PartialFunction>,
}

/// Akış boyunca biriken tek bir tool çağrısı.
#[derive(Default)]
struct ToolCallBuilder {
    index: usize,
    id: String,
    name: String,
    arguments: String,
}

/// Gelen parçaları mevcut çağrılara ekler.
fn merge_tool_calls(pending: &mut Vec<ToolCallBuilder>, incoming: Vec<PartialToolCall>) {
    for part in incoming {
        let slot = match pending.iter_mut().find(|b| b.index == part.index) {
            Some(existing) => existing,
            None => {
                pending.push(ToolCallBuilder {
                    index: part.index,
                    ..Default::default()
                });
                pending.last_mut().expect("az önce eklendi")
            }
        };

        if let Some(id) = part.id {
            if !id.is_empty() {
                slot.id = id;
            }
        }
        if let Some(f) = part.function {
            if let Some(name) = f.name {
                if !name.is_empty() {
                    slot.name = name;
                }
            }
            if let Some(args) = f.arguments {
                // Argümanlar parça parça gelir — eklenerek birleştirilir.
                slot.arguments.push_str(&args);
            }
        }
    }
}

/// Biriken çağrıları tamamlanmış hâle getirir.
fn finish_calls(pending: Vec<ToolCallBuilder>) -> Vec<ToolCall> {
    pending
        .into_iter()
        // Adı olmayan çağrı kullanılamaz — sessizce at.
        .filter(|b| !b.name.is_empty())
        .map(|b| ToolCall {
            id: if b.id.is_empty() {
                format!("call_{}", b.index)
            } else {
                b.id
            },
            kind: "function".to_string(),
            function: crate::message::FunctionCall {
                name: b.name,
                arguments: if b.arguments.trim().is_empty() {
                    "{}".to_string()
                } else {
                    b.arguments
                },
            },
        })
        .collect()
}

#[derive(Deserialize)]
struct PartialFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Deserialize)]
struct ModelList {
    #[serde(default)]
    data: Vec<ModelEntry>,
}

#[derive(Deserialize)]
struct ModelEntry {
    id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn missing_key_fails_before_any_network_call() {
        let client = BrainClient::new();
        let cfg = ChatConfig::new(Provider::Groq, "llama-3.3-70b-versatile", "");
        let err = client
            .chat_stream(&cfg, vec![Message::user("selam")], |_| {})
            .await
            .unwrap_err();
        assert!(matches!(err, BrainError::MissingKey { .. }));
    }

    #[test]
    fn sse_chunk_parses() {
        let json = r#"{"choices":[{"delta":{"content":"mer"}}]}"#;
        let chunk: StreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("mer"));
    }

    #[test]
    fn empty_delta_does_not_break_parsing() {
        // Sağlayıcılar rol-only ilk parça gönderir; çökmemeli.
        let json = r#"{"choices":[{"delta":{"role":"assistant"}}]}"#;
        let chunk: StreamChunk = serde_json::from_str(json).unwrap();
        assert!(chunk.choices[0].delta.content.is_none());
    }

    #[test]
    fn system_prompt_carries_name_and_language() {
        let p = system_prompt("Vavis", "tr");
        assert!(p.contains("Vavis"));
        assert!(p.contains("Türkçe"));
        assert!(system_prompt("Vavis", "en").contains("English"));
    }

    /// İstem bir araç adı anıyorsa o ad **gerçek** olmalı.
    ///
    /// Yeniden adlandırmada burası atlanmıştı: istem hâlâ `arac_iste`
    /// diyordu, oysa araç `request_tools` olmuştu. Model, var olmayan bir
    /// adı çağırmaya davet ediliyordu — derleyici de test de göremez,
    /// çünkü ad burada düz metin.
    ///
    /// Araç adı değişirse bu test kırılmaz; en azından adın Türkçe eski
    /// biçime geri dönmediğini garanti ediyor.
    #[test]
    fn the_prompt_names_a_tool_that_actually_exists() {
        let p = system_prompt("Vavis", "tr");
        assert!(p.contains("request_tools"), "{p}");
        assert!(!p.contains("arac_iste"), "eski ad kalmış: {p}");
    }

    #[test]
    fn conversation_token_estimate_grows_with_content() {
        let short = vec![Message::user("a")];
        let long = vec![Message::user("a".repeat(4000))];
        assert!(estimate_conversation_tokens(&long) > estimate_conversation_tokens(&short) + 500);
    }
}
