//! Ses verisini çalar — bütün motorların ortak çıkış yolu.
//!
//! # Neden ayrı bir modül
//!
//! Her motor (Edge, Kokoro, ElevenLabs, OpenAI) sonunda aynı şeyi üretiyor:
//! bellekte bir ses dosyası. Bunu çalmak, barge-in için ortasında kesebilmek
//! ve geçici dosyayı temizlemek her seferinde aynı iş — ve dört kez yazılırsa
//! dört kez farklı biçimde yanlış yazılır.
//!
//! Edge TTS bu kodun ilk sahibiydi; buraya taşındı ve diğerleri de aynı
//! yolu kullanıyor. Yani barge-in bir motorda çalışıp diğerinde çalışmıyor
//! olamaz: tek bir uygulama var.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, thiserror::Error)]
pub enum PlaybackError {
    #[error("ses dosyası yazılamadı: {0}")]
    Write(String),
    #[error("çalma başarısız: {0}")]
    Play(String),
}

pub type Result<T> = std::result::Result<T, PlaybackError>;

/// Ses baytlarını geçici dosyaya yazıp çalar, sonra dosyayı siler.
///
/// `extension` uzantı, nokta olmadan: `"mp3"`, `"wav"`. Sistem oynatıcısı
/// biçimi uzantıdan tanıyor, yanlış uzantı sessiz başarısızlık demek.
///
/// `cancel` kaldırılırsa çalma başlamaz ya da ortasında kesilir.
pub fn play_bytes(audio: &[u8], extension: &str, cancel: &AtomicBool) -> Result<()> {
    if audio.is_empty() || cancel.load(Ordering::SeqCst) {
        return Ok(());
    }

    // Süreç kimliği + zaman damgası: aynı anda iki konuşma olsa da çakışmaz.
    let path = std::env::temp_dir().join(format!(
        "vavis_tts_{}_{}.{}",
        std::process::id(),
        timestamp_millis(),
        extension
    ));

    let mut file = std::fs::File::create(&path).map_err(|e| PlaybackError::Write(e.to_string()))?;
    file.write_all(audio)
        .map_err(|e| PlaybackError::Write(e.to_string()))?;
    drop(file);

    let result = play_file(&path, cancel);
    // Çalma başarısız olsa bile geçici dosya bırakılmıyor.
    let _ = std::fs::remove_file(&path);
    result
}

fn timestamp_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[cfg(windows)]
fn play_file(path: &std::path::Path, cancel: &AtomicBool) -> Result<()> {
    use std::process::Command;

    // MediaPlayer: MP3 ve WAV çalar, ek bağımlılık yok, süre bilgisi verir.
    // Açılış eşzamansız olduğu için kısa bir bekleme gerekiyor; süre 0
    // dönerse (henüz yüklenmemişse) makul bir tavana düşülüyor.
    let script = format!(
        "Add-Type -AssemblyName presentationCore; \
         $p = New-Object System.Windows.Media.MediaPlayer; \
         $p.Open([uri]{}); \
         Start-Sleep -Milliseconds 400; \
         $p.Play(); \
         $d = $p.NaturalDuration.TimeSpan.TotalMilliseconds; \
         if ($d -le 0) {{ $d = 3000 }}; \
         Start-Sleep -Milliseconds $d; \
         $p.Stop(); $p.Close()",
        vavis_core::process::ps_quote(&path.display().to_string())
    );

    let mut child = vavis_core::process::hidden(&mut Command::new("powershell"))
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .spawn()
        .map_err(|e| PlaybackError::Play(e.to_string()))?;

    // Barge-in: iptal edilirse oynatıcı öldürülüyor.
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return Ok(()),
            Ok(None) => {
                if cancel.load(Ordering::SeqCst) {
                    let _ = child.kill();
                    return Ok(());
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => return Err(PlaybackError::Play(e.to_string())),
        }
    }
}

#[cfg(not(windows))]
fn play_file(_path: &std::path::Path, _cancel: &AtomicBool) -> Result<()> {
    Err(PlaybackError::Play(
        "oynatma bu platformda desteklenmiyor".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_audio_is_a_no_op() {
        let cancel = AtomicBool::new(false);
        assert!(play_bytes(&[], "mp3", &cancel).is_ok());
    }

    /// İptal edilmişken dosya bile yazılmamalı — anında dönmeli.
    #[test]
    fn a_cancelled_playback_returns_at_once() {
        let cancel = AtomicBool::new(true);
        let start = std::time::Instant::now();
        assert!(play_bytes(&[1, 2, 3, 4], "mp3", &cancel).is_ok());
        assert!(start.elapsed().as_millis() < 100);
    }

    /// Geçici dosya adı çakışmamalı: iki çağrı arasında zaman ilerliyor.
    #[test]
    fn temp_names_differ_between_calls() {
        let a = timestamp_millis();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = timestamp_millis();
        assert!(b > a, "zaman damgası ilerlemeli");
    }
}
