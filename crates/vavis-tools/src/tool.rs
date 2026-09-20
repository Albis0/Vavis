//! Tool arayüzü.
//!
//! Eski projede tool'lar tek dosyada 2.533 satırdı ve şemalar ayrı bir 1.507
//! satırlık dosyada duruyordu — tanım ile uygulama birbirinden kopuktu, biri
//! değişince diğeri unutuluyordu.
//!
//! Burada **tanım ve uygulama aynı yerde**: her tool kendi şemasını da üretir.

use serde_json::Value;
use std::collections::BTreeMap;

/// Bir tool'un ait olduğu alan. Tool seçiminin temeli.
///
/// Modele **sadece ilgili alanın** tool'ları gönderilir — böylece 64 değil,
/// bir avuç seçenek görür.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Domain {
    /// Her zaman sunulan çekirdek (saat, hatırlatma…).
    Core,
    /// Dosya okuma/yazma/listeleme.
    Files,
    /// Sistem durumu **okuma** — CPU, RAM, pil, süreçler, pencereler.
    System,
    /// Sistem **değiştirme** — ses, parlaklık, uygulama açma/kapatma,
    /// komut çalıştırma, pano.
    ///
    /// Okumadan ayrı tutuluyor: "cpu durumu nasıl" sorusuna `run_command`
    /// sunmak hem gereksiz hem riskli. Ayrıca tek alanda 11 tool birikince
    /// çekirdek tool'lar sınırın dışına itiliyordu.
    Control,
    /// Ekran görüntüsü ve bilgisayar kullanımı.
    Vision,
    /// Web arama, sayfa okuma.
    Web,
    /// Medya — müzik/video oynatma kontrolü.
    Media,
    /// Otomasyon — zamanlanmış görevler, koşullu tetikleyiciler.
    Automation,
    /// Hafıza — hatırla/unut/ara.
    Memory,
    /// Obsidian kasası — not okuma/yazma/arama.
    Obsidian,
    /// Steam — kütüphane, oyun başlatma, mağaza.
    Steam,
    /// Spotify — çalma kontrolü, katalog arama.
    Spotify,
    /// Canvas — görsel üretimi.
    Canvas,
    /// VirusTotal — dosya ve adres itibar sorgusu.
    Security,
    /// Bir MCP sunucusu. **Her sunucu kendi alanı**: üç sunucu bağlayan
    /// kullanıcı 60 tool'a ulaşabilir ve hepsini modele göndermek eski
    /// projedeki 353 tool felaketinin aynısı olur.
    ///
    /// Ad çalışma anında bilindiği için havuzdan geliyor — bkz. `mcp::intern`.
    Mcp(&'static str),
}

impl Domain {
    pub const ALL: [Domain; 14] = [
        Self::Core,
        Self::Files,
        Self::System,
        Self::Control,
        Self::Vision,
        Self::Web,
        Self::Media,
        Self::Automation,
        Self::Memory,
        Self::Obsidian,
        Self::Steam,
        Self::Spotify,
        Self::Canvas,
        Self::Security,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Files => "files",
            Self::System => "system",
            Self::Control => "control",
            Self::Vision => "vision",
            Self::Web => "web",
            Self::Media => "media",
            Self::Automation => "automation",
            Self::Memory => "memory",
            Self::Obsidian => "obsidian",
            Self::Steam => "steam",
            Self::Spotify => "spotify",
            Self::Canvas => "canvas",
            Self::Security => "security",
            Self::Mcp(id) => id,
        }
    }
}

/// Bir tool'un tehlike seviyesi — izin kapısı buna bakar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Risk {
    /// Okuma, sorgu — onay gerekmez.
    Safe,
    /// Sistem durumunu değiştirir ama geri alınabilir (ses seviyesi…).
    Moderate,
    /// Yıkıcı veya geri alınamaz — **her zaman onay** (dosya silme, komut çalıştırma).
    Destructive,
}

/// Tool çalıştırma sonucu.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutcome {
    pub ok: bool,
    /// Modele geri verilecek metin.
    pub content: String,
}

impl ToolOutcome {
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            ok: true,
            content: content.into(),
        }
    }

    pub fn err(content: impl Into<String>) -> Self {
        Self {
            ok: false,
            content: content.into(),
        }
    }
}

/// Tek bir parametre tanımı.
#[derive(Debug, Clone)]
pub struct Param {
    pub name: &'static str,
    pub description: &'static str,
    pub required: bool,
}

impl Param {
    pub fn required(name: &'static str, description: &'static str) -> Self {
        Self {
            name,
            description,
            required: true,
        }
    }
    pub fn optional(name: &'static str, description: &'static str) -> Self {
        Self {
            name,
            description,
            required: false,
        }
    }
}

/// Bir yetenek.
///
/// `async_trait` kullanmıyoruz — tool'lar senkron çalışıp bloklayan işi
/// `spawn_blocking`'e devrediyor. Bu, trait nesnesini basit tutuyor.
pub trait Tool: Send + Sync {
    /// Modelin göreceği ad. Kısa ve fiil gibi olmalı.
    fn name(&self) -> &'static str;

