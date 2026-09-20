//! Ayarlar (TOML).
//!
//! Tasarım kararları:
//! 1. **Her alanın varsayılanı var.** Eksik alan hata değil — eski sürümden
//!    gelen dosya yeni alanlar eklendiğinde de okunabilir kalır.
//! 2. **Bozuk dosya app'i öldürmez.** `load_or_default` bozuk dosyayı yedekleyip
//!    varsayılanla devam eder. (Eski projede bozuk ayar açılışı engelliyordu.)
//! 3. **Atomik yazma.** Önce `.tmp`'ye yaz, sonra taşı — yazma sırasında elektrik
//!    giderse ayar dosyası yarım kalmaz.

use crate::error::{CoreError, Result};
use crate::paths::Paths;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[derive(Default)]
pub struct Config {
    pub general: General,
    pub ui: Ui,
    pub security: Security,
    pub voice: Voice,
    pub llm: Llm,
    pub search: Search,
    pub canvas: Canvas,
    pub obsidian: Obsidian,
    pub steam: Steam,
    pub spotify: Spotify,
    pub mcp: Mcp,
}

/// Güvenlik tercihleri.
///
/// Varsayılan korumalı, tavan açık: isteyen makinesinin tamamını verebilmeli,
/// ama bunu bilerek seçmiş olmalı.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Security {
    /// Tam yetki — hiçbir onay sorulmuyor, hiçbir bütçe uygulanmıyor.
    ///
    /// `Default` ile `false` geliyor ve bu bilinçli: ayar dosyasını elle
    /// yazan ya da eski bir dosyadan yükseltilen kullanıcı, istemediği bir
    /// şeyin açık olduğunu bulmasın.
    pub full_authority: bool,
}

/// Konuşma (TTS) ayarları.
///
/// Anahtarlar **burada değil**: onlar şifreli depoda (DPAPI) duruyor ve ayar
/// dosyasına hiç yazılmıyor. Burada yalnızca hangi motorun seçildiği ve
/// hangi sesin isteneceği var.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Voice {
    /// Motor kimliği: sapi | edge | kokoro | elevenlabs | openai.
    pub engine: String,
    /// Konuşma hızı (-10 … +10).
    pub rate: i32,
    /// Ses yüksekliği (0-100).
    pub volume: u32,
    /// SAPI ses adı — boşsa sistem varsayılanı.
    pub sapi_voice: String,
    pub edge_voice: String,
    /// Kokoro sunucusunun adresi. Kullanıcı kendi başlatıyor.
    pub kokoro_url: String,
    pub kokoro_voice: String,
    pub eleven_voice: String,
    pub eleven_model: String,
    pub openai_voice: String,
    pub openai_model: String,
    pub gemini_voice: String,
    pub gemini_model: String,
    /// Sohbet edilen sağlayıcının kendi sesi varsa onu kullan.
    ///
    /// Gemini ile konuşurken Google'ın sesini duymak beklenen şey; başka bir
    /// firmanın motoruyla konuşmak değil. Açıkken sadece **anahtar isteyen**
    /// bir motor seçilmişse devreye giriyor — SAPI ya da Edge seçen biri
    /// yerel ve ücretsiz olanı seçmiş demektir, onu sessizce ücretli bir
    /// buluta taşımak kotasını habersiz harcar.
    ///
    /// Kapatınca seçilen motor neyse o kalıyor.
    pub match_provider: bool,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            // Anahtar istemeyen **ve** kullanıcının dilini konuşan motor.
            // Eskiden "sapi"ydi; SAPI de anahtar istemiyor ama çoğu Windows
            // kurulumunda yalnızca İngilizce ses yüklü, ve Türkçe metin o
            // sesle okununca anlaşılmıyor. Edge ulaşılamazsa zincir zaten
            // SAPI'ye düşüyor.
            engine: "edge".to_string(),
            rate: 1,
            volume: 100,
            sapi_voice: String::new(),
            edge_voice: String::new(),
            kokoro_url: String::new(),
            kokoro_voice: String::new(),
            eleven_voice: String::new(),
            eleven_model: String::new(),
            openai_voice: String::new(),
            openai_model: String::new(),
            gemini_voice: String::new(),
            gemini_model: String::new(),
            // Açık: bir sağlayıcı seçtiğinde onun sesini duymak beklenen
            // davranış. Zaten yalnızca anahtar isteyen bir motor seçilmişse
            // devreye giriyor, yani kimseyi habersiz ücretli bir motora
            // taşımıyor.
            match_provider: true,
        }
    }
}

