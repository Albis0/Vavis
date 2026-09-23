//! LLM istemcisi — akan (streaming) sohbet tamamlama.
//!
//! Akış neden önemli: kullanıcı cevabı harf harf görür, beklemez. Eski projede
//! akış vardı ama TTS gecikmesi yüzünden "akış hissi" kayboluyordu (manuel test
//! notu). Burada akış parçaları kanal üzerinden anında UI'ya gider.
//!
//! Ağ çağrıları tokio üzerinde; UI iş parçacığı **asla bloklanmaz.**

use crate::budget::{estimate_tokens, fit_request, ModelCaps};
use crate::builtin;
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

    /// A setting the request cannot go out without, such as a custom
    /// provider with no URL.
    #[error("ayar eksik: {0}")]
    Config(String),

    /// The Claude Code CLI is not installed, or not where we looked.
    #[error("Claude Code bulunamadı")]
    CliMissing,

    /// The CLI is installed but not logged in.
    #[error("Claude Code oturumu yok: {0}")]
    CliLogin(String),

    /// The subscription's limit for this window is used up. `resets_at` is
    /// a unix timestamp when the CLI named one.
    #[error("Claude kullanım sınırı doldu")]
    UsageLimit { resets_at: Option<i64> },

    /// Anything else the CLI reported.
    #[error("Claude Code: {0}")]
    Cli(String),
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
    /// Where Vavis's tools can be reached over MCP. Only the Claude Code
    /// provider reads it: every other provider takes tool schemas in the
    /// request body instead.
    pub tool_bridge: Option<crate::claude_code::ToolBridge>,
}

