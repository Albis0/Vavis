//! # vavis-tools — ELLER katmanı
//!
//! Asistanın gerçek işler yapmasını sağlayan yetenekler.
//!
//! Mimari kural: bu katman `vavis-core` ve `vavis-brain`'i kullanır; arayüzü
//! tanımaz. Onay sorma işi `AgentHost` trait'i üzerinden dışarı devredilir —
//! böylece hem gerçek arayüz hem de test aynı mantığı çalıştırır.
//!
//! # Tasarımın özü
//!
//! Eski projede 353 tool vardı ve modele her istekte 64'ü sunuluyordu —
//! seçenek kalabalığı seçim kalitesini bitiriyordu.
//!
//! Buradaki cevap sayıyı kısmak değil, **isteğe göre daraltmak**: yetenek
//! sayısı serbestçe büyüyebilir, ama bir isteğe yalnızca o işe yarayanlar
//! sunulur. Kaç tanesinin sunulabileceği modele göre değişir
//! (`vavis_brain::ModelCaps::tool_budget`) — ayrıntı [`selection`] içinde.

pub mod agent;
pub mod blocking;
pub mod builtin;
pub mod canvas;
pub mod mcp;
pub mod obsidian;
pub mod permission;
pub mod router;
pub mod selection;
pub mod spotify;
pub mod steam;
pub mod tool;
pub mod untrusted;
pub mod virustotal;
pub mod websearch;

pub use agent::{Agent, AgentHost, Approval, MAX_STEPS};
pub use blocking::run_async;
pub use permission::{ApprovalReason, Decision, PermissionGate};
pub use router::{KeywordRouter, LlmRouter, Router, ToolBrief};
pub use selection::{match_domains, select_tools, DEFAULT_TOOL_BUDGET};
pub use tool::{Domain, Param, Registry, Risk, Tool, ToolOutcome};
pub use untrusted::{scan as scan_untrusted, wrap as wrap_untrusted, Untrusted};

/// Tüm yerleşik tool'larla dolu bir kayıt defteri.
pub fn default_registry() -> Registry {
    let mut reg = Registry::new();
    builtin::register_all(&mut reg);
    reg
}
