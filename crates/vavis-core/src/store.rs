//! SQLite depolama.
//!
//! F1'de sadece şema + sürümleme var; sohbet kaydı F2'de kullanılacak.
//! Sürümleme baştan konuldu çünkü sonradan eklemek acı verir: `schema_version`
//! tablosu ile her açılışta eksik göçler uygulanır.

use crate::error::Result;
use crate::paths::Paths;
use rusqlite::Connection;

/// Kodun beklediği şema sürümü. Yeni göç eklendikçe artar.
pub const SCHEMA_VERSION: i64 = 5;

/// Kalıcı bir olgu (kullanıcı hakkında hatırlanan bilgi).
#[derive(Debug, Clone, PartialEq)]
pub struct Fact {
    pub id: i64,
    pub text: String,
    pub created_at: i64,
    /// Where it came from: `user` when the user (or the model at their
    /// request) saved it, `auto` when it was picked out of a conversation
    /// in the background. Shown in settings, so an unwanted inference is
    /// easy to spot and remove.
    pub source: String,
}

/// Kayıtlı bir sohbet mesajı.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredMessage {
    pub id: i64,
    pub role: String,
    pub content: String,
    pub created_at: i64,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(paths: &Paths) -> Result<Self> {
        paths.ensure()?;
        let conn = Connection::open(paths.database_file())?;
        Self::init(conn)
    }

    /// Testler için: diskle uğraşmadan bellek içi veritabanı.
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        // WAL: yazma sırasında okuma bloklanmaz. Ses/UI aynı anda okuyacağı için önemli.
        // (in-memory veritabanı WAL desteklemez — hata yutulur, testte sorun değil.)
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        conn.pragma_update(None, "foreign_keys", "ON")?;

        let mut store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&mut self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);",
        )?;

        let current: Option<i64> = self
            .conn
            .query_row("SELECT version FROM schema_version LIMIT 1", [], |r| {
                r.get(0)
            })
            .ok();

        match current {
            Some(v) if v >= SCHEMA_VERSION => return Ok(()),
            Some(v) => tracing::info!(from = v, to = SCHEMA_VERSION, "şema yükseltiliyor"),
            None => tracing::info!(version = SCHEMA_VERSION, "şema oluşturuluyor"),
        }

        // Göçler biriktirilerek uygulanır: v1'den gelen veritabanı da,
        // sıfırdan kurulan da aynı sonuca varır.
        // v1 — sohbet kaydı.
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS messages (
                 id         INTEGER PRIMARY KEY AUTOINCREMENT,
                 role       TEXT    NOT NULL,
                 content    TEXT    NOT NULL,
                 created_at INTEGER NOT NULL DEFAULT (unixepoch())
             );
             CREATE INDEX IF NOT EXISTS idx_messages_created ON messages(created_at);",
        )?;

        // v2 — kalıcı hafıza (olgular).
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS facts (
                 id         INTEGER PRIMARY KEY AUTOINCREMENT,
                 text       TEXT    NOT NULL,
                 created_at INTEGER NOT NULL DEFAULT (unixepoch())
             );
             CREATE INDEX IF NOT EXISTS idx_facts_created ON facts(created_at);",
        )?;

        // v3 — otomasyonlar.
        self.migrate_automations()?;

        // v4 — generated-media index (files stay on disk).
        self.migrate_gallery()?;

        // v5 — conversations, and what semantic memory needs on facts.
        self.migrate_conversations()?;
        self.migrate_fact_memory()?;

        self.conn.execute("DELETE FROM schema_version", [])?;
        self.conn.execute(
            "INSERT INTO schema_version (version) VALUES (?1)",
            [SCHEMA_VERSION],
        )?;
        Ok(())
    }

    // ── Hafıza ──────────────────────────────────────────────────────────

    /// Yeni bir olgu kaydeder, id'sini döner.
    pub fn add_fact(&self, text: &str) -> Result<i64> {
        self.add_fact_from(text, "user")
    }

    /// As [`Self::add_fact`], naming where the fact came from.
    pub fn add_fact_from(&self, text: &str, source: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO facts (text, source) VALUES (?1, ?2)",
            [text, source],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn all_facts(&self) -> Result<Vec<Fact>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, text, created_at, source FROM facts ORDER BY id")?;
        let rows = stmt.query_map([], |r| {
            Ok(Fact {
                id: r.get(0)?,
                text: r.get(1)?,
                created_at: r.get(2)?,
                source: r.get(3)?,
            })
        })?;
        Ok(rows.filter_map(std::result::Result::ok).collect())
    }

    /// Olguyu siler. Silinen satır sayısı > 0 ise true.
    pub fn delete_fact(&self, id: i64) -> Result<bool> {
        let n = self.conn.execute("DELETE FROM facts WHERE id = ?1", [id])?;
        Ok(n > 0)
    }

    pub fn fact_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM facts", [], |r| r.get(0))?)
    }

    // ── Sohbet geçmişi ──────────────────────────────────────────────────

    pub fn add_message(&self, role: &str, content: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO messages (role, content) VALUES (?1, ?2)",
            [role, content],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Son N mesaj, **eskiden yeniye** sıralı (sohbet sırası).
    pub fn recent_messages(&self, limit: usize) -> Result<Vec<StoredMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, role, content, created_at FROM messages
             ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |r| {
            Ok(StoredMessage {
                id: r.get(0)?,
                role: r.get(1)?,
                content: r.get(2)?,
                created_at: r.get(3)?,
            })
        })?;

        let mut out: Vec<StoredMessage> = rows.filter_map(std::result::Result::ok).collect();
        out.reverse(); // sohbet sırasına çevir
        Ok(out)
    }

    /// Tüm sohbet geçmişini siler (olgular kalır).
    pub fn clear_messages(&self) -> Result<()> {
        self.conn.execute("DELETE FROM messages", [])?;
        Ok(())
    }

    /// Alt modüllerin (scheduler) kendi tablolarına erişmesi için.
    ///
    /// `pub(crate)`: dışarıya ham SQL erişimi açmıyoruz — depo API'si
    /// tek giriş noktası olarak kalsın.
    pub(crate) fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT version FROM schema_version LIMIT 1", [], |r| {
                r.get(0)
            })?)
    }

    /// Sağlık ekranı için mesaj sayısı.
    pub fn message_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_db_is_at_current_version() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
        assert_eq!(store.message_count().unwrap(), 0);
    }

    #[test]
    fn a_v4_database_gains_conversations_without_losing_messages() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::with_root(tmp.path());
        paths.ensure().unwrap();
        {
            // The shape a v4 build left behind.
            let conn = Connection::open(paths.database_file()).unwrap();
            conn.execute_batch(
                "CREATE TABLE schema_version (version INTEGER NOT NULL);
                 INSERT INTO schema_version VALUES (4);
                 CREATE TABLE messages (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     role TEXT NOT NULL, content TEXT NOT NULL,
                     created_at INTEGER NOT NULL DEFAULT (unixepoch()));
                 INSERT INTO messages (role, content) VALUES ('user', 'selam');
                 INSERT INTO messages (role, content) VALUES ('assistant', 'merhaba');
                 CREATE TABLE facts (
                     id INTEGER PRIMARY KEY AUTOINCREMENT, text TEXT NOT NULL,
                     created_at INTEGER NOT NULL DEFAULT (unixepoch()));
                 INSERT INTO facts (text) VALUES ('kahveyi sade içer');",
            )
            .unwrap();
        }

        let store = Store::open(&paths).unwrap();
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
        let convs = store.list_conversations(10).unwrap();
        assert_eq!(convs.len(), 1, "old messages land in one conversation");
        let msgs = store.messages_in(convs[0].id, 10).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].content, "selam");
        let facts = store.all_facts().unwrap();
        assert_eq!(facts[0].source, "user");
    }

    #[test]
    fn migrating_twice_changes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::with_root(tmp.path());
        {
            let store = Store::open(&paths).unwrap();
            let c = store.create_conversation("a").unwrap();
            store.add_message_to(c, "user", "x").unwrap();
        }
        {
            let store = Store::open(&paths).unwrap();
            store.migrate_conversations().unwrap();
            store.migrate_fact_memory().unwrap();
            assert_eq!(store.list_conversations(10).unwrap().len(), 1);
        }
    }

    #[test]
    fn reopening_existing_db_does_not_lose_data() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::with_root(tmp.path());

        {
            let store = Store::open(&paths).unwrap();
            store
                .conn
                .execute(
                    "INSERT INTO messages (role, content) VALUES ('user', 'selam')",
                    [],
                )
                .unwrap();
        }

        let store = Store::open(&paths).unwrap(); // ikinci açılış = göç tekrar çalışır
        assert_eq!(store.message_count().unwrap(), 1);
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
    }
}

