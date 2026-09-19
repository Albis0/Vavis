//! Gemini TTS — sohbet için Gemini anahtarı girmiş olanın hazır sesi.
//!
//! Listeye eklenme sebebi OpenAI TTS ile aynı: anahtarı zaten olan biri,
//! ikinci bir hesap açmadan doğal bir sese kavuşuyor.
//!
//! **Gemini'nin "kendi sesi" bu değil.** Modelin gerçek anlamda kendi
//! konuştuğu yer Live API: `bidiGenerateContent`, WebSocket üzerinden, sesi
//! doğrudan üreten modeller (`*-native-audio-*`). Onu konuşabilmek için ayrı
//! bir taşıma katmanı gerekiyor ve bizde yok — o modeller sohbet listesinden
//! de eleniyor (bkz. `vavis_brain::provider::is_chat_model`).
//!
//! Buradaki, REST üzerinden çalışan TTS modeli: metin girer, ses çıkar.
//! Ölçüldü (2026-09-16, canlı API): `gemini-3.1-flash-tts-preview` çalışıyor.
//!
//! ## Biçim farkı — diğer motorlardan ayrıldığı yer
//!
//! Herkes MP3 döndürürken Gemini **ham PCM** döndürüyor:
//!
//! ```text
//! mimeType: "audio/l16; rate=24000; channels=1"
//! ```
//!
//! `l16` = little-endian, 16-bit imzalı örnekler. Başlık yok, yani dosyaya
//! yazıp çalmaya kalkınca hiçbir oynatıcı ne olduğunu anlamıyor — sessiz
//! başarısızlık. Bu yüzden [`wav_from_pcm`] baytların önüne bir WAV başlığı
//! ekliyor ve sonuç `.wav` olarak çalınıyor.

use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum GeminiTtsError {
    #[error("Gemini anahtarı yok — ayarlardan ekle")]
    NoKey,
    #[error("bağlanılamadı: {0}")]
    Network(String),
    #[error("anahtar reddedildi — ayarlardan kontrol et")]
    BadKey,
    #[error("kota doldu ya da hız sınırına takıldı")]
    QuotaOrRateLimit,
    #[error("sunucu hata döndü ({status}): {body}")]
    Server { status: u16, body: String },
    #[error("ses verisi boş geldi")]
    NoAudio,
}

pub type Result<T> = std::result::Result<T, GeminiTtsError>;

const TIMEOUT: Duration = Duration::from_secs(45);

/// Varsayılan TTS modeli.
///
/// Ölçüldü: `gemini-2.5-flash-preview-tts` bu hesapta cevap vermedi,
/// `gemini-3.1-flash-tts-preview` verdi.
pub const DEFAULT_MODEL: &str = "gemini-3.1-flash-tts-preview";

/// Varsayılan ses. Google'ın hazır ses listesinden.
pub const DEFAULT_VOICE: &str = "Kore";

/// Gemini'nin döndürdüğü örnekleme hızı (`rate=24000`).
///
/// Cevabın `mimeType` alanında geliyor ve oradan okunuyor; bu yalnızca
/// alan okunamazsa kullanılacak değer.
const DEFAULT_SAMPLE_RATE: u32 = 24_000;

/// Hazır sesler — (kimlik, gösterilecek ad).
pub fn voices() -> &'static [(&'static str, &'static str)] {
    &[
        ("Kore", "Kore (kadın, dengeli)"),
        ("Aoede", "Aoede (kadın, yumuşak)"),
        ("Leda", "Leda (kadın, genç)"),
        ("Zephyr", "Zephyr (kadın, parlak)"),
        ("Puck", "Puck (erkek, canlı)"),
        ("Charon", "Charon (erkek, derin)"),
        ("Fenrir", "Fenrir (erkek, sert)"),
        ("Orus", "Orus (erkek, sakin)"),
    ]
}

pub fn url(model: &str) -> String {
    let model = model.trim();
    let model = if model.is_empty() {
        DEFAULT_MODEL
    } else {
        model
    };
    format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent")
}

pub fn request_body(text: &str, voice: &str) -> serde_json::Value {
    let voice = if voice.trim().is_empty() {
        DEFAULT_VOICE
    } else {
        voice.trim()
    };
    serde_json::json!({
        "contents": [{ "parts": [{ "text": text }] }],
        "generationConfig": {
            // Metin değil ses istiyoruz; bu alan olmadan model yazıyla
            // cevap veriyor ve elimizde çalınacak bir şey olmuyor.
            "responseModalities": ["AUDIO"],
            "speechConfig": {
                "voiceConfig": { "prebuiltVoiceConfig": { "voiceName": voice } }
            }
        }
    })
}

