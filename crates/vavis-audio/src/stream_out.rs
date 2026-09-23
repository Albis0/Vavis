//! Streaming speaker output: PCM in, sound out, as it arrives.
//!
//! Everything else in this crate plays whole files -- a sentence is
//! synthesised, written, played. A live conversation cannot work that way:
//! the model's voice arrives in 40 ms pieces and has to start playing on
//! the first one, and when the user interrupts, what is queued has to go
//! silent at once, not at the end of the file.
//!
//! So this keeps a sample queue that the sound card's callback drains.
//! [`PcmPlayer::push_pcm16`] appends (resampling to the device rate on the
//! way in), [`PcmPlayer::clear`] empties it -- barge-in is one call.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, thiserror::Error)]
pub enum OutputError {
    #[error("hoparlör bulunamadı")]
    NoDevice,
    #[error("hoparlör açılamadı: {0}")]
    Open(String),
}

struct Shared {
    queue: Mutex<VecDeque<f32>>,
    /// When the last sample left the queue: "playing" lingers a moment
    /// after the queue empties, because the sound is still in the air and
    /// the microphone still hears it.
    last_sound: Mutex<Option<Instant>>,
}

/// A running output stream.
pub struct PcmPlayer {
    shared: Arc<Shared>,
    running: Arc<AtomicBool>,
    device_rate: u32,
}

/// How long after the last sample the player still counts as playing.
const TAIL: Duration = Duration::from_millis(350);

impl PcmPlayer {
    /// Opens the default output device.
    pub fn start() -> Result<Self, OutputError> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

        let host = cpal::default_host();
        let device = host.default_output_device().ok_or(OutputError::NoDevice)?;
        let supported = device
            .default_output_config()
            .map_err(|e| OutputError::Open(e.to_string()))?;
        let device_rate = supported.sample_rate().0;
        let channels = supported.channels() as usize;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();

        let shared = Arc::new(Shared {
            queue: Mutex::new(VecDeque::new()),
            last_sound: Mutex::new(None),
        });
        let running = Arc::new(AtomicBool::new(true));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();

        let thread_shared = shared.clone();
        let thread_running = running.clone();
        // The stream is not Send, so it lives on its own thread, like the
        // microphone's.
        std::thread::Builder::new()
            .name("pcm-out".into())
            .spawn(move || {
                let fill = move |out: &mut dyn FnMut(usize, f32), frames: usize| {
                    let mut queue = thread_shared
                        .queue
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    let had = !queue.is_empty();
                    for i in 0..frames {
                        let sample = queue.pop_front().unwrap_or(0.0);
                        out(i, sample);
                    }
                    drop(queue);
                    if had {
                        *thread_shared
                            .last_sound
                            .lock()
                            .unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
                    }
                };
                let err = |e| tracing::warn!(%e, "speaker stream error");
                let stream = match format {
                    cpal::SampleFormat::I16 => device.build_output_stream(
                        &config,
                        move |data: &mut [i16], _| {
                            let frames = data.len() / channels;
                            fill(
                                &mut |i, s| {
                                    let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                                    for c in 0..channels {
                                        data[i * channels + c] = v;
                                    }
                                },
                                frames,
                            );
                        },
                        err,
                        None,
                    ),
                    _ => device.build_output_stream(
                        &config,
                        move |data: &mut [f32], _| {
                            let frames = data.len() / channels;
                            fill(
                                &mut |i, s| {
                                    for c in 0..channels {
                                        data[i * channels + c] = s;
                                    }
                                },
                                frames,
                            );
                        },
                        err,
                        None,
                    ),
                };
                let stream = match stream {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e.to_string()));
                        return;
                    }
                };
                if let Err(e) = stream.play() {
                    let _ = ready_tx.send(Err(e.to_string()));
                    return;
                }
                let _ = ready_tx.send(Ok(()));
                while thread_running.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(50));
                }
            })
            .map_err(|e| OutputError::Open(e.to_string()))?;

        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(Self {
                shared,
                running,
                device_rate,
            }),
            Ok(Err(e)) => Err(OutputError::Open(e)),
            Err(_) => Err(OutputError::Open("the speaker did not start".into())),
        }
    }

    /// Queues little-endian 16-bit mono PCM recorded at `rate`.
    pub fn push_pcm16(&self, bytes: &[u8], rate: u32) {
        let samples = pcm16_to_f32(bytes);
        let resampled = resample(&samples, rate, self.device_rate);
        self.shared
            .queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .extend(resampled);
    }

    /// Silences everything queued, at once.
    pub fn clear(&self) {
        self.shared
            .queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    /// Whether sound is coming out, or was a moment ago.
    pub fn is_playing(&self) -> bool {
        if !self
            .shared
            .queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty()
        {
            return true;
        }
        self.shared
            .last_sound
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some_and(|t| t.elapsed() < TAIL)
    }
}

impl Drop for PcmPlayer {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

/// Little-endian 16-bit PCM to floats in -1..1. An odd trailing byte is
/// dropped.
pub fn pcm16_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
        .collect()
}

/// Floats in -1..1 to little-endian 16-bit PCM.
pub fn f32_to_pcm16(samples: &[f32]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
        .collect()
}

/// Linear resampling. Speech survives it fine, and it is cheap enough to
/// run on every 40 ms chunk.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() || from == 0 || to == 0 {
        return input.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let out_len = ((input.len() as f64) / ratio).floor() as usize;
    (0..out_len)
        .map(|i| {
            let pos = i as f64 * ratio;
            let idx = pos as usize;
            let frac = (pos - idx as f64) as f32;
            let a = input[idx.min(input.len() - 1)];
            let b = input[(idx + 1).min(input.len() - 1)];
            a + (b - a) * frac
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_round_trips() {
        let s = vec![0.0, 0.5, -0.5, 0.999];
        let back = pcm16_to_f32(&f32_to_pcm16(&s));
        for (a, b) in s.iter().zip(back) {
            assert!((a - b).abs() < 0.001);
        }
    }

    #[test]
    fn an_odd_byte_is_dropped() {
        assert_eq!(pcm16_to_f32(&[0, 0, 7]).len(), 1);
    }

    #[test]
    fn resampling_keeps_the_duration() {
        let second_at_24k = vec![0.1; 24_000];
        assert_eq!(resample(&second_at_24k, 24_000, 48_000).len(), 48_000);
        assert_eq!(resample(&second_at_24k, 24_000, 16_000).len(), 16_000);
        assert_eq!(resample(&second_at_24k, 24_000, 24_000).len(), 24_000);
    }

    #[test]
    fn upsampling_interpolates() {
        let out = resample(&[0.0, 1.0], 1, 2);
        assert_eq!(out.len(), 4);
        assert!((out[1] - 0.5).abs() < 1e-6);
    }
}
