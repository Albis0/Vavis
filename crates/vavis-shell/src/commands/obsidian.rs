//! Obsidian vault selection.

use super::*;

// ---------------------------------------------------------------------------
// Obsidian vault
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultInfo {
    /// Absolute path on disk.
    pub path: String,
    /// Folder name — what the user calls the vault.
    pub name: String,
    pub active: bool,
}

/// Vaults Obsidian knows about, plus whichever one is active.
#[tauri::command]
pub fn list_vaults(state: State<AppState>) -> Vec<VaultInfo> {
    let active = vavis_tools::obsidian::current().map(|v| v.root);
    let configured = AppState::lock(&state.core).config.obsidian.vault.clone();

    let mut paths = vavis_tools::obsidian::discover();
    // A vault chosen by hand may not be in Obsidian's own list.
    let configured_path = std::path::PathBuf::from(configured.trim());
    if !configured.trim().is_empty() && !paths.contains(&configured_path) {
        paths.insert(0, configured_path);
    }

    paths
        .into_iter()
        .map(|path| VaultInfo {
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string()),
            active: active.as_ref() == Some(&path),
            path: path.to_string_lossy().to_string(),
        })
        .collect()
}

/// Switches the active vault. An empty path falls back to auto-detection.
#[tauri::command]
pub fn set_vault(state: State<AppState>, path: String) -> Result<(), String> {
    let trimmed = path.trim().to_string();
    if !trimmed.is_empty() && !std::path::Path::new(&trimmed).is_dir() {
        return Err(format!("no folder at {trimmed}"));
    }

    {
        let mut core = AppState::lock(&state.core);
        core.config.obsidian.vault = trimmed.clone();
        core.config.save(&core.paths).map_err(|e| e.to_string())?;
    }

    vavis_tools::obsidian::set_active(vavis_tools::obsidian::autoselect(&trimmed));
    Ok(())
}
