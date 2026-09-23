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

/// Refuses to switch mid-reply: the reply would land in one conversation
/// while the screen showed another.
fn not_busy(state: &AppState) -> Result<(), String> {
    if state.busy.load(Ordering::SeqCst) {
        Err("wait for the reply to finish first".into())
    } else {
        Ok(())
    }
}

/// Starts a new conversation and makes it current. An empty current one is
/// reused rather than piling up blank entries in the list.
#[tauri::command]
pub fn new_conversation(state: State<AppState>) -> Result<i64, String> {
    not_busy(&state)?;
    let mut current = AppState::lock(&state.conversation);
    let store = AppState::lock(&state.store);
    let empty = store
        .messages_in(*current, 1)
        .map(|m| m.is_empty())
        .unwrap_or(false);
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
    not_busy(&state)?;
    let exists = AppState::lock(&state.store)
        .conversation_exists(id)
        .map_err(|e| e.to_string())?;
    if !exists {
        return Err("that conversation no longer exists".into());
    }
    *AppState::lock(&state.conversation) = id;
    Ok(chat::restore(&state, id))
}

#[tauri::command]
pub fn rename_conversation(state: State<AppState>, id: i64, title: String) -> Result<(), String> {
    AppState::lock(&state.store)
        .rename_conversation(id, &title)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Deletes a conversation. Deleting the current one moves to the most
/// recent other, or a fresh one; the new current id is returned so the
/// interface can reload.
#[tauri::command]
pub fn delete_conversation(state: State<AppState>, id: i64) -> Result<i64, String> {
    not_busy(&state)?;
    let mut current = AppState::lock(&state.conversation);
    let store = AppState::lock(&state.store);
    store.delete_conversation(id).map_err(|e| e.to_string())?;
    if *current == id {
        *current = match store.latest_conversation().map_err(|e| e.to_string())? {
            Some(next) => next,
            None => store.create_conversation("").map_err(|e| e.to_string())?,
        };
        drop(store);
        AppState::lock(&state.history).clear();
    }
    Ok(*current)
}
