//! Sohbet mesajı tipleri — OpenAI-uyumlu şema.
//!
//! Groq, OpenAI, Mistral, DeepSeek, xAI hepsi aynı gövdeyi kabul ediyor.
//! Gemini farklı ama OpenAI-uyumlu uç noktası var; onu kullanıyoruz.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    /// Tool cevabı için — hangi çağrıya ait olduğunu bağlar (F3'te kullanılacak).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Modelin istediği tool çağrıları (F3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Mesaja iliştirilen görüntü (base64 PNG).
    ///
    /// Serileştirilmez — sağlayıcıya gönderilirken `content` alanı
    /// çok parçalı biçime dönüştürülür (her sağlayıcının şekli farklı).
    #[serde(skip)]
    pub image: Option<String>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self::plain(Role::System, content)
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self::plain(Role::User, content)
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::plain(Role::Assistant, content)
    }

    fn plain(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
            image: None,
        }
    }

    /// Goruntu iliştirilmiş kullanıcı mesajı.
    ///
    /// `image_base64`: PNG verisinin base64 kodu (veri URI öneki OLMADAN).
    pub fn user_with_image(content: impl Into<String>, image_base64: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
            image: Some(image_base64.into()),
        }
    }

    /// Tool sonucunu modele geri veren mesaj (F3).
    pub fn tool_result(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            tool_call_id: Some(call_id.into()),
            tool_calls: None,
            image: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type", default = "default_tool_type")]
    pub kind: String,
    pub function: FunctionCall,

    /// Opaque state the provider handed us with this call, to be handed back
    /// with the result.
    ///
    /// Gemini calls it a thought signature. It arrives beside the
    /// `functionCall` in the response and **must** be echoed back in the same
    /// position, or the next request is refused outright:
    ///
    /// ```text
    /// 400  Function call is missing a thought_signature in functionCall
    ///      parts. This is required for tools to work correctly
    /// ```
    ///
    /// Measured against the live API on 2026-09-16: the same conversation
    /// fails without it and answers with it.
    ///
    /// Deliberately untyped and unread — we never look inside, only carry it.
    /// No other provider sends one, so it is absent almost everywhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_state: Option<String>,
}

fn default_tool_type() -> String {
    "function".to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    /// JSON string olarak argümanlar — sağlayıcılar böyle döner.
    pub arguments: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_serializes_without_null_fields() {
        let json = serde_json::to_string(&Message::user("selam")).unwrap();
        assert!(json.contains("\"role\":\"user\""));
        assert!(!json.contains("tool_call_id"), "boş alanlar gönderilmemeli");
    }

    #[test]
    fn tool_result_carries_call_id() {
        let m = Message::tool_result("call_1", "42");
        assert_eq!(m.role, Role::Tool);
        assert_eq!(m.tool_call_id.as_deref(), Some("call_1"));
    }
}