impl ChatConfig {
    pub fn new(provider: Provider, model: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            provider,
            model: model.into(),
            api_key: api_key.into(),
            temperature: 0.7,
            url_override: None,
            tool_bridge: None,
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

        // A custom endpoint has no URL of its own to fall back on.
        if cfg.provider == Provider::Custom && cfg.url_override.is_none() {
            return Err(BrainError::Config(
                "the custom provider has no URL — set one in settings".into(),
            ));
        }

        // Not HTTP at all: a program on the user's machine. Tools reach it
        // over MCP rather than in a request body, so what it gets here is
        // whether to attach them, not the schemas themselves.
        if cfg.provider == Provider::ClaudeCode {
            let bridge = if tools.is_empty() {
                None
            } else {
                cfg.tool_bridge.as_ref()
            };
            return crate::claude_code::run(cfg, messages, bridge, on_event).await;
        }

        let caps = ModelCaps::for_model(&cfg.model);

        // Anthropic tamamen farklı gövde/akış şeması kullanıyor — ayrı yol.
        if cfg.provider == Provider::Anthropic {
            return self
                .anthropic_stream(cfg, messages, tools, caps, on_event)
                .await;
        }

        // Gemini de öyle. Google'ın bir OpenAI-uyumlu kapısı var ve bir süre
        // onu kullandık, ama o kapı bir alt küme: düşünme ayarları, güvenlik
        // eşikleri, çok parçalı içerik oradan geçmiyor.
        if cfg.provider == Provider::Gemini {
            return self
                .gemini_stream(cfg, messages, tools, caps, on_event)
                .await;
        }

        // Bazı modeller araçlarını sunucu tarafında kendileri çalıştırıyor ve
        // bizimkilerden **tek bir tanesini** bile kabul etmiyor: istek 400
        // ile tamamen düşüyor. Süzgeç burada, gövdenin kurulduğu tek yerde —
        // çağıran tarafın atlaması mümkün değil.
        let tools: &[serde_json::Value] = if builtin::tool_support(cfg.provider, &cfg.model)
            .accepts_functions()
        {
            tools
        } else {
            tracing::debug!(model = %cfg.model, "model kendi araçlarını çalıştırıyor; şemalar gönderilmedi");
            &[]
        };

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
            "stream": true,
        });
        let shape = RequestShape::for_model(cfg.provider, &cfg.model);
        body[shape.token_field] = serde_json::json!(caps.max_output);
        if shape.temperature {
            body["temperature"] = serde_json::json!(cfg.temperature);
        }
        // `fitted.tools`, not `tools`: the trimmed list is the one that fits.
        // Sending the full set here is exactly the bug that made the budget
        // module's trimming pointless.
        //
        // Yerleşik araçlar bütçeye sığdırılmıyor: şema değil tek satırlık tip
        // nesneleri, toplamı birkaç token. Budanacak bir şey yok.
        let mut wire_tools = fitted.tools.clone();
        wire_tools.extend(builtin::extra_tools(cfg.provider, &cfg.model));
        if !wire_tools.is_empty() {
            body["tools"] = serde_json::Value::Array(wire_tools);
            body["tool_choice"] = serde_json::Value::String("auto".into());
        }

        let mut req = self
            .http
            .post(cfg.chat_url())
            .header("content-type", "application/json");
        if !cfg.api_key.trim().is_empty() && cfg.provider != Provider::Local {
            req = req.bearer_auth(&cfg.api_key);
        }
        req = provider_headers(req, cfg.provider);

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

    /// Gemini'nin yerel `streamGenerateContent` akışı.
    ///
    /// OpenAI-uyumlu uç noktadan ayrıldık: o kapı bir alt küme sunuyor
    /// (düşünme ayarları, güvenlik eşikleri, çok parçalı içerik oradan
    /// geçmiyor). Dönüşüm `crate::gemini` modülünde; burası sadece HTTP.
    ///
    /// Anthropic yolundan farkı: Gemini'de bitişi bildiren bir olay **yok**.
    /// Akış kapanınca biter, o yüzden döngünün sonrası tek çıkış noktası.
    async fn gemini_stream<F>(
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
        use crate::gemini::{self, Chunk, StreamState};

        let fitted = fit_request(messages, tools.to_vec(), caps);

        let body = gemini::build_body(
            &fitted.messages,
            &fitted.tools,
            caps.max_output,
            cfg.temperature,
        );

        // The model travels in the path, so a URL override has to replace the
        // whole thing rather than have a model appended to it.
        let url = cfg
            .url_override
            .clone()
            .unwrap_or_else(|| gemini::chat_url(&cfg.model));

        let resp = self
            .http
            .post(url)
            // In the header, never the query string: a key in the URL reaches
            // request logs, proxies, and the error body we show the user.
            .header(gemini::KEY_HEADER, &cfg.api_key)
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
        let mut calls: Vec<ToolCall> = Vec::new();
        let mut state = StreamState::new();
        let mut stream = resp.bytes_stream();

        while let Some(chunk) = stream.next().await {
            buf.push_str(&String::from_utf8_lossy(&chunk?));

            while let Some(nl) = buf.find('\n') {
                let line = buf[..nl].trim().to_string();
                buf.drain(..=nl);

                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                let data = data.trim();
                if data.is_empty() {
                    continue;
                }

                for piece in state.feed(data) {
                    match piece {
                        Chunk::Text(text) => {
                            full.push_str(&text);
                            on_event(StreamEvent::Delta(text));
                        }
                        Chunk::Call(call) => calls.push(call),
                        Chunk::Nothing => {}
                    }
                }
            }
        }

        if !calls.is_empty() {
            on_event(StreamEvent::ToolCalls(calls.clone()));
        }
        on_event(StreamEvent::Done);
        Ok(ChatResponse {
            text: full,
            tool_calls: calls,
        })
    }

    /// Embeds texts for semantic memory. See [`crate::embeddings`].
    pub async fn embed(
        &self,
        cfg: &crate::embeddings::EmbedConfig,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        crate::embeddings::embed(&self.http, cfg, texts).await
    }

    /// Gemini models that speak the Live API (`bidiGenerateContent`) -- the
    /// ones the chat picker hides, and the only ones a live conversation can
    /// use.
    pub async fn list_live_models(&self, api_key: &str) -> Result<Vec<String>> {
        if api_key.trim().is_empty() {
            return Err(BrainError::MissingKey {
                provider: Provider::Gemini,
            });
        }
        let resp = self
            .http
            .get(Provider::Gemini.models_url())
            .header(crate::gemini::KEY_HEADER, api_key)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(BrainError::Api {
                status: status.as_u16(),
                body: body.chars().take(300).collect(),
            });
        }
        let list: GeminiModelList = resp
            .json()
            .await
            .map_err(|e| BrainError::Parse(e.to_string()))?;
        let mut out: Vec<String> = list
            .models
            .into_iter()
            .filter(|m| {
                m.supported_generation_methods
                    .iter()
                    .any(|s| s == "bidiGenerateContent")
            })
            .map(|m| m.name.trim_start_matches("models/").to_string())
            .collect();
        out.sort_unstable();
        Ok(out)
    }

    /// Canlı model listesi. Sağlayıcı gürültüsü süzülür.
    pub async fn list_models(&self, provider: Provider, api_key: &str) -> Result<Vec<String>> {
        self.list_models_at(provider, api_key, None).await
    }

    /// As [`Self::list_models`], for a provider reached at a URL of the
    /// user's choosing (`chat_url` is the chat endpoint they configured).
    pub async fn list_models_at(
        &self,
        provider: Provider,
        api_key: &str,
        chat_url: Option<&str>,
    ) -> Result<Vec<String>> {
        if provider.needs_key() && api_key.trim().is_empty() {
            return Err(BrainError::MissingKey { provider });
        }

        // No endpoint to ask: the CLI resolves these aliases itself, to the
        // newest model of each tier. Checking that the CLI is there at all
        // is what makes this a useful answer rather than a constant.
        if provider == Provider::ClaudeCode {
            crate::claude_code::version().await?;
            return Ok(crate::claude_code::MODELS
                .iter()
                .map(|m| m.to_string())
                .collect());
        }

        let url = match chat_url {
            Some(chat) => models_url_from_chat(chat),
            None if provider == Provider::Custom => {
                return Err(BrainError::Config(
                    "the custom provider has no URL — set one in settings".into(),
                ))
            }
            None => provider.models_url().to_string(),
        };

        let mut req = self.http.get(url);
        req = match provider {
            // Gemini's own endpoint takes the key in a header and answers
            // with `models[].name`, not `data[].id`.
            Provider::Gemini => req.header(crate::gemini::KEY_HEADER, api_key),
            Provider::Local => req,
            _ if !api_key.trim().is_empty() => req.bearer_auth(api_key),
            _ => req,
        };
        req = provider_headers(req, provider);

        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(BrainError::Api {
                status: status.as_u16(),
                body: body.chars().take(300).collect(),
            });
        }

        let all: Vec<String> = if provider == Provider::Gemini {
            let list: GeminiModelList = resp
                .json()
                .await
                .map_err(|e| BrainError::Parse(e.to_string()))?;
            list.models
                .into_iter()
                // Google lists embedding and image models here too. Each
                // model declares what it can do, so ask rather than guess
                // from the name -- `generateContent` is the one we need, and
                // a model without it cannot answer a chat request at all.
                //
                // An empty list means the field was absent, not that the
                // model does nothing: keep it and let the name filter judge.
                .filter(|m| {
                    if m.supported_generation_methods.is_empty() {
                        // Not silently: if Google ever renames the field,
                        // every model passes this filter and the only clue
                        // is a WebSocket-only model appearing in the picker.
                        tracing::warn!(
                            model = %m.name,
                            "model declared no generation methods; capability filter skipped"
                        );
                        return true;
                    }
                    m.supported_generation_methods
                        .iter()
                        .any(|s| s == "generateContent")
                })
                // Names come back as `models/gemini-2.5-flash`; the settings
                // screen and the chat config both use the bare name.
                .map(|m| m.name.trim_start_matches("models/").to_string())
                .collect()
        } else if provider == Provider::GitHub {
            // GitHub's catalog is a bare array with its own field names, and
            // it lists embedding models beside the chat ones -- each entry
            // says what it outputs, so ask rather than guess.
            let list: Vec<GitHubModel> = resp
                .json()
                .await
                .map_err(|e| BrainError::Parse(e.to_string()))?;
            list.into_iter()
                .filter(|m| {
                    m.supported_output_modalities.is_empty()
                        || m.supported_output_modalities.iter().any(|o| o == "text")
                })
                .map(|m| m.id)
                .collect()
        } else {
            let list: ModelList = resp
                .json()
                .await
                .map_err(|e| BrainError::Parse(e.to_string()))?;
            list.data.into_iter().map(|m| m.id).collect()
        };
        let useful: Vec<String> = all
            .iter()
            .filter(|id| crate::provider::is_useful_model(provider, id))
            .cloned()
            .collect();

        // Süzgeç her şeyi elerse süzülmemiş listeye dön — boş liste en kötüsü.
        let mut out = if useful.is_empty() { all } else { useful };
        // Free models first where a provider marks them, then by name:
        // OpenRouter lists several hundred, and the ones that cost nothing
        // are the ones a new key can actually use.
        out.sort_unstable_by(|a, b| {
            (!a.ends_with(":free"), a.as_str()).cmp(&(!b.ends_with(":free"), b.as_str()))
        });
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

