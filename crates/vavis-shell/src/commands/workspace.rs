//! The code screen's folder: tree, read, write, search.

use super::*;

// ---------------------------------------------------------------------------
// Code interface — workspace
// ---------------------------------------------------------------------------

/// Opens a folder in the code view.
#[tauri::command]
pub fn open_workspace(path: String) -> Result<String, String> {
    let path = std::path::PathBuf::from(path.trim());
    if !path.is_dir() {
        return Err(format!("no folder at {}", path.display()));
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());

    vavis_tools::workspace::set_root(Some(path));
    Ok(name)
}

/// The folder currently open, if any.
#[tauri::command]
pub fn current_workspace() -> Option<String> {
    vavis_tools::workspace::current_root().map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
pub fn list_workspace(path: String) -> Result<Vec<vavis_tools::workspace::Entry>, String> {
    vavis_tools::workspace::list(&path)
}

#[tauri::command]
pub fn read_workspace_file(path: String) -> Result<String, String> {
    vavis_tools::workspace::read(&path)
}

#[tauri::command]
pub fn write_workspace_file(path: String, content: String) -> Result<(), String> {
    vavis_tools::workspace::write(&path, &content)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub path: String,
    pub line: usize,
    pub text: String,
}

#[tauri::command]
pub fn search_workspace(query: String) -> Result<Vec<SearchHit>, String> {
    Ok(vavis_tools::workspace::grep(&query, 100)?
        .into_iter()
        .map(|(path, line, text)| SearchHit { path, line, text })
        .collect())
}
