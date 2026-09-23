//! The web search chain: keys, order, and the custom endpoint.

use super::*;

// ---------------------------------------------------------------------------
// Web search chain
// ---------------------------------------------------------------------------

/// Search providers that take an API key.
///
/// `duckduckgo` is absent on purpose: it needs no key, which is what makes it
/// the fallback everyone gets.
const KEYED_SEARCH_PROVIDERS: [&str; 3] = ["tavily", "brave", "custom"];

/// Maps a provider id onto its name in the encrypted key store.
fn search_key_name(provider: &str) -> Option<&'static str> {
    match provider {
        "tavily" => Some("tavily"),
        "brave" => Some("brave"),
        "custom" => Some("search_custom"),
        _ => None,
    }
}

/// What the settings panel shows for the search chain.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchSettings {
    /// Provider ids in the order they are tried.
    pub order: Vec<String>,
    /// Ids that currently have a key stored — never the keys themselves.
    pub configured: Vec<String>,
    pub custom: CustomSearchInfo,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomSearchInfo {
    pub url: String,
    pub header_name: String,
    pub header_value: String,
    pub results_path: String,
    pub title_key: String,
    pub url_key: String,
    pub snippet_key: String,
}

#[tauri::command]
pub fn get_search_settings(state: State<AppState>) -> SearchSettings {
    let core = AppState::lock(&state.core);
    let keys = AppState::lock(&state.keys);
    let custom = &core.config.search.custom;

    SearchSettings {
        order: core.config.search.order.clone(),
        configured: KEYED_SEARCH_PROVIDERS
            .iter()
            .filter(|id| {
                // `custom` runs on its address alone -- its key only fills
                // `{key}` in an optional header. Judging it by the key said
                // "no key -- skipped" about a provider that was about to
                // answer, which is the image side's rule and the opposite of
                // what the search chain actually does:
                //
                //   fn is_usable(&self) -> bool {
                //       !self.url.trim().is_empty() && self.url.contains("{query}")
                //   }
                if **id == "custom" {
                    return custom.url.contains("{query}") && !custom.url.trim().is_empty();
                }
                search_key_name(id)
                    .and_then(|name| keys.get(name))
                    .is_some()
            })
            .map(|id| id.to_string())
            .collect(),
        custom: CustomSearchInfo {
            url: custom.url.clone(),
            header_name: custom.header_name.clone(),
            header_value: custom.header_value.clone(),
            results_path: custom.results_path.clone(),
            title_key: custom.title_key.clone(),
            url_key: custom.url_key.clone(),
            snippet_key: custom.snippet_key.clone(),
        },
    }
}

/// Stores a search provider key, encrypted. An empty value removes it.
#[tauri::command]
pub fn set_search_key(state: State<AppState>, provider: String, key: String) -> Result<(), String> {
    let Some(name) = search_key_name(&provider) else {
        return Err(format!("unknown search provider: {provider}"));
    };

    {
        // Lock core before keys, matching `refresh_search`. Taking these two
        // in opposite orders on different threads would deadlock.
        let core = AppState::lock(&state.core);
        let mut keys = AppState::lock(&state.keys);
        keys.set(name, key);
        keys.save(core.paths.root()).map_err(|e| e.to_string())?;
    }

    // The chain caches its keys, so it has to be told about the new one.
    state.refresh_search();
    Ok(())
}

/// Reorders the chain. Unknown ids are rejected rather than silently dropped,
/// so a typo surfaces here instead of as a provider that never runs.
#[tauri::command]
pub fn set_search_order(state: State<AppState>, order: Vec<String>) -> Result<(), String> {
    let known = ["tavily", "brave", "custom", "duckduckgo"];
    for id in &order {
        if !known.contains(&id.as_str()) {
            return Err(format!("unknown search provider: {id}"));
        }
    }
    if order.is_empty() {
        return Err("the chain needs at least one provider".into());
    }

    {
        let mut core = AppState::lock(&state.core);
        core.config.search.order = order;
        core.config.save(&core.paths).map_err(|e| e.to_string())?;
    }

    state.refresh_search();
    Ok(())
}

/// Saves the user-described JSON search endpoint.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn set_custom_search(
    state: State<AppState>,
    url: String,
    header_name: String,
    header_value: String,
    results_path: String,
    title_key: String,
    url_key: String,
    snippet_key: String,
) -> Result<(), String> {
    // Without the placeholder the query could not be substituted, so the
    // endpoint would silently return the same results for every search.
    if !url.trim().is_empty() && !url.contains("{query}") {
        return Err("the address must contain the {query} placeholder".into());
    }

    {
        let mut core = AppState::lock(&state.core);
        core.config.search.custom = vavis_core::CustomSearch {
            url: url.trim().to_string(),
            header_name: header_name.trim().to_string(),
            header_value: header_value.trim().to_string(),
            results_path: results_path.trim().to_string(),
            title_key: title_key.trim().to_string(),
            url_key: url_key.trim().to_string(),
            snippet_key: snippet_key.trim().to_string(),
        };
        core.config.save(&core.paths).map_err(|e| e.to_string())?;
    }

    state.refresh_search();
    Ok(())
}
