//! VirusTotal — dosya ve adres itibar sorgusu.
//!
//! Neden hash ile başlıyor: VirusTotal'a **dosya göndermek** o dosyayı
//! kalıcı olarak yükler ve premium kullanıcılar indirebilir. Kullanıcının
//! makinesindeki bir dosyayı habersiz oraya göndermek, bu projede en baştan
//! çizilen "hiçbir şey dışarı sızmaz" kuralının ihlali olur.
//!
//! Bu yüzden akış şu:
//!
//! 1. Dosyanın SHA-256'sı **yerel olarak** hesaplanır
//! 2. Yalnızca bu hash sorulur — dosyanın kendisi makineden çıkmaz
//! 3. Hash bilinmiyorsa "bilinmiyor" denir; **yükleme yapılmaz**
//!
//! Hash bir parmak izi: VirusTotal onu tanıyorsa dosya zaten orada demektir.
//! Tanımıyorsa cevap "bu dosya daha önce görülmemiş" olur, ki bu da tek
//! başına işe yarar bir bilgidir.
//!
//! Anahtar burada değil: şifreli `keys.dat` içinde `virustotal` adıyla durur.

use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const API_ROOT: &str = "https://www.virustotal.com/api/v3";

/// Ücretsiz katmanın dakika sınırı.
///
/// VirusTotal ücretsiz anahtarı dakikada 4, günde 500 istek veriyor. Sınır
/// istemci tarafında da tutuluyor ki kullanıcı 429 yerine ne olduğunu
/// söyleyen bir mesaj görsün.
const FREE_PER_MINUTE: usize = 4;

/// Bir sorgunun sonucu.
#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    /// Sorulan şey — hash, adres ya da alan adı.
    pub subject: String,
    /// Kaç motor zararlı dedi.
    pub malicious: u32,
    /// Kaç motor şüpheli dedi.
    pub suspicious: u32,
    /// Kaç motor temiz dedi.
    pub harmless: u32,
    /// Kaç motor bir şey diyemedi.
    pub undetected: u32,
    /// Varsa dosyanın bilinen adı.
    pub name: String,
    /// Zararlı diyen motorların adları — hangisi olduğu bazen önemli.
    pub flagged_by: Vec<String>,
    /// VirusTotal'daki sayfası, kullanıcı kendi bakmak isterse.
    pub permalink: String,
}

impl Verdict {
    /// Kaç motor baktı.
    pub fn total(&self) -> u32 {
        self.malicious + self.suspicious + self.harmless + self.undetected
    }

    /// Tek cümlelik özet.
    ///
    /// Eşik oran üzerinden, sabit sayı üzerinden değil. İlk halinde "2 motor"
    /// doğrudan "bu ciddi" diyordu; canlı API'ye sorunca **google.com'un 89
    /// motordan 2'si tarafından işaretlendiği** görüldü. Google'a malware
    /// demek, kullanıcıyı uyarıyı tamamen yok saymaya alıştırır — ve o zaman
    /// gerçek uyarı geldiğinde de okumaz.
    ///
    /// Ölçülen gerçek şu: büyük ve tanınmış siteler/dosyalar neredeyse her
    /// zaman birkaç tanınmayan motordan işaret alır. Gerçek zararlı yazılım
    /// onlarca motor tarafından yakalanır. Aradaki fark oran.
    pub fn summary(&self) -> String {
        let total = self.total();
        if total == 0 {
            return "VirusTotal bu konuda bir analiz döndürmedi.".to_string();
        }

        if self.malicious == 0 && self.suspicious == 0 {
            return format!("Temiz görünüyor — {total} motorun hiçbiri işaretlemedi.");
        }
        if self.malicious == 0 {
            return format!(
                "{} motor şüpheli dedi ({} motordan). Zararlı diyen yok.",
                self.suspicious, total
            );
        }

        // Yüzde onu geçiyorsa bu artık gürültü değil.
        let ratio = self.malicious as f32 / total as f32;
        if ratio >= 0.10 {
            return format!(
                "{} motor zararlı dedi ({} motordan). Bu ciddi — açma.",
                self.malicious, total
            );
        }

        // Birkaç motor: büyük sitelerde ve yaygın dosyalarda olağan.
        // Yine de susmuyoruz, çünkü bazen haklı çıkıyorlar.
        format!(
            "{} motor zararlı dedi ({} motordan) — bu kadar az sayı çoğu zaman \
             yanlış alarmdır, ama kaynağına güvenmiyorsan açma.",
            self.malicious, total
        )
    }
}

