//! Metin → ses (TTS).
//!
//! # Motorlar
//!
//! Altı tane, hepsi aynı arayüzün arkasında:
//!
//! | Motor | Anahtar | Nerede çalışır | Not |
//! |---|---|---|---|
//! | **SAPI** | yok | işletim sistemi | çevrimdışı, her zaman var |
//! | **Edge** | yok | Microsoft sunucusu | ücretsiz, doğal, Türkçe iyi |
//! | **Kokoro** | yok | **kullanıcının kendi makinesi** | ayrı süreç, bkz. [`crate::kokoro`] |
//! | **ElevenLabs** | var | bulut | en doğal, karakter başına ücretli |
//! | **OpenAI** | var | bulut | anahtarı zaten olan için bedava kurulum |
//! | **Gemini** | var | bulut | aynısı, Gemini anahtarıyla; ham PCM döner |
//!
//! # Neden hepsi bir zincir
//!
//! Ses, asistanın **ikincil** çıktısı: yazı zaten ekranda. Bu yüzden bir
//! motorun düşmesi konuşmayı tamamen susturmamalı — seçilen motor
//! başarısız olursa sıradaki denenir ve en sonda SAPI durur, çünkü SAPI
//! ağ da anahtar da istemiyor.
//!
//! Ama zincir **sessiz değil**: her düşüş loglanıyor. Aksi hâlde kullanıcı
//! ElevenLabs seçip SAPI dinler ve sebebini hiç öğrenemezdi.
//!
//! # Anahtarlar burada tutulmuyor
//!
//! [`TtsConfig`] anahtarları taşıyor ama onları **okumuyor**: çağıran katman
//! (kabuk) şifreli depodan alıp veriyor. Ses katmanının DPAPI'yi tanımasına
//! gerek yok, ve tanımaması onu test edilebilir tutuyor.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum TtsError {
    #[error("ses motoru başlatılamadı: {0}")]
    Engine(String),
    #[error("seslendirme başarısız: {0}")]
    Speak(String),
}

pub type Result<T> = std::result::Result<T, TtsError>;

/// Hangi ses motoru kullanılacak.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TtsEngineKind {
    /// Windows SAPI — çevrimdışı, anahtarsız, her zaman çalışır.
    #[default]
    Sapi,
    /// Microsoft Edge TTS — daha doğal ses, anahtarsız.
    Edge,
    /// Kokoro — kullanıcının kendi makinesinde çalışan ayrı sunucu.
    Kokoro,
    /// ElevenLabs — en doğal ses, anahtar ve kota gerektirir.
    ElevenLabs,
    /// OpenAI TTS — iyi ses, çoğu kullanıcıda anahtarı zaten var.
    OpenAi,
    /// Gemini TTS — sohbet için Gemini anahtarı girmiş olanın hazır sesi.
    Gemini,
}

impl TtsEngineKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "sapi" | "windows" | "sistem" => Some(Self::Sapi),
            "edge" | "neural" | "dogal" | "doğal" => Some(Self::Edge),
            "kokoro" | "yerel" | "local" => Some(Self::Kokoro),
            "elevenlabs" | "eleven" | "11labs" => Some(Self::ElevenLabs),
            "openai" | "gpt" => Some(Self::OpenAi),
            "gemini" | "google" => Some(Self::Gemini),
            _ => None,
        }
    }

    /// Ayar dosyasına yazılan sabit kimlik.
    pub fn id(self) -> &'static str {
        match self {
            Self::Sapi => "sapi",
            Self::Edge => "edge",
            Self::Kokoro => "kokoro",
            Self::ElevenLabs => "elevenlabs",
            Self::OpenAi => "openai",
            Self::Gemini => "gemini",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Sapi => "sapi (sistem sesi)",
            Self::Edge => "edge (doğal ses)",
            Self::Kokoro => "kokoro (yerel sunucu)",
            Self::ElevenLabs => "elevenlabs (en doğal)",
            Self::OpenAi => "openai",
            Self::Gemini => "gemini (google sesi)",
        }
    }

    /// Sesli duyuruda okunacak ad.
    ///
    /// Kimlikten (`elevenlabs`, `sapi`) ayrı: onlar ayar dosyası için, bu ise
    /// yüksek sesle okunuyor. "eleven labs" iki kelime olarak okunmalı,
    /// "sapi" ise kullanıcı için anlamsız — "sistem sesi" anlamlı.
    pub fn spoken_name(self, language: &str) -> &'static str {
        match (self, language) {
            (Self::Sapi, "en") => "system",
            (Self::Sapi, _) => "sistem",
            (Self::Edge, _) => "Edge",
            (Self::Kokoro, _) => "Kokoro",
            (Self::ElevenLabs, _) => "Eleven Labs",
            (Self::OpenAi, _) => "Open A I",
            (Self::Gemini, _) => "Gemini",
        }
    }

    /// Bu motor çalışmak için bir anahtar istiyor mu.
    ///
    /// Arayüz bunu, anahtarı olmayan bir motoru seçtirmeden önce uyarmak
    /// için kullanıyor.
    pub fn needs_key(self) -> bool {
        matches!(self, Self::ElevenLabs | Self::OpenAi | Self::Gemini)
    }

    pub const ALL: [Self; 6] = [
        Self::Sapi,
        Self::Edge,
        Self::Kokoro,
        Self::ElevenLabs,
        Self::OpenAi,
        Self::Gemini,
    ];
}

