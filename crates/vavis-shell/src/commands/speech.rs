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
    }
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
