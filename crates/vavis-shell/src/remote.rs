//! The phone: a Telegram bot that is a second way into the same assistant.
//!
//! One background thread long-polls the Bot API. Messages from the paired
//! account become turns (`commands::chat::run_remote`), each on its own
//! thread so the poller keeps reading -- a turn waiting for an approval
//! needs the poller to deliver the button press that answers it.
//!
//! See `vavis_tools::telegram` for why this is locked down the way it is.

use crate::state::AppState;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::Manager;
use vavis_brain::Message;
use vavis_tools::telegram::{self, Bot, Decision, Update};
use vavis_tools::{Approval, ApprovalReason};

/// How long a pairing code stays valid.
const PAIRING_VALID: Duration = Duration::from_secs(10 * 60);

/// How long a turn waits for an approval from the phone before taking
/// silence as no.
const APPROVAL_WAIT: Duration = Duration::from_secs(5 * 60);

#[derive(Default)]
pub struct Remote {
    /// Bumped to stop the current poller (token changed, disabled).
    generation: AtomicU64,
    pairing: Mutex<Option<(String, Instant)>>,
    /// The phone conversation, kept apart from the window's.
    history: Arc<Mutex<Vec<Message>>>,
    /// Questions waiting for a button press, by id.
    pending: Mutex<HashMap<u64, Sender<Decision>>>,
    next_question: AtomicU64,
    bot_name: Mutex<String>,
    last_error: Mutex<Option<String>>,
}

pub fn remote() -> Arc<Remote> {
    static REMOTE: OnceLock<Arc<Remote>> = OnceLock::new();
    REMOTE.get_or_init(|| Arc::new(Remote::default())).clone()
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Remote {
    /// A fresh pairing code, valid for ten minutes and one use.
    pub fn new_pairing_code(&self) -> String {
        let code = telegram::new_code();
        *lock(&self.pairing) = Some((code.clone(), Instant::now() + PAIRING_VALID));
        code
    }

    pub fn pairing_code(&self) -> Option<String> {
        lock(&self.pairing)
            .as_ref()
            .filter(|(_, until)| Instant::now() < *until)
            .map(|(c, _)| c.clone())
    }

    /// Consumes the pairing code if `code` is it and it has not expired.
    fn take_pairing(&self, code: &str) -> bool {
        let mut slot = lock(&self.pairing);
        let ok = slot
            .as_ref()
            .is_some_and(|(c, until)| c == code && Instant::now() < *until);
        if ok {
            *slot = None;
        }
        ok
    }

    pub fn bot_name(&self) -> String {
        lock(&self.bot_name).clone()
    }

    pub fn last_error(&self) -> Option<String> {
        lock(&self.last_error).clone()
    }

    fn set_error(&self, e: Option<String>) {
        *lock(&self.last_error) = e;
    }
}

/// (Re)starts the poller with the current settings, or stops it when the
/// bot is disabled or has no token.
pub fn restart(app: &tauri::AppHandle) {
    let r = remote();
    let generation = r.generation.fetch_add(1, Ordering::SeqCst) + 1;

    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let (enabled, owner, token) = {
        let core = AppState::lock(&state.core);
        let keys = AppState::lock(&state.keys);
        (
            core.config.telegram.enabled,
            core.config.telegram.owner_id,
            keys.get("telegram").unwrap_or_default().to_string(),
        )
    };
    // What `send_to_phone` reaches.
    telegram::set_target(
        (enabled && owner != 0 && !token.is_empty()).then(|| (token.clone(), owner)),
    );
    if !enabled || token.is_empty() {
        r.set_error(None);
        return;
    }

    let app = app.clone();
    let _ = std::thread::Builder::new()
        .name("telegram".into())
        .spawn(move || poll(app, token, generation));
}

fn poll(app: tauri::AppHandle, token: String, generation: u64) {
    let r = remote();
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => return r.set_error(Some(e.to_string())),
    };
    let bot = Bot::new(token);

    match runtime.block_on(bot.username()) {
        Ok(name) => {
            *lock(&r.bot_name) = name;
            r.set_error(None);
        }
        Err(e) => return r.set_error(Some(e)),
    }

    // Skip whatever piled up while Vavis was closed. A command sent last
    // night must not run the moment the computer is switched on.
    let mut offset = runtime
        .block_on(bot.updates_within(-1, 0))
        .ok()
        .and_then(|u| u.iter().map(Update::id).max())
        .map_or(0, |id| id + 1);

    while r.generation.load(Ordering::SeqCst) == generation {
        let updates = match runtime.block_on(bot.updates(offset)) {
            Ok(u) => {
                r.set_error(None);
                u
            }
            Err(e) => {
                r.set_error(Some(e));
                std::thread::sleep(Duration::from_secs(5));
                continue;
            }
        };
        for update in updates {
            offset = offset.max(update.id() + 1);
            if r.generation.load(Ordering::SeqCst) != generation {
                return;
            }
            handle(&app, &bot, &runtime, update);
        }
    }
}

