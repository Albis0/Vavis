//! On-device wake word: hearing "Vavis" without sending audio anywhere.
//!
//! Wake-word mode used to transcribe *everything* the microphone picked up
//! -- every sentence said in the room went to the cloud recogniser, and only
//! then was the text checked for the assistant's name. That is a privacy
//! problem, a cost problem (one recognition request per overheard sentence)
//! and a latency problem.
//!
//! This module decides locally whether an utterance starts with the wake
//! word. Only the ones that do are transcribed.
//!
//! ## How
//!
//! Template matching, the way speech recognition worked before neural
//! networks, and still a good fit for one word and one speaker:
//!
//! 1. **Enrolment.** The user says the wake word a few times. Each recording
//!    becomes a template: a sequence of MFCC frames, 13 numbers per 10 ms
//!    describing the shape of the sound's spectrum the way the ear hears it.
//! 2. **Detection.** An incoming utterance is turned into the same kind of
//!    sequence, and dynamic time warping measures how far its beginning is
//!    from each template -- stretching and squeezing time, because nobody
//!    says a word at the same speed twice.
//! 3. **Threshold.** From how far the templates are from *each other*: the
//!    natural variation of the user's own voice sets how close is close
//!    enough.
//!
//! No model file to download, no runtime to ship, a few kilobytes of state,
//! and it works in whatever language the name is said in. The trade-off is
//! that it is tuned to the voice that enrolled it; another person saying
//! the word may not wake it. For a personal assistant that is arguably a
//! feature.

use crate::capture::SAMPLE_RATE;
use serde::{Deserialize, Serialize};

/// Analysis window: 25 ms, the classic speech frame.
const FRAME: usize = (SAMPLE_RATE as usize) * 25 / 1000;
/// Hop between frames: 10 ms.
const HOP: usize = (SAMPLE_RATE as usize) / 100;
/// FFT size, the power of two above the frame.
const NFFT: usize = 512;
const MEL_FILTERS: usize = 26;
/// Cepstral coefficients kept. The first is dropped (it is loudness, and
/// loudness says nothing about which word was said).
const COEFFS: usize = 12;

/// Recordings needed to enrol.
pub const ENROL_SAMPLES: usize = 3;

/// Longest recording accepted as a sample. The wake word alone is well
/// under this; anything longer is a sentence, and a template of a sentence
/// would match nothing.
pub const MAX_SAMPLE_MS: u32 = 2_500;

/// One frame of features.
pub type Frame = [f32; COEFFS];

/// What enrolment produced, saved to disk between runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WakeModel {
    /// Bumped if the feature layout changes, so an old file is rejected
    /// instead of silently matching nothing.
    pub version: u32,
    pub templates: Vec<Vec<Frame>>,
    /// How far the templates sit from each other, on average. The detection
    /// threshold is derived from this and the sensitivity setting.
    pub spread: f32,
}

const MODEL_VERSION: u32 = 1;

impl WakeModel {
    /// Builds a model from recordings of the wake word.
    pub fn enrol(recordings: &[Vec<f32>]) -> Result<Self, String> {
        if recordings.len() < 2 {
            return Err("at least two recordings are needed".into());
        }
        let templates: Vec<Vec<Frame>> =
            recordings.iter().map(|r| mfcc(&trim_silence(r))).collect();
        if templates.iter().any(|t| t.len() < 15) {
            return Err("a recording was too short to use — say the whole word".into());
        }

        let n = templates.len();
        let mut distances = Vec::new();
        // Each recording's distance to the recording most like it.
        let mut nearest = vec![f32::INFINITY; n];
        for i in 0..n {
            for j in (i + 1)..n {
                let d = dtw(&templates[i], &templates[j]);
                distances.push(d);
                nearest[i] = nearest[i].min(d);
                nearest[j] = nearest[j].min(d);
            }
        }
        let closest_pair = distances.iter().cloned().fold(f32::INFINITY, f32::min);

        // A recording far from every other is not the same word said again:
        // it was cut off, or something else was said. Measured against the
        // closest pair, which is the user's own variation at its most
        // consistent -- an average would be dragged up by the odd one out.
        if n > 2 && nearest.iter().any(|d| *d > closest_pair * 2.5 + 4.0) {
            return Err("the recordings do not sound alike — try again in a quiet moment".into());
        }
        let spread = distances.iter().sum::<f32>() / distances.len() as f32;

        Ok(Self {
            version: MODEL_VERSION,
            templates,
            spread,
        })
    }

