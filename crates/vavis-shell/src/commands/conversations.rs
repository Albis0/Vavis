//! The conversation list: new chats, old chats, switching between them.

use super::*;

/// One conversation, as the list shows it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationView {
    pub id: i64,
    pub title: String,
    pub updated_at: i64,
    pub message_count: i64,
    pub current: bool,
}

/// Recent conversations, newest first; with `query`, those whose title or
/// messages contain it.
#[tauri::command]
pub fn list_conversations(
    state: State<AppState>,
    query: Option<String>,
) -> Result<Vec<ConversationView>, String> {
    let current = *AppState::lock(&state.conversation);
    let store = AppState::lock(&state.store);
    let list = match query.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        Some(q) => store.search_conversations(q, 100),
        None => store.list_conversations(100),
    }
    .map_err(|e| e.to_string())?;
    Ok(list
        .into_iter()
        .map(|c| ConversationView {
            current: c.id == current,
            id: c.id,
            title: c.title,
            updated_at: c.updated_at,
            message_count: c.message_count,
        })
        .collect())
}

pub(super) fn claim(state: &AppState) -> Result<crate::state::BusyGuard, String> {
    state
        .try_claim()
        .ok_or_else(|| "wait for the reply to finish first".into())
}

/// Starts a new conversation and makes it current. An empty current one is
/// reused rather than piling up blank entries in the list.
#[tauri::command]
pub fn new_conversation(state: State<AppState>) -> Result<i64, String> {
    let _guard = claim(&state)?;
    let mut current = AppState::lock(&state.conversation);
    let store = AppState::lock(&state.store);
    let empty = store
        .messages_in(*current, 1)
        .map_err(|e| e.to_string())?
        .is_empty();
    if !empty {
        *current = store.create_conversation("").map_err(|e| e.to_string())?;
    }
    drop(store);
    AppState::lock(&state.history).clear();
    Ok(*current)
}

/// Switches to a conversation and returns its messages for the screen.
#[tauri::command]
pub fn open_conversation(state: State<AppState>, id: i64) -> Result<Vec<chat::StoredLine>, String> {
    let _guard = claim(&state)?;
    let mut current = AppState::lock(&state.conversation);
    let store = AppState::lock(&state.store);
    let exists = store.conversation_exists(id).map_err(|e| e.to_string())?;
    if !exists {
        return Err("that conversation no longer exists".into());
    }
    let lines = chat::saved_lines(&store, id)?;
    *AppState::lock(&state.history) = chat::model_history(&lines);
    *current = id;
    Ok(lines)
}

#[tauri::command]
pub fn rename_conversation(state: State<AppState>, id: i64, title: String) -> Result<(), String> {
    AppState::lock(&state.store)
        .rename_conversation(id, &title)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Deletes a conversation. Deleting the current one moves to the most
/// recent other, or a fresh one, and returns its messages.
#[tauri::command]
pub fn delete_conversation(
    state: State<AppState>,
    id: i64,
) -> Result<Option<Vec<chat::StoredLine>>, String> {
    let _guard = claim(&state)?;
    let mut current = AppState::lock(&state.conversation);
    let store = AppState::lock(&state.store);
    if *current != id {
        store.delete_conversation(id).map_err(|e| e.to_string())?;
        return Ok(None);
    }
    let (next, messages) = store
        .delete_current_conversation(id, chat::RESTORE_LIMIT)
        .map_err(|e| e.to_string())?;
    let lines = chat::visible_lines(messages);
    *AppState::lock(&state.history) = chat::model_history(&lines);
    *current = next;
    Ok(Some(lines))
}
