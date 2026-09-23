//! Steam account settings.

use super::*;

// ---------------------------------------------------------------------------
// Steam
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamSettings {
    pub steam_id: String,
    /// Whether a key is stored — never the key itself.
    pub has_key: bool,
}

#[tauri::command]
pub fn get_steam_settings(state: State<AppState>) -> SteamSettings {
    let core = AppState::lock(&state.core);
    let keys = AppState::lock(&state.keys);
    SteamSettings {
        steam_id: core.config.steam.steam_id.clone(),
        has_key: keys.get("steam").is_some(),
    }
}

/// Saves Steam credentials and checks them.
///
/// The check matters more than it looks: a private profile answers HTTP 200
/// with an empty list, so without it every later question would come back
/// "you own no games" and the user would have no idea why.
#[tauri::command(async)]
pub fn set_steam(state: State<AppState>, steam_id: String, key: String) -> Result<String, String> {
    let id = steam_id.trim().to_string();
    if !id.is_empty() && (id.len() != 17 || !id.chars().all(|c| c.is_ascii_digit())) {
        return Err("A SteamID64 is 17 digits.".into());
    }

    {
        let mut core = AppState::lock(&state.core);
        core.config.steam.steam_id = id.clone();
        core.config.save(&core.paths).map_err(|e| e.to_string())?;

        let mut keys = AppState::lock(&state.keys);
        // An empty key means "leave what is stored" rather than "erase it":
        // the field is a password input and comes back blank on every reload.
        if !key.trim().is_empty() {
            keys.set("steam", key.trim());
            keys.save(core.paths.root()).map_err(|e| e.to_string())?;
        }
    }

    state.refresh_steam();

    if !vavis_tools::steam::current().is_configured() {
        return Ok("Saved. The library needs both a key and a SteamID.".into());
    }

    // Verify once, now, while the user is looking at the settings panel.
    match vavis_tools::steam::library() {
        Ok(games) => Ok(format!("Connected — {} games visible.", games.len())),
        Err(e) => Ok(format!("Saved, but: {e}")),
    }
}
