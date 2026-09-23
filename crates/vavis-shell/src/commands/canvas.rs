//! Image and video generation, and the gallery behind it.

use super::*;

// ---------------------------------------------------------------------------
// Canvas interface — image and video generation
// ---------------------------------------------------------------------------

/// One row of the gallery, as the grid needs it.
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GalleryItem {
    pub id: i64,
    pub kind: String,
    /// Absolute path. The frontend turns it into an `asset:` URL rather than
    /// pulling megabytes of base64 through IPC for every tile.
    pub path: String,
    pub prompt: String,
    pub provider: String,
    pub model: String,
    pub params: String,
    /// Absent when the provider did not report one, which means this result
    /// cannot be reproduced exactly — the interface says so.
    pub seed: Option<i64>,
    pub width: i64,
    pub height: i64,
    pub bytes: i64,
    pub parent_id: Option<i64>,
    pub favourite: bool,
    pub created_at: i64,
}

fn to_gallery_item(item: &vavis_core::GalleryItem, media_dir: &std::path::Path) -> GalleryItem {
    GalleryItem {
        id: item.id,
        kind: item.kind.as_str().to_string(),
        path: media_dir.join(&item.path).to_string_lossy().to_string(),
        prompt: item.prompt.clone(),
        provider: item.provider.clone(),
        model: item.model.clone(),
        params: item.params.clone(),
        seed: item.seed,
        width: item.width,
        height: item.height,
        bytes: item.bytes,
        parent_id: item.parent_id,
        favourite: item.favourite,
        created_at: item.created_at,
    }
}

/// What the canvas needs to draw itself before anything is generated.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasSettings {
    pub image_order: Vec<String>,
    pub video_order: Vec<String>,
    pub image_model: String,
    pub video_model: String,
    pub size: String,
    pub count: u32,
    /// Which provider ids have a key — so the interface can say "add a key"
    /// instead of failing after the user has typed a prompt.
    pub configured: Vec<String>,
    pub can_image: bool,
    pub can_video: bool,
    pub can_upscale: bool,
    pub custom_url: String,
    pub custom_header_name: String,
    pub custom_header_value: String,
    pub custom_model: String,
    pub items: i64,
    pub bytes: i64,
}

#[tauri::command]
pub fn get_canvas_settings(state: State<AppState>) -> CanvasSettings {
    let core = AppState::lock(&state.core);
    let keys = AppState::lock(&state.keys);
    let canvas = &core.config.canvas;

    let mut configured = Vec::new();
    // The chat key doubles as the image key unless a separate one was set.
    if keys.get("canvas_openai").is_some() || keys.get("openai").is_some() {
        configured.push("openai".to_string());
    }
    for id in ["stability", "replicate"] {
        if keys.get(&format!("canvas_{id}")).is_some() {
            configured.push(id.to_string());
        }
    }
    if !canvas.custom.url.trim().is_empty() {
        configured.push("custom".to_string());
    }

    let usage = AppState::lock(&state.store)
        .gallery_usage()
        .unwrap_or_default();

    CanvasSettings {
        image_order: canvas.image_order.clone(),
        video_order: canvas.video_order.clone(),
        image_model: canvas.image_model.clone(),
        video_model: canvas.video_model.clone(),
        size: canvas.size.clone(),
        count: canvas.count,
        configured,
        can_image: vavis_tools::canvas::is_ready(vavis_tools::canvas::Kind::Image),
        can_video: vavis_tools::canvas::is_ready(vavis_tools::canvas::Kind::Video),
        can_upscale: vavis_tools::canvas::can_upscale(),
        custom_url: canvas.custom.url.clone(),
        custom_header_name: canvas.custom.header_name.clone(),
        custom_header_value: canvas.custom.header_value.clone(),
        custom_model: canvas.custom.model.clone(),
        items: usage.items,
        bytes: usage.bytes,
    }
}

/// Stores a generation key. Same rule as everywhere else: secrets go in the
/// encrypted store, never in the settings file.
#[tauri::command]
pub fn set_canvas_key(state: State<AppState>, provider: String, key: String) -> Result<(), String> {
    let name = format!("canvas_{}", provider.trim());
    {
        let core = AppState::lock(&state.core);
        let mut keys = AppState::lock(&state.keys);
        keys.set(name, key.trim().to_string());
        keys.save(core.paths.root()).map_err(|e| e.to_string())?;
    }
    // Locks released first: refresh takes core and keys in that order, and
    // holding them here would deadlock against it.
    state.refresh_canvas();
    Ok(())
}

/// Reorders a provider chain. `kind` is "image" or "video".
#[tauri::command]
pub fn set_canvas_order(
    state: State<AppState>,
    kind: String,
    order: Vec<String>,
) -> Result<(), String> {
    {
        let mut core = AppState::lock(&state.core);
        if kind == "video" {
            core.config.canvas.video_order = order;
        } else {
            core.config.canvas.image_order = order;
        }
        let paths = core.paths.clone();
        core.config.save(&paths).map_err(|e| e.to_string())?;
    }
    state.refresh_canvas();
    Ok(())
}

