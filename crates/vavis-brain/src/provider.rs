//! LLM sağlayıcıları.
//!
//! Eski projede her sağlayıcı için ayrı fonksiyon vardı (ai-client.ts 720 satır).
//! Burada hepsi tek şekil: OpenAI-uyumlu uç nokta + anahtar. Farklı olan sadece
//! URL ve model listesi.
//!
//! Üç istisna kendi yoluna sahip: Anthropic ve Gemini'nin gövde şemaları
//! farklı, Claude Code ise bir HTTP uç noktası değil, kullanıcının
//! bilgisayarındaki bir program (bkz. `crate::claude_code`).

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
    /// Claude through the Claude Code CLI, on the user's own subscription.
    /// No key: the CLI holds the login. See `crate::claude_code`.
    #[serde(rename = "claude-code")]
    ClaudeCode,
    /// OpenRouter — one key, hundreds of models, and a set of `:free` ones
    /// that cost nothing at all.
    OpenRouter,
    /// Cerebras — a free tier measured in a million tokens a day, and the
    /// fastest inference of the lot.
    Cerebras,
    /// GitHub Models — free with any GitHub account, using a personal access
    /// token. Small daily limits, but real frontier models.
    #[serde(rename = "github")]
    GitHub,
    /// Any OpenAI-compatible endpoint the user names (`llm.custom_url`).
    Custom,
    /// Yerel sunucu (Ollama / LM Studio) — anahtar istemez.
    Local,
}

