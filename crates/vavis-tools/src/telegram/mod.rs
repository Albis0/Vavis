//! Telegram: reaching Vavis from the phone.
//!
//! The user makes a bot with @BotFather, pastes its token into settings,
//! and pairs it by sending the bot a one-time code shown on the computer.
//! From then on, messages from that one Telegram account are turns like
//! any typed in the window, and the answer comes back as a reply.
//!
//! This module is the Bot API client and the pure parts of the protocol:
//! what an update says, what a pairing attempt looks like, how approval
//! buttons are encoded. The loop that runs turns lives in the shell.
//!
//! ## Why this is safe enough to ship
//!
//! A chat bot that runs commands on a PC is a remote shell with a friendly
//! face, so:
//!
//! - only the paired account is ever answered; anyone else who finds the
//!   bot gets nothing back, not even an error;
//! - pairing needs a code that is only visible on the computer, expires,
//!   and is single-use;
//! - every destructive tool asks on the phone, with buttons, **even with
//!   full authority on** -- full authority is a statement about the person
//!   at the keyboard, and a phone can be picked up by someone else;
//! - the token lives in the encrypted key store like every other key.

use serde_json::{json, Value};
use std::time::Duration;

const API: &str = "https://api.telegram.org";

/// How long one long-poll waits for news before returning empty.
pub const POLL_SECONDS: u64 = 25;

/// Longest message Telegram accepts, in characters.
pub const MAX_MESSAGE: usize = 4096;

/// One thing that happened, as far as Vavis cares.
#[derive(Debug, Clone, PartialEq)]
pub enum Update {
    /// A text message.
    Text {
        update_id: i64,
        user_id: i64,
        chat_id: i64,
        text: String,
        /// First name, or username, for showing who paired.
        name: String,
    },
    /// An inline button was pressed.
    Button {
        update_id: i64,
        user_id: i64,
        callback_id: String,
        data: String,
    },
    /// Anything else (a sticker, an edit, a join): only its id matters, so
    /// the offset moves past it.
    Other { update_id: i64 },
}

impl Update {
    pub fn id(&self) -> i64 {
        match self {
            Self::Text { update_id, .. }
            | Self::Button { update_id, .. }
            | Self::Other { update_id } => *update_id,
        }
    }
}

/// Parses a `getUpdates` result.
pub fn parse_updates(body: &Value) -> Vec<Update> {
    body["result"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|u| {
            let update_id = u["update_id"].as_i64()?;
            if let Some(msg) = u.get("message") {
                if let (Some(text), Some(user_id), Some(chat_id)) = (
                    msg["text"].as_str(),
                    msg["from"]["id"].as_i64(),
                    msg["chat"]["id"].as_i64(),
                ) {
                    let from = &msg["from"];
                    let name = from["first_name"]
                        .as_str()
                        .or_else(|| from["username"].as_str())
                        .unwrap_or_default()
                        .to_string();
                    return Some(Update::Text {
                        update_id,
                        user_id,
                        chat_id,
                        text: text.to_string(),
                        name,
                    });
                }
            }
            if let Some(cb) = u.get("callback_query") {
                if let (Some(user_id), Some(callback_id)) =
                    (cb["from"]["id"].as_i64(), cb["id"].as_str())
                {
                    return Some(Update::Button {
                        update_id,
                        user_id,
                        callback_id: callback_id.to_string(),
                        data: cb["data"].as_str().unwrap_or_default().to_string(),
                    });
                }
            }
            Some(Update::Other { update_id })
        })
        .collect()
}

/// The code in a `/pair 123456` message, if that is what it is.
pub fn pairing_code(text: &str) -> Option<&str> {
    let rest = text.trim().strip_prefix("/pair")?.trim();
    // `/start 123456` is what a deep link sends; accept it too.
    (!rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())).then_some(rest)
}

/// Same, for the deep-link form.
pub fn start_code(text: &str) -> Option<&str> {
    let rest = text.trim().strip_prefix("/start")?.trim();
    (!rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())).then_some(rest)
}

/// A fresh six-digit pairing code.
pub fn new_code() -> String {
    use rand::Rng;
    format!("{:06}", rand::rngs::OsRng.gen_range(0..1_000_000u32))
}

/// Approval buttons carry `ok:<id>`, `always:<id>` or `no:<id>`, where the id
/// ties the press to the question it answers -- a stale button from an old
/// question must not answer a new one.
pub fn approval_keyboard(id: u64) -> Value {
    json!({"inline_keyboard": [[
        {"text": "✅ İzin ver", "callback_data": format!("ok:{id}")},
        {"text": "♾️ Hep izin ver", "callback_data": format!("always:{id}")},
        {"text": "⛔ Reddet", "callback_data": format!("no:{id}")},
    ]]})
}

/// What a button press decided, for question `id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Always,
    Deny,
}

pub fn parse_decision(data: &str) -> Option<(Decision, u64)> {
    let (kind, id) = data.split_once(':')?;
    let id = id.parse().ok()?;
    let decision = match kind {
        "ok" => Decision::Allow,
        "always" => Decision::Always,
        "no" => Decision::Deny,
        _ => return None,
    };
    Some((decision, id))
}