/// MCP sunucuları.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Mcp {
    pub servers: Vec<McpServer>,
}

/// Kullanıcının tanımladığı bir MCP sunucusu.
///
/// **Bu tanım rastgele kod çalıştırıyor** — `command` kullanıcının
/// makinesinde, Vavis'in başlattığı bir süreç oluyor. Sunucu eklerken ne
/// çalıştırılacağı arayüzde açıkça gösteriliyor.
///
/// Sırlar burada değil: `env` değerlerindeki ve `header_value` içindeki
/// `{key}` yer tutucusu, şifreli `keys.dat`'te `mcp_<id>` adıyla duran
/// değerle doldurulur.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct McpServer {
    /// Kısa kimlik; tool adlarının öneki ve tool seçim alanı olur.
    pub id: String,
    /// "stdio" | "http"
    pub transport: String,
    /// stdio: çalıştırılacak komut.
    pub command: String,
    pub args: Vec<String>,
    /// stdio: ortam değişkenleri (ad, değer). Değer `{key}` içerebilir.
    pub env: Vec<(String, String)>,
    /// http: sunucu adresi.
    pub url: String,
    pub header_name: String,
    /// `{key}` yer tutucusu saklanan sırla doldurulur.
    pub header_value: String,
    /// Kullanıcının kapattığı tool adları.
    pub disabled: Vec<String>,
    pub enabled: bool,
}

/// Spotify.
///
/// No client secret: the flow is PKCE, which exists because a desktop app
/// cannot keep one. Tokens do not live here — they go in the encrypted
/// `keys.dat` under `spotify_token`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Spotify {
    /// A client id of the user's own, from their Spotify developer dashboard.
    /// Optional, and empty for almost everyone: empty means the application's
    /// built-in id, which is what makes connecting one click.
    pub client_id: String,
}

/// Steam.
///
/// API anahtarı burada değil, şifreli `keys.dat` içinde `steam` adıyla durur.
/// SteamID gizli bilgi değil, ayar dosyasında kalabilir.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Steam {
    /// SteamID64 — profil adresindeki 17 haneli sayı.
    pub steam_id: String,
}

/// Obsidian kasası.
///
/// Yol boşsa Obsidian'ın kendi `obsidian.json`'ından en son açılan kasa
/// seçilir — kullanıcı hiçbir şey yapmadan çalışsın diye.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Obsidian {
    /// Kasa klasörünün mutlak yolu.
    pub vault: String,
}

/// Image and video generation.
///
/// The same reasoning as [`Search`]: image services change fast, so nothing is
/// hard-wired to one of them. The user picks the order and pastes their own
/// key; a provider without a key is skipped at runtime.
///
/// Keys are not here. They live in the encrypted key store under
/// `canvas_<provider>`, like every other secret.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Canvas {
    /// Image provider ids, most preferred first.
    pub image_order: Vec<String>,
    /// Video provider ids, most preferred first.
    pub video_order: Vec<String>,
    /// Empty means "whatever the provider defaults to", so a new model at the
    /// provider does not need a Vavis release.
    pub image_model: String,
    pub video_model: String,
    /// Default size for new images, `WIDTHxHEIGHT`.
    pub size: String,
    /// How many images one request produces by default.
    pub count: u32,
    /// The user's own endpoint — a self-hosted or aggregator service.
    pub custom: CustomCanvas,
}

/// A user-defined image endpoint.
///
/// It has to speak the OpenAI `/images/generations` shape. That is not a
/// limitation in practice: every self-hosted stack and every aggregator
/// exposes it, and the alternative is asking the user to describe a JSON
/// request body in a settings form.
///
/// The key is not here — it lives in the encrypted store as `canvas_custom`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CustomCanvas {
    /// Full endpoint, e.g. `http://localhost:8080/v1/images/generations`.
    pub url: String,
    /// Authentication header, e.g. `Authorization`.
    pub header_name: String,
    /// Header value; a `{key}` placeholder is filled from the key store.
    pub header_value: String,
    pub model: String,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            image_order: ["openai", "stability", "replicate", "custom"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            video_order: ["replicate", "custom"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            image_model: String::new(),
            video_model: String::new(),
            size: "1024x1024".to_string(),
            count: 1,
            custom: CustomCanvas::default(),
        }
    }
}