pub fn classify(status: u16, body: &str) -> Option<GeminiTtsError> {
    match status {
        200..=299 => None,
        401 | 403 => Some(GeminiTtsError::BadKey),
        429 => Some(GeminiTtsError::QuotaOrRateLimit),
        _ => Some(GeminiTtsError::Server {
            status,
            body: body.chars().take(300).collect(),
        }),
    }
}

/// `"audio/l16; rate=24000; channels=1"` → `24000`.
///
/// Yanlış hız, sesi tiz ya da pes çalmak demek — duyulur bir hata, ama
/// sessizce olur. O yüzden varsayılana düşmeden önce cevabın kendi
/// söylediğine bakılıyor.
pub fn sample_rate_from_mime(mime: &str) -> u32 {
    mime.split(';')
        .filter_map(|part| part.trim().strip_prefix("rate="))
        .find_map(|v| v.trim().parse().ok())
        .unwrap_or(DEFAULT_SAMPLE_RATE)
}

/// Ham PCM'in önüne WAV başlığı ekler.
///
/// Gemini başlıksız `l16` gönderiyor; oynatıcılar başlıksız baytları
/// tanımıyor ve hata da vermiyor, sadece sessiz kalıyor.
///
/// 16-bit, tek kanal varsayılıyor — `audio/l16` tanımı bu, ve kanal sayısı
/// cevapta `channels=1` olarak geliyor.
pub fn wav_from_pcm(pcm: &[u8], sample_rate: u32) -> Vec<u8> {
    const CHANNELS: u16 = 1;
    const BITS: u16 = 16;

    let byte_rate = sample_rate * CHANNELS as u32 * (BITS / 8) as u32;
    let block_align = CHANNELS * (BITS / 8);
    let data_len = pcm.len() as u32;

    let mut out = Vec::with_capacity(44 + pcm.len());
    out.extend_from_slice(b"RIFF");
    // Dosya boyutu eksi "RIFF" ve bu alanın kendisi = 36 + veri.
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");

    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // PCM için parça boyu
    out.extend_from_slice(&1u16.to_le_bytes()); // 1 = sıkıştırmasız PCM
    out.extend_from_slice(&CHANNELS.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&BITS.to_le_bytes());

    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(pcm);
    out
}

/// Cevaptan ses baytlarını çıkarır — dönen: çalmaya hazır WAV.
pub fn audio_from_response(body: &str) -> Result<Vec<u8>> {
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|e| GeminiTtsError::Network(e.to_string()))?;

    let part = &v["candidates"][0]["content"]["parts"][0]["inlineData"];
    let data = part["data"].as_str().unwrap_or_default();
    if data.is_empty() {
        return Err(GeminiTtsError::NoAudio);
    }

    let pcm = base64_decode(data).ok_or(GeminiTtsError::NoAudio)?;
    if pcm.is_empty() {
        return Err(GeminiTtsError::NoAudio);
    }

    let rate = sample_rate_from_mime(part["mimeType"].as_str().unwrap_or_default());
    Ok(wav_from_pcm(&pcm, rate))
}

/// Standart base64 çözücü (dolgu karakteri `=` yok sayılır).
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits = 0u8;

    for c in s.bytes() {
        if c == b'=' || c.is_ascii_whitespace() {
            continue;
        }
        let v = TABLE.iter().position(|&t| t == c)? as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}