/// Splits a long answer at paragraph, then line, then word boundaries so
/// every piece fits in one Telegram message.
pub fn split_message(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text.trim();
    while rest.chars().count() > MAX_MESSAGE {
        let cut_at: usize = rest
            .char_indices()
            .nth(MAX_MESSAGE)
            .map(|(i, _)| i)
            .unwrap_or(rest.len());
        let window = &rest[..cut_at];
        let cut = window
            .rfind("\n\n")
            .or_else(|| window.rfind('\n'))
            .or_else(|| window.rfind(' '))
            .filter(|i| *i > 0)
            .unwrap_or(cut_at);
        out.push(rest[..cut].trim().to_string());
        rest = rest[cut..].trim_start();
    }
    if !rest.is_empty() {
        out.push(rest.to_string());
    }
    out
}

/// The Bot API, for one token.
#[derive(Clone)]
pub struct Bot {
    token: String,
    http: reqwest::Client,
}

impl Bot {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(15))
                .timeout(Duration::from_secs(POLL_SECONDS + 15))
                .build()
                .expect("http client"),
        }
    }

    async fn call(&self, method: &str, body: Value) -> Result<Value, String> {
        let url = format!("{API}/bot{}/{method}", self.token);
        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            // reqwest puts the URL in its errors, and the URL holds the token.
            .map_err(|e| e.without_url().to_string())?;
        let v: Value = resp.json().await.map_err(|e| e.without_url().to_string())?;
        if v["ok"] == true {
            Ok(v)
        } else {
            Err(v["description"]
                .as_str()
                .unwrap_or("Telegram refused the request")
                .to_string())
        }
    }

    /// The bot's own username, which also proves the token works.
    pub async fn username(&self) -> Result<String, String> {
        let v = self.call("getMe", json!({})).await?;
        Ok(v["result"]["username"]
            .as_str()
            .unwrap_or_default()
            .to_string())
    }

    /// Waits up to [`POLL_SECONDS`] for updates after `offset`.
    pub async fn updates(&self, offset: i64) -> Result<Vec<Update>, String> {
        self.updates_within(offset, POLL_SECONDS).await
    }

    /// As [`Self::updates`], waiting at most `seconds` (0 answers at once).
    pub async fn updates_within(&self, offset: i64, seconds: u64) -> Result<Vec<Update>, String> {
        let v = self
            .call(
                "getUpdates",
                json!({
                    "offset": offset,
                    "timeout": seconds,
                    "allowed_updates": ["message", "callback_query"]
                }),
            )
            .await?;
        Ok(parse_updates(&v))
    }

    /// Sends text, split to fit. Plain text: model output is markdown-ish,
    /// and Telegram's own markdown rejects a message over one stray `_`.
    pub async fn send(&self, chat_id: i64, text: &str) -> Result<(), String> {
        for piece in split_message(text) {
            self.call("sendMessage", json!({"chat_id": chat_id, "text": piece}))
                .await?;
        }
        Ok(())
    }

    /// Sends a question with approval buttons; returns the message id.
    pub async fn ask(&self, chat_id: i64, text: &str, question: u64) -> Result<i64, String> {
        let v = self
            .call(
                "sendMessage",
                json!({
                    "chat_id": chat_id,
                    "text": text,
                    "reply_markup": approval_keyboard(question)
                }),
            )
            .await?;
        Ok(v["result"]["message_id"].as_i64().unwrap_or_default())
    }

    /// Replaces a question's text once answered and drops its buttons, so
    /// it cannot be pressed twice.
    pub async fn settle(&self, chat_id: i64, message_id: i64, text: &str) -> Result<(), String> {
        self.call(
            "editMessageText",
            json!({"chat_id": chat_id, "message_id": message_id, "text": text}),
        )
        .await
        .map(|_| ())
    }

    /// Acknowledges a button press (stops the spinner on the phone).
    pub async fn ack(&self, callback_id: &str, text: &str) -> Result<(), String> {
        self.call(
            "answerCallbackQuery",
            json!({"callback_query_id": callback_id, "text": text}),
        )
        .await
        .map(|_| ())
    }

    /// Shows "typing…" while a turn runs.
    pub async fn typing(&self, chat_id: i64) {
        let _ = self
            .call(
                "sendChatAction",
                json!({"chat_id": chat_id, "action": "typing"}),
            )
            .await;
    }
}

// ── Notifying the phone ─────────────────────────────────────────────────────

/// Where `send_to_phone` delivers: the bot token and the paired account.
/// Installed by the shell whenever either changes; empty means unpaired.
static TARGET: std::sync::Mutex<Option<(String, i64)>> = std::sync::Mutex::new(None);

pub fn set_target(target: Option<(String, i64)>) {
    *TARGET.lock().unwrap_or_else(|e| e.into_inner()) = target;
}

