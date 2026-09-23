//! Spotify: connection, now playing, playback control.

use super::*;

// ---------------------------------------------------------------------------
// Spotify
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotifySettings {
    /// The user's own id, if they set one. Empty means the built-in app.
    pub client_id: String,
    pub connected: bool,
    /// Only needed by someone registering an app of their own; the built-in
    /// one already has this URI on it.
    pub redirect_uri: String,
}

#[tauri::command]
pub fn get_spotify_settings(state: State<AppState>) -> SpotifySettings {
    let core = AppState::lock(&state.core);
    SpotifySettings {
        client_id: core.config.spotify.client_id.clone(),
        connected: vavis_tools::spotify::current().is_connected(),
        redirect_uri: vavis_tools::spotify::auth::redirect_uri(),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NowPlayingInfo {
    pub track: String,
    pub artist: String,
    pub album_art: Option<String>,
    pub duration_ms: u64,
    pub progress_ms: u64,
    pub playing: bool,
    pub device: Option<String>,
}

/// What Spotify is playing, or `None`.
///
/// Kept out of `get_status` on purpose: status is polled every second and
/// must never wait on the network, while this can. The tool layer caches the
/// answer for a few seconds and the interface counts the progress bar
/// forward locally, so Spotify is not asked on every tick.
#[tauri::command(async)]
pub fn spotify_now_playing() -> Option<NowPlayingInfo> {
    match vavis_tools::spotify::now_playing() {
        Ok(Some(np)) => Some(NowPlayingInfo {
            track: np.track,
            artist: np.artist,
            album_art: np.album_art,
            duration_ms: np.duration_ms,
            progress_ms: np.progress_ms,
            playing: np.playing,
            device: np.device,
        }),
        // Not connected or nothing playing both mean "show nothing".
        _ => None,
    }
}

/// Album art as a `data:` URI, cached on disk.
///
/// Two reasons not to point the `<img>` straight at Spotify's CDN: the
/// window's content-security policy allows no remote images, and re-fetching
/// the same cover on every poll would be wasteful. Fetching once here and
/// keeping the bytes means the art survives restarts too.
#[tauri::command(async)]
pub fn spotify_album_art(state: State<AppState>, url: String) -> Result<String, String> {
    if !url.starts_with("https://") {
        return Err("only https artwork is fetched".into());
    }

    let cache_dir = AppState::lock(&state.core).paths.root().join("art");
    std::fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;

    // The CDN path is already a content hash; its last segment is a stable,
    // filesystem-safe name.
    let name: String = url
        .rsplit('/')
        .next()
        .unwrap_or("cover")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    let file = cache_dir.join(format!("{name}.jpg"));

    let bytes = match std::fs::read(&file) {
        Ok(bytes) => bytes,
        Err(_) => {
            let fetched = vavis_tools::spotify::fetch_album_art(&url).map_err(|e| e.to_string())?;
            // A failed write is not fatal; the art still displays this time.
            if let Err(e) = std::fs::write(&file, &fetched) {
                tracing::debug!(%e, "album art not cached");
            }
            fetched
        }
    };

    Ok(format!("data:image/jpeg;base64,{}", base64(&bytes)))
}

/// Standard base64 with padding, for the data URI.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);

    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;

        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }

    out
}

/// Transport control from the now-playing panel.
///
/// Direct rather than routed through the model: pressing pause is not a
/// request for the assistant to think about.
#[tauri::command(async)]
pub fn spotify_control(action: String) -> Result<(), String> {
    let mapped = match action.as_str() {
        "play" | "pause" | "next" | "previous" => action.as_str(),
        other => return Err(format!("unknown action: {other}")),
    };
    vavis_tools::spotify::transport(mapped).map_err(|e| e.to_string())
}

/// Saves a Spotify client id of the user's own.
///
/// Optional. Empty clears it and puts the built-in application back, which is
/// the path almost everyone stays on; only someone who wants their own name on
/// the consent screen sets one.
///
/// A non-empty id is validated before it is stored: an id that is really the
/// redirect URI gets no error from Spotify, just a blank page, so the check
/// has to happen here.
#[tauri::command]
pub fn set_spotify_client_id(state: State<AppState>, client_id: String) -> Result<(), String> {
    if !client_id.trim().is_empty() {
        vavis_tools::spotify::auth::check_client_id(&client_id)?;
    }
    {
        let mut core = AppState::lock(&state.core);
        core.config.spotify.client_id = client_id.trim().to_string();
        core.config.save(&core.paths).map_err(|e| e.to_string())?;
    }
    state.refresh_spotify();
    Ok(())
}

/// Starts the authorisation flow.
///
/// Returns as soon as the browser is opened; the rest happens on a worker
/// thread and lands as a `spotify:auth` event. Blocking a command for the
/// ninety seconds someone might spend logging in would freeze the interface.
#[tauri::command(async)]
pub fn connect_spotify(app: tauri::AppHandle, state: State<AppState>) -> Result<(), String> {
    let configured = AppState::lock(&state.core).config.spotify.client_id.clone();
    // Empty is the ordinary case and means the built-in application, so there
    // is nothing to set up before pressing connect.
    let client_id = vavis_tools::spotify::auth::client_id_or_default(&configured).to_string();
    // Checked here as well as on save: a config file edited by hand, or written
    // by an older build without the check, would otherwise open a browser tab
    // that can only fail.
    vavis_tools::spotify::auth::check_client_id(&client_id)?;

    let pkce = vavis_tools::spotify::auth::Pkce::generate();
    let url = vavis_tools::spotify::auth::authorize_url(&client_id, &pkce);

    // Bind the listener before opening the browser, so a very fast redirect
    // cannot arrive before anything is listening.
    let expected_state = pkce.state.clone();
    let verifier = pkce.verifier.clone();

    std::thread::spawn(move || {
        let outcome = match vavis_tools::spotify::auth::wait_for_callback(&expected_state) {
            Ok(vavis_tools::spotify::auth::Callback::Code(code)) => {
                vavis_tools::spotify::exchange_code(&client_id, &code, &verifier)
                    .map_err(|e| e.to_string())
            }
            Ok(vavis_tools::spotify::auth::Callback::Denied(reason)) => Err(reason),
            Err(e) => Err(e),
        };

        let payload = match outcome {
            Ok(token) => {
                // Persisting is the shell's job; the tool layer only holds it
                // in memory.
                if let Some(state) = app.try_state::<AppState>() {
                    state.save_spotify_token(&token);
                }
                serde_json::json!({ "ok": true, "message": "Spotify connected." })
            }
            Err(message) => serde_json::json!({ "ok": false, "message": message }),
        };
        let _ = app.emit("spotify:auth", payload);
    });

    open_in_browser(&url)
}

/// Disconnects Spotify by forgetting the stored token.
#[tauri::command]
pub fn disconnect_spotify(state: State<AppState>) -> Result<(), String> {
    {
        let core = AppState::lock(&state.core);
        let mut keys = AppState::lock(&state.keys);
        keys.remove("spotify_token");
        keys.save(core.paths.root()).map_err(|e| e.to_string())?;
    }
    state.refresh_spotify();
    Ok(())
}
