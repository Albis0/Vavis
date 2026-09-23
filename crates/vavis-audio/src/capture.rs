//! Mikrofon yakalama ve konuşma algılama (VAD).
//!
//! # Neden Rust burada kazanıyor
//!
//! Ses kartı saniyede 16.000 örnek üretir ve bu akış **durmamalıdır**. Ses
//! geri çağırması gerçek zamanlı bir thread'de koşar; orada takılırsan
//! kullanıcı çıtırtı duyar.
//!
//! JavaScript'te çöp toplayıcı istediği an devreye girip 50-200 ms duraklatır
//! — tam da bu yüzden eski projede ses kesiliyordu. Rust'ta GC yok: geri
//! çağırma önceden ayrılmış tampona yazar, hiçbir tahsis yapmaz.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

/// Whisper 16 kHz mono bekler.
pub const SAMPLE_RATE: u32 = 16_000;

/// Sessizlik eşiği (RMS). Bunun altı "konuşma yok" sayılır.
const SILENCE_RMS: f32 = 0.012;

/// Konuşma bittikten sonra bu kadar sessizlik = cümle bitti.
const SILENCE_TAIL_MS: u32 = 700;

/// Bundan kısa sesler yok sayılır (kapı çarpması, öksürük).
const MIN_SPEECH_MS: u32 = 300;

/// Tek seferde kaydedilecek en uzun süre — sonsuz kayıt olmasın.
const MAX_UTTERANCE_MS: u32 = 30_000;

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("mikrofon bulunamadı")]
    NoDevice,
    #[error("mikrofon açılamadı: {0}")]
    Open(String),
    #[error("ses akışı başlatılamadı: {0}")]
    Stream(String),
}

pub type Result<T> = std::result::Result<T, CaptureError>;

/// Bir konuşma parçası — 16 kHz mono f32.
#[derive(Debug, Clone, PartialEq)]
pub struct Utterance {
    pub samples: Vec<f32>,
}

impl Utterance {
    pub fn duration_ms(&self) -> u32 {
        (self.samples.len() as u64 * 1000 / SAMPLE_RATE as u64) as u32
    }
}

/// Konuşma algılayıcı — örnekleri biriktirir, cümle bitince parça verir.
///
/// Saf mantık: ses donanımı bilmez, dolayısıyla **test edilebilir**.
#[derive(Debug)]
pub struct VoiceDetector {
    buffer: Vec<f32>,
    /// Konuşma başladı mı?
    in_speech: bool,
    /// Kaç örnektir sessizlik sürüyor?
    silence_samples: u32,
    /// Konuşma içinde kaç örnek toplandı?
    speech_samples: u32,
}

impl Default for VoiceDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl VoiceDetector {
    pub fn new() -> Self {
        Self {
            // Tipik bir cümle için önceden yer ayır — çalışırken tahsis olmasın.
            buffer: Vec::with_capacity(SAMPLE_RATE as usize * 5),
            in_speech: false,
            silence_samples: 0,
            speech_samples: 0,
        }
    }

    /// Bir ses karesi işler. Cümle tamamlandıysa döner.
    pub fn feed(&mut self, frame: &[f32]) -> Option<Utterance> {
        let rms = rms(frame);
        let is_voice = rms > SILENCE_RMS;
        let frame_len = frame.len() as u32;

        if is_voice {
            if !self.in_speech {
                self.in_speech = true;
                self.buffer.clear();
                self.speech_samples = 0;
            }
            self.silence_samples = 0;
            self.speech_samples += frame_len;
            self.buffer.extend_from_slice(frame);
        } else if self.in_speech {
            // Konuşma içindeki kısa duraklamalar cümlenin parçası — sakla.
            self.silence_samples += frame_len;
            self.buffer.extend_from_slice(frame);

            if ms_to_samples(SILENCE_TAIL_MS) <= self.silence_samples {
                return self.finish();
            }
        }

        // Çok uzadıysa zorla kes.
        if self.in_speech && self.speech_samples >= ms_to_samples(MAX_UTTERANCE_MS) {
            return self.finish();
        }

        None
    }