/// Bir sorgunun neden cevapsız kaldığı.
#[derive(Debug, Clone, PartialEq)]
pub enum VtError {
    /// Anahtar yok.
    NotConfigured,
    /// VirusTotal bu şeyi hiç görmemiş.
    Unknown(String),
    /// Dakika/gün sınırı doldu.
    RateLimited,
    /// Anahtar reddedildi.
    BadKey,
    Failed(String),
}

impl std::fmt::Display for VtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConfigured => write!(
                f,
                "VirusTotal anahtarı yok — ayarlar → API anahtarları bölümünden ekle \
                 (virustotal.com'da ücretsiz hesap yeter)"
            ),
            Self::Unknown(what) => write!(
                f,
                "VirusTotal bunu daha önce görmemiş ({what}). Bu tek başına \
                 'temiz' demek değil, 'bilinmiyor' demek — dosyayı yüklemedim, \
                 çünkü yüklemek onu kalıcı olarak dışarı çıkarır."
            ),
            Self::RateLimited => write!(
                f,
                "VirusTotal sınırına takıldı (ücretsiz anahtar dakikada 4, günde \
                 500 istek). Bir dakika bekle."
            ),
            Self::BadKey => write!(f, "VirusTotal anahtarı reddedildi — ayarlardan kontrol et"),
            Self::Failed(e) => write!(f, "{e}"),
        }
    }
}

/// Saklanan anahtar.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Settings {
    pub api_key: String,
}

impl Settings {
    pub fn is_configured(&self) -> bool {
        !self.api_key.trim().is_empty()
    }
}

fn settings() -> &'static Mutex<Settings> {
    static SETTINGS: OnceLock<Mutex<Settings>> = OnceLock::new();
    SETTINGS.get_or_init(|| Mutex::new(Settings::default()))
}

/// Anahtarı kurar. Açılışta ve ayar değişince çağrılıyor.
pub fn configure(new: Settings) {
    *settings().lock().unwrap_or_else(|e| e.into_inner()) = new;
}

pub fn current() -> Settings {
    settings().lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Son bir dakikadaki isteklerin zamanları.
fn recent() -> &'static Mutex<Vec<std::time::Instant>> {
    static RECENT: OnceLock<Mutex<Vec<std::time::Instant>>> = OnceLock::new();
    RECENT.get_or_init(|| Mutex::new(Vec::new()))
}

/// Dakika sınırını istemci tarafında tutar.
///
/// Sunucu zaten 429 döndürürdü, ama o zaman kullanıcı "bir şeyler ters
/// gitti" görüyor. Burada sayınca ne olduğunu söyleyebiliyoruz.
fn take_slot() -> Result<(), VtError> {
    let now = std::time::Instant::now();
    let mut times = recent().lock().unwrap_or_else(|e| e.into_inner());
    times.retain(|t| now.duration_since(*t) < Duration::from_secs(60));
    if times.len() >= FREE_PER_MINUTE {
        return Err(VtError::RateLimited);
    }
    times.push(now);
    Ok(())
}

/// Bir dosyanın SHA-256'sı — **dosya makineden çıkmadan**.
///
/// Parça parça okunuyor: 4 GB'lık bir ISO'yu belleğe almak gereksiz ve
/// çoğu makinede imkânsız.
pub fn hash_file(path: &std::path::Path) -> Result<String, VtError> {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let file =
        std::fs::File::open(path).map_err(|e| VtError::Failed(format!("dosya açılamadı: {e}")))?;
    let mut reader = std::io::BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];

    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|e| VtError::Failed(format!("dosya okunamadı: {e}")))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// GET, gövdeyi döndürür.
