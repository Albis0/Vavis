//! LLM sağlayıcıları.
//!
//! Eski projede her sağlayıcı için ayrı fonksiyon vardı (ai-client.ts 720 satır).
//! Burada hepsi tek şekil: OpenAI-uyumlu uç nokta + anahtar. Farklı olan sadece
//! URL ve model listesi.
//!
//! Anthropic bilinçli olarak **yok** — farklı gövde şeması istiyor, F2'yi
//! şişirmemek için sonraya bırakıldı (F6).

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Groq,
    OpenAI,
    Gemini,
    Mistral,
    DeepSeek,
    XAI,
    /// NVIDIA NIM — build.nvidia.com. OpenAI uyumlu; ücretsiz katmanında
    /// Llama, Qwen, DeepSeek gibi modelleri barındırıyor.
    Nvidia,
    /// Claude — OpenAI-uyumlu DEGIL, ayri govde semasi (bkz. `anthropic` modulu).
    Anthropic,
    /// Yerel sunucu (Ollama / LM Studio) — anahtar istemez.
    Local,
}

impl Provider {
    pub const ALL: [Provider; 9] = [
        Self::Groq,
        Self::OpenAI,
        Self::Gemini,
        Self::Mistral,
        Self::DeepSeek,
        Self::XAI,
        Self::Nvidia,
        Self::Anthropic,
        Self::Local,
    ];

    /// Sohbet tamamlama uç noktası (OpenAI-uyumlu).
    pub fn chat_url(self) -> &'static str {
        match self {
            Self::Groq => "https://api.groq.com/openai/v1/chat/completions",
            Self::OpenAI => "https://api.openai.com/v1/chat/completions",
            // Kendi API'sine gidiyoruz, OpenAI-uyumlu kapıya değil. Model
            // adı yolda geçtiği için gerçek URL burada kurulamıyor —
            // `crate::gemini::chat_url(model)` kuruyor. Burası sadece
            // "iki sağlayıcı aynı URL'yi paylaşamaz" kuralı için bir taban.
            Self::Gemini => crate::gemini::API_BASE,
            Self::Mistral => "https://api.mistral.ai/v1/chat/completions",
            Self::DeepSeek => "https://api.deepseek.com/v1/chat/completions",
            Self::XAI => "https://api.x.ai/v1/chat/completions",
            Self::Nvidia => "https://integrate.api.nvidia.com/v1/chat/completions",
            Self::Anthropic => crate::anthropic::CHAT_URL,
            Self::Local => "http://127.0.0.1:11434/v1/chat/completions",
        }
    }

    /// Canlı model listesi uç noktası.
    pub fn models_url(self) -> &'static str {
        match self {
            Self::Groq => "https://api.groq.com/openai/v1/models",
            Self::OpenAI => "https://api.openai.com/v1/models",
            Self::Gemini => crate::gemini::MODELS_URL,
            Self::Mistral => "https://api.mistral.ai/v1/models",
            Self::DeepSeek => "https://api.deepseek.com/v1/models",
            Self::XAI => "https://api.x.ai/v1/models",
            Self::Nvidia => "https://integrate.api.nvidia.com/v1/models",
            Self::Anthropic => crate::anthropic::MODELS_URL,
            Self::Local => "http://127.0.0.1:11434/v1/models",
        }
    }

    /// Ayar dosyasındaki anahtar adı.
    pub fn key_name(self) -> &'static str {
        match self {
            Self::Groq => "groq",
            Self::OpenAI => "openai",
            Self::Gemini => "gemini",
            Self::Mistral => "mistral",
            Self::DeepSeek => "deepseek",
            Self::XAI => "xai",
            Self::Nvidia => "nvidia",
            Self::Anthropic => "anthropic",
            Self::Local => "local",
        }
    }

    pub fn needs_key(self) -> bool {
        !matches!(self, Self::Local)
    }

    /// Anahtar yokken kullanılacak makul varsayılan model.
    pub fn default_model(self) -> &'static str {
        match self {
            Self::Groq => "llama-3.3-70b-versatile",
            Self::OpenAI => "gpt-4o-mini",
            Self::Gemini => crate::gemini::DEFAULT_MODEL,
            Self::Mistral => "mistral-small-latest",
            Self::DeepSeek => "deepseek-chat",
            Self::XAI => "grok-3",
            Self::Nvidia => "meta/llama-3.3-70b-instruct",
            Self::Anthropic => crate::anthropic::DEFAULT_MODEL,
            Self::Local => "llama3.2",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "groq" => Some(Self::Groq),
            "openai" => Some(Self::OpenAI),
            "gemini" | "google" => Some(Self::Gemini),
            "mistral" => Some(Self::Mistral),
            "deepseek" => Some(Self::DeepSeek),
            "xai" | "grok" => Some(Self::XAI),
            "nvidia" | "nim" => Some(Self::Nvidia),
            "anthropic" | "claude" => Some(Self::Anthropic),
            "local" | "ollama" => Some(Self::Local),
            _ => None,
        }
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key_name())
    }
}