    fn finish(&mut self) -> Option<Utterance> {
        let speech_ms = (self.speech_samples as u64 * 1000 / SAMPLE_RATE as u64) as u32;
        let samples = std::mem::take(&mut self.buffer);

        self.in_speech = false;
        self.silence_samples = 0;
        self.speech_samples = 0;
        self.buffer = Vec::with_capacity(SAMPLE_RATE as usize * 5);

        // Çok kısa sesler gürültüdür — Whisper'a gönderip boşa para/zaman harcama.
        if speech_ms < MIN_SPEECH_MS {
            tracing::debug!(speech_ms, "çok kısa ses yok sayıldı");
            return None;
        }
        Some(Utterance { samples })
    }

    pub fn is_speaking(&self) -> bool {
        self.in_speech
    }

    /// Biriken her şeyi at (mod değiştirince).
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.in_speech = false;
        self.silence_samples = 0;
        self.speech_samples = 0;
    }
}

fn ms_to_samples(ms: u32) -> u32 {
    SAMPLE_RATE / 1000 * ms
}

/// Kare enerjisi (RMS).
fn rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let sum: f32 = frame.iter().map(|s| s * s).sum();
    (sum / frame.len() as f32).sqrt()
}

/// Kare seviyesini metre için 0–1000 aralığına ölçekler.
///
/// Not the raw RMS. Speech sits around 0.02–0.15 there, so a bar drawn from
/// it would barely move; the square root spreads the quiet end out, which is
/// where the interesting difference between "silent" and "someone is
/// talking" actually lives.
fn level_permille(frame: &[f32]) -> u32 {
    let scaled = rms(frame).sqrt();
    (scaled * 1000.0).clamp(0.0, 1000.0) as u32
}

/// Mikrofon akışı.
///
/// `Stream` `Send` değildir, bu yüzden kendi thread'inde tutulur; dışarıya
/// sadece kanal görünür.
pub struct Microphone {
    running: Arc<AtomicBool>,
    /// Yakalanan cümleler buradan gelir.
    receiver: std::sync::mpsc::Receiver<Utterance>,
    /// Mikrofonu geçici sustur (kendi sesimizi duymayalım).
    muted: Arc<AtomicBool>,
    /// Son karenin enerjisi, binde bir olarak.
    ///
    /// The interface draws a level meter from this. An atomic rather than a
    /// channel because it is a gauge, not a stream: the reader wants the
    /// current value, and a missed frame is of no consequence.
    level: Arc<AtomicU32>,
    /// Where every 16 kHz mono frame goes while something is tapping the
    /// raw stream (the live conversation mode), in addition to the
    /// utterance detector.
    tap: Arc<std::sync::Mutex<Option<std::sync::mpsc::Sender<Vec<f32>>>>>,
}

impl Microphone {
    /// Varsayılan giriş cihazından dinlemeye başlar.
    pub fn start() -> Result<Self> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or(CaptureError::NoDevice)?;
        let config = device
            .default_input_config()
            .map_err(|e| CaptureError::Open(e.to_string()))?;

        let (tx, receiver) = std::sync::mpsc::channel();
        let running = Arc::new(AtomicBool::new(true));
        let muted = Arc::new(AtomicBool::new(false));
        let level = Arc::new(AtomicU32::new(0));

        let device_rate = config.sample_rate().0;
        let channels = config.channels() as usize;

        let tap: Arc<std::sync::Mutex<Option<std::sync::mpsc::Sender<Vec<f32>>>>> =
            Arc::new(std::sync::Mutex::new(None));

        let thread_running = running.clone();
        let thread_muted = muted.clone();
        let thread_level = level.clone();
        let thread_tap = tap.clone();