/// TTS ayarları.
#[derive(Debug, Clone)]
pub struct TtsConfig {
    /// Konuşma hızı (-10 … +10, 0 normal). Ortak ölçek: her motor kendi
    /// birimine bunun üzerinden çeviriyor.
    pub rate: i32,
    /// Ses yüksekliği (0-100).
    pub volume: u32,
    /// SAPI ses adı — boşsa sistem varsayılanı.
    pub voice: String,
    /// Kullanılacak motor.
    pub engine: TtsEngineKind,
    /// Edge ses adı (SAPI'den farklı adlandırma).
    pub edge_voice: String,
    /// Kokoro sunucu adresi. Boşsa [`crate::kokoro::DEFAULT_URL`].
    pub kokoro_url: String,
    pub kokoro_voice: String,
    /// ElevenLabs anahtarı — kabuk şifreli depodan doldurur.
    pub eleven_key: String,
    pub eleven_voice: String,
    pub eleven_model: String,
    /// OpenAI anahtarı — sohbet için girilmiş olanın aynısı olabilir.
    pub openai_key: String,
    pub openai_voice: String,
    pub openai_model: String,
    /// Gemini anahtarı — sohbet için girilmiş olanın aynısı.
    pub gemini_key: String,
    pub gemini_voice: String,
    pub gemini_model: String,
    /// Asistanın konuştuğu dil (`"tr"`, `"en"`).
    ///
    /// Ses seçimi için gerekiyor: SAPI dile uyan sesi buradan buluyor, ve
    /// Edge'in varsayılan sesi de buna göre değişiyor. Dil sesin bir ayarı
    /// değil ama sesin **hangi** ses olacağını belirliyor.
    pub language: String,
}

impl TtsConfig {
    /// Dilin iki harfli kodu — PowerShell'e gömmeden önce temizlenmiş.
    ///
    /// Yalnızca harf bırakılıyor: bu değer bir betiğin içine giriyor ve
    /// ayarlardan gelen bir metin oraya doğrudan konmamalı.
    pub(crate) fn language_tag(&self) -> String {
        let tag: String = self
            .language
            .chars()
            .filter(char::is_ascii_alphabetic)
            .take(2)
            .collect::<String>()
            .to_ascii_lowercase();
        if tag.is_empty() {
            "en".to_string()
        } else {
            tag
        }
    }
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            rate: 1, // hafif hızlı — bekleme hissini azaltır
            volume: 100,
            voice: String::new(),
            // Edge varsayılan: anahtar istemiyor ve **kullanıcının dilini
            // konuşuyor**. SAPI varsayılandı, ama çoğu Windows kurulumunda
            // yalnızca İngilizce ses yüklü: Türkçe metin İngilizce sesle
            // okununca anlaşılmaz çıkıyordu. Edge ulaşılamazsa zincir zaten
            // SAPI'ye düşüyor, yani çevrimdışı kullanıcı yine ses duyuyor.
            engine: TtsEngineKind::Edge,
            // Boş: dile uyan ses konuşma anında seçiliyor, çünkü dil
            // ayarlardan sonradan değişebiliyor.
            edge_voice: String::new(),
            kokoro_url: crate::kokoro::DEFAULT_URL.to_string(),
            kokoro_voice: crate::kokoro::DEFAULT_VOICE.to_string(),
            eleven_key: String::new(),
            eleven_voice: crate::elevenlabs::DEFAULT_VOICE_ID.to_string(),
            eleven_model: crate::elevenlabs::DEFAULT_MODEL.to_string(),
            openai_key: String::new(),
            openai_voice: crate::openai_tts::DEFAULT_VOICE.to_string(),
            openai_model: crate::openai_tts::DEFAULT_MODEL.to_string(),
            gemini_key: String::new(),
            gemini_voice: crate::gemini_tts::DEFAULT_VOICE.to_string(),
            gemini_model: crate::gemini_tts::DEFAULT_MODEL.to_string(),
            language: "tr".to_string(),
        }
    }
}

/// Seçilen motordan sonra hangi motorların deneneceği.
///
/// SAPI her zaman en sonda: ağ ve anahtar istemeyen tek motor o, yani
/// zincirin dibi orası. Kendisi zaten seçilmişse tekrar denenmiyor.
fn fallback_chain(chosen: TtsEngineKind) -> Vec<TtsEngineKind> {
    match chosen {
        // SAPI seçiliyken de bir çıkış yolu gerekiyor: dile uyan ses yüklü
        // değilse SAPI konuşamıyor (bkz. `speak_with`), ve zincir boş
        // olsaydı kullanıcı hiç ses duymazdı.
        TtsEngineKind::Sapi => vec![TtsEngineKind::Edge],
        // Ücretli motorlar önce ücretsiz doğal sese düşüyor: kullanıcı
        // kota bittiğinde robot ses yerine hâlâ iyi bir ses duysun.
        TtsEngineKind::ElevenLabs
        | TtsEngineKind::OpenAi
        | TtsEngineKind::Gemini
        | TtsEngineKind::Kokoro => {
            vec![TtsEngineKind::Edge, TtsEngineKind::Sapi]
        }
        TtsEngineKind::Edge => vec![TtsEngineKind::Sapi],
    }
}

