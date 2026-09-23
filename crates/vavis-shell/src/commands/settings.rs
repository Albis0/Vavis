//! General settings and the window mode.

use super::*;

/// Updates one setting.
#[tauri::command]
pub fn set_setting(state: State<AppState>, field: String, value: String) -> Result<(), String> {
    let mut core = AppState::lock(&state.core);
    let mut language_changed = false;

    match field.as_str() {
        "name" => core.config.general.assistant_name = value,
        "language" => {
            if vavis_core::Lang::parse(&value).is_none() {
                return Err("language must be one of: en, tr, de, fr, es".into());
            }
            core.config.general.language = value.clone();
            AppState::lock(&state.voice).set_language(value);
            // Which voice speaks follows the language, so the engine has to
            // be rebuilt too -- see the refresh below.
            language_changed = true;
        }
        "fontSize" => {
            let size: f32 = value.parse().map_err(|_| "font size must be a number")?;
            if !(8.0..=32.0).contains(&size) {
                return Err("font size must be between 8 and 32".into());
            }
            core.config.ui.font_size = size;
        }
        "windowMode" => {
            if !["windowed", "borderless", "fullscreen"].contains(&value.as_str()) {
                return Err("window mode: windowed, borderless or fullscreen".into());
            }
            core.config.ui.window_mode = value;
        }
        // The cheap model that picks tools. Empty turns routing off, which
        // is the default -- so clearing the box is a supported answer, not
        // an error.
        "routerModel" => core.config.llm.router_model = value.trim().to_string(),
        // Endpoints for the two providers that live wherever the user put
        // them. Empty is valid: custom then refuses until set, local goes
        // back to Ollama's default port.
        "customUrl" => core.config.llm.custom_url = value.trim().to_string(),
        "memoryInject" => core.config.memory.inject = value == "true",
        "memoryAutoExtract" => core.config.memory.auto_extract = value == "true",
        "memoryEmbeddings" => {
            let v = value.trim().to_ascii_lowercase();
            let known = v == "auto"
                || v == "off"
                || vavis_brain::embeddings::PROVIDERS
                    .iter()
                    .any(|p| p.key_name() == v);
            if !known {
                return Err(format!("unknown embedding provider: {value}"));
            }
            core.config.memory.embeddings = v;
        }
        "localUrl" => core.config.llm.local_url = value.trim().to_string(),
        // Full authority: every approval off, every budget off. The warning
        // belongs in the interface, once, at the moment it is switched on --
        // re-asking here on every turn is the thing being switched off.
        "fullAuthority" => {
            let on = match value.as_str() {
                "true" => true,
                "false" => false,
                _ => return Err("full authority must be true or false".into()),
            };
            core.config.security.full_authority = on;
            tracing::warn!(on, "full authority changed");
        }
        // ── Speech ──────────────────────────────────────────────────────
        //
        // The engine and the voices. Keys are not here: those go through
        // `set_key`, into the encrypted store, and never touch this file.
        "voiceEngine" => {
            let Some(engine) = vavis_audio::TtsEngineKind::parse(&value) else {
                return Err("voice engine: sapi, edge, kokoro, elevenlabs or openai".into());
            };
            core.config.voice.engine = engine.id().to_string();
        }
        "voiceRate" => {
            let rate: i32 = value.parse().map_err(|_| "rate must be a number")?;
            if !(-10..=10).contains(&rate) {
                return Err("rate must be between -10 and 10".into());
            }
            core.config.voice.rate = rate;
        }
        "voiceVolume" => {
            let volume: u32 = value.parse().map_err(|_| "volume must be a number")?;
            if volume > 100 {
                return Err("volume must be between 0 and 100".into());
            }
            core.config.voice.volume = volume;
        }
        "sapiVoice" => core.config.voice.sapi_voice = value.trim().to_string(),
        "edgeVoice" => core.config.voice.edge_voice = value.trim().to_string(),
        // Blank means "use the default address", which is what the Kokoro
        // container listens on -- so most users never type anything here.
        "kokoroUrl" => core.config.voice.kokoro_url = value.trim().to_string(),
        "kokoroVoice" => core.config.voice.kokoro_voice = value.trim().to_string(),
        "elevenVoice" => core.config.voice.eleven_voice = value.trim().to_string(),
        "elevenModel" => core.config.voice.eleven_model = value.trim().to_string(),
        "openaiVoice" => core.config.voice.openai_voice = value.trim().to_string(),
        "openaiModel" => core.config.voice.openai_model = value.trim().to_string(),
        "geminiVoice" => core.config.voice.gemini_voice = value.trim().to_string(),
        "geminiModel" => core.config.voice.gemini_model = value.trim().to_string(),
        // Anything other than "true" reads as off, so a malformed value
        // pins the engine the user picked rather than silently moving them.
        "matchProvider" => core.config.voice.match_provider = value.trim() == "true",

        other => return Err(format!("unknown setting: {other}")),
    }

    // Listed rather than pattern-matched: "routerModel" ends in "Model" and
    // has nothing to do with speech, so a suffix rule would quietly rebuild
    // the speech engine every time the router changed.
    const VOICE_FIELDS: [&str; 14] = [
        "voiceEngine",
        "voiceRate",
        "voiceVolume",
        "sapiVoice",
        "edgeVoice",
        "kokoroUrl",
        "kokoroVoice",
        "elevenVoice",
        "elevenModel",
        "openaiVoice",
        "openaiModel",
        "geminiVoice",
        "geminiModel",
        // Changes which engine speaks, so the voice layer has to be rebuilt.
        "matchProvider",
    ];
    let is_voice = VOICE_FIELDS.contains(&field.as_str()) || language_changed;

    core.config.save(&core.paths).map_err(|e| e.to_string())?;

    // The lock has to go before refreshing: `refresh_voice` takes it again,
    // and holding it here would deadlock the moment anyone changed a voice.
    drop(core);
    if is_voice {
        state.refresh_voice();
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Window mode
// ---------------------------------------------------------------------------

/// Puts the window into `mode`.
///
/// # Why this is not two lines in the frontend
///
/// It used to be, and startup had its own copy — which is how the two drifted
/// apart. The frontend called `maximize()` and the window stayed the size it
/// was, because of two things `maximize()` alone does not handle:
///
/// * **Leaving fullscreen is not instant.** Asking to maximize in the same
///   breath as `set_fullscreen(false)` races the window manager, and the
///   maximize is the one that loses.
/// * **An undecorated window maximizes oddly.** With the OS title bar off,
///   "maximized" is not reliably the whole work area, which is what the user
///   means by borderless.
///
/// So borderless is applied by measuring the monitor and setting the size
/// directly. One function, called from startup and from the F11 toggle.
pub fn apply_window_mode(
    window: &tauri::WebviewWindow,
    mode: vavis_core::WindowMode,
) -> Result<(), String> {
    use vavis_core::WindowMode;

    // The interface draws its own title strip, so the OS decorations stay off
    // in every mode — turning them on gives two title bars.
    let was_fullscreen = window.is_fullscreen().unwrap_or(false);
    if was_fullscreen && mode != WindowMode::Fullscreen {
        window.set_fullscreen(false).map_err(|e| e.to_string())?;
    }

    match mode {
        WindowMode::Fullscreen => window.set_fullscreen(true).map_err(|e| e.to_string())?,

        WindowMode::Borderless => {
            // Unmaximize first: a window that is already maximized ignores a
            // set_size, and would keep whatever bounds it had.
            let _ = window.unmaximize();

            match window.current_monitor() {
                Ok(Some(monitor)) => {
                    let size = *monitor.size();
                    let position = *monitor.position();
                    window.set_position(position).map_err(|e| e.to_string())?;
                    window.set_size(size).map_err(|e| e.to_string())?;
                }
                // No monitor information — maximize is the honest fallback,
                // and is what this always did before.
                _ => window.maximize().map_err(|e| e.to_string())?,
            }
        }

        WindowMode::Windowed => {
            let _ = window.unmaximize();
        }
    }

    Ok(())
}

/// Sets the window mode and saves it.
///
/// Replaces the frontend doing the move itself: the two paths had drifted, and
/// only one of them worked.
#[tauri::command]
pub fn set_window_mode(
    state: State<AppState>,
    window: tauri::WebviewWindow,
    mode: String,
) -> Result<(), String> {
    if !["windowed", "borderless", "fullscreen"].contains(&mode.as_str()) {
        return Err("window mode: windowed, borderless or fullscreen".into());
    }

    apply_window_mode(&window, vavis_core::WindowMode::parse(&mode))?;

    let mut core = AppState::lock(&state.core);
    core.config.ui.window_mode = mode;
    core.config.save(&core.paths).map_err(|e| e.to_string())
}