        // Ses akışı kendi thread'inde yaşar — `Stream` Send olmadığı için.
        std::thread::spawn(move || {
            let mut detector = VoiceDetector::new();
            // Yeniden örnekleme tamponu — her karede tahsis yapmamak için
            // dışarıda tutuluyor.
            let mut mono = Vec::with_capacity(4096);

            let stream = device.build_input_stream(
                &config.into(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if thread_muted.load(Ordering::Relaxed) {
                        // Muted means the level is genuinely zero, not stale.
                        thread_level.store(0, Ordering::Relaxed);
                        return;
                    }

                    // Çok kanallı → mono, sonra 16 kHz'e indir.
                    mono.clear();
                    downmix_and_resample(data, channels, device_rate, &mut mono);

                    // Published before the detector runs: the meter should
                    // move while someone speaks, not only when a sentence ends.
                    thread_level.store(level_permille(&mono), Ordering::Relaxed);

                    // try_lock: the audio callback must never wait. A frame
                    // missed while the tap is being swapped is inaudible.
                    if let Ok(guard) = thread_tap.try_lock() {
                        if let Some(sender) = guard.as_ref() {
                            let _ = sender.send(mono.clone());
                        }
                    }

                    if let Some(utterance) = detector.feed(&mono) {
                        let _ = tx.send(utterance);
                    }
                },
                |err| tracing::error!(%err, "mikrofon akış hatası"),
                None,
            );

            let stream = match stream {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(%e, "mikrofon akışı kurulamadı");
                    return;
                }
            };

            if let Err(e) = stream.play() {
                tracing::error!(%e, "mikrofon başlatılamadı");
                return;
            }

            tracing::info!(device_rate, channels, "mikrofon dinliyor");

            // Akış canlı kalsın diye thread burada bekler.
            while thread_running.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            tracing::info!("mikrofon durduruldu");
        });

        Ok(Self {
            running,
            receiver,
            muted,
            level,
            tap,
        })
    }

    /// Starts receiving every captured frame, 16 kHz mono, as it arrives.
    /// Replaces any earlier tap.
    pub fn tap(&self) -> std::sync::mpsc::Receiver<Vec<f32>> {
        let (tx, rx) = std::sync::mpsc::channel();
        *self.tap.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
        rx
    }

    /// Stops the raw stream.
    pub fn untap(&self) {
        *self.tap.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// Bekleyen konuşmaları alır — bloklamaz.
    pub fn poll(&self) -> Vec<Utterance> {
        self.receiver.try_iter().collect()
    }

    /// Kendi sesimizi duymamak için mikrofonu sustur.
    ///
    /// TTS konuşurken şart: yoksa asistan kendi cevabını dinleyip
    /// sonsuz döngüye girer.
    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
    }

    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    /// Son karenin seviyesi, 0.0–1.0.
    ///
    /// For a meter, not for decisions: the detector has its own threshold,
    /// and two places deciding what counts as speech would drift apart.
    pub fn level(&self) -> f32 {
        self.level.load(Ordering::Relaxed) as f32 / 1000.0
    }
}