    pub fn is_current(&self) -> bool {
        self.version == MODEL_VERSION && !self.templates.is_empty()
    }

    /// The largest distance still counted as the wake word, for a
    /// sensitivity from 1 (strict) to 10 (lenient).
    pub fn threshold(&self, sensitivity: u8) -> f32 {
        let s = sensitivity.clamp(1, 10) as f32;
        self.spread * (1.05 + 0.07 * s)
    }

    /// How closely the start of `samples` matches the wake word: the best
    /// distance over all templates. Lower is closer.
    pub fn score(&self, samples: &[f32]) -> f32 {
        let raw = mfcc_raw(&trim_leading_silence(samples));
        self.templates
            .iter()
            .map(|t| {
                // Normalised over the stretch the word would occupy, not the
                // whole utterance: a template is the word alone, and the
                // request after it would otherwise shift every frame's mean
                // away from the template's.
                let window = raw.len().min(t.len());
                let mut frames = raw[..raw.len().min(t.len() * 2)].to_vec();
                subtract_mean(&mut frames, window);
                prefix_dtw(t, &frames)
            })
            .fold(f32::INFINITY, f32::min)
    }

    /// Whether `samples` begins with the wake word.
    pub fn detect(&self, samples: &[f32], sensitivity: u8) -> bool {
        self.score(samples) <= self.threshold(sensitivity)
    }

    /// Whether an utterance is the wake word and nothing else: short enough
    /// that no request could follow it. Such an utterance needs no
    /// transcription at all.
    pub fn is_word_alone(&self, samples: &[f32]) -> bool {
        let longest = self.templates.iter().map(Vec::len).max().unwrap_or(0);
        let frames = trim_silence(samples).len() / HOP;
        frames <= longest * 3 / 2 + 10
    }
}

// ── Features ────────────────────────────────────────────────────────────────

/// Energy below which a frame is silence, relative to the loudest frame.
const SILENCE_RATIO: f32 = 0.05;

fn frame_energy(samples: &[f32]) -> Vec<f32> {
    samples
        .chunks(HOP)
        .map(|c| c.iter().map(|x| x * x).sum::<f32>() / c.len().max(1) as f32)
        .collect()
}

/// Cuts silence off both ends, so templates are the word and only the word.
fn trim_silence(samples: &[f32]) -> Vec<f32> {
    let energy = frame_energy(samples);
    let peak = energy.iter().cloned().fold(0.0f32, f32::max);
    if peak == 0.0 {
        return Vec::new();
    }
    let floor = peak * SILENCE_RATIO;
    let first = energy.iter().position(|e| *e > floor).unwrap_or(0);
    let last = energy.iter().rposition(|e| *e > floor).unwrap_or(0);
    let start = first * HOP;
    let end = ((last + 1) * HOP).min(samples.len());
    samples[start..end].to_vec()
}

/// Cuts silence off the start only: in a whole sentence, the end is the
/// request, not padding.
fn trim_leading_silence(samples: &[f32]) -> Vec<f32> {
    let energy = frame_energy(samples);
    let peak = energy.iter().cloned().fold(0.0f32, f32::max);
    if peak == 0.0 {
        return Vec::new();
    }
    let floor = peak * SILENCE_RATIO;
    let first = energy.iter().position(|e| *e > floor).unwrap_or(0);
    samples[first * HOP..].to_vec()
}