fn owner(app: &tauri::AppHandle) -> i64 {
    app.try_state::<AppState>()
        .map(|s| AppState::lock(&s.core).config.telegram.owner_id)
        .unwrap_or(0)
}

fn handle(app: &tauri::AppHandle, bot: &Bot, runtime: &tokio::runtime::Runtime, update: Update) {
    let r = remote();
    let owner_id = owner(app);
    match update {
        Update::Text {
            user_id,
            chat_id,
            text,
            name,
            ..
        } => {
            if owner_id == 0 || user_id != owner_id {
                // Only a pairing attempt gets an answer; everyone else gets
                // silence, which gives away nothing.
                let code = telegram::pairing_code(&text).or_else(|| telegram::start_code(&text));
                if let Some(code) = code {
                    if r.take_pairing(code) {
                        pair(app, user_id, &name);
                        let _ = runtime.block_on(bot.send(
                            chat_id,
                            "✅ Eşleşti. Artık Vavis'e buradan yazabilirsin.\n\n\
                             Yıkıcı işlemler (dosya yazma, komut, tıklama) her seferinde \
                             burada butonla onayına sunulur.",
                        ));
                    }
                }
                return;
            }
            match text.trim() {
                "/start" | "/help" | "/yardim" => {
                    let _ = runtime.block_on(bot.send(
                        chat_id,
                        "Vavis burada. Ne istersen yaz.\n\n/yeni — yeni konuşma başlat",
                    ));
                }
                "/new" | "/yeni" => {
                    lock(&r.history).clear();
                    let _ = runtime.block_on(bot.send(chat_id, "🆕 Yeni konuşma."));
                }
                _ => {
                    let app = app.clone();
                    let bot = bot.clone();
                    let _ = std::thread::Builder::new()
                        .name("telegram-turn".into())
                        .spawn(move || turn(app, bot, chat_id, text));
                }
            }
        }
        Update::Button {
            user_id,
            callback_id,
            data,
            ..
        } => {
            if owner_id == 0 || user_id != owner_id {
                return;
            }
            let answered = telegram::parse_decision(&data).and_then(|(decision, id)| {
                lock(&r.pending)
                    .remove(&id)
                    .map(|tx| tx.send(decision).is_ok())
            });
            let note = match answered {
                Some(true) => "tamam",
                _ => "bu soru artık geçerli değil",
            };
            let _ = runtime.block_on(bot.ack(&callback_id, note));
        }
        Update::Other { .. } => {}
    }
}

fn pair(app: &tauri::AppHandle, user_id: i64, name: &str) {
    if let Some(state) = app.try_state::<AppState>() {
        let mut core = AppState::lock(&state.core);
        core.config.telegram.owner_id = user_id;
        core.config.telegram.owner_name = name.to_string();
        let _ = core.config.save(&core.paths);
    }
    restart_target(app);
}

