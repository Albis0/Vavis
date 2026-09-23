//! Live conversation: talking with the model the way you talk with a
//! person.
//!
//! The ordinary voice path is a relay -- wait for the sentence to end,
//! upload it, transcribe it, ask the model, synthesise the answer, play it.
//! Each hop adds latency, and the whole thing is strictly turn-by-turn: the
//! assistant cannot be interrupted except by a key press.
//!
//! Gemini's Live API is one WebSocket carrying audio both ways. The
//! microphone streams in continuously; the model's voice streams back as it
//! is generated, typically starting well under a second after the user
//! stops; and when the user starts talking over it, the server notices and
//! says so, and playback stops mid-word. Tools still work: the model asks
//! for a call over the same socket, Vavis runs it (through the permission
//! gate, via the caller's `run_tool`), and sends the result back.
//!
//! This module speaks the protocol. [`setup_message`], [`audio_message`],
//! [`parse`] and [`tool_response`] are pure and tested; [`run`] is the
//! loop that connects them to a socket, a microphone and a speaker.

use crate::stream_out::{f32_to_pcm16, PcmPlayer};
use base64::Engine;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Duration;

const ENDPOINT: &str = "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent";

/// A live model the Live API has served. Overridable in settings; the
/// settings screen lists what the key can actually reach.
pub const DEFAULT_MODEL: &str = "gemini-3.1-flash-live-preview";

/// Prebuilt voices the Live API offers.
pub const VOICES: [&str; 8] = [
    "Puck", "Charon", "Kore", "Fenrir", "Aoede", "Leda", "Orus", "Zephyr",
];
pub const DEFAULT_VOICE: &str = "Puck";

/// The model's audio comes back at this rate unless it says otherwise.
const OUTPUT_RATE: u32 = 24_000;

/// Microphone level below which input is not sent while the assistant's
/// own voice is playing. Without echo cancellation the microphone hears the
/// speaker, and the server would take the assistant's voice for the user's
/// and interrupt itself. Speaking clearly over it still gets through --
/// that is how the user barges in. Headphones make this moot.
const BARGE_IN_RMS: f32 = 0.06;

/// Where the model's voice goes. The speaker in real use; a recorder in
/// tests, where there is no sound card.
pub trait Speaker {
    fn push_pcm16(&self, pcm: &[u8], rate: u32);
    fn clear(&self);
    fn is_playing(&self) -> bool;
}

impl Speaker for PcmPlayer {
    fn push_pcm16(&self, pcm: &[u8], rate: u32) {
        PcmPlayer::push_pcm16(self, pcm, rate);
    }
    fn clear(&self) {
        PcmPlayer::clear(self);
    }
    fn is_playing(&self) -> bool {
        PcmPlayer::is_playing(self)
    }
}

/// Everything a session needs.
#[derive(Debug, Clone)]
pub struct LiveConfig {
    /// Overrides the Live API address; for tests.
    pub endpoint: Option<String>,
    pub api_key: String,
    pub model: String,
    pub voice: String,
    pub system: String,
    /// Tool schemas in the OpenAI shape the registry produces.
    pub tools: Vec<Value>,
}

/// Something that happened on the socket.
#[derive(Debug, Clone, PartialEq)]
pub enum LiveEvent {
    /// The session is set up and listening.
    Ready,
    /// A piece of the model's voice: 16-bit mono PCM and its sample rate.
    Audio { pcm: Vec<u8>, rate: u32 },
    /// The user started talking over the model; stop playing.
    Interrupted,
    /// The model finished its turn.
    TurnComplete,
    /// Transcription of what the user said, in pieces.
    UserText(String),
    /// Transcription of what the model said, in pieces.
    ModelText(String),
    /// The model wants tools run.
    ToolCalls(Vec<ToolCall>),
    /// The server will close the session soon.
    GoingAway,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub args: Value,
}

