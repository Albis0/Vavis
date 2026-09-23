//! Voice subsystem wiring.
//!
//! Wraps the pieces in `vavis-audio` and exposes them as something the
//! interface can drive: cycle the mode, speak a reply, stop mid-sentence.
//!
//! # Barge-in
//!
//! [`VoiceState::stop_speaking`] clears the queue *before* stopping
//! playback, and nothing in between can restart it. The predecessor
//! project had this backwards — its stop call synchronously drained the
//! queue, which started the next sentence.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use vavis_audio::{
    contains_wake_word, split_sentences, strip_wake_word, Microphone, SpeechQueue, SttClient,
    TtsConfig, TtsEngine, VoiceMode, WakeModel,
};

/// After the wake word alone, the next thing said is taken as the request
/// without saying the name again -- "Vavis." … "open Spotify." -- for this
/// long.
const FOLLOW_UP: Duration = Duration::from_secs(8);

/// Something the voice layer wants the interface to know.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum VoiceEvent {
    /// Speech was recognised and should be sent to the model.
    Heard { text: String },
    /// The wake word was heard with no request after it.
    Woke,
    /// Informational message for the feed.
    Notice { text: String },
    /// Speaking started or stopped — drives the core animation.
    Speaking { active: bool },
    /// Wake-word training: `count` of `needed` recordings taken.
    Enrol { count: usize, needed: usize },
    /// Wake-word training finished, or failed and needs starting over.
    EnrolDone { ok: bool, message: String },
}

pub struct VoiceState {
    mode: VoiceMode,
    mic: Option<Microphone>,
    queue: SpeechQueue,
    tts: Arc<TtsEngine>,
    speaking: Arc<AtomicBool>,
    runtime: tokio::runtime::Runtime,
    stt: Arc<SttClient>,
    tx: Sender<VoiceEvent>,
    rx: Receiver<VoiceEvent>,
    api_key: String,
    language: String,
    wake_word: String,
    /// Text streamed in this turn that has not been spoken yet.
    ///
    /// Speech used to wait for the whole answer: the screen filled with
    /// words while the speaker stayed silent, and on a long reply that gap
    /// was most of the wait. Sentences are now handed over as they complete,
    /// so the first one plays while the model is still writing the rest.
    ///
    /// `Mutex` because the stream callback runs on the request thread while
    /// the interface reads state from its own.
    pending: std::sync::Mutex<String>,

    /// The trained wake word, when there is one. With it, wake-word mode
    /// decides on this machine whether it was addressed; without it, every
    /// utterance still has to be transcribed to find out.
    wake: Option<Arc<WakeModel>>,
    wake_sensitivity: u8,
    /// Where the model is saved, so training survives a restart.
    wake_path: Option<std::path::PathBuf>,
    /// Recordings collected while training, `None` when not training.
    enrolling: Option<Vec<Vec<f32>>>,
    /// Until when the next utterance counts as addressed without the name.
    awake_until: Arc<Mutex<Option<Instant>>>,
}

impl VoiceState {
    pub fn new(wake_word: String, language: String, api_key: String) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("tokio runtime");

        let (tx, rx) = std::sync::mpsc::channel();