    /// Modelin ne zaman çağıracağına karar vereceği açıklama.
    /// **Kısa tut** — her tool'un açıklaması token yer.
    fn description(&self) -> &'static str;

    fn domain(&self) -> Domain;

    fn risk(&self) -> Risk {
        Risk::Safe
    }

    fn params(&self) -> Vec<Param> {
        Vec::new()
    }

    /// Tool'u çalıştır. `args` modelin verdiği JSON nesnesi.
    fn run(&self, args: &Value) -> ToolOutcome;

    /// Bu tool'un tetiklendiği Türkçe/İngilizce anahtar kelimeler.
    /// Alan seçimi bunları kullanır.
    fn keywords(&self) -> &'static [&'static str] {
        &[]
    }

    /// OpenAI tool şeması.
    ///
    /// Sayısal parametreler bilinçli olarak "string" — sağlayıcılar sayı
    /// beklerken string alınca hata veriyor (eski projede öğrenilmiş ders).
    fn schema(&self) -> Value {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for p in self.params() {
            properties.insert(
                p.name.to_string(),
                serde_json::json!({
                    "type": "string",
                    "description": p.description,
                }),
            );
            if p.required {
                required.push(Value::String(p.name.to_string()));
            }
        }

        serde_json::json!({
            "type": "function",
            "function": {
                "name": self.name(),
                "description": self.description(),
                "parameters": {
                    "type": "object",
                    "properties": Value::Object(properties),
                    "required": Value::Array(required),
                    "additionalProperties": false,
                }
            }
        })
    }
}

/// Argümanlardan string okuma yardımcıları.
pub fn arg_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// Sayısal argüman — model string de gönderebilir, ikisini de kabul et.
pub fn arg_num(args: &Value, key: &str) -> Option<f64> {
    let v = args.get(key)?;
    v.as_f64().or_else(|| v.as_str()?.trim().parse().ok())
}

/// Tool kayıt defteri.
pub struct Registry {
    tools: BTreeMap<&'static str, Box<dyn Tool>>,
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

impl Registry {
    pub fn new() -> Self {
        Self {
            tools: BTreeMap::new(),
        }
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        let name = tool.name();
        if self.tools.insert(name, tool).is_some() {
            // Aynı adla iki tool = model hangisini çağıracağını bilemez.
            panic!("tool adı çakışması: {name}");
        }
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(AsRef::as_ref)
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &dyn Tool> {
        self.tools.values().map(AsRef::as_ref)
    }

    pub fn in_domain(&self, domain: Domain) -> impl Iterator<Item = &dyn Tool> {
        self.iter().filter(move |t| t.domain() == domain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dummy;
    impl Tool for Dummy {
        fn name(&self) -> &'static str {
            "dummy"
        }
        fn description(&self) -> &'static str {
            "test tool"
        }
        fn domain(&self) -> Domain {
            Domain::Core
        }
        fn params(&self) -> Vec<Param> {
            vec![
                Param::required("a", "zorunlu"),
                Param::optional("b", "isteğe bağlı"),
            ]
        }
        fn run(&self, _args: &Value) -> ToolOutcome {
            ToolOutcome::ok("çalıştı")
        }
    }

    #[test]
    fn schema_marks_required_params_only() {
        let schema = Dummy.schema();
        let required = schema["function"]["parameters"]["required"]
            .as_array()
            .unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "a");
    }

    #[test]
    fn schema_uses_string_type_for_all_params() {
        // Sağlayıcılar sayı tipinde string alınca patlıyor — hepsi string.
        let schema = Dummy.schema();
        let props = &schema["function"]["parameters"]["properties"];
        assert_eq!(props["a"]["type"], "string");
    }

    #[test]
    fn registry_finds_registered_tool() {
        let mut reg = Registry::new();
        reg.register(Box::new(Dummy));
        assert_eq!(reg.len(), 1);
        assert!(reg.get("dummy").is_some());
        assert!(reg.get("yok").is_none());
    }

    #[test]
    #[should_panic(expected = "tool adı çakışması")]
    fn duplicate_tool_name_panics_loudly() {
        let mut reg = Registry::new();
        reg.register(Box::new(Dummy));
        reg.register(Box::new(Dummy));
    }

    #[test]
    fn arg_num_accepts_both_string_and_number() {
        let args = serde_json::json!({"a": "30", "b": 40});
        assert_eq!(arg_num(&args, "a"), Some(30.0));
        assert_eq!(arg_num(&args, "b"), Some(40.0));
        assert_eq!(arg_num(&args, "yok"), None);
    }

    #[test]
    fn arg_str_rejects_blank_values() {
        let args = serde_json::json!({"a": "  ", "b": " değer "});
        assert_eq!(arg_str(&args, "a"), None, "boşluk = değer yok");
        assert_eq!(arg_str(&args, "b"), Some("değer"));
    }
}
