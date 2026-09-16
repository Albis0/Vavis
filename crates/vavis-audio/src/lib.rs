//! # vavis-audio — DUYULAR katmanı
//!
//! Mikrofon, konuşma algılama (VAD), ses→metin (STT), metin→ses (TTS),
//! konuşma kuyruğu ve barge-in.
//!
//! # Neden bu katman Rust'ta
//!
//! Ses akışı saniyede 16.000 örnek üretir ve **durmamalıdır**. JavaScript'te
//! çöp toplayıcı istediği an 50-200 ms duraklatır — eski projedeki ses
//! kesilmelerinin sebebi buydu. Rust'ta GC yok: ses geri çağırması önceden
//! ayrılmış tampona yazar, hiç tahsis yapmaz.
//!
//! # Barge-in
//!
//! [`queue::SpeechQueue`] eski projedeki barge-in bugunu yapısal olarak
//! imkânsız kılar — ayrıntı o modülün belgesinde.

pub mod capture;
pub mod edge_tts;
pub mod elevenlabs;
pub mod kokoro;
pub mod gemini_tts;
pub mod openai_tts;
pub mod playback;
pub mod queue;
pub mod speakable;
pub mod stt;
pub mod tts;

pub use capture::{Microphone, Utterance, VoiceDetector, SAMPLE_RATE};
pub use queue::{split_sentences, SpeechQueue};
pub use speakable::to_speech;
pub use stt::{contains_wake_word, strip_wake_word, SttClient};
pub use tts::{TtsConfig, TtsEngine, TtsEngineKind};

/// Ses modu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VoiceMode {
    /// Mikrofon kapalı.
    #[default]
    Off,
    /// Her konuşma isteğe çevrilir.
    Continuous,
    /// Sadece uyandırma kelimesinden sonra dinler.
    WakeWord,
}

impl VoiceMode {
    /// M tuşu döngüsü: Off → Continuous → WakeWord → Off
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Continuous,
            Self::Continuous => Self::WakeWord,
            Self::WakeWord => Self::Off,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "ses kapalı",
            Self::Continuous => "sürekli dinliyor",
            Self::WakeWord => "uyandırma kelimesi bekliyor",
        }
    }

    pub fn is_listening(self) -> bool {
        !matches!(self, Self::Off)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_cycles_through_all_states_and_back() {
        let mut mode = VoiceMode::Off;
        mode = mode.next();
        assert_eq!(mode, VoiceMode::Continuous);
        mode = mode.next();
        assert_eq!(mode, VoiceMode::WakeWord);
        mode = mode.next();
        assert_eq!(mode, VoiceMode::Off, "döngü başa dönmeli");
    }

    #[test]
    fn off_is_the_only_non_listening_mode() {
        assert!(!VoiceMode::Off.is_listening());
        assert!(VoiceMode::Continuous.is_listening());
        assert!(VoiceMode::WakeWord.is_listening());
    }

    #[test]
    fn every_mode_has_a_label() {
        for mode in [VoiceMode::Off, VoiceMode::Continuous, VoiceMode::WakeWord] {
            assert!(!mode.label().is_empty());
        }
    }
}