/// Mel-frequency cepstral coefficients, one frame per 10 ms, with the mean
/// of each coefficient removed across the utterance so the microphone's own
/// colouring cancels out.
pub fn mfcc(samples: &[f32]) -> Vec<Frame> {
    let mut frames = mfcc_raw(samples);
    let all = frames.len();
    subtract_mean(&mut frames, all);
    frames
}

/// Cepstral mean normalisation, with the mean taken over the first
/// `window` frames and removed from all of them.
fn subtract_mean(frames: &mut [Frame], window: usize) {
    let window = window.min(frames.len());
    if window == 0 {
        return;
    }
    let mut mean = [0.0f32; COEFFS];
    for f in &frames[..window] {
        for (m, v) in mean.iter_mut().zip(f) {
            *m += v;
        }
    }
    for m in &mut mean {
        *m /= window as f32;
    }
    for f in frames.iter_mut() {
        for (v, m) in f.iter_mut().zip(&mean) {
            *v -= m;
        }
    }
}

/// MFCC frames without mean normalisation.
fn mfcc_raw(samples: &[f32]) -> Vec<Frame> {
    if samples.len() < FRAME {
        return Vec::new();
    }

    // Pre-emphasis lifts the high frequencies speech carries its consonants in.
    let mut emphasised = Vec::with_capacity(samples.len());
    emphasised.push(samples[0]);
    for i in 1..samples.len() {
        emphasised.push(samples[i] - 0.97 * samples[i - 1]);
    }

    let window: Vec<f32> = (0..FRAME)
        .map(|n| 0.54 - 0.46 * (2.0 * std::f32::consts::PI * n as f32 / (FRAME - 1) as f32).cos())
        .collect();
    let filters = mel_filters();

    let mut frames = Vec::new();
    let mut start = 0;
    while start + FRAME <= emphasised.len() {
        let mut re = [0.0f32; NFFT];
        let mut im = [0.0f32; NFFT];
        for i in 0..FRAME {
            re[i] = emphasised[start + i] * window[i];
        }
        fft(&mut re, &mut im);

        let power: Vec<f32> = (0..=NFFT / 2)
            .map(|k| (re[k] * re[k] + im[k] * im[k]) / NFFT as f32)
            .collect();
        let log_mel: Vec<f32> = filters
            .iter()
            .map(|f| {
                let e: f32 = f.iter().map(|(k, w)| power[*k] * w).sum();
                e.max(1e-10).ln()
            })
            .collect();

        // DCT-II; coefficient 0 skipped.
        let mut frame = [0.0f32; COEFFS];
        for (c, slot) in frame.iter_mut().enumerate() {
            let n = (c + 1) as f32;
            *slot = log_mel
                .iter()
                .enumerate()
                .map(|(m, v)| {
                    v * (std::f32::consts::PI * n * (m as f32 + 0.5) / MEL_FILTERS as f32).cos()
                })
                .sum();
        }
        frames.push(frame);
        start += HOP;
    }
    frames
}

/// Triangular filters spaced evenly on the mel scale, as sparse
/// `(bin, weight)` lists.
fn mel_filters() -> Vec<Vec<(usize, f32)>> {
    let mel = |hz: f32| 2595.0 * (1.0 + hz / 700.0).log10();
    let hz = |mel: f32| 700.0 * (10f32.powf(mel / 2595.0) - 1.0);
    let (low, high) = (mel(60.0), mel(SAMPLE_RATE as f32 / 2.0));
    let points: Vec<usize> = (0..MEL_FILTERS + 2)
        .map(|i| {
            let m = low + (high - low) * i as f32 / (MEL_FILTERS + 1) as f32;
            ((NFFT + 1) as f32 * hz(m) / SAMPLE_RATE as f32).floor() as usize
        })
        .collect();

    (1..=MEL_FILTERS)
        .map(|i| {
            let (l, c, r) = (points[i - 1], points[i], points[i + 1]);
            let mut f = Vec::new();
            for k in l..c {
                if c > l {
                    f.push((k, (k - l) as f32 / (c - l) as f32));
                }
            }
            for k in c..=r.min(NFFT / 2) {
                if r > c {
                    f.push((k, (r - k) as f32 / (r - c) as f32));
                }
            }
            f
        })
        .collect()
}