/// The first message of a session.
pub fn setup_message(cfg: &LiveConfig) -> Value {
    let model = if cfg.model.starts_with("models/") {
        cfg.model.clone()
    } else {
        format!("models/{}", cfg.model)
    };
    let mut setup = json!({
        "model": model,
        "generationConfig": {
            "responseModalities": ["AUDIO"],
            "speechConfig": {
                "voiceConfig": {"prebuiltVoiceConfig": {"voiceName": cfg.voice}}
            }
        },
        "systemInstruction": {"parts": [{"text": cfg.system}]},
        // Both sides transcribed, so the conversation lands in the chat
        // feed and the history like any other.
        "inputAudioTranscription": {},
        "outputAudioTranscription": {}
    });
    let declarations: Vec<Value> = cfg.tools.iter().filter_map(declaration).collect();
    if !declarations.is_empty() {
        setup["tools"] = json!([{"functionDeclarations": declarations}]);
    }
    json!({"setup": setup})
}

/// An OpenAI-shaped schema as a Gemini function declaration. Gemini takes
/// an OpenAPI subset and rejects a few JSON Schema keywords outright.
fn declaration(schema: &Value) -> Option<Value> {
    let f = &schema["function"];
    let name = f["name"].as_str()?;
    let mut decl = json!({
        "name": name,
        "description": f["description"].as_str().unwrap_or_default(),
    });
    let params = strip_unsupported(f["parameters"].clone());
    let has_props = params["properties"]
        .as_object()
        .is_some_and(|p| !p.is_empty());
    if has_props {
        decl["parameters"] = params;
    }
    Some(decl)
}

fn strip_unsupported(mut v: Value) -> Value {
    if let Some(obj) = v.as_object_mut() {
        for key in ["additionalProperties", "$schema", "default", "examples"] {
            obj.remove(key);
        }
        for (_, child) in obj.iter_mut() {
            *child = strip_unsupported(child.take());
        }
    } else if let Some(arr) = v.as_array_mut() {
        for child in arr.iter_mut() {
            *child = strip_unsupported(child.take());
        }
    }
    v
}

/// A frame of microphone audio (16 kHz mono floats) as a message.
pub fn audio_message(samples: &[f32]) -> String {
    let data = base64::engine::general_purpose::STANDARD.encode(f32_to_pcm16(samples));
    json!({"realtimeInput": {"audio": {"data": data, "mimeType": "audio/pcm;rate=16000"}}})
        .to_string()
}

/// Results for tool calls, as a message.
pub fn tool_response(results: &[(ToolCall, String)]) -> String {
    let responses: Vec<Value> = results
        .iter()
        .map(|(call, output)| {
            json!({"id": call.id, "name": call.name, "response": {"output": output}})
        })
        .collect();
    json!({"toolResponse": {"functionResponses": responses}}).to_string()
}

/// `audio/pcm;rate=24000` → 24000.
fn rate_of(mime: &str) -> u32 {
    mime.split(';')
        .find_map(|p| p.trim().strip_prefix("rate="))
        .and_then(|r| r.trim().parse().ok())
        .unwrap_or(OUTPUT_RATE)
}

