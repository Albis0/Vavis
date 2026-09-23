//! Voice: engines, keys, modes, and the microphone level.

use super::*;

/// Stores the ElevenLabs key.
///
/// Separate from `set_key` because ElevenLabs is not a chat provider: it does
/// speech only, so it has no place in the provider list the model picker
/// reads. The key still goes to the same encrypted store.
#[tauri::command]
pub fn set_voice_key(state: State<AppState>, key: String) -> Result<(), String> {
    {
        let mut keys = AppState::lock(&state.keys);
        keys.set("elevenlabs", key.trim());
        let root = AppState::lock(&state.core).paths.root().to_path_buf();
        keys.save(&root).map_err(|e| e.to_string())?;
    }
    state.refresh_voice();
    Ok(())
}

/// What the speech section of settings shows.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceSettings {
    /// Engine ids, in the order the interface should list them.
    pub engines: Vec<VoiceEngineInfo>,
    pub engine: String,
    pub rate: i32,
    pub volume: u32,
    pub sapi_voice: String,
    pub edge_voice: String,
    pub kokoro_url: String,
    pub kokoro_voice: String,
    pub eleven_voice: String,
    pub openai_voice: String,
    pub gemini_voice: String,
    /// True when an ElevenLabs key is stored. The key never crosses this bridge.
    pub has_eleven_key: bool,
    pub has_openai_key: bool,
    pub has_gemini_key: bool,
    /// Whether to speak with the chat provider's own voice when it has one.
    pub match_provider: bool,
    /// The engine actually speaking right now. Differs from `engine` when
    /// `match_provider` swapped it, and the interface says so rather than
    /// leaving the user wondering why the picker disagrees with what they
    /// hear.
    pub effective_engine: String,
    /// Voices offered per engine, so the interface can show a real list
    /// rather than making the user guess an identifier.
    pub sapi_voices: Vec<String>,
    pub edge_voices: Vec<[String; 2]>,
    pub kokoro_voices: Vec<[String; 2]>,
    pub eleven_voices: Vec<[String; 2]>,
    pub openai_voices: Vec<[String; 2]>,
    pub gemini_voices: Vec<[String; 2]>,
    /// The address Kokoro listens on out of the box, shown as the placeholder.
    pub kokoro_default_url: String,
    /// Which Edge voice an empty choice resolves to, for the current
    /// language. The interface names it rather than showing a blank
    /// "default" the user cannot interpret.
    pub default_edge_voice: String,
    /// A wake word has been trained on this machine.
    pub wake_trained: bool,
    pub wake_sensitivity: u8,
    /// Live conversation model and voice; empty means the default.
    pub live_model: String,
    pub live_voice: String,
    pub live_default_model: String,
    pub live_voices: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceEngineInfo {
    pub id: String,
    pub label: String,
    pub needs_key: bool,
}

fn pairs(list: &[(&'static str, &'static str)]) -> Vec<[String; 2]> {
    list.iter()
        .map(|(id, label)| [id.to_string(), label.to_string()])
        .collect()
}