/// Metni sese çevirir. Dönen baytlar WAV — `play_bytes(.., "wav", ..)`.
pub fn synthesize(text: &str, key: &str, voice: &str, model: &str) -> Result<Vec<u8>> {
    if key.trim().is_empty() {
        return Err(GeminiTtsError::NoKey);
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .map_err(|e| GeminiTtsError::Network(e.to_string()))?;

    let response = client
        .post(url(model))
        // Anahtar başlıkta, sorgu dizesinde değil: URL'e giren anahtar
        // loglara ve hata gövdesine sızar (bkz. `vavis_brain::gemini`).
        .header("x-goog-api-key", key.trim())
        .header("content-type", "application/json")
        .json(&request_body(text, voice))
        .send()
        .map_err(|e| GeminiTtsError::Network(e.to_string()))?;

    let status = response.status().as_u16();
    let body = response
        .text()
        .map_err(|e| GeminiTtsError::Network(e.to_string()))?;

    if let Some(err) = classify(status, &body) {
        return Err(err);
    }

    audio_from_response(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_never_lands_in_the_url() {
        let u = url(DEFAULT_MODEL);
        assert!(!u.contains("key="), "{u}");
        assert!(u.contains(DEFAULT_MODEL), "{u}");
    }

    #[test]
    fn an_empty_model_falls_back_to_the_default() {
        assert_eq!(url(""), url(DEFAULT_MODEL));
        assert_eq!(url("  "), url(DEFAULT_MODEL));
    }

    /// Ses istemediğimizi söylemezsek model yazıyla cevap veriyor.
    #[test]
    fn the_request_asks_for_audio_not_text() {
        let b = request_body("merhaba", "Kore");
        assert_eq!(b["generationConfig"]["responseModalities"][0], "AUDIO");
        assert_eq!(
            b["generationConfig"]["speechConfig"]["voiceConfig"]["prebuiltVoiceConfig"]
                ["voiceName"],
            "Kore"
        );
    }

    #[test]
    fn an_empty_voice_falls_back_to_the_default() {
        let b = request_body("merhaba", "  ");
        assert_eq!(
            b["generationConfig"]["speechConfig"]["voiceConfig"]["prebuiltVoiceConfig"]
                ["voiceName"],
            DEFAULT_VOICE
        );
    }

    /// Yanlış örnekleme hızı sesi tiz ya da pes çalar — duyulur ama sessiz
    /// bir hata. Cevabın kendi söylediği okunmalı.
    #[test]
    fn the_sample_rate_is_read_from_the_mime_type() {
        assert_eq!(
            sample_rate_from_mime("audio/l16; rate=24000; channels=1"),
            24_000
        );
        assert_eq!(sample_rate_from_mime("audio/l16;rate=16000"), 16_000);
        // Söylenmemişse makul bir varsayılan, panik değil.
        assert_eq!(sample_rate_from_mime("audio/l16"), DEFAULT_SAMPLE_RATE);
        assert_eq!(sample_rate_from_mime(""), DEFAULT_SAMPLE_RATE);
    }

    /// Başlıksız PCM hiçbir oynatıcıda çalmıyor ve hata da vermiyor.
    #[test]
    fn the_wav_header_says_what_the_bytes_are() {
        let pcm = vec![0u8; 100];
        let wav = wav_from_pcm(&pcm, 24_000);

        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(wav.len(), 44 + pcm.len());

        // Örnekleme hızı 24. baytta, little-endian.
        assert_eq!(
            u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]),
            24_000
        );
        // Veri uzunluğu başlıkta doğru yazmalı.
        assert_eq!(
            u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]) as usize,
            pcm.len()
        );
    }

    #[test]
    fn base64_round_trips_a_known_value() {
        // "Vavis" -> "VmF2aXM="
        assert_eq!(base64_decode("VmF2aXM=").unwrap(), b"Vavis");
        assert_eq!(base64_decode("").unwrap(), Vec::<u8>::new());
        // Satır sonu içeren gövdeler de geliyor.
        assert_eq!(base64_decode("VmF2\naXM=").unwrap(), b"Vavis");
    }

    #[test]
    fn a_response_with_audio_becomes_a_playable_wav() {
        let body = r#"{"candidates":[{"content":{"parts":[{"inlineData":{
            "mimeType":"audio/l16; rate=24000; channels=1","data":"VmF2aXM="}}]}}]}"#;
        let wav = audio_from_response(body).unwrap();
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[44..], b"Vavis");
    }

    /// Model yazıyla cevap verdiyse elimizde ses yok — sessizce boş bir
    /// dosya çalmak yerine söylenmeli.
    #[test]
    fn a_text_only_reply_is_reported_as_missing_audio() {
        let body = r#"{"candidates":[{"content":{"parts":[{"text":"merhaba"}]}}]}"#;
        assert!(matches!(
            audio_from_response(body),
            Err(GeminiTtsError::NoAudio)
        ));
    }

    #[test]
    fn a_bad_key_is_named_rather_than_shown_raw() {
        assert!(matches!(
            classify(401, "whatever"),
            Some(GeminiTtsError::BadKey)
        ));
        assert!(matches!(
            classify(429, "whatever"),
            Some(GeminiTtsError::QuotaOrRateLimit)
        ));
        assert!(classify(200, "").is_none());
    }

    #[test]
    fn synthesis_without_a_key_fails_before_the_network() {
        assert!(matches!(
            synthesize("merhaba", "  ", "Kore", ""),
            Err(GeminiTtsError::NoKey)
        ));
    }
}