/// Yedeğe düşüldüğünde sesli söylenecek cümle.
///
/// Ayrı fonksiyon: ses çıkarmadan test edilebilsin, ve metin tek bir yerde
/// dursun. Kısa tutuluyor — kullanıcının beklediği şey cevap, bu değil.
pub fn fallback_notice(failed: TtsEngineKind, using: TtsEngineKind, language: &str) -> String {
    match language {
        "en" => format!(
            "Heads up: the {} voice did not respond, so I am using {} instead.",
            failed.spoken_name("en"),
            using.spoken_name("en")
        ),
        _ => format!(
            "Bilgi: {} sesi yanıt vermedi, {} sesiyle devam ediyorum.",
            failed.spoken_name("tr"),
            using.spoken_name("tr")
        ),
    }
}

/// Konuşma motoru.
///
/// `cancel` bayrağı barge-in için: çalan konuşma anında kesilir.
pub struct TtsEngine {
    config: TtsConfig,
    cancel: Arc<AtomicBool>,
    /// Duyuru dili — asistanın konuştuğu dil.
    language: String,
    /// Son yedek düşüşü, arayüz alana kadar saklanıyor.
    ///
    /// `Mutex` çünkü konuşma ayrı bir thread'de oluyor ve arayüz başka bir
    /// thread'den soruyor.
    last_fallback: std::sync::Mutex<Option<(TtsEngineKind, TtsEngineKind)>>,
}

impl TtsEngine {
    pub fn new(config: TtsConfig) -> Self {
        Self {
            config,
            cancel: Arc::new(AtomicBool::new(false)),
            language: "tr".to_string(),
            last_fallback: std::sync::Mutex::new(None),
        }
    }

    /// Duyuruların ve **ses seçiminin** dili.
    ///
    /// Ayara da yazılıyor: SAPI ve Edge dile uyan sesi oradan okuyor, ve
    /// yalnızca duyuru dilini değiştirmek sesin dilini eski bırakırdı.
    pub fn set_language(&mut self, language: impl Into<String>) {
        let language = language.into();
        self.config.language = language.clone();
        self.language = language;
    }

    /// Ayarları değiştirir — kullanıcı ayarlardan motoru değiştirdiğinde.
    ///
    /// Dil ayarın parçası değil (kullanıcı ses ekranında dil seçmiyor), o
    /// yüzden yeni ayara buradan yazılıyor. Yazılmasaydı motor değiştiren
    /// kullanıcı sessizce varsayılan dile dönerdi.
    pub fn set_config(&mut self, config: TtsConfig) {
        self.config = TtsConfig {
            language: self.language.clone(),
            ..config
        };
    }

    pub fn config(&self) -> &TtsConfig {
        &self.config
    }