fn target() -> Option<(String, i64)> {
    TARGET.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Sends a message to the user's phone.
pub struct SendToPhone;

impl crate::tool::Tool for SendToPhone {
    fn name(&self) -> &'static str {
        "send_to_phone"
    }

    fn description(&self) -> &'static str {
        "Sends a message to the user's phone over Telegram. Use when they ask to be \
         told something on their phone, or for an automation's result when they \
         are away from the computer."
    }

    fn domain(&self) -> crate::tool::Domain {
        crate::tool::Domain::Automation
    }

    /// It only ever reaches the user themselves, but it does leave the
    /// machine.
    fn risk(&self) -> crate::tool::Risk {
        crate::tool::Risk::Moderate
    }

    fn params(&self) -> Vec<crate::tool::Param> {
        vec![crate::tool::Param::required("text", "The message")]
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["telefon", "telegram", "phone", "bildir", "haber ver"]
    }

    fn run(&self, args: &Value) -> crate::tool::ToolOutcome {
        use crate::tool::ToolOutcome;
        let Some(text) = crate::tool::arg_str(args, "text").filter(|t| !t.trim().is_empty()) else {
            return ToolOutcome::err("text gerekli");
        };
        let Some((token, chat)) = target() else {
            return ToolOutcome::err(
                "Telegram bağlı değil — Ayarlar > Telefon bölümünden bir bot eşleştir.",
            );
        };
        let bot = Bot::new(token);
        let text = text.to_string();
        match crate::run_async(async move { bot.send(chat, &text).await }) {
            Ok(Ok(())) => ToolOutcome::ok("telefona gönderildi"),
            Ok(Err(e)) | Err(e) => ToolOutcome::err(format!("gönderilemedi: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_and_button_presses_parse() {
        let body = json!({"ok": true, "result": [
            {"update_id": 10, "message": {"text": "saat kaç", "from": {"id": 7}, "chat": {"id": 70}}},
            {"update_id": 11, "callback_query": {"id": "cb", "from": {"id": 7}, "data": "ok:3"}},
            {"update_id": 12, "message": {"sticker": {}, "from": {"id": 7}, "chat": {"id": 70}}},
        ]});
        let u = parse_updates(&body);
        assert_eq!(
            u[0],
            Update::Text {
                update_id: 10,
                user_id: 7,
                chat_id: 70,
                text: "saat kaç".into(),
                name: String::new(),
            }
        );
        assert_eq!(
            u[1],
            Update::Button {
                update_id: 11,
                user_id: 7,
                callback_id: "cb".into(),
                data: "ok:3".into()
            }
        );
        assert_eq!(u[2], Update::Other { update_id: 12 });
        assert_eq!(u[2].id(), 12);
    }

    #[test]
    fn pairing_codes_are_recognised_and_nothing_else_is() {
        assert_eq!(pairing_code("/pair 123456"), Some("123456"));
        assert_eq!(pairing_code("  /pair   042042 "), Some("042042"));
        assert_eq!(pairing_code("/pair"), None);
        assert_eq!(pairing_code("/pair abc"), None);
        assert_eq!(pairing_code("pair 123456"), None);
        assert_eq!(start_code("/start 123456"), Some("123456"));
    }

    #[test]
    fn codes_are_six_digits() {
        for _ in 0..50 {
            let c = new_code();
            assert_eq!(c.len(), 6);
            assert!(c.chars().all(|ch| ch.is_ascii_digit()));
        }
    }

    #[test]
    fn button_data_round_trips() {
        let kb = approval_keyboard(9);
        let row = kb["inline_keyboard"][0].as_array().unwrap();
        let decisions: Vec<_> = row
            .iter()
            .map(|b| parse_decision(b["callback_data"].as_str().unwrap()).unwrap())
            .collect();
        assert_eq!(
            decisions,
            vec![
                (Decision::Allow, 9),
                (Decision::Always, 9),
                (Decision::Deny, 9)
            ]
        );
        assert_eq!(parse_decision("maybe:1"), None);
        assert_eq!(parse_decision("ok:x"), None);
    }

    #[test]
    fn long_answers_split_at_natural_places() {
        let para = "a".repeat(3000);
        let text = format!("{para}\n\n{para}");
        let parts = split_message(&text);
        assert_eq!(parts.len(), 2);
        assert!(parts.iter().all(|p| p.chars().count() <= MAX_MESSAGE));
        assert_eq!(parts[0], para);
    }

    #[test]
    fn a_wall_of_text_still_splits() {
        let text = "x".repeat(MAX_MESSAGE * 2 + 10);
        let parts = split_message(&text);
        assert_eq!(parts.len(), 3);
        assert_eq!(parts.iter().map(|p| p.len()).sum::<usize>(), text.len());
    }

    #[test]
    fn sending_to_an_unpaired_phone_says_how_to_pair() {
        use crate::tool::Tool;
        set_target(None);
        let out = SendToPhone.run(&json!({"text": "merhaba"}));
        assert!(!out.ok);
        assert!(out.content.contains("eşleştir"), "{}", out.content);
    }

    #[test]
    fn a_short_answer_is_one_message() {
        assert_eq!(split_message("  merhaba  "), vec!["merhaba"]);
        assert!(split_message("").is_empty());
    }
}
