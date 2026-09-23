//! Conversations: more than one chat, each with its own history.
//!
//! Until v5 there was one conversation and "clear" was the only way to
//! start another, which threw the old one away. Now a new chat is a new
//! row here, the old ones stay, and the interface can list and reopen them.
//!
//! Messages carry the conversation they belong to. Deleting a conversation
//! deletes its messages with it (the foreign key cascades); facts are not
//! tied to any conversation and survive everything.

use crate::error::Result;
use crate::store::{Store, StoredMessage};

/// One conversation, as the list shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Conversation {
    pub id: i64,
    /// Taken from the first thing the user said, until renamed. Empty for
    /// a conversation nothing has been said in yet.
    pub title: String,
    pub created_at: i64,
    /// Last message, or creation for an empty one. The list is sorted on
    /// this, so the chat you were just in is at the top.
    pub updated_at: i64,
    pub message_count: i64,
}

/// Longest automatic title, in characters. A title is for recognising a
/// conversation in a list, not for reading it.
const TITLE_MAX: usize = 60;

/// A title from the first user message: its first line, cut at a word.
pub fn title_from(text: &str) -> String {
    let line = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    if line.chars().count() <= TITLE_MAX {
        return line.to_string();
    }
    let cut: String = line.chars().take(TITLE_MAX).collect();
    let at_word = match cut.rfind(' ') {
        Some(i) if i > TITLE_MAX / 2 => &cut[..i],
        _ => cut.as_str(),
    };
    format!("{}…", at_word.trim_end())
}

impl Store {
    pub(crate) fn migrate_conversations(&self) -> Result<()> {
        let conn = self.connection();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS conversations (
                 id         INTEGER PRIMARY KEY AUTOINCREMENT,
                 title      TEXT    NOT NULL DEFAULT '',
                 created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                 updated_at INTEGER NOT NULL DEFAULT (unixepoch())
             );",
        )?;

