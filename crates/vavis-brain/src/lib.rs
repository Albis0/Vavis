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
pub mod claude_code;
pub mod client;
pub mod embeddings;
pub mod gemini;
pub mod keys;
pub mod message;
pub mod provider;

pub use budget::{estimate_cost, estimate_tokens, fit_request, FitResult, ModelCaps};
pub use builtin::{tool_support, ToolSupport};
pub use client::{
    chat_url_from_user, system_prompt, system_prompt_for, BrainClient, BrainError, ChatConfig,
    ChatResponse, Result, StreamEvent,
};
pub use keys::KeyStore;
pub use message::{FunctionCall, Message, Role, ToolCall};
pub use provider::Provider;