/// Web arama zinciri.
///
/// Sıra kullanıcıya ait — ayarlardan sürüklenerek değiştirilir. Anahtarı
/// olmayan sağlayıcı çalışma anında atlanır, bu yüzden varsayılan sıra
/// hepsini içerebilir: anahtar girilmemişse geriye DuckDuckGo kalır.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Search {
    /// Sağlayıcı kimlikleri, tercih sırasıyla.
    pub order: Vec<String>,
    /// Kullanıcının kendi JSON arama uç noktası (self-hosted Searx vb.).
    pub custom: CustomSearch,
}

impl Default for Search {
    fn default() -> Self {
        Self {
            order: ["tavily", "brave", "custom", "duckduckgo"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            custom: CustomSearch::default(),
        }
    }
}

/// Kullanıcı tanımlı arama sağlayıcısı.
///
/// Anahtar burada değil, şifreli `keys.dat` içinde `search_custom` adıyla
/// durur — ayar dosyası düz metin.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CustomSearch {
    /// `{query}` yer tutucusu içeren adres.
    pub url: String,
    /// Kimlik doğrulama başlığı, örn. `Authorization`.
    pub header_name: String,
    /// Başlık değeri; `{key}` yer tutucusu saklanan anahtarla doldurulur.
    pub header_value: String,
    /// Sonuç dizisinin bulunduğu alan; nokta ile iç içe geçilir (`data.items`).
    pub results_path: String,
    pub title_key: String,
    pub url_key: String,
    pub snippet_key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Llm {
    /// "groq" | "openai" | "gemini" | "mistral" | "deepseek" | "xai" | "local"
    pub provider: String,
    /// Boş bırakılırsa sağlayıcının varsayılan modeli kullanılır.
    pub model: String,
    /// Hangi araçların gerektiğini seçen ucuz model.
    ///
    /// Boşsa yönlendirme anahtar kelimeyle yapılır — ağ çağrısı yok, para
    /// harcanmaz. Doldurulursa asıl modele yalnızca gerçekten gereken
    /// araçların şeması gider; ucuz model seçer, pahalı model işi yapar.
    ///
    /// Aynı sağlayıcıda çalışır: anahtar zaten var, ikinci bir kurulum
    /// gerekmiyor.
    #[serde(default)]
    pub router_model: String,

    /// Kod işi için ayrı sağlayıcı. Boşsa sohbetinki kullanılır.
    ///
    /// İkisi farklı işler: sohbet hızlı ve ucuz olsun ister, kod doğru
    /// olsun ister. Tek slot varken kullanıcı ikisinden birini feda
    /// etmek zorunda kalıyordu — ya sohbet için gereksiz pahalı bir model,
    /// ya kod için yetersiz bir model.
    ///
    /// Boş varsayılan kasıtlı: ayrı model kuran kullanıcı bunu bilerek
    /// yapar, kurmayanın davranışı hiç değişmez.
    #[serde(default)]
    pub code_provider: String,

    /// Kod işi için model. `code_provider` boşken anlamsız; dolu olduğunda
    /// boş bırakılırsa o sağlayıcının varsayılanı kullanılır.
    #[serde(default)]
    pub code_model: String,
}

impl Default for Llm {
    fn default() -> Self {
        Self {
            // Groq varsayılan: ücretsiz katmanı var ve hızlı.
            provider: "groq".to_string(),
            model: String::new(),
            // Kapalı gelir: yönlendirme fazladan bir model çağrısı demek.
            // Kullanıcı isterse açar.
            router_model: String::new(),
            // Boş: kod da sohbetle aynı modeli kullanır. Ayıran kullanıcı
            // doldurur.
            code_provider: String::new(),
            code_model: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct General {
    /// Asistanın kendine verdiği ad. TTS bunu okuyacağı için telaffuza uygun.
    pub assistant_name: String,
    /// Arayüz dili: "tr" | "en"
    pub language: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ui {
    /// Terminal görünümü için yazı boyutu.
    pub font_size: f32,
    /// Pencere modu: "windowed" | "borderless" | "fullscreen"
    /// (Manuel testte istenen özellik — F1'den itibaren var.)
    pub window_mode: String,
}

impl Default for General {
    fn default() -> Self {
        Self {
            assistant_name: "Vavis".to_string(),
            language: "tr".to_string(),
        }
    }
}

impl Default for Ui {
    fn default() -> Self {
        Self {
            font_size: 14.0,
            window_mode: "windowed".to_string(),
        }
    }
}

/// Pencere modu — string yerine tip. Geçersiz değer sessizce `Windowed` olur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowMode {
    Windowed,
    Borderless,
    Fullscreen,
}

impl WindowMode {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "borderless" => Self::Borderless,
            "fullscreen" => Self::Fullscreen,
            _ => Self::Windowed,
        }
    }
}

impl Config {
    pub fn window_mode(&self) -> WindowMode {
        WindowMode::parse(&self.ui.window_mode)
    }