impl Drop for Microphone {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

/// Çok kanallı sesi mono'ya indirir ve 16 kHz'e örnekler.
///
/// Basit doğrusal örnekleme — konuşma tanıma için yeterli, ucuz.
fn downmix_and_resample(input: &[f32], channels: usize, from_rate: u32, out: &mut Vec<f32>) {
    if channels == 0 || input.is_empty() {
        return;
    }

    let frames = input.len() / channels;
    let ratio = from_rate as f32 / SAMPLE_RATE as f32;
    let out_len = (frames as f32 / ratio) as usize;

    for i in 0..out_len {
        let src = (i as f32 * ratio) as usize;
        if src >= frames {
            break;
        }
        // Kanalların ortalaması.
        let mut sum = 0.0;
        for c in 0..channels {
            sum += input[src * channels + c];
        }
        out.push(sum / channels as f32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Belirli genlikte ses üretir.
    fn tone(len: usize, amplitude: f32) -> Vec<f32> {
        (0..len)
            .map(|i| (i as f32 * 0.1).sin() * amplitude)
            .collect()
    }

    fn silence(len: usize) -> Vec<f32> {
        vec![0.0; len]
    }

    #[test]
    fn silence_alone_produces_nothing() {
        let mut vad = VoiceDetector::new();
        for _ in 0..50 {
            assert!(vad.feed(&silence(1600)).is_none());
        }
        assert!(!vad.is_speaking());
    }

    #[test]
    fn speech_then_silence_yields_an_utterance() {
        let mut vad = VoiceDetector::new();

        // 1 saniye konuşma.
        for _ in 0..10 {
            assert!(vad.feed(&tone(1600, 0.3)).is_none());
        }
        assert!(vad.is_speaking());

        // Sessizlik kuyruğu — cümle bitmeli.
        let mut result = None;
        for _ in 0..10 {
            if let Some(u) = vad.feed(&silence(1600)) {
                result = Some(u);
                break;
            }
        }

        let utterance = result.expect("cümle tamamlanmalıydı");
        assert!(utterance.duration_ms() >= MIN_SPEECH_MS);
        assert!(!vad.is_speaking(), "cümleden sonra durum sıfırlanmalı");
    }

    #[test]
    fn very_short_sound_is_discarded_as_noise() {
        let mut vad = VoiceDetector::new();

        // 100 ms ses — eşiğin altında.
        vad.feed(&tone(1600, 0.3));

        let mut result = None;
        for _ in 0..10 {
            if let Some(u) = vad.feed(&silence(1600)) {
                result = Some(u);
            }
        }
        assert!(result.is_none(), "kısa gürültü cümle sayılmamalı");
    }

    #[test]
    fn brief_pause_does_not_split_a_sentence() {
        let mut vad = VoiceDetector::new();

        for _ in 0..10 {
            vad.feed(&tone(1600, 0.3));
        }
        // 200 ms duraklama — cümle bitmemeli.
        assert!(vad.feed(&silence(3200)).is_none());
        // Konuşma devam ediyor.
        assert!(vad.feed(&tone(1600, 0.3)).is_none());
        assert!(vad.is_speaking(), "kısa duraklama cümleyi bölmemeli");
    }

    #[test]
    fn very_long_speech_is_force_cut() {
        let mut vad = VoiceDetector::new();
        let mut got = None;

        // Sınırı aşana kadar sürekli konuş.
        for _ in 0..400 {
            if let Some(u) = vad.feed(&tone(1600, 0.3)) {
                got = Some(u);
                break;
            }
        }
        assert!(got.is_some(), "uzun konuşma zorla kesilmeli");
    }

    #[test]
    fn reset_discards_partial_speech() {
        let mut vad = VoiceDetector::new();
        for _ in 0..10 {
            vad.feed(&tone(1600, 0.3));
        }
        assert!(vad.is_speaking());

        vad.reset();
        assert!(!vad.is_speaking());
        assert!(vad.buffer.is_empty());
    }

    #[test]
    fn rms_distinguishes_speech_from_silence() {
        assert!(rms(&silence(100)) < SILENCE_RMS);
        assert!(rms(&tone(100, 0.5)) > SILENCE_RMS);
        assert_eq!(rms(&[]), 0.0);
    }

    #[test]
    fn the_meter_reads_zero_in_silence_and_high_when_loud() {
        assert_eq!(level_permille(&silence(100)), 0);
        assert_eq!(level_permille(&[]), 0);
        assert!(level_permille(&tone(100, 0.9)) > 500);
    }

    #[test]
    fn the_meter_rises_with_volume() {
        let quiet = level_permille(&tone(100, 0.05));
        let loud = level_permille(&tone(100, 0.5));
        assert!(loud > quiet, "quiet {quiet}, loud {loud}");
    }

    #[test]
    fn quiet_speech_still_moves_the_meter_visibly() {
        // Raw RMS at this amplitude is about 0.035 — a bar 3.5% full, which
        // reads as broken. The curve is what makes it visible.
        let level = level_permille(&tone(100, 0.05));
        assert!(
            level > 100,
            "a real voice must not look like silence: {level}"
        );
    }

    #[test]
    fn the_meter_never_exceeds_its_range() {
        // A clipping microphone must not produce a bar wider than the box.
        assert!(level_permille(&tone(100, 1.0)) <= 1000);
    }

    #[test]
    fn stereo_is_downmixed_to_mono() {
        // 2 kanal, 16 kHz → mono 16 kHz (oran 1:1)
        let stereo: Vec<f32> = vec![1.0, 0.0, 1.0, 0.0, 1.0, 0.0];
        let mut out = Vec::new();
        downmix_and_resample(&stereo, 2, SAMPLE_RATE, &mut out);

        assert_eq!(out.len(), 3);
        for s in out {
            assert!((s - 0.5).abs() < 1e-6, "kanal ortalaması alınmalı");
        }
    }

    #[test]
    fn resampling_reduces_sample_count() {
        // 48 kHz → 16 kHz = 3'te 1
        let input = vec![0.5f32; 3000];
        let mut out = Vec::new();
        downmix_and_resample(&input, 1, 48_000, &mut out);

        assert!(
            (out.len() as i32 - 1000).abs() < 10,
            "48k→16k üçte bire inmeli, gelen: {}",
            out.len()
        );
    }

    #[test]
    fn empty_input_is_handled() {
        let mut out = Vec::new();
        downmix_and_resample(&[], 2, 48_000, &mut out);
        assert!(out.is_empty());

        // Sıfır kanal çökmemeli.
        downmix_and_resample(&[1.0], 0, 48_000, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn utterance_duration_is_computed_from_sample_rate() {
        let u = Utterance {
            samples: vec![0.0; SAMPLE_RATE as usize], // 1 saniye
        };
        assert_eq!(u.duration_ms(), 1000);
    }
}