fn get(path: &str) -> Result<(u16, String), VtError> {
    let s = current();
    if !s.is_configured() {
        return Err(VtError::NotConfigured);
    }
    take_slot()?;

    let url = format!("{API_ROOT}/{path}");
    let key = s.api_key.trim().to_string();

    crate::run_async(async move {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| VtError::Failed(e.to_string()))?;

        let resp = client
            .get(&url)
            .header("x-apikey", key)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    VtError::Failed("VirusTotal yanıt vermedi (zaman aşımı)".into())
                } else {
                    VtError::Failed(e.to_string())
                }
            })?;

        let status = resp.status().as_u16();
        let text = resp
            .text()
            .await
            .map_err(|e| VtError::Failed(e.to_string()))?;
        Ok((status, text))
    })
    .map_err(VtError::Failed)?
}

/// Durum kodunu anlamlı bir hataya çevirir.
fn check_status(status: u16, subject: &str) -> Result<(), VtError> {
    match status {
        200..=299 => Ok(()),
        401 | 403 => Err(VtError::BadKey),
        404 => Err(VtError::Unknown(subject.to_string())),
        429 => Err(VtError::RateLimited),
        s => Err(VtError::Failed(format!("VirusTotal {s} döndü"))),
    }
}

/// Ortak gövde çözümlemesi: üç uç nokta da aynı `last_analysis_*` şeklini
/// döndürüyor, o yüzden tek yerde çözülüyor.
fn parse_verdict(body: &str, subject: &str, permalink: String) -> Result<Verdict, VtError> {
    let json: serde_json::Value = serde_json::from_str(body)
        .map_err(|_| VtError::Failed("VirusTotal yanıtı çözümlenemedi".into()))?;

    let attrs = json
        .get("data")
        .and_then(|d| d.get("attributes"))
        .ok_or_else(|| VtError::Failed("VirusTotal yanıtı beklenen şekilde değil".into()))?;

    let stats = attrs.get("last_analysis_stats");
    let count = |k: &str| -> u32 {
        stats
            .and_then(|s| s.get(k))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32
    };

    // Zararlı diyen motorların adları. Hangisinin işaretlediği bazen
    // cevabın kendisi: tek bir tanınmayan motor ile Microsoft+Kaspersky
    // çok farklı şeyler.
    let mut flagged_by = Vec::new();
    if let Some(results) = attrs
        .get("last_analysis_results")
        .and_then(|r| r.as_object())
    {
        for (engine, result) in results {
            let category = result
                .get("category")
                .and_then(|c| c.as_str())
                .unwrap_or("");
            if category == "malicious" {
                flagged_by.push(engine.clone());
            }
        }
        flagged_by.sort();
    }

    let name = attrs
        .get("meaningful_name")
        .or_else(|| attrs.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(Verdict {
        subject: subject.to_string(),
        malicious: count("malicious"),
        suspicious: count("suspicious"),
        harmless: count("harmless"),
        undetected: count("undetected"),
        name,
        flagged_by,
        permalink,
    })
}

/// Bir hash'i sorar. Dosyanın kendisi gönderilmez.
pub fn lookup_hash(hash: &str) -> Result<Verdict, VtError> {
    let hash = hash.trim().to_lowercase();
    if !is_hash(&hash) {
        return Err(VtError::Failed(
            "geçerli bir MD5, SHA-1 veya SHA-256 değil".into(),
        ));
    }

    let (status, body) = get(&format!("files/{hash}"))?;
    check_status(status, &hash)?;
    parse_verdict(
        &body,
        &hash,
        format!("https://www.virustotal.com/gui/file/{hash}"),
    )
}

/// Bir dosyayı **hash'leyerek** sorar.
pub fn scan_file(path: &std::path::Path) -> Result<Verdict, VtError> {
    let hash = hash_file(path)?;
    let mut verdict = lookup_hash(&hash)?;
    // Yerel ad, VirusTotal'ın bildiğinden daha anlamlı olabilir.
    if verdict.name.is_empty() {
        if let Some(n) = path.file_name().and_then(|n| n.to_str()) {
            verdict.name = n.to_string();
        }
    }
    Ok(verdict)
}

/// Bir adresi sorar.
pub fn lookup_url(raw: &str) -> Result<Verdict, VtError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(VtError::Failed("adres boş".into()));
    }
    // VirusTotal adresleri, padding'siz base64url ile kimliklendiriyor.
    let id = url_id(raw);
    let (status, body) = get(&format!("urls/{id}"))?;
    check_status(status, raw)?;
    parse_verdict(
        &body,
        raw,
        format!("https://www.virustotal.com/gui/url/{id}"),
    )
}