    /// Dosyadan oku. Dosya yoksa varsayılanı döner (hata değil).
    pub fn load(paths: &Paths) -> Result<Self> {
        let file = paths.config_file();
        if !file.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&file).map_err(|source| CoreError::ConfigRead {
            path: file.clone(),
            source,
        })?;
        toml::from_str(&text).map_err(|source| CoreError::ConfigParse {
            path: file,
            source: Box::new(source),
        })
    }

    /// Bozuk dosyada çökmek yerine: yedekle, uyar, varsayılanla devam et.
    ///
    /// Açılışın hiçbir koşulda engellenmemesi için ana yol budur.
    pub fn load_or_default(paths: &Paths) -> Self {
        match Self::load(paths) {
            Ok(cfg) => cfg,
            Err(err) => {
                tracing::warn!(%err, "ayar dosyası okunamadı — varsayılana dönülüyor");
                let file = paths.config_file();
                if file.exists() {
                    let backup = file.with_extension("toml.bozuk");
                    if let Err(e) = std::fs::rename(&file, &backup) {
                        tracing::warn!(%e, "bozuk ayar yedeklenemedi");
                    } else {
                        tracing::info!(backup = %backup.display(), "bozuk ayar yedeklendi");
                    }
                }
                Self::default()
            }
        }
    }