/// In-place iterative radix-2 FFT.
fn fft(re: &mut [f32; NFFT], im: &mut [f32; NFFT]) {
    let n = NFFT;
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let angle = -2.0 * std::f32::consts::PI / len as f32;
        let (wr, wi) = (angle.cos(), angle.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let next = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = next;
            }
        }
        len <<= 1;
    }
}

// ── Matching ────────────────────────────────────────────────────────────────

fn distance(a: &Frame, b: &Frame) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y) * (x - y))
        .sum::<f32>()
        .sqrt()
}

/// Full DTW between two whole sequences, normalised by path length.
pub fn dtw(a: &[Frame], b: &[Frame]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return f32::INFINITY;
    }
    let (n, m) = (a.len(), b.len());
    let mut prev = vec![f32::INFINITY; m + 1];
    let mut cur = vec![f32::INFINITY; m + 1];
    prev[0] = 0.0;
    for i in 1..=n {
        cur[0] = f32::INFINITY;
        for j in 1..=m {
            let best = prev[j - 1].min(prev[j]).min(cur[j - 1]);
            cur[j] = distance(&a[i - 1], &b[j - 1]) + best;
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[m] / (n + m) as f32
}

/// DTW of a template against the *beginning* of an utterance: the match
/// must start at the utterance's start (after leading silence) but may end
/// anywhere, since whatever follows the wake word is the request.
///
/// Only a prefix up to twice the template's length is considered, so a long
/// sentence costs no more than a short one.
pub fn prefix_dtw(template: &[Frame], utterance: &[Frame]) -> f32 {
    if template.is_empty() || utterance.is_empty() {
        return f32::INFINITY;
    }
    let n = template.len();
    let m = utterance.len().min(n * 2);
    let mut prev = vec![f32::INFINITY; m + 1];
    let mut cur = vec![f32::INFINITY; m + 1];
    prev[0] = 0.0;
    for i in 1..=n {
        cur[0] = f32::INFINITY;
        for j in 1..=m {
            let best = prev[j - 1].min(prev[j]).min(cur[j - 1]);
            cur[j] = distance(&template[i - 1], &utterance[j - 1]) + best;
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    // Open end: the best place to stop, from about half the template's
    // length on (a match that ends almost immediately is not a match).
    ((n / 2).max(1)..=m)
        .map(|j| prev[j] / (n + j) as f32)
        .fold(f32::INFINITY, f32::min)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic "word": a sequence of tones, each held for a while, the
    /// way a spoken word is a sequence of vowel and consonant shapes. Two
    /// renditions differ in speed and loudness, as two sayings do.
    fn word(tones: &[(f32, f32)], speed: f32, gain: f32) -> Vec<f32> {
        let mut out = vec![0.0; SAMPLE_RATE as usize / 5]; // leading silence
        for &(hz, seconds) in tones {
            let n = (seconds * speed * SAMPLE_RATE as f32) as usize;
            for i in 0..n {
                let t = i as f32 / SAMPLE_RATE as f32;
                let s = (2.0 * std::f32::consts::PI * hz * t).sin()
                    + 0.5 * (2.0 * std::f32::consts::PI * hz * 2.1 * t).sin();
                out.push(gain * 0.3 * s);
            }
        }
        out.extend(vec![0.0; SAMPLE_RATE as usize / 5]);
        out
    }

    const VAVIS: &[(f32, f32)] = &[(300.0, 0.12), (850.0, 0.15), (2400.0, 0.1), (600.0, 0.15)];
    const OTHER: &[(f32, f32)] = &[(1800.0, 0.2), (400.0, 0.1), (3000.0, 0.2)];

    fn enrolled() -> WakeModel {
        WakeModel::enrol(&[
            word(VAVIS, 1.0, 1.0),
            word(VAVIS, 0.9, 0.7),
            word(VAVIS, 1.1, 1.2),
        ])
        .unwrap()
    }

    #[test]
    fn the_fft_finds_a_pure_tone() {
        let mut re = [0.0f32; NFFT];
        let mut im = [0.0f32; NFFT];
        // A tone on bin 32 exactly.
        for (i, v) in re.iter_mut().enumerate() {
            *v = (2.0 * std::f32::consts::PI * 32.0 * i as f32 / NFFT as f32).cos();
        }
        fft(&mut re, &mut im);
        let mag: Vec<f32> = (0..NFFT / 2)
            .map(|k| (re[k] * re[k] + im[k] * im[k]).sqrt())
            .collect();
        let peak = mag
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap()
            .0;
        assert_eq!(peak, 32);
    }

    #[test]
    fn features_come_ten_milliseconds_apart() {
        let second = vec![0.1f32; SAMPLE_RATE as usize];
        let f = mfcc(&second);
        assert!((97..=99).contains(&f.len()), "{}", f.len());
    }

    #[test]
    fn a_sequence_is_nearest_to_itself() {
        let a = mfcc(&word(VAVIS, 1.0, 1.0));
        let b = mfcc(&word(OTHER, 1.0, 1.0));
        assert!(dtw(&a, &a) < 1e-4);
        assert!(dtw(&a, &b) > dtw(&a, &mfcc(&word(VAVIS, 1.2, 0.5))));
    }

    #[test]
    fn the_word_said_again_is_detected() {
        let model = enrolled();
        assert!(model.detect(&word(VAVIS, 1.05, 0.9), 5));
    }

    #[test]
    fn another_word_is_not() {
        let model = enrolled();
        assert!(!model.detect(&word(OTHER, 1.0, 1.0), 5));
    }

    #[test]
    fn the_word_followed_by_a_request_is_detected() {
        let model = enrolled();
        let mut sentence = word(VAVIS, 1.0, 1.0);
        sentence.truncate(sentence.len() - SAMPLE_RATE as usize / 5);
        sentence.extend(word(OTHER, 1.0, 1.0));
        sentence.extend(word(&[(500.0, 0.6), (1500.0, 0.4)], 1.0, 1.0));
        assert!(model.detect(&sentence, 5));
        assert!(!model.is_word_alone(&sentence));
    }

    #[test]
    fn a_request_without_the_word_is_not() {
        let model = enrolled();
        let mut sentence = word(OTHER, 1.0, 1.0);
        sentence.extend(word(VAVIS, 1.0, 1.0)); // the name mid-sentence
        assert!(!model.detect(&sentence, 5));
    }

    #[test]
    fn the_word_alone_is_recognised_as_alone() {
        let model = enrolled();
        assert!(model.is_word_alone(&word(VAVIS, 1.0, 1.0)));
    }

    #[test]
    fn sensitivity_moves_the_threshold_the_right_way() {
        let model = enrolled();
        assert!(model.threshold(1) < model.threshold(10));
    }

    #[test]
    fn silence_cannot_be_enrolled() {
        let silent = vec![0.0f32; SAMPLE_RATE as usize];
        assert!(WakeModel::enrol(&[silent.clone(), silent.clone(), silent]).is_err());
    }

    #[test]
    fn recordings_of_different_words_are_refused() {
        let err = WakeModel::enrol(&[
            word(VAVIS, 1.0, 1.0),
            word(VAVIS, 0.92, 0.8),
            word(&[(4000.0, 0.8), (100.0, 0.5)], 1.0, 1.0),
        ]);
        assert!(err.is_err());
    }

    #[test]
    fn a_model_survives_being_saved() {
        let model = enrolled();
        let json = serde_json::to_string(&model).unwrap();
        let back: WakeModel = serde_json::from_str(&json).unwrap();
        assert_eq!(back, model);
        assert!(back.is_current());
    }
}