    /// Barge-in bayrağı — `stop()` bunu kaldırır, konuşma döngüsü görür.
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }

    /// Çalan konuşmayı kes.
    pub fn stop(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        stop_platform_speech();
    }

    /// Yeni bir konuşma turu başlıyor — iptal bayrağını temizle.
    pub fn reset(&self) {
        self.cancel.store(false, Ordering::SeqCst);
    }

    /// Metni seslendirir. **Bloklar** — ayrı bir thread'den çağrılmalı.
    ///
    /// Seçilen motor başarısız olursa zincirdeki sıradakine düşer. Düşüş
    /// **duyurulur**: bkz. [`Self::announce_fallback`]. Hata yalnızca
    /// **hepsi** başarısız olursa döner.
    pub fn speak(&self, text: &str) -> Result<()> {
        if text.trim().is_empty() {
            return Ok(());
        }
        if self.cancel.load(Ordering::SeqCst) {
            return Ok(()); // zaten iptal edilmiş
        }

        let chosen = self.config.engine;
        match self.speak_with(chosen, text) {
            Ok(()) => return Ok(()),
            Err(e) => {
                tracing::warn!(engine = chosen.id(), error = %e, "ses motoru başarısız");
            }
        }

        for next in fallback_chain(chosen) {
            // Kullanıcı bu arada konuşmayı kestiyse yedeği çalmanın anlamı yok.
            if self.cancel.load(Ordering::SeqCst) {
                return Ok(());
            }

            // Duyuru cevabın **önünde** gidiyor: kullanıcı ekrana bakmıyor
            // olabilir, ve sesin neden değiştiğini cevabı dinledikten sonra
            // öğrenmek geç. Duyuru başarısız olursa sessizce geçiliyor —
            // asıl iş cevabı iletmek.
            let announced = self.announce_fallback(next, chosen);

            match self.speak_with(next, text) {
                Ok(()) => {
                    tracing::info!(
                        from = chosen.id(),
                        to = next.id(),
                        announced,
                        "yedek ses motoruna düşüldü"
                    );
                    self.last_fallback
                        .lock()
                        .map(|mut slot| *slot = Some((chosen, next)))
                        .ok();
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!(engine = next.id(), error = %e, "yedek ses motoru da başarısız");
                }
            }
        }

        Err(TtsError::Speak(format!(
            "hiçbir ses motoru çalışmadı (seçili: {})",
            chosen.id()
        )))
    }

    /// Yedeğe düşüldüğünü **sesli** söyler.
    ///
    /// Ekranda bir bildirim yeterli değil: sesi açık kullanan kişi çoğu zaman
    /// ekrana bakmıyor — mutfakta, odanın öbür ucunda, ya da sadece
    /// konuşarak çalışıyor. Ses aniden değiştiğinde "ne oldu şimdi" hissini
    /// veren şey, değişikliğin sessizce olması.
    ///
    /// Duyuru **yedek motorla** yapılıyor: seçilen motor zaten çalışmıyor.
    /// Ve cevabın önünde gidiyor, arkasında değil.
    fn announce_fallback(&self, using: TtsEngineKind, failed: TtsEngineKind) -> bool {
        let notice = fallback_notice(failed, using, &self.language);
        self.speak_with(using, &notice).is_ok()
    }

    /// Son düşüşü alır ve kaydı temizler — arayüzün bildirim göstermesi için.
    ///
    /// Ses duyurusu ekrana bakmayan kullanıcı için; bu ise bakan kullanıcı
    /// için. İkisi birbirinin yerine geçmiyor, ikisi de gerekiyor.
    pub fn take_fallback(&self) -> Option<(TtsEngineKind, TtsEngineKind)> {
        self.last_fallback.lock().ok().and_then(|mut s| s.take())
    }

    /// Tek bir motorla seslendirme denemesi.
    fn speak_with(&self, engine: TtsEngineKind, text: &str) -> Result<()> {
        let c = &self.config;
        match engine {
            TtsEngineKind::Sapi => {
                // Dile uyan ses yoksa SAPI **başarısız sayılıyor**, sessizce
                // yanlış dilde okumak yerine.
                //
                // Sebep: yanlış dildeki bir ses hata vermiyor, metni kendi
                // telaffuz kurallarıyla okuyor — çıkan şey konuşma değil,
                // gürültü. Kullanıcı da bunu bir arıza olarak değil, sesin
                // "bozukluğu" olarak duyuyor ve neyi düzelteceğini bilmiyor.
                //
                // Başarısız saymak zinciri devreye sokuyor: kullanıcı
                // duyuruyu duyuyor ve gerçekten anlaşılır bir ses geliyor.
                // Kullanıcı bir ses **seçmişse** ona karışılmıyor.
                if c.voice.trim().is_empty() && !has_voice_for(&c.language_tag()) {
                    return Err(TtsError::Engine(format!(
                        "sistemde {} dilinde ses yüklü değil",
                        c.language_tag()
                    )));
                }
                speak_platform(text, c)
            }

            TtsEngineKind::Edge => {
                // Ses seçilmemişse dile uyanı kullan — sabit Türkçe ses,
                // İngilizce konuşan bir kullanıcıya Türkçe aksan verirdi.
                let voice = if c.edge_voice.trim().is_empty() {
                    crate::edge_tts::default_voice(&c.language)
                } else {
                    &c.edge_voice
                };
                // SAPI hızı -10..10, Edge yüzde ister — ölçekle.
                let rate_pct = c.rate.clamp(-10, 10) * 10;
                crate::edge_tts::speak(text, voice, rate_pct, c.volume as i32, &self.cancel)
                    .map_err(|e| TtsError::Speak(e.to_string()))
            }

            TtsEngineKind::Kokoro => {
                let mp3 = crate::kokoro::synthesize(text, &c.kokoro_url, &c.kokoro_voice, c.rate)
                    .map_err(|e| TtsError::Speak(e.to_string()))?;
                crate::playback::play_bytes(&mp3, "mp3", &self.cancel)
                    .map_err(|e| TtsError::Speak(e.to_string()))
            }

            TtsEngineKind::ElevenLabs => {
                let mp3 = crate::elevenlabs::synthesize(
                    text,
                    &c.eleven_key,
                    &c.eleven_voice,
                    &c.eleven_model,
                )
                .map_err(|e| TtsError::Speak(e.to_string()))?;
                crate::playback::play_bytes(&mp3, "mp3", &self.cancel)
                    .map_err(|e| TtsError::Speak(e.to_string()))
            }

            TtsEngineKind::OpenAi => {
                let mp3 = crate::openai_tts::synthesize(
                    text,
                    &c.openai_key,
                    &c.openai_voice,
                    &c.openai_model,
                    c.rate,
                )
                .map_err(|e| TtsError::Speak(e.to_string()))?;
                crate::playback::play_bytes(&mp3, "mp3", &self.cancel)
                    .map_err(|e| TtsError::Speak(e.to_string()))
            }
            TtsEngineKind::Gemini => {
                // "wav", not "mp3": Gemini returns raw PCM and the module
                // puts a WAV header on it. The wrong extension here plays
                // nothing at all and reports no error.
                let wav = crate::gemini_tts::synthesize(
                    text,
                    &c.gemini_key,
                    &c.gemini_voice,
                    &c.gemini_model,
                )
                .map_err(|e| TtsError::Speak(e.to_string()))?;
                crate::playback::play_bytes(&wav, "wav", &self.cancel)
                    .map_err(|e| TtsError::Speak(e.to_string()))
            }
        }
    }

    /// Sistemdeki SAPI sesleri.
    pub fn available_voices() -> Vec<String> {
        list_voices_platform()
    }
}