/// Saves the defaults and the custom endpoint.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn set_canvas_defaults(
    state: State<AppState>,
    image_model: String,
    video_model: String,
    size: String,
    count: u32,
    custom_url: String,
    custom_header_name: String,
    custom_header_value: String,
    custom_model: String,
) -> Result<(), String> {
    {
        let mut core = AppState::lock(&state.core);
        let canvas = &mut core.config.canvas;
        canvas.image_model = image_model.trim().to_string();
        canvas.video_model = video_model.trim().to_string();
        canvas.size = size.trim().to_string();
        canvas.count = count.clamp(1, 8);
        canvas.custom.url = custom_url.trim().to_string();
        canvas.custom.header_name = custom_header_name.trim().to_string();
        canvas.custom.header_value = custom_header_value.trim().to_string();
        canvas.custom.model = custom_model.trim().to_string();

        let paths = core.paths.clone();
        core.config.save(&paths).map_err(|e| e.to_string())?;
    }
    state.refresh_canvas();
    Ok(())
}

#[tauri::command]
pub fn list_gallery(state: State<AppState>, limit: Option<usize>) -> Vec<GalleryItem> {
    let media_dir = AppState::lock(&state.core).paths.media_dir();
    let store = AppState::lock(&state.store);
    store
        .gallery_items(limit.unwrap_or(200))
        .unwrap_or_default()
        .iter()
        .map(|item| to_gallery_item(item, &media_dir))
        .collect()
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CanvasDonePayload {
    items: Vec<GalleryItem>,
    provider: String,
    /// Providers that failed before the one that worked — a note, not an error.
    notes: Vec<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CanvasErrorPayload {
    message: String,
}

/// Starts a generation.
///
/// Returns as soon as the work is handed to a thread: an image takes seconds
/// and a video takes minutes, and neither should freeze the window. The result
/// arrives as a `canvas:done` or `canvas:error` event.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn canvas_generate(
    app: tauri::AppHandle,
    state: State<AppState>,
    prompt: String,
    kind: String,
    model: String,
    width: u32,
    height: u32,
    count: u32,
    seed: Option<i64>,
    negative: String,
    duration_secs: u32,
    from_id: Option<i64>,
    strength: f32,
    upscale: bool,
) -> Result<(), String> {
    use vavis_tools::canvas;

    let prompt = prompt.trim().to_string();
    // An enlargement has a source instead of a prompt, so only a fresh
    // generation needs one.
    if prompt.is_empty() && !upscale {
        return Err("nothing to draw — write a prompt first".into());
    }

    let kind = canvas::Kind::parse(&kind);
    if upscale {
        if from_id.is_none() {
            return Err("pick a result to enlarge".into());
        }
        if !canvas::can_upscale() {
            return Err("enlarging needs a Stability or Replicate key".into());
        }
    } else if !canvas::is_ready(kind) {
        return Err(format!(
            "no {} provider has a key yet — add one in settings",
            kind.as_str()
        ));
    }

    let media_dir = AppState::lock(&state.core).paths.media_dir();

    // Continuing from an existing result: its bytes become the starting image,
    // which is what makes "another like this" and "animate this" possible.
    let init = match from_id {
        Some(id) => {
            let store = AppState::lock(&state.store);
            let parent = store
                .gallery_item(id)
                .map_err(|e| e.to_string())?
                .ok_or("that result is no longer in the gallery")?;
            Some(canvas::storage::read(&media_dir, &parent.path).map_err(|e| e.to_string())?)
        }
        None => None,
    };

    let request = canvas::Request {
        prompt,
        kind,
        model: model.trim().to_string(),
        width: if width == 0 { 1024 } else { width },
        height: if height == 0 { 1024 } else { height },
        count: count.clamp(1, 8),
        seed,
        negative: negative.trim().to_string(),
        duration_secs: duration_secs.clamp(1, 60),
        init,
        strength: strength.clamp(0.0, 1.0),
        upscale,
    };

    let store = state.store.clone();

    std::thread::spawn(move || {
        let generated = match canvas::generate(request.clone()) {
            Ok(generated) => generated,
            Err(attempts) => {
                let message = if attempts.is_empty() {
                    "no provider was able to take this request".to_string()
                } else {
                    attempts
                        .iter()
                        .map(|a| format!("{}: {}", a.provider, a.error))
                        .collect::<Vec<_>>()
                        .join(" · ")
                };
                let _ = app.emit("canvas:error", CanvasErrorPayload { message });
                return;
            }
        };

        // Saved before it is announced: a result the interface can see but the
        // gallery does not have is a result that vanishes on the next restart.
        let mut saved = Vec::new();
        for asset in &generated.assets {
            let (name, bytes) = match canvas::storage::save(&media_dir, asset, request.kind) {
                Ok(result) => result,
                Err(e) => {
                    let _ = app.emit(
                        "canvas:error",
                        CanvasErrorPayload {
                            message: format!("generated, but could not be saved: {e}"),
                        },
                    );
                    return;
                }
            };

            let params = serde_json::json!({
                "size": request.size_label(),
                "aspect": request.aspect_ratio(),
                "count": request.count,
                "negative": request.negative,
                "duration": request.duration_secs,
                "strength": request.strength,
                "upscale": request.upscale,
            })
            .to_string();

            let row = vavis_core::NewGalleryItem {
                kind: Some(match request.kind {
                    canvas::Kind::Image => vavis_core::GalleryKind::Image,
                    canvas::Kind::Video => vavis_core::GalleryKind::Video,
                }),
                path: name,
                prompt: request.prompt.clone(),
                provider: generated.provider.clone(),
                model: generated.model.clone(),
                params,
                // The provider's seed, not the one that was asked for: they
                // differ whenever none was given, and only the real one
                // reproduces the result.
                seed: asset.seed.or(request.seed),
                width: i64::from(if asset.width > 0 {
                    asset.width
                } else {
                    request.width
                }),
                height: i64::from(if asset.height > 0 {
                    asset.height
                } else {
                    request.height
                }),
                bytes: bytes as i64,
                parent_id: from_id,
            };

            let inserted = {
                let store = AppState::lock(&store);
                store
                    .add_gallery_item(&row)
                    .and_then(|id| store.gallery_item(id))
            };

            match inserted {
                Ok(Some(item)) => saved.push(to_gallery_item(&item, &media_dir)),
                Ok(None) => {}
                Err(e) => tracing::warn!(%e, "generated file was not indexed"),
            }
        }

        let notes = generated
            .attempts
            .iter()
            .map(|a| format!("{}: {}", a.provider, a.error))
            .collect();

        let _ = app.emit(
            "canvas:done",
            CanvasDonePayload {
                items: saved,
                provider: generated.provider,
                notes,
            },
        );
    });

    Ok(())
}

/// Removes one result, file and row.
#[tauri::command]
pub fn delete_gallery_item(state: State<AppState>, id: i64) -> Result<(), String> {
    let media_dir = AppState::lock(&state.core).paths.media_dir();
    let removed = AppState::lock(&state.store)
        .delete_gallery_item(id)
        .map_err(|e| e.to_string())?;

    if let Some(name) = removed {
        // A file that will not delete (open in a viewer, say) is not worth
        // failing over: the row is gone, and the next sweep catches the file.
        if let Err(e) = vavis_tools::canvas::storage::remove(&media_dir, &name) {
            tracing::warn!(%e, "gallery file could not be deleted");
        }
    }
    Ok(())
}

#[tauri::command]
pub fn favourite_gallery_item(
    state: State<AppState>,
    id: i64,
    favourite: bool,
) -> Result<(), String> {
    AppState::lock(&state.store)
        .set_gallery_favourite(id, favourite)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Empties the gallery, optionally sparing what the user starred.
///
/// Also sweeps files no row points at — they accumulate from interrupted
/// generations and are otherwise invisible disk usage.
#[tauri::command]
pub fn clear_gallery(state: State<AppState>, keep_favourites: bool) -> Result<u64, String> {
    let media_dir = AppState::lock(&state.core).paths.media_dir();

    let (removed, kept) = {
        let store = AppState::lock(&state.store);
        let removed = store
            .clear_gallery(keep_favourites)
            .map_err(|e| e.to_string())?;
        let kept: Vec<String> = store
            .gallery_items(usize::MAX)
            .unwrap_or_default()
            .into_iter()
            .map(|i| i.path)
            .collect();
        (removed, kept)
    };

    let mut freed = 0u64;
    for name in removed
        .iter()
        .cloned()
        .chain(vavis_tools::canvas::storage::orphans(&media_dir, &kept))
    {
        if let Some(path) = vavis_tools::canvas::storage::resolve(&media_dir, &name) {
            freed += std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        }
        if let Err(e) = vavis_tools::canvas::storage::remove(&media_dir, &name) {
            tracing::warn!(%e, "gallery file could not be deleted");
        }
    }
    Ok(freed)
}

/// Opens the media folder in the system file manager.
///
/// The note asks that the user be able to see what this is costing them in
/// disk; a number in settings answers "how much", this answers "where".
#[tauri::command]
pub fn open_media_folder(state: State<AppState>) -> Result<(), String> {
    let dir = AppState::lock(&state.core).paths.media_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer").arg(&dir).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(&dir).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(&dir).spawn();

    // explorer.exe returns a non-zero exit code even when it worked, so only
    // a spawn failure counts as a failure here.
    result.map(|_| ()).map_err(|e| e.to_string())
}