/// Refreshes what `send_to_phone` reaches without restarting the poller.
fn restart_target(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        let core = AppState::lock(&state.core);
        let keys = AppState::lock(&state.keys);
        let token = keys.get("telegram").unwrap_or_default().to_string();
        let owner = core.config.telegram.owner_id;
        telegram::set_target(
            (core.config.telegram.enabled && owner != 0 && !token.is_empty())
                .then_some((token, owner)),
        );
    }
}

/// Runs one phone message as a turn and replies with the answer.
fn turn(app: tauri::AppHandle, bot: Bot, chat_id: i64, text: String) {
    let r = remote();
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(_) => return,
    };
    runtime.block_on(bot.typing(chat_id));

    let asker_bot = bot.clone();
    let asker: crate::commands::chat::Asker =
        Arc::new(move |tool, args, reason| ask_on_phone(&asker_bot, chat_id, tool, args, reason));

    let history = r.history.clone();
    let reply = match crate::commands::chat::run_remote(&app, &history, &text, asker) {
        Ok(answer) if answer.trim().is_empty() => "(boş cevap)".to_string(),
        Ok(answer) => answer,
        Err(e) if e == "busy" => {
            "⏳ Vavis şu an bilgisayarda bir cevap yazıyor — birazdan tekrar dene.".to_string()
        }
        Err(e) => format!("⚠️ {e}"),
    };
    if let Err(e) = runtime.block_on(bot.send(chat_id, &reply)) {
        r.set_error(Some(e));
    }
}

/// Asks for an approval with buttons and waits for the press.
fn ask_on_phone(
    bot: &Bot,
    chat_id: i64,
    tool: &str,
    args: &str,
    reason: ApprovalReason,
) -> Approval {
    let r = remote();
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return Approval::Deny;
    };
    let id = r.next_question.fetch_add(1, Ordering::SeqCst) + 1;
    let (tx, rx) = std::sync::mpsc::channel();
    lock(&r.pending).insert(id, tx);

    let why = match reason {
        ApprovalReason::RiskLevel => "bu işlem bilgisayarda bir şey değiştiriyor",
        ApprovalReason::BudgetExceeded => "bu turda çok sayıda yıkıcı işlem oldu",
        ApprovalReason::TaintedContext => "okunan bir sayfa talimat vermeye çalıştı",
    };
    let shown_args: String = args.chars().take(600).collect();
    let question = format!("🔐 Onay gerekiyor — {why}\n\n{tool}\n{shown_args}");

    let message = runtime.block_on(bot.ask(chat_id, &question, id));
    let decision = rx.recv_timeout(APPROVAL_WAIT).ok();
    lock(&r.pending).remove(&id);

    let (approval, outcome) = match decision {
        Some(Decision::Allow) => (Approval::Allow, "✅ izin verildi"),
        Some(Decision::Always) => (Approval::AllowAlways, "♾️ bu oturumda hep izinli"),
        Some(Decision::Deny) => (Approval::Deny, "⛔ reddedildi"),
        None => (Approval::Deny, "⌛ cevap gelmedi — reddedildi"),
    };
    if let Ok(message_id) = message {
        let _ = runtime.block_on(bot.settle(chat_id, message_id, &format!("{outcome}: {tool}")));
    }
    approval
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pairing_code_works_once() {
        let r = Remote::default();
        let code = r.new_pairing_code();
        assert_eq!(r.pairing_code().as_deref(), Some(code.as_str()));
        assert!(!r.take_pairing("000000x"));
        assert!(r.take_pairing(&code));
        assert!(!r.take_pairing(&code), "single use");
        assert!(r.pairing_code().is_none());
    }

    #[test]
    fn an_expired_code_is_refused() {
        let r = Remote::default();
        *lock(&r.pairing) = Some(("123456".into(), Instant::now() - Duration::from_secs(1)));
        assert!(!r.take_pairing("123456"));
        assert!(r.pairing_code().is_none());
    }
}