/// Sohbet dışı modelleri eler (whisper, embedding, tts…).
///
/// Eski `models.ts`'ten taşındı — kullanıcı "çok fazla gereksiz Gemini modeli
/// var" diye şikayet etmişti; gürültü burada kesiliyor.
pub fn is_chat_model(id: &str) -> bool {
    const NOISE: [&str; 16] = [
        "whisper",
        "tts",
        "embed",
        "moderation",
        "rerank",
        "dall-e",
        "image",
        "ocr",
        "aqa",
        "imagen",
        "learnlm",
        // Speech synthesis models Groq lists next to its chat models. They
        // answer a chat request with nonsense rather than an error, so the
        // name is the only thing that separates them.
        "orpheus",
        "playai",
        "canopylabs",
        // Guard and classifier models: a 512-token window and a one-word
        // answer. Named rather than matched on "guard", because that
        // substring also hides `gpt-oss-safeguard`, a full chat model.
        "prompt-guard",
        "llama-guard",
    ];
    let lower = id.to_ascii_lowercase();
    !NOISE.iter().any(|n| lower.contains(n))
}

/// Sağlayıcıya özel "işe yarar model" süzgeci.
///
/// Hiçbir şey kalmazsa çağıran taraf süzülmemiş listeye döner — boş liste
/// göstermek, gürültülü liste göstermekten kötüdür.
pub fn is_useful_model(provider: Provider, id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    if !is_chat_model(&lower) {
        return false;
    }
    match provider {
        Provider::Gemini => {
            // Emekli nesiller ve deneysel varyantlar elenir.
            if lower.contains("gemini-1.0") || lower.contains("gemini-1.5") {
                return false;
            }
            lower.contains("gemini-2") || lower.contains("gemini-3")
        }
        Provider::OpenAI => {
            if lower.contains("gpt-3.5") || lower.contains("davinci") || lower.contains("instruct")
            {
                return false;
            }
            lower.starts_with("gpt-4") || lower.starts_with("gpt-5") || lower.starts_with('o')
        }
        Provider::XAI => !lower.contains("grok-2") && !lower.contains("beta"),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_providers_have_distinct_urls() {
        let mut urls: Vec<&str> = Provider::ALL.iter().map(|p| p.chat_url()).collect();
        urls.sort_unstable();
        let before = urls.len();
        urls.dedup();
        assert_eq!(before, urls.len(), "iki sağlayıcı aynı URL'yi paylaşamaz");
    }

    /// Gemini's `chat_url()` is a base, not a usable endpoint: the model name
    /// belongs in the path, so the real URL is built per request by
    /// `crate::gemini::chat_url`. Posting to the base returns 404, and the
    /// failure would look like a bad key rather than a wrong URL -- so say it
    /// here, where someone reaching for `chat_url()` will see it.
    #[test]
    fn gemini_needs_its_url_built_per_model() {
        let base = Provider::Gemini.chat_url();
        assert!(
            !base.contains("generateContent"),
            "taban URL bir uç nokta gibi görünüyor: {base}"
        );
        let real = crate::gemini::chat_url(Provider::Gemini.default_model());
        assert!(real.starts_with(base), "{real} / {base}");
        assert!(real.contains(":streamGenerateContent"), "{real}");
    }

    /// The OpenAI-compatible door is deliberately no longer used.
    #[test]
    fn gemini_does_not_go_through_the_openai_compatible_door() {
        assert!(!Provider::Gemini.chat_url().contains("/openai/"));
        assert!(!Provider::Gemini.models_url().contains("/openai/"));
    }

    #[test]
    fn local_needs_no_key() {
        assert!(!Provider::Local.needs_key());
        assert!(Provider::Groq.needs_key());
    }

    #[test]
    fn parse_accepts_aliases() {
        assert_eq!(Provider::parse("GROQ"), Some(Provider::Groq));
        assert_eq!(Provider::parse("google"), Some(Provider::Gemini));
        assert_eq!(Provider::parse("ollama"), Some(Provider::Local));
        assert_eq!(Provider::parse("yok-böyle"), None);
    }

    #[test]
    fn noise_models_are_filtered() {
        assert!(!is_chat_model("whisper-large-v3"));
        assert!(!is_chat_model("text-embedding-3-small"));
        assert!(is_chat_model("llama-3.3-70b-versatile"));
    }

    /// Everything below was measured against Groq's live `/models` list on
    /// 2026-09-16 — these exact ids were being offered as chat models.
    #[test]
    fn groqs_speech_and_guard_models_are_not_offered_as_chat() {
        // Speech synthesis. Answers a chat request with nonsense rather than
        // an error, so nothing downstream catches the mistake.
        assert!(!is_chat_model("canopylabs/orpheus-v1-english"));
        assert!(!is_chat_model("canopylabs/orpheus-arabic-saudi"));
        // Classifiers with a 512-token window.
        assert!(!is_chat_model("meta-llama/llama-prompt-guard-2-22m"));
        assert!(!is_chat_model("meta-llama/llama-prompt-guard-2-86m"));
    }

    /// The old filter matched on "guard" and hid this one, which is a full
    /// chat model: 131k window, tools, structured outputs.
    #[test]
    fn a_safeguard_chat_model_is_not_mistaken_for_a_classifier() {
        assert!(is_chat_model("openai/gpt-oss-safeguard-20b"));
        assert!(is_useful_model(Provider::Groq, "openai/gpt-oss-safeguard-20b"));
    }

    #[test]
    fn gemini_filter_drops_retired_generations() {
        assert!(!is_useful_model(Provider::Gemini, "gemini-1.5-pro"));
        assert!(is_useful_model(Provider::Gemini, "gemini-2.5-flash"));
        assert!(is_useful_model(Provider::Gemini, "gemini-3.5-flash"));
    }

    #[test]
    fn every_provider_has_a_default_model() {
        for p in Provider::ALL {
            assert!(!p.default_model().is_empty(), "{p} varsayılansız");
        }
    }
}
