//! # vavis-core — ÇEKİRDEK katman
//!
//! Mimari kural: bu katman **hiçbir üst katmanı tanımaz.** Arayüz, ses, tool'lar
//! burayı kullanır; buradan onlara referans yoktur. Böylece arayüz değiştiğinde
//! (terminal görünümü → başka bir şey) çekirdeğe dokunulmaz.
//!
//! İçerik: ayarlar · dosya yolları · SQLite · loglama.

pub mod config;
pub mod conversations;
pub mod error;
pub mod gallery;
pub mod i18n;
pub mod logging;
pub mod memory;
pub mod paths;
pub mod scheduler;
pub mod search;
pub mod store;
pub mod version;

pub use config::{
    Canvas, Config, CustomCanvas, CustomSearch, Llm, Mcp, McpServer, Memory, Obsidian, Search,
    Spotify, Steam, WindowMode,
};
pub use conversations::Conversation;
pub use error::{CoreError, Result};
pub use gallery::{Item as GalleryItem, Kind as GalleryKind, NewItem as NewGalleryItem, Usage};
pub use i18n::{t, Key, Lang};
pub use paths::Paths;
pub use scheduler::{Automation, Trigger};
pub use search::{Document, Hit, SearchIndex};
pub use store::{Fact, Store, StoredMessage};

/// `Cargo.toml`'dan gelen sürüm — arayüzdeki sağlık ekranı bunu gösterir.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Açılışta bir kez kurulan uygulama durumu.
///
/// Sıra önemli: log → yollar → ayar → veritabanı. Böylece ayar veya veritabanı
/// hatası **loglanabilir** (eski projede log en son kuruluyordu, açılış hataları
/// hiçbir yere yazılmıyordu).
pub struct App {
    pub paths: Paths,
    pub config: Config,
    pub store: Store,
}

impl App {
    /// Standart açılış. Bozuk ayar açılışı engellemez (yedeklenir).
    pub fn boot(paths: Paths) -> Result<Self> {
        paths.ensure()?;
        let config = Config::load_or_default(&paths);
        let store = Store::open(&paths)?;

        tracing::info!(
            version = VERSION,
            data_dir = %paths.root().display(),
            "vavis açıldı"
        );

        Ok(Self {
            paths,
            config,
            store,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_creates_everything_from_scratch() {
        let tmp = tempfile::tempdir().unwrap();
        let app = App::boot(Paths::with_root(tmp.path().join("veri"))).unwrap();

        assert!(app.paths.root().exists());
        assert!(app.paths.database_file().exists());
        assert_eq!(app.config, Config::default());
        assert_eq!(app.store.schema_version().unwrap(), store::SCHEMA_VERSION);
    }

    #[test]
    fn boot_survives_corrupt_config() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::with_root(tmp.path());
        paths.ensure().unwrap();
        std::fs::write(paths.config_file(), "bozuk {{{").unwrap();

        // Açılış hata vermemeli — bu, S5'in (doğrulama boşluğu) ilk testi.
        let app = App::boot(paths).unwrap();
        assert_eq!(app.config, Config::default());
    }
}