/// Bir alan adını sorar.
pub fn lookup_domain(domain: &str) -> Result<Verdict, VtError> {
    let domain = domain.trim().trim_start_matches("www.").to_lowercase();
    if domain.is_empty() || !domain.contains('.') {
        return Err(VtError::Failed("geçerli bir alan adı değil".into()));
    }
    let (status, body) = get(&format!("domains/{domain}"))?;
    check_status(status, &domain)?;
    parse_verdict(
        &body,
        &domain,
        format!("https://www.virustotal.com/gui/domain/{domain}"),
    )
}

/// VirusTotal'ın adres kimliği: padding'siz base64url.
fn url_id(raw: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw.as_bytes())
}

/// Uzunluğu ve alfabesi bir hash'e uyuyor mu.
fn is_hash(s: &str) -> bool {
    matches!(s.len(), 32 | 40 | 64) && s.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_shapes_are_recognised() {
        assert!(is_hash(&"a".repeat(32)), "MD5");
        assert!(is_hash(&"b".repeat(40)), "SHA-1");
        assert!(is_hash(&"c".repeat(64)), "SHA-256");
        assert!(!is_hash(&"d".repeat(50)), "yanlış uzunluk");
        assert!(!is_hash(&"z".repeat(64)), "hex değil");
        assert!(!is_hash(""), "boş");
    }

    /// Kimlik padding taşımamalı: VirusTotal padding'li kimliği tanımıyor.
    #[test]
    fn a_url_id_carries_no_padding() {
        let id = url_id("http://example.com");
        assert!(!id.contains('='), "padding var: {id}");
        assert!(
            !id.contains('+') && !id.contains('/'),
            "url-safe değil: {id}"
        );
    }

    /// Tek motorun işaretlemesi çoğu zaman yanlış alarm; özet bunu
    /// söylemezse kullanıcı gereksiz paniğe kapılır.
    #[test]
    fn a_single_detection_is_described_as_probably_a_false_alarm() {
        let v = Verdict {
            subject: "x".into(),
            malicious: 1,
            suspicious: 0,
            harmless: 60,
            undetected: 9,
            name: String::new(),
            flagged_by: vec!["SomeEngine".into()],
            permalink: String::new(),
        };
        let s = v.summary();
        assert!(s.contains("yanlış alarm"), "dedi ki: {s}");
        assert!(!s.contains("ciddi"), "1/70 ciddi denmemeli: {s}");
        assert_eq!(v.total(), 70);
    }

    /// Ölçülmüş gerçek: google.com, 89 motorun 2'si tarafından zararlı
    /// işaretleniyor. İlk eşik sabit sayıydı ve buna "bu ciddi" diyordu --
    /// Google'a malware demek kullanıcıyı uyarıyı tamamen yok saymaya
    /// alıştırır, ve o zaman gerçek uyarıyı da okumaz.
    #[test]
    fn a_handful_of_engines_on_a_huge_sample_is_not_called_serious() {
        let v = Verdict {
            subject: "google.com".into(),
            malicious: 2,
            suspicious: 0,
            harmless: 65,
            undetected: 22,
            name: String::new(),
            flagged_by: vec!["ObscureOne".into(), "ObscureTwo".into()],
            permalink: String::new(),
        };
        assert_eq!(v.total(), 89);
        let s = v.summary();
        assert!(!s.contains("ciddi"), "2/89 ciddi denmemeli: {s}");
        assert!(s.contains("yanlış alarm"), "dedi ki: {s}");
    }

    /// Ama aynı 2 motor, küçük bir havuzda oran olarak büyük: susmamalı.
    #[test]
    fn the_same_count_on_a_small_pool_is_still_serious() {
        let v = Verdict {
            subject: "x".into(),
            malicious: 2,
            suspicious: 0,
            harmless: 8,
            undetected: 5,
            name: String::new(),
            flagged_by: vec![],
            permalink: String::new(),
        };
        assert_eq!(v.total(), 15);
        assert!(
            v.summary().contains("ciddi"),
            "2/15 ciddi olmalı: {}",
            v.summary()
        );
    }

    /// Hiç analiz dönmediyse "temiz" denmemeli.
    #[test]
    fn no_analysis_is_not_reported_as_clean() {
        let v = Verdict {
            subject: "x".into(),
            malicious: 0,
            suspicious: 0,
            harmless: 0,
            undetected: 0,
            name: String::new(),
            flagged_by: vec![],
            permalink: String::new(),
        };
        let s = v.summary();
        assert!(!s.contains("Temiz"), "dedi ki: {s}");
    }

    #[test]
    fn many_detections_are_not_softened() {
        let v = Verdict {
            subject: "x".into(),
            malicious: 45,
            suspicious: 2,
            harmless: 10,
            undetected: 13,
            name: String::new(),
            flagged_by: vec![],
            permalink: String::new(),
        };
        let s = v.summary();
        assert!(s.contains("ciddi"), "dedi ki: {s}");
        assert!(!s.contains("yanlış alarm"), "yumuşatmamalı: {s}");
    }

    #[test]
    fn a_clean_result_says_so_plainly() {
        let v = Verdict {
            subject: "x".into(),
            malicious: 0,
            suspicious: 0,
            harmless: 65,
            undetected: 5,
            name: String::new(),
            flagged_by: vec![],
            permalink: String::new(),
        };
        assert!(v.summary().contains("Temiz"));
    }

    /// "Bilinmiyor" ile "temiz" aynı şey değil, ve mesajın bunu söylemesi
    /// gerekiyor -- yoksa kullanıcı bulunamayan bir dosyayı güvenli sanır.
    #[test]
    fn unknown_is_not_reported_as_clean() {
        let msg = VtError::Unknown("abc".into()).to_string();
        assert!(msg.contains("bilinmiyor"), "dedi ki: {msg}");
        assert!(
            msg.contains("yüklemedim"),
            "dosyanın gönderilmediği söylenmeli: {msg}"
        );
    }

    /// Anahtarsız çağrı ağa hiç çıkmamalı.
    #[test]
    fn nothing_is_requested_without_a_key() {
        configure(Settings::default());
        assert_eq!(lookup_hash(&"a".repeat(64)), Err(VtError::NotConfigured));
    }

    /// Dakika sınırı istemci tarafında tutuluyor; dolunca ne olduğu
    /// söylenebilsin diye.
    #[test]
    fn the_per_minute_limit_is_counted_locally() {
        recent().lock().unwrap().clear();
        for _ in 0..FREE_PER_MINUTE {
            assert_eq!(take_slot(), Ok(()));
        }
        assert_eq!(take_slot(), Err(VtError::RateLimited));
        recent().lock().unwrap().clear();
    }

    /// Hash yerel hesaplanıyor: bilinen bir değerle doğrula.
    #[test]
    fn a_file_is_hashed_locally() {
        let dir = std::env::temp_dir().join("vavis-vt-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty.bin");
        std::fs::write(&path, b"").unwrap();

        // Boş girdinin SHA-256'sı sabittir.
        assert_eq!(
            hash_file(&path).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );

        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(
            hash_file(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = std::fs::remove_file(&path);
    }
}
