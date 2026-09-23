//! Ses → metin (STT).
//!
//! Groq'un Whisper uç noktası kullanılıyor: hızlı, ucuz, Türkçe'de iyi.
//! Zaten LLM için Groq anahtarı var — ek kurulum gerekmiyor.
//!
//! Yerel Whisper (whisper-rs) F6'da eklenebilir; F4'te **çalışan** yolu
//! kuruyoruz, model indirme derdi olmadan. (Eski projede Kokoro indirmesi
//! başarısız oluyordu — manuel test bulgusu.)

use crate::capture::{Utterance, SAMPLE_RATE};

#[derive(Debug, thiserror::Error)]
pub enum SttError {
    #[error("API anahtarı yok")]
    MissingKey,
    #[error("ağ hatası: {0}")]
    Network(#[from] reqwest::Error),
    #[error("tanıma başarısız ({status}): {body}")]
    Api { status: u16, body: String },
}

pub type Result<T> = std::result::Result<T, SttError>;

pub struct SttClient {
    http: reqwest::Client,
}

impl Default for SttClient {
    fn default() -> Self {
        Self::new()
    }
}

impl SttClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .expect("http istemcisi"),
        }
    }

    /// Sesi metne çevirir.
    ///
    /// `language`: "tr" | "en" … — belirtmek doğruluğu belirgin artırır.
    pub async fn transcribe(
        &self,
        utterance: &Utterance,
        api_key: &str,
        language: &str,
    ) -> Result<String> {
        if api_key.trim().is_empty() {
            return Err(SttError::MissingKey);
        }

        let wav = to_wav(&utterance.samples);

        let part = reqwest::multipart::Part::bytes(wav)
            .file_name("ses.wav")
            .mime_str("audio/wav")
            .expect("geçerli mime");

        let form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("model", "whisper-large-v3-turbo")
            .text("language", language.to_string())
            .text("response_format", "json")
            // Sıcaklık 0: uydurma yerine "duyduğunu yaz".
            .text("temperature", "0");

        let resp = self
            .http
            .post("https://api.groq.com/openai/v1/audio/transcriptions")
            .bearer_auth(api_key)
            .multipart(form)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(SttError::Api {
                status: status.as_u16(),
                body: body.chars().take(300).collect(),
            });
        }

        #[derive(serde::Deserialize)]
        struct TranscriptionResponse {
            #[serde(default)]
            text: String,
        }

        let parsed: TranscriptionResponse = resp.json().await.unwrap_or(TranscriptionResponse {
            text: String::new(),
        });

        Ok(clean_transcript(&parsed.text))
    }
}

/// A Gemini model for transcription when there is no Groq key. Any
/// current Flash model hears audio; this is the one the chat side defaults
/// to, and a user without Groq is likely to have this key.
pub const GEMINI_STT_MODEL: &str = "gemini-3.6-flash";

impl SttClient {
    /// Transcribes with Gemini: the audio goes inline in an ordinary
    /// `generateContent` request, with an instruction to write down exactly
    /// what was said.
    ///
    /// Slower than Whisper on Groq and not built for it, but it means voice
    /// works for someone whose only key is the free Gemini one -- before,
    /// no Groq key meant no voice at all.
    pub async fn transcribe_gemini(
        &self,
        utterance: &Utterance,
        api_key: &str,
        language: &str,
    ) -> Result<String> {
        use base64::Engine;
        if api_key.trim().is_empty() {
            return Err(SttError::MissingKey);
        }
        let wav = to_wav(&utterance.samples);
        let data = base64::engine::general_purpose::STANDARD.encode(wav);
        let body = serde_json::json!({
            "contents": [{"role": "user", "parts": [
                {"inline_data": {"mime_type": "audio/wav", "data": data}},
                {"text": gemini_instruction(language)}
            ]}],
            "generationConfig": {"temperature": 0}
        });
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{GEMINI_STT_MODEL}:generateContent"
        );
        let resp = self
            .http
            .post(url)
            .header("x-goog-api-key", api_key)
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(SttError::Api {
                status: status.as_u16(),
                body: body.chars().take(300).collect(),
            });
        }
        let v: serde_json::Value = resp.json().await.unwrap_or_default();
        Ok(clean_transcript(&gemini_text(&v)))
    }
}

fn gemini_instruction(language: &str) -> String {
    format!(
        "Transcribe this recording verbatim. The speaker most likely speaks the \
         language with code '{language}'. Output only the words that were said, \
         with no quotes, labels or commentary. If nothing intelligible was said, \
         output nothing."
    )
}