/// Everything one server message says, in order.
pub fn parse(message: &str) -> Vec<LiveEvent> {
    let Ok(v) = serde_json::from_str::<Value>(message) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if v.get("setupComplete").is_some() {
        out.push(LiveEvent::Ready);
    }
    let content = &v["serverContent"];
    if content.is_object() {
        if content["interrupted"] == true {
            out.push(LiveEvent::Interrupted);
        }
        if let Some(text) = content["inputTranscription"]["text"].as_str() {
            if !text.is_empty() {
                out.push(LiveEvent::UserText(text.to_string()));
            }
        }
        for part in content["modelTurn"]["parts"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let inline = &part["inlineData"];
            if let Some(data) = inline["data"].as_str() {
                if let Ok(pcm) = base64::engine::general_purpose::STANDARD.decode(data) {
                    let rate = rate_of(inline["mimeType"].as_str().unwrap_or_default());
                    out.push(LiveEvent::Audio { pcm, rate });
                }
            }
        }
        if let Some(text) = content["outputTranscription"]["text"].as_str() {
            if !text.is_empty() {
                out.push(LiveEvent::ModelText(text.to_string()));
            }
        }
        if content["turnComplete"] == true {
            out.push(LiveEvent::TurnComplete);
        }
    }
    if let Some(calls) = v["toolCall"]["functionCalls"].as_array() {
        let calls: Vec<ToolCall> = calls
            .iter()
            .filter_map(|c| {
                Some(ToolCall {
                    id: c["id"].as_str().unwrap_or_default().to_string(),
                    name: c["name"].as_str()?.to_string(),
                    args: c.get("args").cloned().unwrap_or_else(|| json!({})),
                })
            })
            .collect();
        if !calls.is_empty() {
            out.push(LiveEvent::ToolCalls(calls));
        }
    }
    if v.get("goAway").is_some() {
        out.push(LiveEvent::GoingAway);
    }
    out
}

fn rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt()
}