        Self {
            mode: VoiceMode::Off,
            mic: None,
            queue: SpeechQueue::new(),
            tts: Arc::new(TtsEngine::new(TtsConfig::default())),
            speaking: Arc::new(AtomicBool::new(false)),
            runtime,
            stt: Arc::new(SttClient::new()),
            tx,
            rx,
            api_key,
            language,
            wake_word,
            pending: std::sync::Mutex::new(String::new()),
            wake: None,
            wake_sensitivity: 5,
            wake_path: None,
            enrolling: None,
            awake_until: Arc::new(Mutex::new(None)),
        }
    }

    /// Loads a trained wake word from `path`, and saves future training there.
    pub fn attach_wake_model(&mut self, path: std::path::PathBuf) {
        self.wake = std::fs::read_to_string(&path)
            .ok()
            .and_then(|json| serde_json::from_str::<WakeModel>(&json).ok())
            .filter(WakeModel::is_current)
            .map(Arc::new);
        self.wake_path = Some(path);
    }

    pub fn set_wake_sensitivity(&mut self, sensitivity: u8) {
        self.wake_sensitivity = sensitivity.clamp(1, 10);
    }

    pub fn wake_trained(&self) -> bool {
        self.wake.is_some()
    }

    /// Starts wake-word training: the next few things heard are recordings
    /// of the word, not requests. Opens the microphone if nothing had it.
    pub fn start_enrolment(&mut self) -> Result<(), String> {
        if self.mic.is_none() {
            self.mic = Some(Microphone::start().map_err(|e| format!("microphone: {e}"))?);
        }
        self.stop_speaking();
        self.enrolling = Some(Vec::new());
        Ok(())
    }

    pub fn cancel_enrolment(&mut self) {
        self.enrolling = None;
        if !self.mode.is_listening() {
            self.mic = None;
        }
    }

    /// Forgets the trained wake word; wake mode goes back to transcribing.
    pub fn forget_wake_model(&mut self) -> Result<(), String> {
        self.wake = None;
        if let Some(path) = &self.wake_path {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(())
    }

    /// Takes one training recording.
    fn enrol(&mut self, utterance: vavis_audio::Utterance) {
        let Some(samples) = self.enrolling.as_mut() else {
            return;
        };
        if utterance.duration_ms() > vavis_audio::wake::MAX_SAMPLE_MS {
            let _ = self.tx.send(VoiceEvent::Notice {
                text: "Too long — say only the wake word.".into(),
            });
            return;
        }
        samples.push(utterance.samples);
        let count = samples.len();
        let needed = vavis_audio::wake::ENROL_SAMPLES;
        let _ = self.tx.send(VoiceEvent::Enrol { count, needed });
        if count < needed {
            return;
        }

        let recordings = self.enrolling.take().unwrap_or_default();
        let outcome = WakeModel::enrol(&recordings).and_then(|model| {
            if let Some(path) = &self.wake_path {
                let json = serde_json::to_string(&model).map_err(|e| e.to_string())?;
                std::fs::write(path, json).map_err(|e| format!("could not save: {e}"))?;
            }
            Ok(model)
        });
        let event = match outcome {
            Ok(model) => {
                self.wake = Some(Arc::new(model));
                VoiceEvent::EnrolDone {
                    ok: true,
                    message: "Wake word learned. It is now recognised on this computer — \
                              only what follows it is sent for transcription."
                        .into(),
                }
            }
            Err(e) => VoiceEvent::EnrolDone {
                ok: false,
                message: e,
            },
        };
        let _ = self.tx.send(event);
        if !self.mode.is_listening() {
            self.mic = None;
        }
    }

    pub fn mode(&self) -> VoiceMode {
        self.mode
    }

    /// Current microphone level, 0.0–1.0, or 0 when nothing is listening.
    ///
    /// The interface draws a meter from this. A meter that only moved once
    /// speech had been recognised would be useless — the question it answers
    /// is "is this thing hearing me at all", and it has to answer while the
    /// user is still talking.
    pub fn mic_level(&self) -> f32 {
        self.mic.as_ref().map_or(0.0, Microphone::level)
    }

    pub fn is_speaking(&self) -> bool {
        self.speaking.load(Ordering::Relaxed)
    }

    pub fn set_api_key(&mut self, key: String) {
        self.api_key = key;
    }

    pub fn set_language(&mut self, language: String) {
        self.language = language;
    }

    /// Advances to the next mode, opening or releasing the microphone.
    ///
    /// Returns an error string if the microphone could not be opened —
    /// the caller surfaces it rather than failing silently.
    pub fn cycle_mode(&mut self) -> Result<VoiceMode, String> {
        self.set_mode(self.mode.next())
    }

    pub fn set_mode(&mut self, mode: VoiceMode) -> Result<VoiceMode, String> {
        if mode == self.mode {
            return Ok(mode);
        }

        if mode.is_listening() {
            if self.api_key.trim().is_empty() {
                return Err("speech recognition needs a Groq key".into());
            }
            if self.mic.is_none() {
                match Microphone::start() {
                    Ok(mic) => self.mic = Some(mic),
                    Err(e) => return Err(format!("microphone: {e}")),
                }
            }
        } else {
            // Dropping the handle stops the capture thread.
            self.mic = None;
            self.stop_speaking();
        }

        self.mode = mode;
        Ok(mode)
    }

    /// Applies the user's speech settings.
    ///
    /// The engine is rebuilt rather than mutated: `TtsEngine` is handed to
    /// speaking threads inside an `Arc`, and swapping the whole thing avoids
    /// a lock on a path that runs for every sentence. Anything already
    /// speaking keeps the old engine until it finishes, which is correct --
    /// changing the voice mid-sentence would be worse than finishing it.
    pub fn set_tts_config(&mut self, config: TtsConfig) {
        let mut engine = TtsEngine::new(config);
        engine.set_language(self.language.clone());
        self.tts = Arc::new(engine);
    }

    /// Speaks one line regardless of voice mode.
    ///
    /// Used by the settings preview. It ignores the mode on purpose: someone
    /// choosing a voice wants to hear it, and making them enable the
    /// microphone first to audition a speaker would be backwards.
    pub fn preview(&self, text: &str) {
        self.tts.reset();
        let tts = self.tts.clone();
        let text = text.to_string();
        std::thread::spawn(move || {
            if let Err(e) = tts.speak(&text) {
                tracing::warn!(%e, "voice preview failed");
            }
        });
    }

    /// **Barge-in.** Cuts speech immediately.
    ///
    /// Order matters: clear the queue first so no further utterance can
    /// start, then stop what is playing.
    pub fn stop_speaking(&self) {
        // The buffer goes with it: a barge-in cancels the rest of the
        // answer, and a sentence left here would be spoken after it.
        if let Ok(mut buf) = self.pending.lock() {
            buf.clear();
        }
        self.queue.stop();
        self.tts.stop();
        self.speaking.store(false, Ordering::Relaxed);
        if let Some(mic) = &self.mic {
            mic.set_muted(false);
        }
        let _ = self.tx.send(VoiceEvent::Speaking { active: false });
    }

    /// Starts a new spoken turn, discarding anything buffered from the last.
    ///
    /// Called when a request begins, so an abandoned turn cannot leak a
    /// half-sentence into the next answer.
    pub fn begin_stream(&self) {
        if let Ok(mut buf) = self.pending.lock() {
            buf.clear();
        }
        // Clearing the barge-in flag belongs here, once per turn: doing it
        // per chunk would undo a stop the user made mid-answer.
        self.tts.reset();
    }

    /// Feeds a chunk of the reply as it arrives and speaks whole sentences.
    ///
    /// Only **complete** sentences are handed over: a fragment read aloud
    /// stops mid-thought and the next chunk restarts it, which sounds worse
    /// than waiting. The tail stays buffered until [`Self::finish_stream`].
    pub fn push_stream(&self, chunk: &str) {
        if self.mode == VoiceMode::Off || chunk.is_empty() {
            return;
        }

        let ready = {
            let Ok(mut buf) = self.pending.lock() else {
                return;
            };
            buf.push_str(chunk);

            // Split at the last sentence end; everything after it is still
            // being written.
            let Some(cut) = buf.rfind(['.', '!', '?', '\n']) else {
                return;
            };
            let ready: String = buf[..=cut].to_string();
            buf.drain(..=cut);
            ready
        };

        self.enqueue(&ready);
    }

    /// Speaks whatever is left over once the reply is complete.
    pub fn finish_stream(&self) {
        let rest = match self.pending.lock() {
            Ok(mut buf) => std::mem::take(&mut *buf),
            Err(_) => return,
        };
        self.enqueue(&rest);
    }

    /// Cleans one piece of text and queues it for speaking.
    fn enqueue(&self, text: &str) {
        if self.mode == VoiceMode::Off || text.trim().is_empty() {
            return;
        }

        // The model writes markdown, and markdown read aloud is noise:
        // asterisks, backticks and URLs all get pronounced. Only the copy
        // going to the speaker is cleaned -- what is on screen keeps its
        // formatting.
        let spoken = vavis_audio::to_speech(text, &self.language);
        if spoken.trim().is_empty() {
            return;
        }

        for piece in split_sentences(&spoken) {
            self.queue.push(piece);
        }
        self.drain();
    }

    fn drain(&self) {
        let Some(utterance) = self.queue.next() else {
            return; // empty, or already speaking
        };

        let queue = self.queue.clone();
        let tts = self.tts.clone();
        let speaking = self.speaking.clone();
        let tx = self.tx.clone();

        speaking.store(true, Ordering::Relaxed);
        let _ = tx.send(VoiceEvent::Speaking { active: true });

        std::thread::spawn(move || {
            let generation = utterance.generation;

            if let Err(e) = tts.speak(&utterance.text) {
                tracing::warn!(%e, "speech synthesis failed");
            }

            // The engine already said this out loud, for the user who is not
            // looking at the screen. This is the other half: the user who is.
            if let Some((failed, using)) = tts.take_fallback() {
                let _ = tx.send(VoiceEvent::Notice {
                    text: format!(
                        "{} sesi yanıt vermedi — {} ile devam ediliyor.",
                        failed.spoken_name("tr"),
                        using.spoken_name("tr")
                    ),
                });
            }

            // A barge-in bumped the generation while we were speaking:
            // everything after this point belongs to a cancelled turn.
            if generation != queue.generation() {
                return;
            }
            queue.finished(generation);

            // Loop rather than recurse — a long answer would otherwise
            // grow the stack one sentence at a time.
            while let Some(next) = queue.next() {
                if next.generation != queue.generation() {
                    break;
                }
                if tts.speak(&next.text).is_err() {
                    break;
                }
                queue.finished(next.generation);
            }

            speaking.store(false, Ordering::Relaxed);
            let _ = tx.send(VoiceEvent::Speaking { active: false });
        });
    }

    /// Polls the microphone and drains pending events.
    ///
    /// Called on a timer by the shell; returns whatever has accumulated.
    pub fn poll(&mut self) -> Vec<VoiceEvent> {
        let heard = match &self.mic {
            Some(mic) => {
                // Mute the microphone while speaking, or the assistant hears
                // itself and answers its own voice.
                let should_mute = self.is_speaking();
                if mic.is_muted() != should_mute {
                    mic.set_muted(should_mute);
                }
                mic.poll()
            }
            None => Vec::new(),
        };
        for utterance in heard {
            if self.enrolling.is_some() {
                self.enrol(utterance);
            } else if self.mode.is_listening() {
                self.transcribe(utterance);
            }
        }
        self.rx.try_iter().collect()
    }

    /// Whether the follow-up window after a bare wake word is still open.
    fn awake(&self) -> bool {
        let guard = self.awake_until.lock().unwrap_or_else(|e| e.into_inner());
        guard.is_some_and(|until| Instant::now() < until)
    }

    fn transcribe(&self, utterance: vavis_audio::Utterance) {
        let stt = self.stt.clone();
        let tx = self.tx.clone();
        let key = self.api_key.clone();
        let language = self.language.clone();
        let wake_word = self.wake_word.clone();
        let mode = self.mode;
        let awake_until = self.awake_until.clone();
        let follow_up = self.awake();

        // The local gate. With a trained wake word, an utterance that does
        // not start with it is dropped here and never leaves the machine --
        // unless it is the follow-up to a bare wake word a moment ago.
        let mut heard_locally = false;
        if let (VoiceMode::WakeWord, Some(model)) = (mode, &self.wake) {
            if model.detect(&utterance.samples, self.wake_sensitivity) {
                if model.is_word_alone(&utterance.samples) {
                    // Nothing to transcribe: the name alone is a summons.
                    *awake_until.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some(Instant::now() + FOLLOW_UP);
                    let _ = tx.send(VoiceEvent::Woke);
                    return;
                }
                heard_locally = true;
            } else if !follow_up {
                return;
            }
        }

        self.runtime.spawn(async move {
            let text = match stt.transcribe(&utterance, &key, &language).await {
                Ok(t) => t,
                Err(e) => {
                    let _ = tx.send(VoiceEvent::Notice {
                        text: format!("speech not recognised: {e}"),
                    });
                    return;
                }
            };

            if text.trim().is_empty() {
                return; // silence, or a filtered hallucination
            }

            let event = match mode {
                VoiceMode::WakeWord => {
                    let named = contains_wake_word(&text, &wake_word);
                    let request = if named {
                        strip_wake_word(&text, &wake_word)
                    } else if heard_locally {
                        // The local detector heard the name but the
                        // recogniser spelled it as something else ("Davis").
                        // It was the first word either way.
                        drop_first_word(&text)
                    } else if follow_up {
                        text.trim().to_string()
                    } else {
                        return; // not addressed to us
                    };
                    let mut awake = awake_until.lock().unwrap_or_else(|e| e.into_inner());
                    if request.is_empty() {
                        *awake = Some(Instant::now() + FOLLOW_UP);
                        VoiceEvent::Woke
                    } else {
                        *awake = None;
                        VoiceEvent::Heard { text: request }
                    }
                }
                VoiceMode::Continuous => VoiceEvent::Heard { text },
                VoiceMode::Off => return,
            };

            let _ = tx.send(event);
        });
    }
}