impl Provider {
    /// In the order the settings screen lists them: the ones that cost
    /// nothing first, since that is where most people start.
    pub const ALL: [Provider; 14] = [
        Self::ClaudeCode,
        Self::Gemini,
        Self::Groq,
        Self::Cerebras,
        Self::OpenRouter,
        Self::GitHub,
        Self::Mistral,
        Self::Nvidia,
        Self::OpenAI,
        Self::Anthropic,
        Self::DeepSeek,
        Self::XAI,
        Self::Custom,
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
            Self::ClaudeCode => crate::claude_code::PSEUDO_URL,
            Self::OpenRouter => "https://openrouter.ai/api/v1/chat/completions",
            Self::Cerebras => "https://api.cerebras.ai/v1/chat/completions",
            Self::GitHub => "https://models.github.ai/inference/chat/completions",
            // No default: the user supplies it, and a request with no URL
            // fails before it leaves (see `ChatConfig::chat_url`).
            Self::Custom => "custom://unset",
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
            Self::ClaudeCode => crate::claude_code::PSEUDO_URL,
            Self::OpenRouter => "https://openrouter.ai/api/v1/models",
            Self::Cerebras => "https://api.cerebras.ai/v1/models",
            Self::GitHub => "https://models.github.ai/catalog/models",
            Self::Custom => "custom://unset",
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
            Self::ClaudeCode => "claude-code",
            Self::OpenRouter => "openrouter",
            Self::Cerebras => "cerebras",
            Self::GitHub => "github",
            Self::Custom => "custom",
            Self::Local => "local",
        }
    }

    /// Whether a request cannot go out without a stored key.
    ///
    /// A custom endpoint may or may not want one -- a self-hosted proxy often
    /// does not -- so it is sent when present and never demanded.
    pub fn needs_key(self) -> bool {
        !matches!(self, Self::Local | Self::ClaudeCode | Self::Custom)
    }

    /// Whether the settings screen should offer a key field at all.
    pub fn takes_key(self) -> bool {
        !matches!(self, Self::Local | Self::ClaudeCode)
    }

    /// Whether this provider is free to use without paying anyone -- a free
    /// tier, free models, or hardware the user already owns. Shown as a tag,
    /// because for a lot of people it is the first thing they filter on.
    pub fn has_free_tier(self) -> bool {
        matches!(
            self,
            Self::Gemini
                | Self::Groq
                | Self::Cerebras
                | Self::OpenRouter
                | Self::GitHub
                | Self::Mistral
                | Self::Nvidia
                | Self::Local
        )
    }

    /// Anahtar yokken kullanılacak makul varsayılan model.
    pub fn default_model(self) -> &'static str {
        match self {
            // gpt-oss-120b rather than Llama 3.3: same free tier, far better
            // at tools and reasoning, and Groq serves its browser search.
            Self::Groq => "openai/gpt-oss-120b",
            Self::OpenAI => "gpt-5-mini",
            Self::Gemini => crate::gemini::DEFAULT_MODEL,
            Self::Mistral => "mistral-small-latest",
            Self::DeepSeek => "deepseek-chat",
            Self::XAI => "grok-4",
            Self::Nvidia => "meta/llama-3.3-70b-instruct",
            Self::Anthropic => crate::anthropic::DEFAULT_MODEL,
            Self::ClaudeCode => crate::claude_code::DEFAULT_MODEL,
            // A `:free` model, so a new key works before any credit is
            // bought. The live list offers the rest.
            Self::OpenRouter => "meta-llama/llama-3.3-70b-instruct:free",
            Self::Cerebras => "gpt-oss-120b",
            Self::GitHub => "openai/gpt-4.1",
            // Whatever the endpoint serves; the user names it in settings.
            Self::Custom => "default",
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
            "claude-code" | "claudecode" | "claude_code" | "cli" => Some(Self::ClaudeCode),
            "openrouter" => Some(Self::OpenRouter),
            "cerebras" => Some(Self::Cerebras),
            "github" | "github-models" => Some(Self::GitHub),
            "custom" | "openai-compatible" => Some(Self::Custom),
            "local" | "ollama" | "lmstudio" => Some(Self::Local),
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
    const NOISE: [&str; 18] = [
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
        // Models that only speak over a WebSocket (`bidiGenerateContent`).
        // The capability filter in `client::list_models` already drops these,
        // but that filter trusts a field the provider might rename; this is
        // the name-level backstop. Sending one a chat request earns:
        //
        //   only supports real-time bidirectional streaming via WebSocket
        //   (bidiGenerateContent). Please use the Gemini Live API
        //
        // `-live` and not `live`, so `learnlm-live` style names are caught
        // without also hiding a model that merely has "live" inside a word.
        "native-audio",
        "-live",
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
            // Emekli nesiller elenir.
            //
            // `gemini-2` artık burada: Google 2.5 ailesini emekli etti ama
            // `/models` listesinden **çıkarmadı**. Listede duran üç model
            // (`2.5-flash`, `2.5-pro`, `2.5-flash-lite`) kullanılınca 404
            // veriyor ve mesajında yerine geçecek modeli söylüyor.
            // Ölçüldü, 2026-09-16, canlı API.
            //
            // Ders: "listede var" ile "çalışıyor" aynı şey değil.
            if lower.contains("gemini-1.0")
                || lower.contains("gemini-1.5")
                || lower.contains("gemini-2.0")
                || lower.contains("gemini-2.5")
            {
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
        // OpenRouter lists image, audio and embedding models among the chat
        // ones; the name filter above has already dropped most. What is left
        // is the user's call.
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
    fn claude_code_needs_no_key_and_offers_no_key_field() {
        assert!(!Provider::ClaudeCode.needs_key());
        assert!(!Provider::ClaudeCode.takes_key());
    }

    #[test]
    fn a_custom_endpoint_takes_a_key_without_demanding_one() {
        assert!(!Provider::Custom.needs_key());
        assert!(Provider::Custom.takes_key());
    }

    #[test]
    fn every_provider_round_trips_through_its_key_name() {
        for p in Provider::ALL {
            assert_eq!(Provider::parse(p.key_name()), Some(p), "{p}");
        }
    }

    #[test]
    fn serde_spells_every_provider_the_way_the_config_does() {
        for p in Provider::ALL {
            let json = serde_json::to_string(&p).unwrap();
            assert_eq!(json, format!("\"{}\"", p.key_name()), "{p}");
        }
    }

    #[test]
    fn the_free_providers_come_first() {
        let first_paid = Provider::ALL
            .iter()
            .position(|p| !p.has_free_tier() && *p != Provider::ClaudeCode)
            .unwrap();
        assert!(Provider::ALL[first_paid..]
            .iter()
            .filter(|p| **p != Provider::Local)
            .all(|p| !p.has_free_tier()));
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
        assert!(is_useful_model(
            Provider::Groq,
            "openai/gpt-oss-safeguard-20b"
        ));
    }

    #[test]
    fn gemini_filter_drops_retired_generations() {
        assert!(!is_useful_model(Provider::Gemini, "gemini-1.5-pro"));
        assert!(is_useful_model(Provider::Gemini, "gemini-3.5-flash"));
        assert!(is_useful_model(Provider::Gemini, "gemini-3.6-flash"));
    }

    /// Google retired the 2.5 family but left it in `/models`.
    ///
    /// Measured 2026-09-16 against the live API: each of these is listed and
    /// each returns 404 when used —
    ///
    /// ```text
    /// This model models/gemini-2.5-flash is no longer available to new
    /// users. Please update your code to use models/gemini-3.6-flash
    /// ```
    ///
    /// Being in the list is not the same as working, so the list alone
    /// cannot be trusted to say what a user may pick.
    #[test]
    fn gemini_models_that_are_listed_but_retired_are_not_offered() {
        for dead in [
            "gemini-2.5-flash",
            "gemini-2.5-pro",
            "gemini-2.5-flash-lite",
            "gemini-2.0-flash",
        ] {
            assert!(
                !is_useful_model(Provider::Gemini, dead),
                "{dead} emekli ama listede bırakılmış"
            );
        }
    }

    /// The default must be a model that actually answers.
    #[test]
    fn the_gemini_default_is_a_model_we_would_offer() {
        let default = Provider::Gemini.default_model();
        assert!(
            is_useful_model(Provider::Gemini, default),
            "varsayılan kendi süzgecimizden geçmiyor: {default}"
        );
    }

    /// WebSocket-only models answer a chat request with an error telling you
    /// to use the Live API. The capability filter drops them first; this is
    /// the name-level backstop for when that field is missing or renamed.
    #[test]
    fn websocket_only_models_are_not_chat_models() {
        for live in [
            "gemini-2.5-flash-native-audio-latest",
            "gemini-2.5-flash-native-audio-preview-12-2025",
            "gemini-3.8-live",
            "gemini-3.8-live-extended-thinking",
            "gemini-3.5-transcribe-live",
            "gemini-3.1-flash-live-preview",
        ] {
            assert!(!is_chat_model(live), "{live} sohbet modeli sayıldı");
            assert!(!is_useful_model(Provider::Gemini, live), "{live}");
        }
    }

    /// And the backstop must not be so wide that it hides working models.
    /// These all answered a real request on 2026-09-16.
    #[test]
    fn the_live_backstop_does_not_hide_working_models() {
        for ok in [
            "gemini-3.6-flash",
            "gemini-3.5-flash",
            "gemini-3.5-flash-lite",
            "gemini-3.1-flash-lite",
            "gemini-3-flash-preview",
            // Not a live model despite the name -- it answers generateContent.
            "gemini-3.5-transcribe",
        ] {
            assert!(is_useful_model(Provider::Gemini, ok), "{ok} elendi");
        }
    }

    #[test]
    fn every_provider_has_a_default_model() {
        for p in Provider::ALL {
            assert!(!p.default_model().is_empty(), "{p} varsayılansız");
        }
    }
}