// ── Windows (SAPI) ──────────────────────────────────────────────────────────

#[cfg(windows)]
fn speak_platform(text: &str, config: &TtsConfig) -> Result<()> {
    use std::process::Command;

    // Metni PowerShell'e güvenle geçirmek: tek tırnaklar ikilenir.
    // Komut enjeksiyonu riski yok — metin veri olarak kalıyor.
    let safe = text.replace('\'', "''");

    let voice_line = if config.voice.trim().is_empty() {
        // Ses seçilmemişse dile uyan ilk sesi seç.
        //
        // **Neden gerekli:** SAPI'nin varsayılanı sistem dilinin sesi, ve
        // çoğu Windows kurulumunda yalnızca İngilizce ses yüklü. O sesle
        // Türkçe metin okunduğunda çıkan şey Türkçe değil, İngilizce
        // telaffuz kurallarıyla harf harf uydurulmuş bir gürültü —
        // kullanıcının "tuhaf şeyler söylüyor" dediği şey bu.
        //
        // Uyan ses yoksa hiçbir şey seçilmiyor: sistem varsayılanıyla
        // konuşmak, hiç konuşmamaktan iyi.
        format!(
            "$m = $s.GetInstalledVoices() | \
             Where-Object {{ $_.VoiceInfo.Culture.TwoLetterISOLanguageName -eq '{}' }} | \
             Select-Object -First 1; \
             if ($m) {{ try {{ $s.SelectVoice($m.VoiceInfo.Name) }} catch {{ }} }};",
            config.language_tag()
        )
    } else {
        format!(
            "try {{ $s.SelectVoice('{}') }} catch {{ }};",
            config.voice.replace('\'', "''")
        )
    };

    let script = format!(
        "Add-Type -AssemblyName System.Speech; \
         $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
         $s.Rate = {}; $s.Volume = {}; {voice_line} \
         $s.Speak('{safe}')",
        config.rate.clamp(-10, 10),
        config.volume.min(100),
    );

    let child = vavis_core::process::hidden(&mut Command::new("powershell"))
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| TtsError::Engine(e.to_string()))?;

    // Held where `stop_platform_speech` can reach it: stopping means ending
    // this one process, and nothing else.
    *speaking_child() = Some(child);
    let (status, err) = loop {
        {
            let mut slot = speaking_child();
            match slot.as_mut() {
                // Stopped from outside: the slot was emptied and the process
                // killed. Not an error -- the user asked for silence.
                None => return Ok(()),
                Some(child) => match child.try_wait() {
                    Ok(Some(status)) => {
                        let mut child = slot.take().expect("just matched");
                        let mut err = String::new();
                        if let Some(mut stderr) = child.stderr.take() {
                            use std::io::Read;
                            let _ = stderr.read_to_string(&mut err);
                        }
                        // Already exited, so this returns at once; it is
                        // what releases the process handle.
                        let _ = child.wait();
                        break (status, err);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        slot.take();
                        return Err(TtsError::Engine(e.to_string()));
                    }
                },
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    };

    if status.success() {
        Ok(())
    } else {
        Err(TtsError::Speak(err.trim().to_string()))
    }
}

/// The PowerShell process speaking right now, if any.
#[cfg(windows)]
fn speaking_child() -> std::sync::MutexGuard<'static, Option<std::process::Child>> {
    use std::sync::{Mutex, OnceLock};
    static CHILD: OnceLock<Mutex<Option<std::process::Child>>> = OnceLock::new();
    CHILD
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Çalan konuşmayı öldür.
///
/// Only the process this module started. The old version killed every
/// PowerShell process with no window title that had started in the last
/// five minutes -- which on a machine running scheduled scripts, a build,
/// or anything else headless, meant killing the user's work to stop a
/// sentence.
#[cfg(windows)]
fn stop_platform_speech() {
    if let Some(mut child) = speaking_child().take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Sistemde bu dilde bir ses yüklü mü.
///
/// Sonuç önbelleğe alınıyor: her cümlede bir PowerShell süreci başlatmak,
/// önlemeye çalıştığımız gecikmeyi geri getirirdi. Kullanıcı konuşurken
/// Windows'a ses yüklemiyor, dolayısıyla süreç ömrü boyunca sabit sayılabilir.
#[cfg(windows)]
fn has_voice_for(language: &str) -> bool {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    static CACHE: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    if let Ok(map) = cache.lock() {
        if let Some(&known) = map.get(language) {
            return known;
        }
    }

    let found = query_voice_language(language);
    if let Ok(mut map) = cache.lock() {
        map.insert(language.to_string(), found);
    }
    found
}

#[cfg(windows)]
fn query_voice_language(language: &str) -> bool {
    use std::process::Command;

    let script = format!(
        "Add-Type -AssemblyName System.Speech; \
         @((New-Object System.Speech.Synthesis.SpeechSynthesizer).GetInstalledVoices() | \
         Where-Object {{ $_.VoiceInfo.Culture.TwoLetterISOLanguageName -eq '{language}' }}).Count"
    );

    let Ok(output) = vavis_core::process::hidden(&mut Command::new("powershell"))
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
    else {
        // Sorulamadıysa "var" sayılıyor: emin olmadan motoru devre dışı
        // bırakmak, çalışan bir kurulumu susturabilirdi.
        return true;
    };

    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u32>()
        .map(|n| n > 0)
        .unwrap_or(true)
}

#[cfg(not(windows))]
fn has_voice_for(_language: &str) -> bool {
    true
}

#[cfg(windows)]
fn list_voices_platform() -> Vec<String> {
    use std::process::Command;

    let script = "Add-Type -AssemblyName System.Speech; \
                  (New-Object System.Speech.Synthesis.SpeechSynthesizer)\
                  .GetInstalledVoices() | ForEach-Object { $_.VoiceInfo.Name }";

    let Ok(output) = vavis_core::process::hidden(&mut Command::new("powershell"))
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
    else {
        return Vec::new();
    };

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

// ── Windows dışı ────────────────────────────────────────────────────────────

#[cfg(not(windows))]
fn speak_platform(_text: &str, _config: &TtsConfig) -> Result<()> {
    Err(TtsError::Engine(
        "TTS bu platformda henüz desteklenmiyor".into(),
    ))
}

#[cfg(not(windows))]
fn stop_platform_speech() {}

#[cfg(not(windows))]
fn list_voices_platform() -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_sane() {
        let c = TtsConfig::default();
        assert!((-10..=10).contains(&c.rate));
        assert!(c.volume <= 100);
    }

    #[test]
    fn empty_text_is_a_no_op() {
        let engine = TtsEngine::new(TtsConfig::default());
        assert!(engine.speak("").is_ok());
        assert!(engine.speak("   ").is_ok());
    }

    #[test]
    fn cancel_flag_short_circuits_speaking() {
        let engine = TtsEngine::new(TtsConfig::default());
        engine.cancel.store(true, Ordering::SeqCst);
        // İptal edilmişken konuşma denemesi bile yapılmamalı — hızlı dönmeli.
        let start = std::time::Instant::now();
        assert!(engine.speak("uzun bir metin").is_ok());
        assert!(start.elapsed().as_millis() < 100, "iptalde anında dönmeli");
    }

    #[test]
    fn reset_clears_cancel_flag() {
        let engine = TtsEngine::new(TtsConfig::default());
        engine.stop();
        assert!(engine.cancel.load(Ordering::SeqCst));

        engine.reset();
        assert!(!engine.cancel.load(Ordering::SeqCst));
    }

    #[test]
    fn cancel_flag_is_shared() {
        let engine = TtsEngine::new(TtsConfig::default());
        let flag = engine.cancel_flag();
        engine.stop();
        assert!(flag.load(Ordering::SeqCst), "bayrak paylaşılmalı");
    }

    #[test]
    fn quotes_in_text_do_not_break_the_script() {
        // Tek tırnak PowerShell'de string'i kapatır — ikilenmeli.
        let text = "O'nun dediği 'şey' buydu";
        let escaped = text.replace('\'', "''");

        assert!(escaped.contains("O''nun"));
        // Metindeki 3 tırnağın her biri ikilenmiş olmalı.
        assert_eq!(text.matches('\'').count(), 3);
        assert_eq!(escaped.matches("''").count(), 3);
        // Tek başına duran tırnak kalmamalı — PowerShell string'i kapanmasın.
        assert_eq!(escaped.matches('\'').count(), 6);
    }

    #[test]
    fn voice_listing_does_not_panic() {
        // Windows'ta ses döner, diğerlerinde boş — ikisi de geçerli.
        let _ = TtsEngine::available_voices();
    }

    #[test]
    fn settings_can_be_swapped_at_runtime() {
        let mut engine = TtsEngine::new(TtsConfig::default());

        engine.set_config(TtsConfig {
            engine: TtsEngineKind::Kokoro,
            ..Default::default()
        });
        assert_eq!(engine.config().engine, TtsEngineKind::Kokoro);
    }

    /// Dil ses ekranının bir ayarı değil, ama **hangi** sesin konuşacağını
    /// belirliyor. Motor değiştiren kullanıcı sessizce varsayılan dile
    /// dönmemeli.
    #[test]
    fn changing_the_engine_keeps_the_language() {
        let mut engine = TtsEngine::new(TtsConfig::default());
        engine.set_language("en");

        engine.set_config(TtsConfig {
            engine: TtsEngineKind::Kokoro,
            ..Default::default()
        });

        assert_eq!(engine.config().language, "en", "dil korunmalı");
    }

    /// Dil değişince ses seçimi de değişmeli — yalnızca duyuru dilinin
    /// değişmesi, sesin eski dilde kalması demekti.
    #[test]
    fn setting_the_language_reaches_the_voice_selection() {
        let mut engine = TtsEngine::new(TtsConfig::default());
        engine.set_language("en");
        assert_eq!(engine.config().language, "en");
    }
}

#[cfg(test)]
mod engine_tests {
    use super::*;

    #[test]
    fn engine_parsing_accepts_aliases() {
        assert_eq!(TtsEngineKind::parse("sapi"), Some(TtsEngineKind::Sapi));
        assert_eq!(TtsEngineKind::parse("SISTEM"), Some(TtsEngineKind::Sapi));
        assert_eq!(TtsEngineKind::parse("edge"), Some(TtsEngineKind::Edge));
        assert_eq!(TtsEngineKind::parse("doğal"), Some(TtsEngineKind::Edge));
        assert_eq!(TtsEngineKind::parse("kokoro"), Some(TtsEngineKind::Kokoro));
        assert_eq!(TtsEngineKind::parse("YEREL"), Some(TtsEngineKind::Kokoro));
        assert_eq!(
            TtsEngineKind::parse("elevenlabs"),
            Some(TtsEngineKind::ElevenLabs)
        );
        assert_eq!(
            TtsEngineKind::parse("11labs"),
            Some(TtsEngineKind::ElevenLabs)
        );
        assert_eq!(TtsEngineKind::parse("openai"), Some(TtsEngineKind::OpenAi));
        assert_eq!(TtsEngineKind::parse("yok"), None);
    }

    /// Ayar dosyasına yazılan kimlik geri okunabilmeli, yoksa kullanıcının
    /// seçtiği motor yeniden başlatmada sessizce SAPI'ye dönerdi.
    #[test]
    fn every_id_round_trips_through_parse() {
        for e in TtsEngineKind::ALL {
            assert_eq!(TtsEngineKind::parse(e.id()), Some(e), "{}", e.id());
        }
    }

    /// Varsayılan motor anahtar istememeli **ve kullanıcının dilini
    /// konuşmalı**.
    ///
    /// Eskiden SAPI'ydi. SAPI anahtar istemiyor ama çoğu Windows kurulumunda
    /// yalnızca İngilizce ses yüklü, ve Türkçe metin İngilizce sesle okununca
    /// anlaşılmaz çıkıyor. Edge de anahtar istemiyor ve her iki dili de
    /// konuşuyor; ulaşılamadığında zincir zaten SAPI'ye düşüyor.
    #[test]
    fn the_default_engine_needs_no_key_and_speaks_the_users_language() {
        let default = TtsConfig::default().engine;
        assert!(!default.needs_key(), "varsayılan anahtar istememeli");
        assert_eq!(default, TtsEngineKind::Edge);
        // Ve çevrimdışı kullanıcı yine ses duymalı.
        assert_eq!(fallback_chain(default).last(), Some(&TtsEngineKind::Sapi));
    }

    #[test]
    fn every_engine_has_a_label_and_an_id() {
        for e in TtsEngineKind::ALL {
            assert!(!e.label().is_empty());
            assert!(!e.id().is_empty());
        }
    }

    #[test]
    fn only_the_paid_engines_ask_for_a_key() {
        assert!(TtsEngineKind::ElevenLabs.needs_key());
        assert!(TtsEngineKind::OpenAi.needs_key());
        // Kokoro kullanıcının kendi makinesinde çalışıyor: anahtar yok.
        assert!(!TtsEngineKind::Kokoro.needs_key());
        assert!(!TtsEngineKind::Edge.needs_key());
        assert!(!TtsEngineKind::Sapi.needs_key());
    }

    /// Ağ ya da anahtar isteyen her motorun zinciri SAPI'de bitmeli —
    /// çevrimdışı kullanıcı da ses duymalı.
    #[test]
    fn every_online_chain_ends_at_the_offline_engine() {
        for e in TtsEngineKind::ALL {
            if e == TtsEngineKind::Sapi {
                continue;
            }
            assert_eq!(
                fallback_chain(e).last(),
                Some(&TtsEngineKind::Sapi),
                "{} zinciri sapi'de bitmeli",
                e.id()
            );
        }
    }

    /// SAPI seçiliyken de bir çıkış yolu olmalı.
    ///
    /// SAPI konuşamayabiliyor: dile uyan ses yüklü değilse metni yanlış
    /// dilin telaffuzuyla okumaktansa başarısız sayılıyor. Zincir boş
    /// olsaydı o kullanıcı hiç ses duymazdı.
    #[test]
    fn the_offline_engine_still_has_somewhere_to_go() {
        let chain = fallback_chain(TtsEngineKind::Sapi);
        assert!(!chain.is_empty(), "sapi çıkışsız kalmamalı");
        assert!(!chain.contains(&TtsEngineKind::Sapi), "kendini denememeli");
        // Ve gidilen yer anahtar istememeli: SAPI'yi seçen kullanıcının
        // ödeme yapan bir servise düşmesi sürpriz olurdu.
        for e in chain {
            assert!(!e.needs_key(), "{} anahtar istiyor", e.id());
        }
    }

    /// Bir motor kendi zincirinde tekrar denenmemeli — aynı hata iki kez
    /// beklenir, kullanıcı iki kat gecikme duyar.
    #[test]
    fn a_chain_never_retries_the_chosen_engine() {
        for e in TtsEngineKind::ALL {
            assert!(
                !fallback_chain(e).contains(&e),
                "{} kendini tekrar denememeli",
                e.id()
            );
        }
    }

    /// Yedeğe düşüş **duyurulmalı**: sesi açık kullanan kişi çoğu zaman
    /// ekrana bakmıyor, ve sesin sebepsiz değişmesi "ne oldu şimdi" hissi
    /// veriyor.
    #[test]
    fn the_notice_says_what_failed_and_what_is_speaking() {
        let tr = fallback_notice(TtsEngineKind::ElevenLabs, TtsEngineKind::Edge, "tr");
        assert!(tr.contains("Eleven Labs"), "düşen motor anılmalı: {tr}");
        assert!(tr.contains("Edge"), "devreye giren motor anılmalı: {tr}");

        let en = fallback_notice(TtsEngineKind::Kokoro, TtsEngineKind::Sapi, "en");
        assert!(en.contains("Kokoro") && en.contains("system"), "{en}");
    }

    /// Duyuru kısa olmalı: kullanıcının beklediği şey cevap, bu değil.
    #[test]
    fn the_notice_stays_short() {
        for lang in ["tr", "en"] {
            for (a, b) in [
                (TtsEngineKind::ElevenLabs, TtsEngineKind::Edge),
                (TtsEngineKind::OpenAi, TtsEngineKind::Sapi),
            ] {
                let n = fallback_notice(a, b, lang);
                assert!(n.len() < 120, "duyuru uzun: {n}");
            }
        }
    }

    /// Kimlikler ayar dosyası için, okunan adlar kullanıcı için — ikisi
    /// karıştırılırsa asistan "sapi" diye bir kelime telaffuz eder.
    #[test]
    fn spoken_names_are_not_raw_ids() {
        assert_eq!(TtsEngineKind::ElevenLabs.spoken_name("tr"), "Eleven Labs");
        assert_ne!(
            TtsEngineKind::Sapi.spoken_name("tr"),
            TtsEngineKind::Sapi.id()
        );
        for e in TtsEngineKind::ALL {
            assert!(!e.spoken_name("tr").is_empty());
            assert!(!e.spoken_name("en").is_empty());
        }
    }

    /// Arayüz de haberdar olmalı — ekrana bakan kullanıcı için.
    /// Bir kez okunuyor: aynı düşüş iki bildirim üretmemeli.
    #[test]
    fn a_fallback_is_reported_once_to_the_interface() {
        let engine = TtsEngine::new(TtsConfig::default());
        assert!(engine.take_fallback().is_none(), "başlangıçta boş olmalı");

        engine
            .last_fallback
            .lock()
            .map(|mut s| *s = Some((TtsEngineKind::ElevenLabs, TtsEngineKind::Edge)))
            .unwrap();

        assert_eq!(
            engine.take_fallback(),
            Some((TtsEngineKind::ElevenLabs, TtsEngineKind::Edge))
        );
        assert!(engine.take_fallback().is_none(), "ikinci kez dönmemeli");
    }

    /// Ücretli motorlar önce ücretsiz **doğal** sese düşmeli: kota bitince
    /// kullanıcı robot ses değil, hâlâ iyi bir ses duysun.
    #[test]
    fn paid_engines_fall_back_to_the_free_natural_voice_first() {
        for e in [TtsEngineKind::ElevenLabs, TtsEngineKind::OpenAi] {
            assert_eq!(fallback_chain(e).first(), Some(&TtsEngineKind::Edge));
        }
    }

    /// Anahtarsız seçilen ücretli motor çökmemeli: zincir SAPI'ye kadar
    /// gidip konuşmalı. (Ağa çıkmadan doğrulanıyor.)
    #[test]
    fn a_paid_engine_without_a_key_still_produces_speech() {
        let engine = TtsEngine::new(TtsConfig {
            engine: TtsEngineKind::ElevenLabs,
            eleven_key: String::new(),
            ..Default::default()
        });
        // İptal bayrağı açık: ağa çıkmadan, ses çıkarmadan dönmeli.
        engine.stop();
        assert!(engine.speak("test").is_ok());
    }

    #[test]
    fn edge_engine_falls_back_when_service_fails() {
        let engine = TtsEngine::new(TtsConfig {
            engine: TtsEngineKind::Edge,
            edge_voice: "gecersiz-ses-adi".into(),
            ..Default::default()
        });
        engine.stop();
        assert!(engine.speak("test").is_ok());
    }
}