/// How an OpenAI-compatible request has to be shaped for one model.
///
/// OpenAI's reasoning models (the `o` series and GPT-5) refuse two things
/// every other chat model accepts: `max_tokens`, which they want spelled
/// `max_completion_tokens`, and any temperature but the default. Sending
/// the usual body gets a 400 before a word is generated -- so every one of
/// these models, all of which the picker offers, simply did not work.
struct RequestShape {
    token_field: &'static str,
    temperature: bool,
}

impl RequestShape {
    fn for_model(provider: Provider, model: &str) -> Self {
        // The part after a vendor prefix: GitHub and OpenRouter name models
        // `openai/gpt-5-mini`.
        let bare = model
            .rsplit('/')
            .next()
            .unwrap_or(model)
            .to_ascii_lowercase();
        let reasoning = bare.starts_with("gpt-5")
            || (bare.starts_with('o') && bare[1..].starts_with(|c: char| c.is_ascii_digit()));
        let openai_backed = matches!(provider, Provider::OpenAI | Provider::GitHub)
            || (provider == Provider::OpenRouter && model.starts_with("openai/"));
        match (openai_backed, reasoning) {
            (true, true) => Self {
                token_field: "max_completion_tokens",
                temperature: false,
            },
            // OpenAI's own API takes the new spelling for every model, and
            // is the only one guaranteed to.
            (true, false) if provider == Provider::OpenAI => Self {
                token_field: "max_completion_tokens",
                temperature: true,
            },
            _ => Self {
                token_field: "max_tokens",
                temperature: true,
            },
        }
    }
}

