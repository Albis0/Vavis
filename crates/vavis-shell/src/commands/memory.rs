//! Remembered facts, automations, and the tool list.

use super::*;

/// A remembered fact.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FactView {
    pub id: i64,
    pub text: String,
}

#[tauri::command]
pub fn list_facts(state: State<AppState>) -> Vec<FactView> {
    AppState::lock(&state.store)
        .all_facts()
        .unwrap_or_default()
        .into_iter()
        .map(|f| FactView {
            id: f.id,
            text: f.text,
        })
        .collect()
}

#[tauri::command]
pub fn forget_fact(state: State<AppState>, id: i64) -> Result<bool, String> {
    AppState::lock(&state.store)
        .delete_fact(id)
        .map_err(|e| e.to_string())
}

/// A scheduled or conditional automation.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationView {
    pub id: i64,
    pub prompt: String,
    pub trigger: String,
    pub enabled: bool,
}

#[tauri::command]
pub fn list_automations(state: State<AppState>) -> Vec<AutomationView> {
    AppState::lock(&state.store)
        .all_automations()
        .unwrap_or_default()
        .into_iter()
        .map(|a| AutomationView {
            id: a.id,
            prompt: a.prompt,
            trigger: a.trigger.describe(),
            enabled: a.enabled,
        })
        .collect()
}

#[tauri::command]
pub fn delete_automation(state: State<AppState>, id: i64) -> Result<bool, String> {
    AppState::lock(&state.store)
        .delete_automation(id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn toggle_automation(state: State<AppState>, id: i64, enabled: bool) -> Result<bool, String> {
    AppState::lock(&state.store)
        .set_automation_enabled(id, enabled)
        .map_err(|e| e.to_string())
}

/// A registered tool, for the tools panel.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolView {
    pub name: String,
    pub description: String,
    pub domain: String,
    pub risk: String,
}

#[tauri::command]
pub fn list_tools(state: State<AppState>) -> Vec<ToolView> {
    AppState::lock(&state.agent)
        .registry
        .iter()
        .map(|t| ToolView {
            name: t.name().to_string(),
            description: t.description().to_string(),
            domain: t.domain().name().to_string(),
            risk: match t.risk() {
                vavis_tools::Risk::Safe => "safe",
                vavis_tools::Risk::Moderate => "moderate",
                vavis_tools::Risk::Destructive => "destructive",
            }
            .to_string(),
        })
        .collect()
}
