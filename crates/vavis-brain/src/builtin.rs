//! Sağlayıcının kendi sunucusunda çalıştırdığı araçlar.
//!
//! Bazı modeller araçları **kendileri** çalıştırıyor: web araması, kod
//! çalıştırma, sayfa okuma. Bunlar bizim araçlarımız değil; istek gövdesinde
//! ayrı bir şekilde bildiriliyorlar ve bizim şemalarımızla aynı `tools`
//! dizisini paylaşıyorlar.
//!
//! Buradaki asıl iş **bir şeyin gönderilmesini engellemek**: Groq'un Compound
//! sistemleri kendi araçlarından başkasını kabul etmiyor. Onlara tek bir şema
//! göndermek isteği tamamen düşürüyor — kısmi bozulma değil, 400.
//!
//! ```text
//! POST /openai/v1/chat/completions  model=groq/compound  tools=[bizimki]
//! -> 400 {"message": "`tool calling` is not supported with this model"}
//! ```
//!
//! Ölçüldü (2026-09-16, canlı API): `groq/compound` ve `groq/compound-mini`
//! tek bir fonksiyon şemasıyla bile 400 veriyor; aynı istek `tools` alanı
//! olmadan 200 dönüyor. `openai/gpt-oss-120b` ise yerleşik araçlarla bizim
//! şemalarımızı **aynı** dizide kabul ediyor. `qwen/qwen3.8-27b` yerleşik
//! tipleri reddediyor (`tools[0].type must be one of [function, mcp]`).
//!
//! Bu yüzden karar modele göre veriliyor, sağlayıcıya göre değil: Groq
//! kullanmak tek başına bir şey söylemiyor — hangi Groq modeli olduğu söylüyor.

use crate::provider::Provider;
use serde_json::{json, Value};

/// Bir modelin bizim araç şemalarımızla ne yapabildiği.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolSupport {
    /// Fonksiyon şemaları normal şekilde gönderilir.
    Functions,
    /// Model araçları **sunucu tarafında** kendi çalıştırıyor; bizimkiler
    /// gönderilemez. Gönderilirse istek tamamen reddedilir.
    ServerSideOnly,
}

impl ToolSupport {
    pub fn accepts_functions(self) -> bool {
        matches!(self, Self::Functions)
    }
}

/// Groq'un yerleşik araçları kendi çalıştırdığı sistemler.
///
/// Tam ad değil **önek** eşleşmesi: Groq bu ailelere sürüm etiketli adlar
/// veriyor (`groq/compound-mini`, ileride `groq/compound-2` gibi). Yanlış
/// tarafa düşmenin bedeli asimetrik — yanlışlıkla "araç yok" demek asistanı
/// sadece zayıflatır, yanlışlıkla "araç var" demek her isteği 400'e düşürür.
const GROQ_SERVER_SIDE: &[&str] = &["groq/compound"];

/// Bu model bizim araç şemalarımızı kabul eder mi?
pub fn tool_support(provider: Provider, model: &str) -> ToolSupport {
    let m = model.trim().to_ascii_lowercase();
    match provider {
        Provider::Groq if GROQ_SERVER_SIDE.iter().any(|p| m.starts_with(p)) => {
            ToolSupport::ServerSideOnly
        }
        _ => ToolSupport::Functions,
    }
}

/// Modelin kendi çalıştırdığı, isteğe **eklenecek** yerleşik araçlar.
///
/// Şu an sadece GPT-OSS ailesi: yerleşik araçları `tools` dizisinde tip
/// nesnesi olarak bildiriyor ve bizim fonksiyonlarımızla aynı dizide
/// yaşayabiliyorlar. Compound için boş — onun araçları `compound_custom`
/// altında ve zaten varsayılan olarak açık.
///
/// Neden sadece arama: kod çalıştırma (`code_interpreter`) bilerek **yok**.
/// Vavis'in kendi dosya ve kabuk araçları var; ikisini birden vermek modeli
/// aynı işi iki ayrı yerde yapmaya davet ediyor ve kullanıcı hangisinin
/// çalıştığını göremiyor. Web araması ise bizde sağlayıcı anahtarı istiyor,
/// yerleşik olan istemiyor — orada gerçek bir kazanç var.
pub fn extra_tools(provider: Provider, model: &str) -> Vec<Value> {
    let m = model.trim().to_ascii_lowercase();
    if provider == Provider::Groq && m.contains("gpt-oss") {
        return vec![json!({ "type": "browser_search" })];
    }
    Vec::new()
}

/// Sunucu tarafı araçların kapsadığı alanlar — bizim aynı işi yapan
/// araçlarımız gereksiz hâle gelir.
///
/// Şimdilik sadece web: yerleşik arama açıkken kendi arama aracımızı da
/// göndermek modele aynı iş için iki kapı açıyor.
pub fn covers_web_search(provider: Provider, model: &str) -> bool {
    !extra_tools(provider, model).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compound_refuses_our_schemas() {
        // Canlı API 400 veriyor; bu davranış koda gömülü olmalı.
        assert_eq!(
            tool_support(Provider::Groq, "groq/compound"),
            ToolSupport::ServerSideOnly
        );
        assert_eq!(
            tool_support(Provider::Groq, "groq/compound-mini"),
            ToolSupport::ServerSideOnly
        );
    }

    #[test]
    fn an_unreleased_compound_variant_is_still_refused() {
        // Önek eşleşmesinin sebebi: adı bilinmeyen bir sürüm çıktığında
        // güvenli tarafa düşsün.
        assert_eq!(
            tool_support(Provider::Groq, "groq/compound-3-preview"),
            ToolSupport::ServerSideOnly
        );
    }

    #[test]
    fn ordinary_groq_models_take_functions() {
        // Ölçüldü: qwen tool_calls dönüyor.
        assert!(tool_support(Provider::Groq, "qwen/qwen3.8-27b").accepts_functions());
        assert!(tool_support(Provider::Groq, "llama-3.3-70b-versatile").accepts_functions());
        assert!(tool_support(Provider::Groq, "openai/gpt-oss-120b").accepts_functions());
    }

    #[test]
    fn the_rule_is_groq_only() {
        // Başka bir sağlayıcıda "compound" adında bir model çıkarsa onu
        // sessizce araçsız bırakmayalım.
        assert!(tool_support(Provider::OpenAI, "groq/compound").accepts_functions());
        assert!(tool_support(Provider::Local, "compound").accepts_functions());
    }

    #[test]
    fn only_gpt_oss_gets_the_builtin_search() {
        assert_eq!(
            extra_tools(Provider::Groq, "openai/gpt-oss-120b"),
            vec![json!({ "type": "browser_search" })]
        );
        // Ölçüldü: qwen yerleşik tipleri reddediyor (400).
        assert!(extra_tools(Provider::Groq, "qwen/qwen3.8-27b").is_empty());
        // Compound'a dizi üzerinden hiçbir şey gönderilmiyor.
        assert!(extra_tools(Provider::Groq, "groq/compound").is_empty());
        // Aynı modeli başka sağlayıcıdan çekmek Groq'un alanını getirmez.
        assert!(extra_tools(Provider::Local, "gpt-oss-20b").is_empty());
    }

    #[test]
    fn the_builtin_search_replaces_our_own() {
        assert!(covers_web_search(Provider::Groq, "openai/gpt-oss-120b"));
        assert!(!covers_web_search(Provider::Groq, "qwen/qwen3.8-27b"));
    }
}