/// Headers a provider asks for beyond authentication.
fn provider_headers(req: reqwest::RequestBuilder, provider: Provider) -> reqwest::RequestBuilder {
    match provider {
        // Optional, but it is how OpenRouter attributes traffic, and apps
        // that send it are ranked rather than anonymous.
        Provider::OpenRouter => req
            .header("HTTP-Referer", "https://github.com/albis0/vavis")
            .header("X-Title", "Vavis"),
        Provider::GitHub => req.header("X-GitHub-Api-Version", "2022-11-28"),
        _ => req,
    }
}

/// The models endpoint that sits beside a chat endpoint.
///
/// OpenAI-compatible servers put both under the same `/v1`, so
/// `…/v1/chat/completions` becomes `…/v1/models`. A URL that does not end
/// that way is taken to be the base itself.
fn models_url_from_chat(chat: &str) -> String {
    let chat = chat.trim().trim_end_matches('/');
    match chat.strip_suffix("/chat/completions") {
        Some(base) => format!("{base}/models"),
        None => format!("{chat}/models"),
    }
}

/// The chat endpoint for a URL the user typed.
///
/// People paste the base (`http://localhost:1234/v1`) as often as the full
/// endpoint; both should work.
pub fn chat_url_from_user(url: &str) -> String {
    let url = url.trim().trim_end_matches('/');
    if url.ends_with("/chat/completions") {
        url.to_string()
    } else {
        format!("{url}/chat/completions")
    }
}

/// Sistem istemi — asistanın kimliği.
///
/// Every provider but Claude Code gets the same text; see
/// [`system_prompt_for`] for the one that differs.
pub fn system_prompt(assistant_name: &str, language: &str) -> String {
    system_prompt_for(Provider::Groq, assistant_name, language)
}

/// The language to answer in, named in the prompt's own words.
///
/// Every interface language gets its own name. This used to know only
/// English and fall through to Turkish for the rest, so choosing German,
/// French or Spanish in settings got an assistant told to speak Turkish.
fn language_name(language: &str) -> &'static str {
    match language {
        "en" => "English",
        "de" => "Deutsch",
        "fr" => "Français",
        "es" => "Español",
        _ => "Türkçe",
    }
}

