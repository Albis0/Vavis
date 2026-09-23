//! The status snapshot the interface polls, and the provider list it shows.

use super::*;

/// A snapshot of everything the interface shows in its side panels.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub version: String,
    pub assistant_name: String,
    pub language: String,
    pub provider: String,
    pub model: String,
    /// Cheap model that picks tools. Empty when routing is off.
    pub router_model: String,
    /// Provider for code work. Empty means code uses the chat provider.
    pub code_provider: String,
    /// Model for code work. Meaningless while `code_provider` is empty.
    pub code_model: String,
    /// Full authority is on -- nothing will be asked. The interface shows a
    /// standing indicator for this, because a mode that removes every prompt
    /// must not itself be invisible.
    pub full_authority: bool,
    /// Providers tried in order when the chosen one cannot answer.
    pub fallback: Vec<String>,
    /// The `custom` provider's endpoint, as typed.
    pub custom_url: String,
    /// Where the `local` provider listens, when not Ollama's default.
    pub local_url: String,
    pub providers: Vec<ProviderInfo>,
    pub keys: Vec<String>,
    pub tool_count: usize,
    pub history_len: usize,
    pub fact_count: i64,
    pub automation_count: usize,
    pub message_count: i64,
    pub voice_mode: String,
    /// A live conversation is running.
    pub live: bool,
    /// Microphone level, 0.0-1.0. Drives the meter next to the mic button.
    pub mic_level: f32,
    pub busy: bool,
    pub speaking: bool,
    pub cpu: Option<u32>,
    pub battery: Option<u32>,
    pub uptime_secs: u64,
    pub data_dir: String,
    pub window_mode: String,
    pub font_size: f32,
    /// Steam game running right now, detected locally. None when idle.
    pub steam_game: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: String,
    pub needs_key: bool,
    /// Whether a key field makes sense at all. False for Claude Code, whose
    /// login lives in the CLI, and for a local server.
    pub takes_key: bool,
    pub has_key: bool,
    /// Usable right now: a key if one is needed, a URL if one is needed.
    pub usable: bool,
    /// Costs nothing to use: a free tier, free models, or the user's own
    /// hardware.
    pub free_tier: bool,
    pub default_model: String,
}

/// Everything the panels need, in one round trip.
///
/// One call rather than a dozen: the interface polls this on a timer, and
/// a dozen separate IPC calls per second would be wasteful.
#[tauri::command]
pub fn get_status(state: State<AppState>) -> Status {
    let core = AppState::lock(&state.core);
    let keys = AppState::lock(&state.keys);

    let provider = Provider::parse(&core.config.llm.provider).unwrap_or(Provider::Groq);
    let model = model_for(&core.config, provider);

    let (facts, automations, messages) = {
        let store = AppState::lock(&state.store);
        (
            store.fact_count().unwrap_or(0),
            store.all_automations().map(|a| a.len()).unwrap_or(0),
            store.message_count().unwrap_or(0),
        )
    };

    let voice = AppState::lock(&state.voice);

    Status {
        version: vavis_core::VERSION.to_string(),
        assistant_name: core.config.general.assistant_name.clone(),
        language: core.config.general.language.clone(),
        provider: provider.key_name().to_string(),
        model,
        router_model: core.config.llm.router_model.clone(),
        code_provider: core.config.llm.code_provider.clone(),
        code_model: core.config.llm.code_model.clone(),
        full_authority: core.config.security.full_authority,
        fallback: core.config.llm.fallback.clone(),
        custom_url: core.config.llm.custom_url.clone(),
        local_url: core.config.llm.local_url.clone(),
        providers: Provider::ALL
            .iter()
            .map(|p| ProviderInfo {
                id: p.key_name().to_string(),
                needs_key: p.needs_key(),
                takes_key: p.takes_key(),
                has_key: keys.get(p.key_name()).is_some(),
                usable: super::is_usable(&core.config, &keys, *p),
                free_tier: p.has_free_tier(),
                default_model: p.default_model().to_string(),
            })
            .collect(),
        keys: keys.configured().iter().map(|s| s.to_string()).collect(),
        tool_count: AppState::lock(&state.agent).registry.len(),
        history_len: AppState::lock(&state.history).len(),
        fact_count: facts,
        automation_count: automations,
        message_count: messages,
        voice_mode: crate::voice::mode_name(voice.mode()).to_string(),
        live: voice.is_live(),
        mic_level: voice.mic_level(),
        busy: state.busy.load(Ordering::SeqCst),
        speaking: voice.is_speaking(),
        cpu: vavis_tools::builtin::system::cpu_percent(),
        battery: vavis_tools::builtin::system::battery_percent(),
        uptime_secs: state.started.elapsed().as_secs(),
        data_dir: core.paths.root().display().to_string(),
        window_mode: core.config.ui.window_mode.clone(),
        font_size: core.config.ui.font_size,
        steam_game: vavis_tools::steam::running_game_cached().map(|g| g.name),
    }
}