/// Runs one session until `stop` is set, the server closes it, or it fails.
///
/// `on_event` sees everything except audio (which goes straight to the
/// speaker) and tool calls (which go to `run_tool`, one call at a time,
/// blocking the session while it runs -- the model is waiting for the
/// result anyway).
pub fn run(
    cfg: &LiveConfig,
    mic: Receiver<Vec<f32>>,
    player: &impl Speaker,
    stop: Arc<AtomicBool>,
    mut on_event: impl FnMut(&LiveEvent),
    mut run_tool: impl FnMut(&ToolCall) -> String,
) -> Result<(), String> {
    use tungstenite::Message;

    if cfg.api_key.trim().is_empty() {
        return Err("the live conversation needs a Gemini key".into());
    }

    // The key goes in the URL because that is where the Live API reads it
    // on a socket; this URL is never logged or shown.
    let base = cfg.endpoint.as_deref().unwrap_or(ENDPOINT);
    let url = format!("{base}?key={}", cfg.api_key.trim());
    let (mut socket, _) = tungstenite::connect(url.as_str()).map_err(|e| {
        // The error can echo the request line; keep the key out of it.
        e.to_string().replace(cfg.api_key.trim(), "***")
    })?;

    // Short reads, so the loop can interleave sending microphone audio
    // with receiving the model's.
    let timeout = Some(Duration::from_millis(20));
    match socket.get_mut() {
        tungstenite::stream::MaybeTlsStream::Plain(s) => {
            let _ = s.set_read_timeout(timeout);
        }
        tungstenite::stream::MaybeTlsStream::Rustls(s) => {
            let _ = s.get_mut().set_read_timeout(timeout);
        }
        _ => {}
    }

    socket
        .send(Message::Text(setup_message(cfg).to_string().into()))
        .map_err(|e| e.to_string())?;

    let mut ready = false;
    while !stop.load(Ordering::Relaxed) {
        // Microphone → server, once the session is set up. Frames that pile
        // up while a tool ran are sent in one go.
        let mut outgoing: Vec<f32> = Vec::new();
        for frame in mic.try_iter() {
            outgoing.extend(frame);
        }
        if ready && !outgoing.is_empty() {
            let echo = player.is_playing() && rms(&outgoing) < BARGE_IN_RMS;
            let frame = if echo {
                // Silence, not nothing: the server's voice detector needs to
                // keep hearing that the user is quiet.
                vec![0.0; outgoing.len()]
            } else {
                outgoing
            };
            socket
                .send(Message::Text(audio_message(&frame).into()))
                .map_err(|e| e.to_string())?;
        }

        let message = match socket.read() {
            Ok(m) => m,
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(tungstenite::Error::ConnectionClosed) => return Ok(()),
            Err(e) => return Err(e.to_string()),
        };
        let text = match message {
            Message::Text(t) => t.to_string(),
            // The server sends its JSON in binary frames as often as text.
            Message::Binary(b) => String::from_utf8_lossy(&b).into_owned(),
            Message::Close(frame) => {
                let reason = frame.map(|f| f.reason.to_string()).unwrap_or_default();
                return if reason.is_empty() || ready {
                    Ok(())
                } else {
                    // A close before setup completed is a refusal: bad key,
                    // unknown model, a field the model does not take.
                    Err(reason)
                };
            }
            _ => continue,
        };

        for event in parse(&text) {
            match &event {
                LiveEvent::Ready => ready = true,
                LiveEvent::Audio { pcm, rate } => {
                    player.push_pcm16(pcm, *rate);
                    continue;
                }
                LiveEvent::Interrupted => player.clear(),
                LiveEvent::ToolCalls(calls) => {
                    let results: Vec<(ToolCall, String)> =
                        calls.iter().map(|c| (c.clone(), run_tool(c))).collect();
                    socket
                        .send(Message::Text(tool_response(&results).into()))
                        .map_err(|e| e.to_string())?;
                }
                _ => {}
            }
            on_event(&event);
        }
    }
    let _ = socket.close(None);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> LiveConfig {
        LiveConfig {
            endpoint: None,
            api_key: "k".into(),
            model: DEFAULT_MODEL.into(),
            voice: DEFAULT_VOICE.into(),
            system: "Sen Vavis'sin.".into(),
            tools: vec![
                json!({"type": "function", "function": {
                    "name": "get_time",
                    "description": "time",
                    "parameters": {"type": "object", "additionalProperties": false,
                                   "properties": {"zone": {"type": "string"}}}
                }}),
                json!({"type": "function", "function": {
                    "name": "no_args", "description": "x",
                    "parameters": {"type": "object", "properties": {}}
                }}),
            ],
        }
    }

    #[test]
    fn setup_names_the_model_voice_and_tools() {
        let m = setup_message(&cfg());
        let s = &m["setup"];
        assert_eq!(s["model"], format!("models/{DEFAULT_MODEL}"));
        assert_eq!(s["generationConfig"]["responseModalities"][0], "AUDIO");
        assert_eq!(
            s["generationConfig"]["speechConfig"]["voiceConfig"]["prebuiltVoiceConfig"]
                ["voiceName"],
            "Puck"
        );
        assert_eq!(s["systemInstruction"]["parts"][0]["text"], "Sen Vavis'sin.");
        let decls = &s["tools"][0]["functionDeclarations"];
        assert_eq!(decls[0]["name"], "get_time");
        assert!(decls[0]["parameters"].get("additionalProperties").is_none());
        assert!(
            decls[1].get("parameters").is_none(),
            "empty parameters are left out"
        );
        assert!(s.get("inputAudioTranscription").is_some());
    }

    #[test]
    fn a_qualified_model_is_not_qualified_twice() {
        let mut c = cfg();
        c.model = "models/x-live".into();
        assert_eq!(setup_message(&c)["setup"]["model"], "models/x-live");
    }

    #[test]
    fn audio_goes_out_as_base64_pcm16() {
        let m: Value = serde_json::from_str(&audio_message(&[0.0, 0.5])).unwrap();
        let audio = &m["realtimeInput"]["audio"];
        assert_eq!(audio["mimeType"], "audio/pcm;rate=16000");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(audio["data"].as_str().unwrap())
            .unwrap();
        assert_eq!(bytes.len(), 4);
    }

    #[test]
    fn server_content_parses_in_order() {
        let pcm = base64::engine::general_purpose::STANDARD.encode([1u8, 0, 2, 0]);
        let msg = json!({"serverContent": {
            "inputTranscription": {"text": "saat kaç"},
            "modelTurn": {"parts": [{"inlineData": {"mimeType": "audio/pcm;rate=24000", "data": pcm}}]},
            "outputTranscription": {"text": "on üç"},
            "turnComplete": true
        }})
        .to_string();
        assert_eq!(
            parse(&msg),
            vec![
                LiveEvent::UserText("saat kaç".into()),
                LiveEvent::Audio {
                    pcm: vec![1, 0, 2, 0],
                    rate: 24_000
                },
                LiveEvent::ModelText("on üç".into()),
                LiveEvent::TurnComplete,
            ]
        );
    }

    #[test]
    fn setup_complete_interruption_and_go_away_parse() {
        assert_eq!(parse(r#"{"setupComplete":{}}"#), vec![LiveEvent::Ready]);
        assert_eq!(
            parse(r#"{"serverContent":{"interrupted":true}}"#),
            vec![LiveEvent::Interrupted]
        );
        assert_eq!(
            parse(r#"{"goAway":{"timeLeft":"10s"}}"#),
            vec![LiveEvent::GoingAway]
        );
        assert!(parse("not json").is_empty());
    }

    #[test]
    fn tool_calls_parse_and_answers_match_them() {
        let msg = r#"{"toolCall":{"functionCalls":[{"id":"c1","name":"get_time","args":{"zone":"TR"}}]}}"#;
        let events = parse(msg);
        let LiveEvent::ToolCalls(calls) = &events[0] else {
            panic!("{events:?}");
        };
        assert_eq!(calls[0].name, "get_time");
        assert_eq!(calls[0].args["zone"], "TR");

        let reply: Value =
            serde_json::from_str(&tool_response(&[(calls[0].clone(), "13:37".into())])).unwrap();
        let r = &reply["toolResponse"]["functionResponses"][0];
        assert_eq!(r["id"], "c1");
        assert_eq!(r["name"], "get_time");
        assert_eq!(r["response"]["output"], "13:37");
    }

    #[derive(Default)]
    struct Recorder {
        audio: std::sync::Mutex<Vec<u8>>,
        cleared: std::sync::atomic::AtomicUsize,
    }

    impl Speaker for Recorder {
        fn push_pcm16(&self, pcm: &[u8], _rate: u32) {
            self.audio.lock().unwrap().extend_from_slice(pcm);
        }
        fn clear(&self) {
            self.cleared.fetch_add(1, Ordering::SeqCst);
        }
        fn is_playing(&self) -> bool {
            false
        }
    }

    /// A whole session against a stand-in server: setup, audio in, a tool
    /// call answered, the model's voice out, an interruption, the turn's
    /// transcripts, and a clean close.
    #[test]
    fn a_session_runs_end_to_end() {
        use tungstenite::Message;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut ws = tungstenite::accept(stream).unwrap();
            let mut seen = Vec::new();
            let next = |ws: &mut tungstenite::WebSocket<std::net::TcpStream>| loop {
                match ws.read().unwrap() {
                    Message::Text(t) => return serde_json::from_str::<Value>(&t).unwrap(),
                    _ => continue,
                }
            };
            let setup = next(&mut ws);
            seen.push(setup.clone());
            ws.send(Message::Binary(br#"{"setupComplete":{}}"#.to_vec().into()))
                .unwrap();

            // Wait for some microphone audio.
            loop {
                let m = next(&mut ws);
                if m.get("realtimeInput").is_some() {
                    seen.push(m);
                    break;
                }
            }
            ws.send(Message::Text(
                r#"{"toolCall":{"functionCalls":[{"id":"c1","name":"get_time","args":{"zone":"TR"}}]}}"#.into(),
            ))
            .unwrap();
            let answer = loop {
                let m = next(&mut ws);
                if m.get("toolResponse").is_some() {
                    break m;
                }
            };
            seen.push(answer);
            let pcm = base64::engine::general_purpose::STANDARD.encode([9u8, 0, 8, 0]);
            ws.send(Message::Text(
                json!({"serverContent": {
                    "inputTranscription": {"text": "saat kaç"},
                    "modelTurn": {"parts": [{"inlineData": {"mimeType": "audio/pcm;rate=24000", "data": pcm}}]},
                    "outputTranscription": {"text": "on üç otuz yedi"}
                }})
                .to_string()
                .into(),
            ))
            .unwrap();
            ws.send(Message::Text(
                r#"{"serverContent":{"interrupted":true}}"#.into(),
            ))
            .unwrap();
            ws.send(Message::Text(
                r#"{"serverContent":{"turnComplete":true}}"#.into(),
            ))
            .unwrap();
            ws.close(None).unwrap();
            // Drain until the client acknowledges the close.
            while ws.read().is_ok() {}
            seen
        });

        let mut config = cfg();
        config.endpoint = Some(format!("ws://127.0.0.1:{port}/live"));
        let (mic_tx, mic_rx) = std::sync::mpsc::channel();
        let feeder = std::thread::spawn(move || {
            for _ in 0..50 {
                if mic_tx.send(vec![0.01f32; 320]).is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        let speaker = Recorder::default();
        let mut events = Vec::new();
        let mut tools = Vec::new();
        let result = run(
            &config,
            mic_rx,
            &speaker,
            Arc::new(AtomicBool::new(false)),
            |e| events.push(e.clone()),
            |call| {
                tools.push(call.clone());
                "13:37".into()
            },
        );
        let seen = server.join().unwrap();
        drop(feeder);

        assert!(result.is_ok(), "{result:?}");
        assert_eq!(seen[0]["setup"]["model"], format!("models/{DEFAULT_MODEL}"));
        assert!(seen[1]["realtimeInput"]["audio"]["data"].is_string());
        assert_eq!(
            seen[2]["toolResponse"]["functionResponses"][0]["response"]["output"],
            "13:37"
        );
        assert_eq!(tools[0].name, "get_time");
        assert_eq!(*speaker.audio.lock().unwrap(), vec![9, 0, 8, 0]);
        assert_eq!(
            speaker.cleared.load(Ordering::SeqCst),
            1,
            "interruption silences"
        );
        assert!(events.contains(&LiveEvent::Ready));
        assert!(events.contains(&LiveEvent::UserText("saat kaç".into())));
        assert!(events.contains(&LiveEvent::ModelText("on üç otuz yedi".into())));
        assert!(events.contains(&LiveEvent::TurnComplete));
    }

    #[test]
    fn a_refusal_before_setup_is_an_error_with_the_reason() {
        use tungstenite::protocol::{frame::coding::CloseCode, CloseFrame};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut ws = tungstenite::accept(stream).unwrap();
            let _ = ws.read();
            ws.close(Some(CloseFrame {
                code: CloseCode::Policy,
                reason: "models/nope is not found".into(),
            }))
            .unwrap();
            while ws.read().is_ok() {}
        });
        let mut config = cfg();
        config.endpoint = Some(format!("ws://127.0.0.1:{port}/live"));
        let (_tx, rx) = std::sync::mpsc::channel();
        let err = run(
            &config,
            rx,
            &Recorder::default(),
            Arc::new(AtomicBool::new(false)),
            |_| {},
            |_| String::new(),
        )
        .unwrap_err();
        server.join().unwrap();
        assert!(err.contains("not found"), "{err}");
    }

    #[test]
    fn the_key_never_appears_in_an_error() {
        let mut config = cfg();
        config.api_key = "SECRET123".into();
        // Nothing listens here.
        config.endpoint = Some("ws://127.0.0.1:1/live".into());
        let (_tx, rx) = std::sync::mpsc::channel();
        let err = run(
            &config,
            rx,
            &Recorder::default(),
            Arc::new(AtomicBool::new(false)),
            |_| {},
            |_| String::new(),
        )
        .unwrap_err();
        assert!(!err.contains("SECRET123"), "{err}");
    }

    #[test]
    fn a_missing_rate_means_the_default() {
        assert_eq!(rate_of("audio/pcm"), 24_000);
        assert_eq!(rate_of("audio/pcm;rate=16000"), 16_000);
    }
}