#[tauri::command]
pub fn get_voice_settings(state: State<AppState>) -> VoiceSettings {
    let core = AppState::lock(&state.core);
    let keys = AppState::lock(&state.keys);
    let v = &core.config.voice;

    VoiceSettings {
        engines: vavis_audio::TtsEngineKind::ALL
            .iter()
            .map(|e| VoiceEngineInfo {
                id: e.id().to_string(),
                label: e.label().to_string(),
                needs_key: e.needs_key(),
            })
            .collect(),
        engine: v.engine.clone(),
        rate: v.rate,
        volume: v.volume,
        sapi_voice: v.sapi_voice.clone(),
        edge_voice: v.edge_voice.clone(),
        kokoro_url: v.kokoro_url.clone(),
        kokoro_voice: v.kokoro_voice.clone(),
        eleven_voice: v.eleven_voice.clone(),
        openai_voice: v.openai_voice.clone(),
        gemini_voice: v.gemini_voice.clone(),
        has_eleven_key: keys.get("elevenlabs").is_some_and(|k| !k.trim().is_empty()),
        has_openai_key: keys.get("openai").is_some_and(|k| !k.trim().is_empty()),
        has_gemini_key: keys.get("gemini").is_some_and(|k| !k.trim().is_empty()),
        match_provider: v.match_provider,
        // Asked of the same function the voice layer uses, so the interface
        // cannot drift from what actually speaks.
        effective_engine: crate::state::effective_engine(&core.config, &keys)
            .id()
            .to_string(),
        sapi_voices: vavis_audio::TtsEngine::available_voices(),
        edge_voices: pairs(vavis_audio::edge_tts::voices()),
        kokoro_voices: pairs(vavis_audio::kokoro::voices()),
        eleven_voices: pairs(vavis_audio::elevenlabs::voices()),
        openai_voices: pairs(vavis_audio::openai_tts::voices()),
        gemini_voices: pairs(vavis_audio::gemini_tts::voices()),
        kokoro_default_url: vavis_audio::kokoro::DEFAULT_URL.to_string(),
        default_edge_voice: vavis_audio::edge_tts::default_voice(&core.config.general.language)
            .to_string(),
        wake_trained: AppState::lock(&state.voice).wake_trained(),
        wake_sensitivity: v.wake_sensitivity,
        live_model: v.live_model.clone(),
        live_voice: v.live_voice.clone(),
        live_default_model: vavis_audio::live::DEFAULT_MODEL.to_string(),
        live_voices: vavis_audio::live::VOICES
            .iter()
            .map(|s| s.to_string())
            .collect(),
    }
}

/// Starts wake-word training. Progress arrives as `voice` events of kind
/// `enrol` and `enrolDone`.
#[tauri::command]
pub fn start_wake_training(state: State<AppState>) -> Result<(), String> {
    AppState::lock(&state.voice).start_enrolment()
}

#[tauri::command]
pub fn cancel_wake_training(state: State<AppState>) {
    AppState::lock(&state.voice).cancel_enrolment();
}

/// Forgets the trained wake word.
#[tauri::command]
pub fn forget_wake_word(state: State<AppState>) -> Result<(), String> {
    AppState::lock(&state.voice).forget_wake_model()
}

/// Speaks a sample line with the current settings.
///
/// The only honest test of a voice is hearing it: a key can be valid and the
/// engine still unreachable, and a voice id can be accepted and still be the
/// wrong voice.
#[tauri::command]
pub fn preview_voice(state: State<AppState>) -> Result<(), String> {
    let text = {
        let core = AppState::lock(&state.core);
        match core.config.general.language.as_str() {
            "en" => "Hello, this is how I sound.",
            _ => "Merhaba, sesim böyle çıkıyor.",
        }
    };
    AppState::lock(&state.voice).preview(text);
    Ok(())
}

/// Cycles the voice mode, returning the new one.
#[tauri::command]
pub fn cycle_voice(state: State<AppState>) -> Result<String, String> {
    let mode = AppState::lock(&state.voice).cycle_mode()?;
    Ok(crate::voice::mode_name(mode).to_string())
}

/// Barge-in: stops speech immediately.
#[tauri::command]
pub fn stop_speaking(state: State<AppState>) {
    AppState::lock(&state.voice).stop_speaking();
}

/// Drains queued voice events.
#[tauri::command]
pub fn poll_voice(state: State<AppState>) -> Vec<crate::voice::VoiceEvent> {
    AppState::lock(&state.voice).poll()
}

/// Just the microphone level.
///
/// Its own command because the meter wants ten readings a second and
/// `get_status` does far too much to be asked that often — CPU sampling,
/// battery, three database counts. This reads one atomic.
#[tauri::command]
pub fn mic_level(state: State<AppState>) -> f32 {
    AppState::lock(&state.voice).mic_level()
}

/// Tools the live conversation is not given: `request_tools` only makes
/// sense for a model handed a subset, and the screenshot pair returns
/// images that cannot travel back over a voice session.
const NOT_LIVE: [&str; 4] = [
    "request_tools",
    "take_screenshot",
    "wait_for_screen",
    "generate_image",
];