/// Everything after the first word, trimmed of the punctuation a recogniser
/// puts after a name ("Davis, open Spotify" → "open Spotify").
fn drop_first_word(text: &str) -> String {
    let text = text.trim();
    match text.split_once(char::is_whitespace) {
        Some((_, rest)) => rest
            .trim_start_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
            .to_string(),
        None => String::new(),
    }
}

/// Mode name for the interface.
pub fn mode_name(mode: VoiceMode) -> &'static str {
    match mode {
        VoiceMode::Off => "off",
        VoiceMode::Continuous => "continuous",
        VoiceMode::WakeWord => "wake",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> VoiceState {
        VoiceState::new("vavis".into(), "en".into(), String::new())
    }

    /// A state that will actually buffer speech.
    ///
    /// `set_mode` cannot be used here: a listening mode opens a real
    /// microphone and needs a key. The field is set directly because these
    /// tests are about the text buffer, not about the device.
    fn speaking_state() -> VoiceState {
        let mut v = state();
        v.mode = VoiceMode::Continuous;
        v
    }

    #[test]
    fn a_misheard_name_is_still_dropped_from_the_request() {
        assert_eq!(drop_first_word("Davis, open Spotify"), "open Spotify");
        assert_eq!(drop_first_word("Davis"), "");
        assert_eq!(drop_first_word("  Vavis   hava nasıl  "), "hava nasıl");
    }

    #[test]
    fn a_missing_model_file_means_no_wake_model() {
        let mut v = state();
        let dir = tempfile::tempdir().unwrap();
        v.attach_wake_model(dir.path().join("wake.json"));
        assert!(!v.wake_trained());
        // Forgetting what is not there is not an error.
        v.forget_wake_model().unwrap();
    }

    #[test]
    fn a_corrupt_model_file_is_ignored() {
        let mut v = state();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wake.json");
        std::fs::write(&path, "{not json").unwrap();
        v.attach_wake_model(path);
        assert!(!v.wake_trained());
    }

    #[test]
    fn starts_off_and_silent() {
        let v = state();
        assert_eq!(v.mode(), VoiceMode::Off);
        assert!(!v.is_speaking());
    }

    #[test]
    fn listening_without_a_key_is_refused() {
        // Failing loudly beats opening a microphone that can never
        // transcribe anything.
        let mut v = state();
        let err = v.set_mode(VoiceMode::Continuous).unwrap_err();
        assert!(err.contains("Groq"));
        assert_eq!(v.mode(), VoiceMode::Off, "mode must not change on failure");
    }

    #[test]
    fn speaking_is_a_no_op_while_voice_is_off() {
        let v = state();
        v.begin_stream();
        v.push_stream("hello. there.");
        v.finish_stream();
        assert!(!v.is_speaking());
    }

    /// Only finished sentences are spoken as they stream.
    ///
    /// A fragment read aloud stops mid-thought and the next chunk restarts
    /// it, which sounds worse than waiting a beat. The tail stays buffered.
    #[test]
    fn a_half_sentence_waits_for_its_ending() {
        let v = speaking_state();
        v.begin_stream();
        v.push_stream("bu cümle daha");
        assert_eq!(
            v.pending.lock().unwrap().as_str(),
            "bu cümle daha",
            "bitmemiş cümle tamponda kalmalı"
        );

        v.push_stream(" bitmedi. ama bu bitti");
        assert_eq!(
            v.pending.lock().unwrap().as_str(),
            " ama bu bitti",
            "yalnızca tamamlanan kısım alınmalı"
        );
    }

    /// A barge-in cancels the rest of the answer — including the sentence
    /// still sitting in the buffer, which would otherwise be spoken after it.
    #[test]
    fn stopping_drops_the_buffered_tail() {
        let v = speaking_state();
        v.begin_stream();
        v.push_stream("yarım kalan");
        v.stop_speaking();
        assert!(v.pending.lock().unwrap().is_empty());
    }

    /// A new turn must not inherit the last one's unfinished sentence.
    #[test]
    fn a_new_turn_starts_with_an_empty_buffer() {
        let v = speaking_state();
        v.begin_stream();
        v.push_stream("terk edilmiş");
        v.begin_stream();
        assert!(v.pending.lock().unwrap().is_empty());
    }

    #[test]
    fn stop_speaking_is_safe_when_idle() {
        let v = state();
        v.stop_speaking();
        v.stop_speaking();
        assert!(!v.is_speaking());
    }

    #[test]
    fn setting_the_same_mode_twice_is_harmless() {
        let mut v = state();
        assert_eq!(v.set_mode(VoiceMode::Off).unwrap(), VoiceMode::Off);
    }

    #[test]
    fn mode_names_are_distinct() {
        let names = [
            mode_name(VoiceMode::Off),
            mode_name(VoiceMode::Continuous),
            mode_name(VoiceMode::WakeWord),
        ];
        let mut sorted = names;
        sorted.sort_unstable();
        let before = sorted.len();
        let mut deduped = sorted.to_vec();
        deduped.dedup();
        assert_eq!(before, deduped.len());
    }

    #[test]
    fn events_drain_in_order() {
        let mut v = state();
        v.tx.send(VoiceEvent::Woke).unwrap();
        v.tx.send(VoiceEvent::Notice {
            text: "second".into(),
        })
        .unwrap();

        let events = v.poll();
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], VoiceEvent::Woke));
    }
}
