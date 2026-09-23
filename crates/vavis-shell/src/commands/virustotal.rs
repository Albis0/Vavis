//! VirusTotal key and settings.

use super::*;

// ---------------------------------------------------------------------------
// VirusTotal
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VirusTotalSettings {
    /// Whether a key is stored — never the key itself.
    pub has_key: bool,
}

#[tauri::command]
pub fn get_virustotal_settings(state: State<AppState>) -> VirusTotalSettings {
    let keys = AppState::lock(&state.keys);
    VirusTotalSettings {
        has_key: keys.get("virustotal").is_some(),
    }
}

/// Saves the VirusTotal key and checks it against the real service.
///
/// The check is worth the one request: a key pasted with a stray character
/// looks identical to a good one until the first scan fails, and by then the
/// user is somewhere else and no longer connects the two.
#[tauri::command(async)]
pub fn set_virustotal_key(state: State<AppState>, key: String) -> Result<String, String> {
    let key = key.trim().to_string();

    {
        let mut keys = AppState::lock(&state.keys);
        if key.is_empty() {
            // Clearing is a supported answer: it turns the tools off again.
            keys.remove("virustotal");
        } else {
            keys.set("virustotal", &key);
        }
        let root = AppState::lock(&state.core).paths.root().to_path_buf();
        keys.save(&root).map_err(|e| e.to_string())?;
    }

    state.refresh_virustotal();

    if key.is_empty() {
        return Ok("VirusTotal key removed.".into());
    }

    // A hash that is certain to be known: the SHA-256 of the empty file.
    // Using a real lookup rather than a made-up hash means a wrong key comes
    // back as a rejection rather than as "never seen", which is the answer
    // that would send someone hunting the wrong problem.
    const EMPTY_FILE_SHA256: &str =
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    match vavis_tools::virustotal::lookup_hash(EMPTY_FILE_SHA256) {
        Ok(_) => Ok("Connected — the key works.".into()),
        Err(vavis_tools::virustotal::VtError::BadKey) => {
            Err("VirusTotal rejected that key. Check it and paste it again.".into())
        }
        Err(vavis_tools::virustotal::VtError::RateLimited) => {
            Ok("Saved. The rate limit is busy right now, so it was not verified.".into())
        }
        Err(e) => Ok(format!("Saved, but the check did not complete: {e}")),
    }
}