/// Starts a live, spoken conversation (Gemini Live). Progress arrives as
/// `voice` events: `live` when it starts and ends, `liveTurn` per exchange.
#[tauri::command(async)]
pub fn start_live(app: tauri::AppHandle, state: State<AppState>) -> Result<(), String> {
    let config = {
        let core = AppState::lock(&state.core);
        let keys = AppState::lock(&state.keys);
        let api_key = keys.get("gemini").unwrap_or_default().to_string();
        if api_key.trim().is_empty() {
            return Err(
                "The live conversation runs on Gemini — add a Gemini key (free at aistudio.google.com)."
                    .into(),
            );
        }
        let v = &core.config.voice;
        let model = if v.live_model.trim().is_empty() {
            vavis_audio::live::DEFAULT_MODEL.to_string()
        } else {
            v.live_model.trim().to_string()
        };
        let voice = if v.live_voice.trim().is_empty() {
            vavis_audio::live::DEFAULT_VOICE.to_string()
        } else {
            v.live_voice.trim().to_string()
        };

        let mut system = vavis_brain::system_prompt_for(
            Provider::Gemini,
            &core.config.general.assistant_name,
            &core.config.general.language,
        );
        system.push_str(
            "\n\nBu sesli, canlı bir konuşma. Konuşur gibi cevap ver: kısa, doğal \
             cümleler; liste, başlık, markdown ya da emoji yok. Kullanıcı sözünü \
             keserse dur ve dinle.",
        );
        // Facts as they are: there is no single question to rank them
        // against in a conversation that has not started yet.
        let facts: Vec<String> = AppState::lock(&state.store)
            .all_facts()
            .unwrap_or_default()
            .into_iter()
            .rev()
            .take(15)
            .map(|f| f.text)
            .collect();
        system.push_str(&crate::recall::block(&facts));

        let tools = {
            let agent = AppState::lock(&state.agent);
            let names: Vec<&str> = agent
                .registry
                .iter()
                .map(|t| t.name())
                .filter(|n| !NOT_LIVE.contains(n))
                .collect();
            agent.schemas_for(&names)
        };

        vavis_audio::live::LiveConfig {
            endpoint: None,
            api_key,
            model,
            voice,
            system,
            tools,
        }
    };

    {
        let full_authority = AppState::lock(&state.core).config.security.full_authority;
        let mut agent = AppState::lock(&state.agent);
        agent.start_run();
        agent.gate.set_full_authority(full_authority);
    }

    let handler = super::chat::ToolBridgeHandler {
        agent: state.agent.clone(),
        app: app.clone(),
        approval_rx: state.approval_rx.clone(),
    };
    let store = state.store.clone();
    let history = state.history.clone();
    let conversation = state.conversation.clone();
    let hooks = crate::voice::LiveHooks {
        run_tool: Box::new(move |id, name, args| {
            use vavis_tools::mcp::bridge::Handler;
            handler.call(id, name, args).text
        }),
        on_turn: Box::new(move |user, assistant| {
            let id = *AppState::lock(&conversation);
            let store = AppState::lock(&store);
            let mut history = AppState::lock(&history);
            if !user.is_empty() {
                let _ = store.add_message_to(id, "user", user);
                history.push(Message::user(user));
            }
            if !assistant.is_empty() {
                let _ = store.add_message_to(id, "assistant", assistant);
                history.push(Message::assistant(assistant));
            }
        }),
    };

    AppState::lock(&state.voice).start_live(config, hooks)
}

#[tauri::command]
pub fn stop_live(state: State<AppState>) {
    AppState::lock(&state.voice).stop_live();
}

/// Live-capable Gemini models the stored key can reach.
#[tauri::command]
pub async fn list_live_models(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let (key, client) = {
        let keys = AppState::lock(&state.keys);
        (
            keys.get("gemini").unwrap_or_default().to_string(),
            state.client.clone(),
        )
    };
    client
        .list_live_models(&key)
        .await
        .map_err(|e| super::friendly_error(&e))
}