        if !self.has_column("messages", "conversation_id")? {
            conn.execute_batch(
                "ALTER TABLE messages ADD COLUMN conversation_id INTEGER
                     REFERENCES conversations(id) ON DELETE CASCADE;",
            )?;
        }
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_messages_conversation
                 ON messages(conversation_id, id);",
        )?;

        // Messages from before conversations existed become one, titled
        // after its first question, so nothing already said disappears.
        let orphans: i64 = conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE conversation_id IS NULL",
            [],
            |r| r.get(0),
        )?;
        if orphans > 0 {
            let first: String = conn
                .query_row(
                    "SELECT content FROM messages
                     WHERE conversation_id IS NULL AND role = 'user'
                     ORDER BY id LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or_default();
            let (created, updated): (i64, i64) = conn.query_row(
                "SELECT MIN(created_at), MAX(created_at) FROM messages
                 WHERE conversation_id IS NULL",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            conn.execute(
                "INSERT INTO conversations (title, created_at, updated_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![title_from(&first), created, updated],
            )?;
            let id = conn.last_insert_rowid();
            conn.execute(
                "UPDATE messages SET conversation_id = ?1 WHERE conversation_id IS NULL",
                [id],
            )?;
        }
        Ok(())
    }

    /// Whether `table` has a column called `column`. SQLite has no
    /// `ADD COLUMN IF NOT EXISTS`, so migrations ask first.
    pub(crate) fn has_column(&self, table: &str, column: &str) -> Result<bool> {
        let mut stmt = self
            .connection()
            .prepare(&format!("PRAGMA table_info({table})"))?;
        let names = stmt.query_map([], |r| r.get::<_, String>(1))?;
        for name in names {
            if name? == column {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Starts a conversation and returns its id.
    pub fn create_conversation(&self, title: &str) -> Result<i64> {
        self.connection()
            .execute("INSERT INTO conversations (title) VALUES (?1)", [title])?;
        Ok(self.connection().last_insert_rowid())
    }

    /// Newest first.
    pub fn list_conversations(&self, limit: usize) -> Result<Vec<Conversation>> {
        let mut stmt = self.connection().prepare(
            "SELECT c.id, c.title, c.created_at, c.updated_at,
                    (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = c.id)
             FROM conversations c
             ORDER BY c.updated_at DESC, c.id DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |r| {
            Ok(Conversation {
                id: r.get(0)?,
                title: r.get(1)?,
                created_at: r.get(2)?,
                updated_at: r.get(3)?,
                message_count: r.get(4)?,
            })
        })?;
        Ok(rows.filter_map(std::result::Result::ok).collect())
    }

    /// The conversation last written to, if there is any.
    pub fn latest_conversation(&self) -> Result<Option<i64>> {
        Ok(self.list_conversations(1)?.first().map(|c| c.id))
    }

    pub fn conversation_exists(&self, id: i64) -> Result<bool> {
        let n: i64 = self.connection().query_row(
            "SELECT COUNT(*) FROM conversations WHERE id = ?1",
            [id],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    pub fn rename_conversation(&self, id: i64, title: &str) -> Result<bool> {
        let n = self.connection().execute(
            "UPDATE conversations SET title = ?1 WHERE id = ?2",
            rusqlite::params![title.trim(), id],
        )?;
        Ok(n > 0)
    }

    /// Deletes a conversation and every message in it.
    pub fn delete_conversation(&self, id: i64) -> Result<bool> {
        // Explicit rather than trusting the cascade alone: foreign keys are
        // a per-connection setting, and a database opened elsewhere without
        // it would leave the messages behind.
        self.connection()
            .execute("DELETE FROM messages WHERE conversation_id = ?1", [id])?;
        let n = self
            .connection()
            .execute("DELETE FROM conversations WHERE id = ?1", [id])?;
        Ok(n > 0)
    }

    /// Adds a message to a conversation and moves it to the top of the
    /// list. The first user message also names it, if it has no name yet.
    pub fn add_message_to(&self, conversation: i64, role: &str, content: &str) -> Result<i64> {
        let conn = self.connection();
        conn.execute(
            "INSERT INTO messages (role, content, conversation_id) VALUES (?1, ?2, ?3)",
            rusqlite::params![role, content, conversation],
        )?;
        let id = conn.last_insert_rowid();
        conn.execute(
            "UPDATE conversations SET updated_at = unixepoch() WHERE id = ?1",
            [conversation],
        )?;
        if role == "user" {
            conn.execute(
                "UPDATE conversations SET title = ?1 WHERE id = ?2 AND title = ''",
                rusqlite::params![title_from(content), conversation],
            )?;
        }
        Ok(id)
    }

    /// The last `limit` messages of a conversation, oldest first.
    pub fn messages_in(&self, conversation: i64, limit: usize) -> Result<Vec<StoredMessage>> {
        let mut stmt = self.connection().prepare(
            "SELECT id, role, content, created_at FROM messages
             WHERE conversation_id = ?1
             ORDER BY id DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![conversation, limit as i64], |r| {
            Ok(StoredMessage {
                id: r.get(0)?,
                role: r.get(1)?,
                content: r.get(2)?,
                created_at: r.get(3)?,
            })
        })?;
        let mut out: Vec<StoredMessage> = rows.filter_map(std::result::Result::ok).collect();
        out.reverse();
        Ok(out)
    }

    /// Empties a conversation but keeps it in the list.
    pub fn clear_conversation(&self, conversation: i64) -> Result<()> {
        self.connection().execute(
            "DELETE FROM messages WHERE conversation_id = ?1",
            [conversation],
        )?;
        self.connection().execute(
            "UPDATE conversations SET title = '' WHERE id = ?1",
            [conversation],
        )?;
        Ok(())
    }

    /// Conversations whose title or messages contain `query`, newest first.
    pub fn search_conversations(&self, query: &str, limit: usize) -> Result<Vec<Conversation>> {
        let like = format!("%{}%", query.trim().replace('%', "\\%").replace('_', "\\_"));
        let mut stmt = self.connection().prepare(
            "SELECT c.id, c.title, c.created_at, c.updated_at,
                    (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = c.id)
             FROM conversations c
             WHERE c.title LIKE ?1 ESCAPE '\\'
                OR EXISTS (SELECT 1 FROM messages m
                           WHERE m.conversation_id = c.id AND m.content LIKE ?1 ESCAPE '\\')
             ORDER BY c.updated_at DESC, c.id DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![like, limit as i64], |r| {
            Ok(Conversation {
                id: r.get(0)?,
                title: r.get(1)?,
                created_at: r.get(2)?,
                updated_at: r.get(3)?,
                message_count: r.get(4)?,
            })
        })?;
        Ok(rows.filter_map(std::result::Result::ok).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_is_the_first_line_cut_at_a_word() {
        assert_eq!(title_from("merhaba"), "merhaba");
        assert_eq!(title_from("\n\n  ilk satır\nikinci"), "ilk satır");
        let long = "bu çok uzun bir soru ve başlık olarak tamamen sığmaması gerekiyor elbette";
        let t = title_from(long);
        assert!(t.ends_with('…'));
        assert!(t.chars().count() <= TITLE_MAX + 1);
        assert!(!t.contains("elbette"));
    }

    #[test]
    fn conversations_keep_their_messages_apart() {
        let s = Store::open_in_memory().unwrap();
        let a = s.create_conversation("").unwrap();
        let b = s.create_conversation("").unwrap();
        s.add_message_to(a, "user", "a1").unwrap();
        s.add_message_to(b, "user", "b1").unwrap();
        s.add_message_to(a, "assistant", "a2").unwrap();

        let in_a: Vec<String> = s
            .messages_in(a, 10)
            .unwrap()
            .into_iter()
            .map(|m| m.content)
            .collect();
        assert_eq!(in_a, vec!["a1", "a2"]);
        assert_eq!(s.messages_in(b, 10).unwrap().len(), 1);
    }

    #[test]
    fn the_first_user_message_names_the_conversation() {
        let s = Store::open_in_memory().unwrap();
        let c = s.create_conversation("").unwrap();
        s.add_message_to(c, "user", "hava nasıl?").unwrap();
        s.add_message_to(c, "user", "peki yarın?").unwrap();
        assert_eq!(s.list_conversations(5).unwrap()[0].title, "hava nasıl?");
    }

    #[test]
    fn a_renamed_conversation_is_not_renamed_again() {
        let s = Store::open_in_memory().unwrap();
        let c = s.create_conversation("").unwrap();
        s.rename_conversation(c, "Tatil planı").unwrap();
        s.add_message_to(c, "user", "otel bak").unwrap();
        assert_eq!(s.list_conversations(5).unwrap()[0].title, "Tatil planı");
    }

    #[test]
    fn deleting_a_conversation_takes_its_messages() {
        let s = Store::open_in_memory().unwrap();
        let c = s.create_conversation("").unwrap();
        s.add_message_to(c, "user", "x").unwrap();
        assert!(s.delete_conversation(c).unwrap());
        assert_eq!(s.message_count().unwrap(), 0);
        assert!(s.list_conversations(5).unwrap().is_empty());
    }

    #[test]
    fn the_list_is_newest_first() {
        let s = Store::open_in_memory().unwrap();
        let a = s.create_conversation("a").unwrap();
        let b = s.create_conversation("b").unwrap();
        // Same second, so the id breaks the tie: the later one first.
        assert_eq!(s.list_conversations(5).unwrap()[0].id, b);
        s.connection()
            .execute(
                "UPDATE conversations SET updated_at = updated_at + 10 WHERE id = ?1",
                [a],
            )
            .unwrap();
        assert_eq!(s.list_conversations(5).unwrap()[0].id, a);
        assert_eq!(s.latest_conversation().unwrap(), Some(a));
    }

    #[test]
    fn clearing_keeps_the_conversation_but_empties_it() {
        let s = Store::open_in_memory().unwrap();
        let c = s.create_conversation("").unwrap();
        s.add_message_to(c, "user", "x").unwrap();
        s.clear_conversation(c).unwrap();
        assert!(s.conversation_exists(c).unwrap());
        assert!(s.messages_in(c, 5).unwrap().is_empty());
    }

    #[test]
    fn search_finds_by_content_and_escapes_wildcards() {
        let s = Store::open_in_memory().unwrap();
        let a = s.create_conversation("").unwrap();
        s.add_message_to(a, "user", "Rust ile %100 güvenli")
            .unwrap();
        let b = s.create_conversation("").unwrap();
        s.add_message_to(b, "user", "başka bir şey").unwrap();
        let hits = s.search_conversations("%100", 5).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, a);
        assert!(s.search_conversations("_", 5).unwrap().is_empty());
    }
}