#[cfg(test)]
mod store_v2_tests {
    use super::*;

    #[test]
    fn facts_round_trip() {
        let store = Store::open_in_memory().unwrap();
        let id = store.add_fact("kahveyi sade içerim").unwrap();

        let facts = store.all_facts().unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].id, id);
        assert_eq!(facts[0].text, "kahveyi sade içerim");
        assert!(facts[0].created_at > 0, "zaman damgası konmalı");
    }

    #[test]
    fn fact_ids_increment_and_do_not_repeat() {
        let store = Store::open_in_memory().unwrap();
        let a = store.add_fact("bir").unwrap();
        let b = store.add_fact("iki").unwrap();
        assert!(b > a);
    }

    #[test]
    fn deleting_a_fact_removes_only_that_one() {
        let store = Store::open_in_memory().unwrap();
        let a = store.add_fact("bir").unwrap();
        store.add_fact("iki").unwrap();

        assert!(store.delete_fact(a).unwrap());
        let remaining = store.all_facts().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].text, "iki");
    }

    #[test]
    fn deleting_a_missing_fact_reports_false() {
        let store = Store::open_in_memory().unwrap();
        assert!(!store.delete_fact(9999).unwrap());
    }

    #[test]
    fn messages_come_back_in_conversation_order() {
        let store = Store::open_in_memory().unwrap();
        store.add_message("user", "soru").unwrap();
        store.add_message("assistant", "cevap").unwrap();
        store.add_message("user", "ikinci soru").unwrap();

        let msgs = store.recent_messages(10).unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].content, "soru", "en eski başta olmalı");
        assert_eq!(msgs[2].content, "ikinci soru");
    }

    #[test]
    fn recent_messages_returns_the_newest_when_limited() {
        let store = Store::open_in_memory().unwrap();
        for i in 1..=10 {
            store.add_message("user", &format!("mesaj {i}")).unwrap();
        }

        let msgs = store.recent_messages(3).unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].content, "mesaj 8", "son 3 alınmalı");
        assert_eq!(msgs[2].content, "mesaj 10");
    }

    #[test]
    fn clearing_messages_keeps_facts() {
        let store = Store::open_in_memory().unwrap();
        store.add_fact("kalıcı bilgi").unwrap();
        store.add_message("user", "geçici").unwrap();

        store.clear_messages().unwrap();

        assert_eq!(store.message_count().unwrap(), 0);
        assert_eq!(store.fact_count().unwrap(), 1, "olgular silinmemeli");
    }

    #[test]
    fn v1_database_upgrades_to_v2_without_data_loss() {
        // Eski sürümden gelen veritabanı: sadece messages tablosu var.
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::with_root(tmp.path());
        paths.ensure().unwrap();

        {
            let conn = Connection::open(paths.database_file()).unwrap();
            conn.execute_batch(
                "CREATE TABLE schema_version (version INTEGER NOT NULL);
                 INSERT INTO schema_version VALUES (1);
                 CREATE TABLE messages (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     role TEXT NOT NULL,
                     content TEXT NOT NULL,
                     created_at INTEGER NOT NULL DEFAULT (unixepoch()));
                 INSERT INTO messages (role, content) VALUES ('user', 'eski mesaj');",
            )
            .unwrap();
        }

        let store = Store::open(&paths).unwrap();

        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
        assert_eq!(store.message_count().unwrap(), 1, "eski veri korunmalı");
        // Yeni tablo kullanılabilir olmalı.
        store.add_fact("yeni olgu").unwrap();
        assert_eq!(store.fact_count().unwrap(), 1);
    }
}
