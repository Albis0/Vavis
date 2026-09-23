//! User-configured MCP servers.

use super::*;

// ---------------------------------------------------------------------------
// MCP servers
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerInfo {
    pub id: String,
    pub transport: String,
    /// Exactly what would be run, so the user can see it before allowing it.
    pub command_line: String,
    pub enabled: bool,
    pub connected: bool,
    /// Every tool the server publishes.
    pub tools: Vec<String>,
    /// The subset the user switched off.
    pub disabled: Vec<String>,
    pub error: Option<String>,
    pub has_secret: bool,
}

/// Configured servers and their current state.
#[tauri::command(async)]
pub fn list_mcp_servers(state: State<AppState>) -> Vec<McpServerInfo> {
    let connected = vavis_tools::mcp::connected_ids();
    let core = AppState::lock(&state.core);
    let keys = AppState::lock(&state.keys);

    core.config
        .mcp
        .servers
        .iter()
        .map(|s| McpServerInfo {
            id: s.id.clone(),
            transport: s.transport.clone(),
            command_line: if s.transport.eq_ignore_ascii_case("http") {
                s.url.clone()
            } else {
                format!("{} {}", s.command, s.args.join(" "))
                    .trim()
                    .to_string()
            },
            enabled: s.enabled,
            connected: connected.contains(&s.id),
            // Tool names are only known while connected; the registry has them.
            tools: {
                let agent = AppState::lock(&state.agent);
                let prefix = format!("{}_", s.id);
                agent
                    .registry
                    .iter()
                    .filter_map(|t| t.name().strip_prefix(&prefix).map(str::to_string))
                    .collect()
            },
            disabled: s.disabled.clone(),
            error: None,
            has_secret: keys.get(&format!("mcp_{}", s.id)).is_some(),
        })
        .collect()
}

/// Adds or replaces a server, then reconnects everything.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn save_mcp_server(
    state: State<AppState>,
    id: String,
    transport: String,
    command: String,
    args: String,
    url: String,
    header_name: String,
    header_value: String,
    secret: String,
) -> Result<String, String> {
    let id = id.trim().to_string();
    // The id prefixes every tool name and names the selection domain, so it
    // has to be a plain identifier.
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("The id has to be letters, digits and dashes.".into());
    }
    if !["stdio", "http"].contains(&transport.as_str()) {
        return Err("The transport has to be stdio or http.".into());
    }
    if transport == "stdio" && command.trim().is_empty() {
        return Err("stdio needs a command to run.".into());
    }
    if transport == "http" && !url.starts_with("http") {
        return Err("http needs an address.".into());
    }

    {
        let mut core = AppState::lock(&state.core);
        let entry = vavis_core::McpServer {
            id: id.clone(),
            transport,
            command: command.trim().to_string(),
            // Split on whitespace: enough for the `npx -y pkg` shape that
            // nearly every server uses.
            args: args.split_whitespace().map(str::to_string).collect(),
            env: Vec::new(),
            url: url.trim().to_string(),
            header_name: header_name.trim().to_string(),
            header_value: header_value.trim().to_string(),
            // Replacing an existing entry keeps whatever the user switched off.
            disabled: core
                .config
                .mcp
                .servers
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.disabled.clone())
                .unwrap_or_default(),
            enabled: true,
        };

        match core.config.mcp.servers.iter_mut().find(|s| s.id == id) {
            Some(existing) => *existing = entry,
            None => core.config.mcp.servers.push(entry),
        }
        core.config.save(&core.paths).map_err(|e| e.to_string())?;

        if !secret.trim().is_empty() {
            let mut keys = AppState::lock(&state.keys);
            keys.set(format!("mcp_{id}"), secret.trim());
            keys.save(core.paths.root()).map_err(|e| e.to_string())?;
        }
    }

    let statuses = state.reload_mcp();
    Ok(match statuses.iter().find(|s| s.id == id) {
        Some(status) if status.connected => {
            format!("{} connected — {} tools.", id, status.tools.len())
        }
        Some(status) => format!(
            "Saved, but could not connect: {}",
            status.error.clone().unwrap_or_default()
        ),
        None => "Saved.".to_string(),
    })
}

/// Removes a server and its stored secret.
#[tauri::command]
pub fn remove_mcp_server(state: State<AppState>, id: String) -> Result<(), String> {
    {
        let mut core = AppState::lock(&state.core);
        core.config.mcp.servers.retain(|s| s.id != id);
        core.config.save(&core.paths).map_err(|e| e.to_string())?;

        let mut keys = AppState::lock(&state.keys);
        keys.remove(&format!("mcp_{id}"));
        keys.save(core.paths.root()).map_err(|e| e.to_string())?;
    }
    state.reload_mcp();
    Ok(())
}

/// Enables or disables a whole server.
#[tauri::command]
pub fn toggle_mcp_server(state: State<AppState>, id: String, enabled: bool) -> Result<(), String> {
    {
        let mut core = AppState::lock(&state.core);
        let Some(server) = core.config.mcp.servers.iter_mut().find(|s| s.id == id) else {
            return Err(format!("no server called {id}"));
        };
        server.enabled = enabled;
        core.config.save(&core.paths).map_err(|e| e.to_string())?;
    }
    state.reload_mcp();
    Ok(())
}

/// Switches one tool on or off.
///
/// A server that brings fourteen tools when three are wanted would otherwise
/// spend the whole per-request budget.
#[tauri::command]
pub fn toggle_mcp_tool(
    state: State<AppState>,
    id: String,
    tool: String,
    enabled: bool,
) -> Result<(), String> {
    {
        let mut core = AppState::lock(&state.core);
        let Some(server) = core.config.mcp.servers.iter_mut().find(|s| s.id == id) else {
            return Err(format!("no server called {id}"));
        };
        if enabled {
            server.disabled.retain(|t| t != &tool);
        } else if !server.disabled.contains(&tool) {
            server.disabled.push(tool);
        }
        core.config.save(&core.paths).map_err(|e| e.to_string())?;
    }
    state.reload_mcp();
    Ok(())
}
