//! The phone: Telegram bot token, pairing, on and off.

use super::*;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneSettings {
    pub enabled: bool,
    pub has_token: bool,
    /// The bot's @username, once the token has been checked.
    pub bot_name: String,
    pub paired: bool,
    pub owner_name: String,
    /// The current pairing code, while one is valid.
    pub pairing_code: Option<String>,
    /// What went wrong last, if the bot is not reachable.
    pub error: Option<String>,
}

#[tauri::command]
pub fn get_phone_settings(state: State<AppState>) -> PhoneSettings {
    let core = AppState::lock(&state.core);
    let keys = AppState::lock(&state.keys);
    let r = crate::remote::remote();
    PhoneSettings {
        enabled: core.config.telegram.enabled,
        has_token: keys.get("telegram").is_some_and(|k| !k.trim().is_empty()),
        bot_name: r.bot_name(),
        paired: core.config.telegram.owner_id != 0,
        owner_name: core.config.telegram.owner_name.clone(),
        pairing_code: r.pairing_code(),
        error: r.last_error(),
    }
}

/// Stores the bot token after checking it with Telegram, and starts the bot.
/// Returns the bot's @username.
#[tauri::command]
pub fn set_telegram_token(
    app: tauri::AppHandle,
    state: State<AppState>,
    token: String,
) -> Result<String, String> {
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err("paste the token @BotFather gave you".into());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let name = runtime
        .block_on(vavis_tools::telegram::Bot::new(token.clone()).username())
        .map_err(|e| format!("Telegram did not accept the token: {e}"))?;

    {
        let mut keys = AppState::lock(&state.keys);
        keys.set("telegram", token);
        let root = AppState::lock(&state.core).paths.root().to_path_buf();
        keys.save(&root).map_err(|e| e.to_string())?;
    }
    {
        let mut core = AppState::lock(&state.core);
        core.config.telegram.enabled = true;
        // A new bot is a new pairing.
        core.config.telegram.owner_id = 0;
        core.config.telegram.owner_name.clear();
        core.config.save(&core.paths).map_err(|e| e.to_string())?;
    }
    crate::remote::restart(&app);
    Ok(name)
}

/// A fresh one-time code to send the bot as `/pair <code>`.
#[tauri::command]
pub fn telegram_pairing_code() -> String {
    crate::remote::remote().new_pairing_code()
}

/// Forgets the paired account; the bot answers no one until paired again.
#[tauri::command]
pub fn telegram_unpair(app: tauri::AppHandle, state: State<AppState>) -> Result<(), String> {
    {
        let mut core = AppState::lock(&state.core);
        core.config.telegram.owner_id = 0;
        core.config.telegram.owner_name.clear();
        core.config.save(&core.paths).map_err(|e| e.to_string())?;
    }
    crate::remote::restart(&app);
    Ok(())
}

#[tauri::command]
pub fn set_telegram_enabled(
    app: tauri::AppHandle,
    state: State<AppState>,
    enabled: bool,
) -> Result<(), String> {
    {
        let mut core = AppState::lock(&state.core);
        core.config.telegram.enabled = enabled;
        core.config.save(&core.paths).map_err(|e| e.to_string())?;
    }
    crate::remote::restart(&app);
    Ok(())
}