/// The text of a `generateContent` answer: every text part of the first
/// candidate, joined.
fn gemini_text(v: &serde_json::Value) -> String {
    v["candidates"][0]["content"]["parts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p["text"].as_str())
        .collect::<Vec<_>>()
        .join("")
}

/// Whisper çıktısını temizler.
///
/// Whisper sessizlikte halüsinasyon görür — "Altyazı M.K.", "Abone olun" gibi
/// eğitim verisinden gelen kalıplar üretir. Bunlar filtrelenmezse asistan
/// kendi kendine konuşmaya başlar.
fn clean_transcript(raw: &str) -> String {
    let text = raw.trim();

    const HALLUCINATIONS: &[&str] = &[
        "altyazı m.k.",
        "altyazı m.k",
        "abone olmayı unutmayın",
        "abone ol",
        "izlediğiniz için teşekkürler",
        "thanks for watching",
        "subtitles by",
        "amara.org",
        "www.",
        "[müzik]",
        "[music]",
        "♪",
    ];

    let lower = text.to_lowercase();
    for h in HALLUCINATIONS {
        if lower == *h || (lower.contains(h) && lower.chars().count() < h.chars().count() + 15) {
            tracing::debug!(text, "whisper halüsinasyonu filtrelendi");
            return String::new();
        }
    }

    // Tek karakterlik/anlamsız çıktılar.
    if text.chars().filter(|c| c.is_alphanumeric()).count() < 2 {
        return String::new();
    }

    text.to_string()
}

/// f32 örnekleri 16-bit PCM WAV'a çevirir.
///
/// Tampon önceden ayrılır — parça parça büyütmek yerine tek tahsis.
fn to_wav(samples: &[f32]) -> Vec<u8> {
    let data_len = samples.len() * 2;
    let mut wav = Vec::with_capacity(44 + data_len);

    // RIFF başlığı
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&((36 + data_len) as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // fmt bölümü
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // bölüm boyu
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // bayt/saniye
    wav.extend_from_slice(&2u16.to_le_bytes()); // blok hizası
    wav.extend_from_slice(&16u16.to_le_bytes()); // bit derinliği

    // data bölümü
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data_len as u32).to_le_bytes());

    for &s in samples {
        // Kırpma şart: sınır dışı değerler sarmalanıp gürültü yapar.
        let clamped = s.clamp(-1.0, 1.0);
        let pcm = (clamped * i16::MAX as f32) as i16;
        wav.extend_from_slice(&pcm.to_le_bytes());
    }

    wav
}

/// Uyandırma kelimesi algılama.
///
/// Manuel testte kullanıcı "önceden aegis'i algılamıyordu, artık algılıyor"
/// demişti. Yeni ad "Vavis" — Whisper bunu farklı yazabilir, o yüzden
/// **birden çok yazım** kabul ediliyor.
pub fn contains_wake_word(text: &str, wake_word: &str) -> bool {
    let normalized = normalize_for_wake(text);
    let target = normalize_for_wake(wake_word);

    if normalized.contains(&target) {
        return true;
    }

    // "Vavis" için yaygın yanlış duymalar.
    if target == "vavis" {
        const VARIANTS: &[&str] = &[
            "veyvis", "vavis", "vabis", "davis", "wavis", "vavız", "vavis",
        ];
        return VARIANTS
            .iter()
            .any(|v| normalized.contains(&normalize_for_wake(v)));
    }

    false
}

fn normalize_for_wake(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter_map(|c| match c {
            'ı' | 'î' => Some('i'),
            'ş' => Some('s'),
            'ğ' => Some('g'),
            'ü' => Some('u'),
            'ö' => Some('o'),
            'ç' => Some('c'),
            'â' => Some('a'),
            c if c.is_alphanumeric() || c.is_whitespace() => Some(c),
            _ => None,
        })
        .collect()
}