/// The system prompt for a particular provider.
///
/// Only how tools are described differs. Most providers are handed a few
/// tools per request and can ask for more with `request_tools`. Claude Code
/// holds every tool at once, over MCP, under the `mcp__vavis__` prefix --
/// telling it to call `request_tools` would send it looking for a tool it
/// was never given.
pub fn system_prompt_for(provider: Provider, assistant_name: &str, language: &str) -> String {
    let lang = language_name(language);
    let tools = if provider == Provider::ClaudeCode {
        "Bilgisayarda iş yapan araçların Vavis'in kendi araçları; adları \
         `mcp__vavis__` ile başlıyor. Web için WebSearch ve WebFetch de elinde. \
         Yıkıcı bir araç (dosya yazma, komut, tıklama) kullanıcıdan onay ister; \
         reddedilirse ısrar etme, başka yol öner. Kendi dosya, kabuk veya düzenleme \
         araçların yok — bilgisayarda yapılacak her şey Vavis araçlarından geçer."
    } else {
        "Sana her istekte yalnızca o iş için gerekli görünen araçlar veriliyor. \
         İhtiyacın olan bir araç elinde yoksa uydurma: `request_tools` ile ne \
         yapmak istediğini yaz, ilgili araçlar bir sonraki adımda elinde olur."
    };
    format!(
        "Sen {assistant_name} adlı kişisel bir asistansın. Kullanıcının bilgisayarında \
         çalışıyorsun. {lang} konuş. Kısa, net ve doğrudan cevap ver — gereksiz \
         nezaket cümleleri kurma. Bilmediğin bir şeyi uydurma, bilmiyorum de.\n\n\
         {tools}"
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
            // OpenAI-shaped providers send no such state.
            provider_state: None,
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

/// One entry in GitHub's model catalog.
#[derive(Deserialize)]
struct GitHubModel {
    /// Qualified with the vendor: `openai/gpt-4.1`.
    id: String,
    #[serde(default)]
    supported_output_modalities: Vec<String>,
}

/// Gemini's own model list — a different shape from the OpenAI one.
#[derive(Deserialize)]
struct GeminiModelList {
    #[serde(default)]
    models: Vec<GeminiModelEntry>,
}

#[derive(Deserialize)]
struct GeminiModelEntry {
    /// Comes back qualified, e.g. `models/gemini-2.5-flash`.
    name: String,
    /// What this model can actually be asked to do. Absent on some entries,
    /// so a missing field is not read as "can do nothing".
    #[serde(default, rename = "supportedGenerationMethods")]
    supported_generation_methods: Vec<String>,
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
    fn every_interface_language_is_named_in_the_prompt() {
        assert!(system_prompt("Vavis", "de").contains("Deutsch"));
        assert!(system_prompt("Vavis", "fr").contains("Français"));
        assert!(system_prompt("Vavis", "es").contains("Español"));
    }

    #[test]
    fn claude_code_is_not_sent_looking_for_request_tools() {
        let p = system_prompt_for(Provider::ClaudeCode, "Vavis", "tr");
        assert!(!p.contains("request_tools"), "{p}");
        assert!(p.contains("mcp__vavis__"), "{p}");
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
    fn reasoning_models_get_the_token_field_and_temperature_they_accept() {
        let s = RequestShape::for_model(Provider::OpenAI, "gpt-5-mini");
        assert_eq!(s.token_field, "max_completion_tokens");
        assert!(!s.temperature);
        let s = RequestShape::for_model(Provider::OpenAI, "o3-mini");
        assert!(!s.temperature);
        let s = RequestShape::for_model(Provider::GitHub, "openai/gpt-5");
        assert_eq!(s.token_field, "max_completion_tokens");
        assert!(!s.temperature);
    }

    #[test]
    fn ordinary_models_keep_the_usual_body() {
        let s = RequestShape::for_model(Provider::Groq, "openai/gpt-oss-120b");
        assert_eq!(s.token_field, "max_tokens");
        assert!(s.temperature);
        let s = RequestShape::for_model(Provider::OpenAI, "gpt-4.1");
        assert!(s.temperature);
        // A model whose name merely starts with "o".
        let s = RequestShape::for_model(Provider::OpenRouter, "openrouter/optimus");
        assert!(s.temperature);
    }

    #[test]
    fn a_models_url_is_found_beside_the_chat_url() {
        assert_eq!(
            models_url_from_chat("http://localhost:1234/v1/chat/completions"),
            "http://localhost:1234/v1/models"
        );
        assert_eq!(
            models_url_from_chat("http://localhost:1234/v1/"),
            "http://localhost:1234/v1/models"
        );
    }

    #[test]
    fn a_pasted_base_url_becomes_a_chat_endpoint() {
        assert_eq!(
            chat_url_from_user("http://localhost:1234/v1"),
            "http://localhost:1234/v1/chat/completions"
        );
        assert_eq!(
            chat_url_from_user("https://x.ai/v1/chat/completions/"),
            "https://x.ai/v1/chat/completions"
        );
    }

    #[tokio::test]
    async fn a_custom_provider_without_a_url_fails_before_the_network() {
        let client = BrainClient::new();
        let cfg = ChatConfig::new(Provider::Custom, "m", "");
        let err = client
            .chat_stream(&cfg, vec![Message::user("selam")], |_| {})
            .await
            .unwrap_err();
        assert!(matches!(err, BrainError::Config(_)), "{err:?}");
    }

    #[test]
    fn conversation_token_estimate_grows_with_content() {
        let short = vec![Message::user("a")];
        let long = vec![Message::user("a".repeat(4000))];
        assert!(estimate_conversation_tokens(&long) > estimate_conversation_tokens(&short) + 500);
    }
}
