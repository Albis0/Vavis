//! Text embeddings, for semantic memory.
//!
//! An embedding turns a sentence into a vector such that sentences meaning
//! similar things point in similar directions. Memory uses them to find
//! "espresso içer" when the user says "kahve" (see `vavis_core::memory`).
//!
//! Several providers offer them, and two do for free: Gemini and GitHub
//! Models. The first one with a key is used, unless the user chose.

use crate::client::{BrainError, Result};
use crate::provider::Provider;
use serde_json::{json, Value};
use std::time::Duration;

/// Vectors are asked for at this size where the model allows a choice.
/// 768 keeps a fact's vector at 3 KB with no measurable loss for short
/// sentences.
pub const DIMENSIONS: usize = 768;

/// Where embeddings come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbedConfig {
    pub provider: Provider,
    pub model: String,
    pub api_key: String,
    /// A chat URL the user set (custom or local). The embeddings endpoint
    /// sits beside it.
    pub url_override: Option<String>,
}

impl EmbedConfig {
    /// The identity stored beside each vector. Two vectors are only
    /// compared if they share it.
    pub fn id(&self) -> String {
        format!("{}:{}", self.provider.key_name(), self.model)
    }
}

/// Providers that serve embeddings, in the order `auto` tries them: free
/// first.
pub const PROVIDERS: [Provider; 5] = [
    Provider::Gemini,
    Provider::GitHub,
    Provider::OpenAI,
    Provider::Mistral,
    Provider::Local,
];

/// The embedding model a provider serves, if it serves one.
pub fn default_model(provider: Provider) -> Option<&'static str> {
    Some(match provider {
        Provider::Gemini => "gemini-embedding-001",
        Provider::GitHub => "openai/text-embedding-3-small",
        Provider::OpenAI => "text-embedding-3-small",
        Provider::Mistral => "mistral-embed",
        // Needs `ollama pull nomic-embed-text` first; a missing model
        // fails the request and memory falls back to words.
        Provider::Local => "nomic-embed-text",
        _ => return None,
    })
}

fn endpoint(cfg: &EmbedConfig) -> String {
    if let Some(chat) = &cfg.url_override {
        let chat = chat.trim_end_matches('/');
        let base = chat.strip_suffix("/chat/completions").unwrap_or(chat);
        return format!("{base}/embeddings");
    }
    match cfg.provider {
        Provider::Gemini => format!(
            "{}/models/{}:batchEmbedContents",
            crate::gemini::API_BASE,
            cfg.model
        ),
        Provider::GitHub => "https://models.github.ai/inference/embeddings".into(),
        Provider::OpenAI => "https://api.openai.com/v1/embeddings".into(),
        Provider::Mistral => "https://api.mistral.ai/v1/embeddings".into(),
        _ => "http://127.0.0.1:11434/v1/embeddings".into(),
    }
}

/// Embeds `texts`, one vector each, in order.
pub async fn embed(
    http: &reqwest::Client,
    cfg: &EmbedConfig,
    texts: &[String],
) -> Result<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    if cfg.provider.needs_key() && cfg.api_key.trim().is_empty() {
        return Err(BrainError::MissingKey {
            provider: cfg.provider,
        });
    }

    let url = endpoint(cfg);
    let gemini = cfg.provider == Provider::Gemini && cfg.url_override.is_none();
    let body = if gemini {
        json!({
            "requests": texts.iter().map(|t| json!({
                "model": format!("models/{}", cfg.model),
                "content": {"parts": [{"text": t}]},
                "outputDimensionality": DIMENSIONS,
            })).collect::<Vec<_>>()
        })
    } else {
        let mut b = json!({"model": cfg.model, "input": texts});
        // Only OpenAI's own v3 models take a size; others reject the field.
        if cfg.model.contains("text-embedding-3") {
            b["dimensions"] = json!(DIMENSIONS);
        }
        b
    };

    let mut req = http.post(&url).timeout(Duration::from_secs(20)).json(&body);
    req = if gemini {
        req.header(crate::gemini::KEY_HEADER, &cfg.api_key)
    } else if !cfg.api_key.trim().is_empty() && cfg.provider != Provider::Local {
        req.bearer_auth(&cfg.api_key)
    } else {
        req
    };

    let resp = req.send().await?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(BrainError::Api {
            status: status.as_u16(),
            body: body.chars().take(300).collect(),
        });
    }
    let v: Value = resp
        .json()
        .await
        .map_err(|e| BrainError::Parse(e.to_string()))?;
    let vectors = parse(&v, gemini);
    if vectors.len() != texts.len() {
        return Err(BrainError::Parse(format!(
            "asked for {} embeddings, got {}",
            texts.len(),
            vectors.len()
        )));
    }
    Ok(vectors)
}

/// Pulls vectors out of either response shape.
fn parse(v: &Value, gemini: bool) -> Vec<Vec<f32>> {
    let floats = |arr: &Value| -> Vec<f32> {
        arr.as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_f64())
                    .map(|x| x as f32)
                    .collect()
            })
            .unwrap_or_default()
    };
    if gemini {
        v["embeddings"]
            .as_array()
            .map(|a| a.iter().map(|e| floats(&e["values"])).collect())
            .unwrap_or_default()
    } else {
        let mut data: Vec<&Value> = v["data"]
            .as_array()
            .map(|a| a.iter().collect())
            .unwrap_or_default();
        // The spec returns them in order with an index; sort anyway.
        data.sort_by_key(|d| d["index"].as_u64().unwrap_or(0));
        data.into_iter().map(|d| floats(&d["embedding"])).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(p: Provider) -> EmbedConfig {
        EmbedConfig {
            provider: p,
            model: default_model(p).unwrap().into(),
            api_key: "k".into(),
            url_override: None,
        }
    }

    #[test]
    fn every_listed_provider_has_a_model() {
        for p in PROVIDERS {
            assert!(default_model(p).is_some(), "{p}");
        }
        assert!(default_model(Provider::Groq).is_none());
    }

    #[test]
    fn gemini_uses_its_batch_endpoint() {
        let url = endpoint(&cfg(Provider::Gemini));
        assert!(
            url.ends_with("models/gemini-embedding-001:batchEmbedContents"),
            "{url}"
        );
    }

    #[test]
    fn a_custom_url_puts_embeddings_beside_chat() {
        let mut c = cfg(Provider::Local);
        c.url_override = Some("http://127.0.0.1:1234/v1/chat/completions".into());
        assert_eq!(endpoint(&c), "http://127.0.0.1:1234/v1/embeddings");
    }

    #[test]
    fn both_response_shapes_parse() {
        let g = json!({"embeddings": [{"values": [0.1, 0.2]}, {"values": [0.3]}]});
        assert_eq!(parse(&g, true), vec![vec![0.1, 0.2], vec![0.3]]);
        let o = json!({"data": [
            {"index": 1, "embedding": [0.5]},
            {"index": 0, "embedding": [0.4]}
        ]});
        assert_eq!(parse(&o, false), vec![vec![0.4], vec![0.5]]);
    }

    #[test]
    fn the_id_names_provider_and_model() {
        assert_eq!(cfg(Provider::Gemini).id(), "gemini:gemini-embedding-001");
    }
}