/// Uyandırma kelimesini metinden çıkarır — asıl istek kalır.
pub fn strip_wake_word(text: &str, wake_word: &str) -> String {
    let lower = normalize_for_wake(text);
    let target = normalize_for_wake(wake_word);

    let Some(pos) = lower.find(&target) else {
        return text.trim().to_string();
    };

    // Uyandırma kelimesinden sonrasını al.
    let after = &text[(pos + target.len()).min(text.len())..];
    after
        .trim_start_matches([',', '.', '!', '?', ' ', ':'])
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_is_well_formed() {
        let wav = to_wav(&[0.0, 0.5, -0.5]);

        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(wav.len(), 44 + 6, "3 örnek = 6 bayt veri");
    }

    #[test]
    fn wav_encodes_sample_rate() {
        let wav = to_wav(&[0.0]);
        let rate = u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]);
        assert_eq!(rate, SAMPLE_RATE);
    }

    #[test]
    fn out_of_range_samples_are_clamped_not_wrapped() {
        // Kırpma olmazsa +2.0 negatife sarmalanır ve çat diye ses çıkar.
        let wav = to_wav(&[2.0, -2.0]);
        let first = i16::from_le_bytes([wav[44], wav[45]]);
        let second = i16::from_le_bytes([wav[46], wav[47]]);

        assert!(first > 30_000, "pozitif tepe kırpılmalı, gelen: {first}");
        assert!(second < -30_000, "negatif tepe kırpılmalı, gelen: {second}");
    }

    #[test]
    fn empty_samples_still_produce_valid_header() {
        let wav = to_wav(&[]);
        assert_eq!(wav.len(), 44);
        assert_eq!(&wav[0..4], b"RIFF");
    }

    // ── Halüsinasyon filtresi ────────────────────────────────────────────

    #[test]
    fn a_gemini_answer_is_read_from_its_parts() {
        let v = serde_json::json!({"candidates": [{"content": {"parts": [
            {"text": "hava "}, {"text": "nasıl"}
        ]}}]});
        assert_eq!(gemini_text(&v), "hava nasıl");
        assert_eq!(gemini_text(&serde_json::json!({})), "");
        assert!(gemini_instruction("tr").contains("'tr'"));
    }

    #[test]
    fn whisper_hallucinations_are_filtered() {
        for junk in [
            "Altyazı M.K.",
            "altyazı m.k.",
            "Abone olmayı unutmayın",
            "Thanks for watching",
            "♪",
        ] {
            assert!(
                clean_transcript(junk).is_empty(),
                "'{junk}' filtrelenmeliydi"
            );
        }
    }

    #[test]
    fn real_speech_survives_the_filter() {
        for real in [
            "saat kaç",
            "bilgisayarın durumu nasıl",
            "bana bir şarkı çal",
        ] {
            assert_eq!(clean_transcript(real), real, "gerçek konuşma korunmalı");
        }
    }

    #[test]
    fn near_empty_transcripts_are_dropped() {
        assert!(clean_transcript("").is_empty());
        assert!(clean_transcript("   ").is_empty());
        assert!(clean_transcript(".").is_empty());
        assert!(clean_transcript("a").is_empty());
    }

    // ── Uyandırma kelimesi ───────────────────────────────────────────────

    #[test]
    fn wake_word_is_detected() {
        assert!(contains_wake_word("Vavis, saat kaç", "vavis"));
        assert!(contains_wake_word("vavis hava nasıl", "vavis"));
        assert!(contains_wake_word("VAVIS!", "vavis"));
    }

    #[test]
    fn common_mishearings_still_wake() {
        // Whisper "Vavis"i farklı duyabilir — kullanıcı tekrar tekrar
        // denemek zorunda kalmamalı.
        for variant in ["veyvis, saat kaç", "Davis saat kaç", "vavız naber"] {
            assert!(
                contains_wake_word(variant, "vavis"),
                "'{variant}' uyandırmalıydı"
            );
        }
    }

    #[test]
    fn unrelated_speech_does_not_wake() {
        for text in ["saat kaç", "hava nasıl", "merhaba dünya"] {
            assert!(!contains_wake_word(text, "vavis"), "'{text}' uyandırmamalı");
        }
    }

    #[test]
    fn wake_word_is_stripped_from_the_request() {
        assert_eq!(strip_wake_word("Vavis, saat kaç", "vavis"), "saat kaç");
        assert_eq!(strip_wake_word("vavis hava nasıl", "vavis"), "hava nasıl");
        assert_eq!(strip_wake_word("Vavis: ışığı aç", "vavis"), "ışığı aç");
    }

    #[test]
    fn stripping_without_wake_word_returns_original() {
        assert_eq!(strip_wake_word("saat kaç", "vavis"), "saat kaç");
    }

    #[tokio::test]
    async fn missing_key_fails_before_network_call() {
        let client = SttClient::new();
        let utterance = Utterance {
            samples: vec![0.0; 16000],
        };
        let err = client.transcribe(&utterance, "", "tr").await.unwrap_err();
        assert!(matches!(err, SttError::MissingKey));
    }
}