    /// Atomik yazma: `.tmp` → rename. Yarım dosya bırakmaz.
    pub fn save(&self, paths: &Paths) -> Result<()> {
        paths.ensure()?;
        let file = paths.config_file();
        let text = toml::to_string_pretty(self).expect("Config her zaman serileştirilebilir");

        let tmp = file.with_extension("toml.tmp");
        std::fs::write(&tmp, &text).map_err(|source| CoreError::ConfigWrite {
            path: tmp.clone(),
            source,
        })?;
        std::fs::rename(&tmp, &file).map_err(|source| CoreError::ConfigWrite {
            path: file.clone(),
            source,
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_paths() -> (tempfile::TempDir, Paths) {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::with_root(tmp.path());
        paths.ensure().unwrap();
        (tmp, paths)
    }

    #[test]
    fn defaults_are_sane() {
        let cfg = Config::default();
        assert_eq!(cfg.general.assistant_name, "Vavis");
        assert_eq!(cfg.general.language, "tr");
        assert_eq!(cfg.window_mode(), WindowMode::Windowed);
    }

    #[test]
    fn missing_file_yields_defaults() {
        let (_tmp, paths) = tmp_paths();
        assert_eq!(Config::load(&paths).unwrap(), Config::default());
    }

    #[test]
    fn save_then_load_roundtrips() {
        let (_tmp, paths) = tmp_paths();
        let mut cfg = Config::default();
        cfg.ui.font_size = 18.0;
        cfg.ui.window_mode = "fullscreen".into();
        cfg.save(&paths).unwrap();

        let loaded = Config::load(&paths).unwrap();
        assert_eq!(loaded, cfg);
        assert_eq!(loaded.window_mode(), WindowMode::Fullscreen);
    }

    #[test]
    fn partial_file_fills_missing_fields() {
        // Eski sürümden gelen, yeni alanları olmayan dosya okunabilmeli.
        let (_tmp, paths) = tmp_paths();
        std::fs::write(paths.config_file(), "[general]\nlanguage = \"en\"\n").unwrap();

        let cfg = Config::load(&paths).unwrap();
        assert_eq!(cfg.general.language, "en");
        assert_eq!(cfg.general.assistant_name, "Vavis"); // varsayılandan geldi
        assert_eq!(cfg.ui.font_size, 14.0);
    }

    #[test]
    fn config_without_a_search_section_still_loads() {
        // Ayarları 0.3.0 öncesinden gelen kullanıcı: arama bölümü yok, ama
        // dosya okunabilmeli ve zincir varsayılan sırayla gelmeli.
        let (_tmp, paths) = tmp_paths();
        std::fs::write(paths.config_file(), "[general]\nlanguage = \"tr\"\n").unwrap();

        let cfg = Config::load(&paths).unwrap();
        assert_eq!(cfg.search.order, Search::default().order);
        assert!(cfg.search.custom.url.is_empty());
    }

    #[test]
    fn default_search_order_prefers_keyed_providers_and_ends_keyless() {
        // DuckDuckGo anahtar istemiyor; zincirin tabanı o olmalı ki hiç
        // anahtar girmemiş kullanıcı da arama yapabilsin.
        let order = Search::default().order;
        assert_eq!(order.last().map(String::as_str), Some("duckduckgo"));
        assert!(order.contains(&"tavily".to_string()));
    }

    #[test]
    fn search_settings_survive_a_save_load_round_trip() {
        let (_tmp, paths) = tmp_paths();
        let mut cfg = Config::default();
        cfg.search.order = vec!["brave".into(), "duckduckgo".into()];
        cfg.search.custom.url = "https://searx.example/search?q={query}&format=json".into();
        cfg.search.custom.results_path = "results".into();
        cfg.save(&paths).unwrap();

        let loaded = Config::load(&paths).unwrap();
        assert_eq!(loaded.search.order, vec!["brave", "duckduckgo"]);
        assert!(loaded.search.custom.url.contains("{query}"));
    }

    #[test]
    fn corrupt_file_is_backed_up_not_fatal() {
        let (_tmp, paths) = tmp_paths();
        std::fs::write(paths.config_file(), "bu geçerli toml değil {{{").unwrap();

        let cfg = Config::load_or_default(&paths);
        assert_eq!(cfg, Config::default());
        assert!(paths.config_file().with_extension("toml.bozuk").exists());
    }

    #[test]
    fn save_leaves_no_temp_file() {
        let (_tmp, paths) = tmp_paths();
        Config::default().save(&paths).unwrap();
        assert!(!paths.config_file().with_extension("toml.tmp").exists());
    }

    #[test]
    fn window_mode_parsing_is_forgiving() {
        assert_eq!(WindowMode::parse("FullScreen"), WindowMode::Fullscreen);
        assert_eq!(WindowMode::parse(" borderless "), WindowMode::Borderless);
        assert_eq!(WindowMode::parse("saçmalık"), WindowMode::Windowed);
    }
}

#[cfg(test)]
mod code_slot_tests {
    use super::*;

    /// Every config written before the code slot existed lacks these two
    /// fields, and `deny_unknown_fields` makes a parse either work or fail
    /// outright -- so "it probably defaults" is not good enough to ship.
    /// This is the user's own file, shape for shape.
    #[test]
    fn a_config_written_before_the_code_slot_still_loads() {
        let older = r#"
[general]
assistant_name = "Vavis"
language = "tr"

[llm]
provider = "gemini"
model = "gemini-3.5-flash"
router_model = ""
"#;
        let parsed: Config = toml::from_str(older).expect("an older config must still load");
        assert_eq!(parsed.llm.provider, "gemini");
        assert_eq!(
            parsed.llm.code_provider, "",
            "a missing code slot means: use the chat model"
        );
        assert_eq!(parsed.llm.code_model, "");
    }

    /// And once set, it survives a save/load round trip.
    #[test]
    fn the_code_slot_round_trips() {
        let mut c = Config::default();
        c.llm.code_provider = "openai".into();
        c.llm.code_model = "o3-mini".into();

        let text = toml::to_string(&c).expect("serialises");
        let back: Config = toml::from_str(&text).expect("parses back");
        assert_eq!(back.llm.code_provider, "openai");
        assert_eq!(back.llm.code_model, "o3-mini");
    }
}
