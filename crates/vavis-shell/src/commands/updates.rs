//! The release check.

/// Asks GitHub whether a newer release exists.
///
/// Nothing about the user goes out with this request: no key, no identifier,
/// no settings. It reads a public release list, unauthenticated.
#[tauri::command]
pub async fn check_update() -> vavis_core::version::UpdateCheck {
    crate::update::check().await
}

/// Opens the release page in the browser.
///
/// The address is the compiled-in constant, never a value that came back over
/// the network. A URL from a response is data, and handing data straight to
/// the shell's "open this" verb is how a compromised or spoofed release feed
/// would get to open anything it liked on the user's machine.
#[tauri::command]
pub fn open_release_page() -> Result<(), String> {
    let url = crate::update::RELEASES_PAGE;

    #[cfg(target_os = "windows")]
    // Through `explorer`, not `cmd /c start`: `start` runs its argument
    // through the command interpreter.
    let result = std::process::Command::new("explorer").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(url).spawn();

    // explorer.exe reports a non-zero exit code even on success, so only a
    // failure to spawn counts.
    result.map(|_| ()).map_err(|e| e.to_string())
}

/// The version this build reports.
#[tauri::command]
pub fn app_version() -> String {
    vavis_core::version::CURRENT.to_string()
}
