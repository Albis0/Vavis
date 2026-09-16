//! # vavis-brain — BEYİN katmanı
//!
//! LLM ile konuşur. Bağlam bütçesini yönetir. Anahtarları saklar.
//!
//! Mimari kural: bu katman arayüzü tanımaz — sonuçları kanal/geri çağırma ile
//! dışarı verir. Tool'ları da tanımaz (F3'te `vavis-tools` bu katmanı kullanacak,
//! tersi değil).

pub mod anthropic;
pub mod budget;
pub mod builtin;
pub mod client;
pub mod keys;
pub mod message;
pub mod provider;

pub use budget::{estimate_cost, estimate_tokens, fit_request, FitResult, ModelCaps};
pub use builtin::{tool_support, ToolSupport};
pub use client::{
    system_prompt, BrainClient, BrainError, ChatConfig, ChatResponse, Result, StreamEvent,
};
pub use keys::KeyStore;
pub use message::{FunctionCall, Message, Role, ToolCall};
pub use provider::Provider;
