//! Commands the interface can invoke.
//!
//! Each `#[tauri::command]` is one call the frontend can make. They are
//! deliberately thin: validate, delegate to a layer, return a plain type.
//! Business logic belongs in `vavis-brain` / `vavis-tools`, not here.
//!
//! Long-running work (a model request) emits events instead of blocking,
//! so the interface stays responsive and can stream tokens as they arrive.

use crate::state::AppState;
use serde::Serialize;
use std::sync::atomic::Ordering;
use tauri::{Emitter, Manager, State};
use vavis_brain::{system_prompt, ChatConfig, Message, Provider, StreamEvent};
use vavis_tools::{AgentHost, Approval, ApprovalReason, ToolOutcome, MAX_STEPS};

/// A snapshot of everything the interface shows in its side panels.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub version: String,
    pub assistant_name: String,
    pub language: String,
    pub provider: String,
    pub model: String,
    /// Cheap model that picks tools. Empty when routing is off.
    pub router_model: String,
    /// Full authority is on -- nothing will be asked. The interface shows a
    /// standing indicator for this, because a mode that removes every prompt
    /// must not itself be invisible.
    pub full_authority: bool,
    pub providers: Vec<ProviderInfo>,
    pub keys: Vec<String>,
    pub tool_count: usize,
    pub history_len: usize,
    pub fact_count: i64,
    pub automation_count: usize,
    pub message_count: i64,
    pub voice_mode: String,
    /// Microphone level, 0.0-1.0. Drives the meter next to the mic button.
    pub mic_level: f32,
    pub busy: bool,
    pub speaking: bool,
    pub cpu: Option<u32>,
    pub battery: Option<u32>,
    pub uptime_secs: u64,
    pub data_dir: String,
    pub window_mode: String,
    pub font_size: f32,
    /// Steam game running right now, detected locally. None when idle.
    pub steam_game: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: String,
    pub needs_key: bool,
    pub has_key: bool,
    pub default_model: String,
}

/// Everything the panels need, in one round trip.
///
/// One call rather than a dozen: the interface polls this on a timer, and
/// a dozen separate IPC calls per second would be wasteful.
#[tauri::command]
pub fn get_status(state: State<AppState>) -> Status {
    let core = AppState::lock(&state.core);
    let keys = AppState::lock(&state.keys);

    let provider = Provider::parse(&core.config.llm.provider).unwrap_or(Provider::Groq);
    let model = if core.config.llm.model.trim().is_empty() {
        provider.default_model().to_string()
    } else {
        core.config.llm.model.clone()
    };

    let (facts, automations, messages) = {
        let store = AppState::lock(&state.store);
        (
            store.fact_count().unwrap_or(0),
            store.all_automations().map(|a| a.len()).unwrap_or(0),
            store.message_count().unwrap_or(0),
        )
    };

    let voice = AppState::lock(&state.voice);

    Status {
        version: vavis_core::VERSION.to_string(),
        assistant_name: core.config.general.assistant_name.clone(),
        language: core.config.general.language.clone(),
        provider: provider.key_name().to_string(),
        model,
        router_model: core.config.llm.router_model.clone(),
        full_authority: core.config.security.full_authority,
        providers: Provider::ALL
            .iter()
            .map(|p| ProviderInfo {
                id: p.key_name().to_string(),
                needs_key: p.needs_key(),
                has_key: keys.get(p.key_name()).is_some(),
                default_model: p.default_model().to_string(),
            })
            .collect(),
        keys: keys.configured().iter().map(|s| s.to_string()).collect(),
        tool_count: AppState::lock(&state.agent).registry.len(),
        history_len: AppState::lock(&state.history).len(),
        fact_count: facts,
        automation_count: automations,
        message_count: messages,
        voice_mode: crate::voice::mode_name(voice.mode()).to_string(),
        mic_level: voice.mic_level(),
        busy: state.busy.load(Ordering::SeqCst),
        speaking: voice.is_speaking(),
        cpu: vavis_tools::builtin::system::cpu_percent(),
        battery: vavis_tools::builtin::system::battery_percent(),
        uptime_secs: state.started.elapsed().as_secs(),
        data_dir: core.paths.root().display().to_string(),
        window_mode: core.config.ui.window_mode.clone(),
        font_size: core.config.ui.font_size,
        steam_game: vavis_tools::steam::running_game_cached().map(|g| g.name),
    }
}

/// A stored message, for restoring the conversation on startup.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredLine {
    pub role: String,
    pub content: String,
}

/// Restores the tail of the previous conversation.
///
/// Only the last few turns: loading everything would fill the context
/// budget before the user typed a word.
#[tauri::command]
pub fn load_history(state: State<AppState>) -> Vec<StoredLine> {
    const RESTORE: usize = 20;

    let stored = AppState::lock(&state.store).recent_messages(RESTORE);
    let Ok(messages) = stored else {
        return Vec::new();
    };

    let mut history = AppState::lock(&state.history);

    // Replaced, not appended. This is called once on startup today, so
    // appending happens to work -- but a second call would hand the model
    // every restored turn twice, and nothing about the name suggests it can
    // only be called once.
    history.clear();

    let mut out = Vec::new();

    for m in messages {
        let role = match m.role.as_str() {
            "user" => vavis_brain::Role::User,
            "assistant" => vavis_brain::Role::Assistant,
            _ => continue, // tool and system messages are not replayed
        };

        history.push(Message {
            role,
            content: m.content.clone(),
            tool_call_id: None,
            tool_calls: None,
            image: None,
        });
        out.push(StoredLine {
            role: m.role,
            content: m.content,
        });
    }

    out
}

/// Sends a message to the model.
///
/// Returns immediately; progress arrives as events:
///
/// | Event | Payload |
/// |---|---|
/// | `chat:delta` | `{ text }` — a chunk of the reply |
/// | `chat:tool-start` | `{ tool, args }` |
/// | `chat:tool-done` | `{ tool, ok, summary, detail }` |
/// | `chat:approval` | `{ tool, args, reason }` |
/// | `chat:done` | `{ text }` |
/// | `chat:error` | `{ message }` |
#[tauri::command]
pub fn send_message(
    app: tauri::AppHandle,
    state: State<AppState>,
    text: String,
) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("empty message".into());
    }

    if !state.try_claim() {
        return Err("a reply is already in progress".into());
    }

    let (cfg, router_model, full_authority, system, history) = {
        let core = AppState::lock(&state.core);
        let keys = AppState::lock(&state.keys);

        let provider = Provider::parse(&core.config.llm.provider).unwrap_or(Provider::Groq);
        let key = keys
            .get(provider.key_name())
            .unwrap_or_default()
            .to_string();

        if provider.needs_key() && key.is_empty() {
            state.release();
            return Err(format!("no API key for {provider}"));
        }

        let model = if core.config.llm.model.trim().is_empty() {
            provider.default_model().to_string()
        } else {
            core.config.llm.model.clone()
        };

        // The system prompt is rebuilt each turn so a settings change
        // takes effect on the very next message.
        let system = Message::system(system_prompt(
            &core.config.general.assistant_name,
            &core.config.general.language,
        ));

        let mut history = AppState::lock(&state.history);
        history.push(Message::user(text.clone()));

        (
            ChatConfig::new(provider, model, key),
            core.config.llm.router_model.clone(),
            core.config.security.full_authority,
            system,
            history.clone(),
        )
    };

    // Persist the user's turn before the request goes out — if the app
    // dies mid-reply the question is still on record.
    if let Err(e) = AppState::lock(&state.store).add_message("user", &text) {
        tracing::warn!(%e, "could not persist message");
    }

    let client = state.client.clone();
    let agent = state.agent.clone();
    let busy = state.busy.clone();
    let approval_rx = state.approval_rx.clone();
    let voice = state.voice.clone();
    let store = state.store.clone();
    let history_handle = state.history.clone();

    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(r) => r,
            Err(e) => {
                let _ = app.emit(
                    "chat:error",
                    ErrorPayload {
                        message: e.to_string(),
                        // A runtime that will not start is not a size problem.
                        too_long: false,
                    },
                );
                busy.store(false, Ordering::SeqCst);
                return;
            }
        };

        let result = runtime.block_on(run_turn(
            &app,
            &client,
            &agent,
            &voice,
            &approval_rx,
            &cfg,
            &router_model,
            full_authority,
            system,
            history,
            &text,
        ));

        match result {
            Ok(reply) => {
                if !reply.trim().is_empty() {
                    AppState::lock(&history_handle).push(Message::assistant(reply.clone()));
                    if let Err(e) = AppState::lock(&store).add_message("assistant", &reply) {
                        tracing::warn!(%e, "could not persist reply");
                    }
                    // Most of the answer has already been spoken as it
                    // streamed; this is the last, unfinished sentence.
                    AppState::lock(&voice).finish_stream();
                }
                let _ = app.emit("chat:done", DonePayload { text: reply });
            }
            Err(TurnError { message, too_long }) => {
                // Drop the unanswered user turn: leaving it would produce
                // two user messages in a row on the next request.
                let mut h = AppState::lock(&history_handle);
                if h.last().map(|m| m.role) == Some(vavis_brain::Role::User) {
                    h.pop();
                }
                drop(h);

                let _ = app.emit("chat:error", ErrorPayload { message, too_long });
            }
        }

        busy.store(false, Ordering::SeqCst);
    });

    Ok(())
}

// Every payload below carries this, including the ones whose fields are all
// single words today and so would serialise identically without it. A missing
// rename is invisible until someone adds a two-word field, and then the
// interface reads undefined and silently renders nothing -- which is exactly
// how the "too long" recovery button came to never appear.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DeltaPayload {
    text: String,
}
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DonePayload {
    text: String,
}
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ErrorPayload {
    message: String,
    /// True when the request was refused for being too large.
    ///
    /// A flag rather than leaving the interface to match on the message text,
    /// which is translated and would break the offer to recover the moment
    /// anyone reworded it.
    too_long: bool,
}
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ToolStartPayload {
    tool: String,
    /// What it was called with, for the expanded view.
    args: String,
}

/// Something the turn wants to say about itself while still working.
///
/// Its own event rather than a delta: a delta is the model's answer, and
/// mixing "waiting 20s" into that text would leave it in the saved reply.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct NoticePayload {
    text: String,
}

/// How many times one turn will wait out a rate limit before giving up.
///
/// Two, because the free tiers refuse on a per-minute budget: one wait covers
/// the usual overspend from a multi-tool turn, and a third would mean the
/// limit is not the per-minute one and waiting is the wrong remedy.
const MAX_RATE_LIMIT_WAITS: u8 = 2;

/// The longest wait worth sitting through. Beyond this the limit is a daily
/// quota wearing a per-minute costume, and the user should be told rather
/// than left watching a spinner.
const MAX_RATE_LIMIT_WAIT_SECS: u64 = 60;

/// How much of a tool's output the collapsed line shows.
const MAX_TOOL_SUMMARY: usize = 120;

/// How much the expanded view shows. Generous, but not unbounded: a
/// directory listing of ten thousand files helps nobody, and the whole
/// output already went to the model.
const MAX_TOOL_DETAIL: usize = 4_000;

/// Collapses whitespace and clips, for a line that has to fit on one line.
///
/// Tool output is frequently multi-line; pasting a newline into the feed's
/// one-line note breaks the layout rather than informing anyone.
fn one_line(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    clip(&flat, max)
}

/// Clips to `max` characters, marking that something was cut.
fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ToolDonePayload {
    tool: String,
    ok: bool,
    /// One line, for the collapsed note in the feed.
    summary: String,
    /// The fuller output, shown when the note is opened.
    detail: String,
}
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ApprovalPayload {
    tool: String,
    args: String,
    reason: String,
}

/// A failed turn: what to show, and whether shortening would help.
struct TurnError {
    message: String,
    too_long: bool,
}

impl From<&vavis_brain::BrainError> for TurnError {
    fn from(err: &vavis_brain::BrainError) -> Self {
        Self {
            message: friendly_error(err),
            too_long: error_is_too_long(err),
        }
    }
}

impl From<String> for TurnError {
    /// Everything raised inside the turn itself rather than by the provider.
    /// None of it is a size problem, so it never offers to shorten.
    fn from(message: String) -> Self {
        Self {
            message,
            too_long: false,
        }
    }
}

/// Drops the older half of a request's history in place, returning how many
/// messages went.
///
/// The system message is index 0 and carries the assistant's identity, so it
/// stays. The last message is the question being asked, so it stays too --
/// dropping it would answer something the user never sent. Everything cut
/// comes from between them, oldest first.
///
/// Returns 0 when there is nothing safe to cut, which tells the caller that
/// retrying would send exactly the same request and fail exactly the same
/// way.
fn drop_oldest_half(messages: &mut Vec<Message>) -> usize {
    // system + at least two turns in between + the question.
    if messages.len() < 4 {
        return 0;
    }

    // The span that may be cut: everything after the system message and
    // before the question.
    let cuttable = messages.len() - 2;
    let mut drop = cuttable / 2;
    if drop == 0 {
        return 0;
    }

    // A tool result has to keep the assistant message that asked for it:
    // providers reject a request whose tool result answers a call they cannot
    // see ("tool not in request.tools"). Cutting in the middle of such a pair
    // is exactly what a blind halving does, so the cut is pushed forward
    // until the first surviving message is not an orphaned tool result.
    while drop < cuttable && messages[drop + 1].tool_call_id.is_some() {
        drop += 1;
    }

    // Pushing forward may have consumed the whole span. Nothing safe to cut.
    if drop >= cuttable {
        return 0;
    }

    messages.drain(1..=drop);
    drop
}

/// Which tools this request gets.
///
/// With `llm.router_model` set, a cheap model reads the request and the tool
/// catalogue and says what is needed; the expensive model then sees only
/// those schemas. Unset -- the default -- this is keyword matching, with no
/// extra call and no extra cost.
///
/// Every failure path lands on keywords. A router that is slow, broken or
/// talking nonsense must never be the reason the assistant cannot act.
async fn route_tools(
    client: &vavis_brain::BrainClient,
    cfg: &ChatConfig,
    router_model: &str,
    agent: &std::sync::Arc<std::sync::Mutex<vavis_tools::Agent>>,
    user_message: &str,
    budget: usize,
) -> Vec<String> {
    let keywords = || {
        let guard = AppState::lock(agent);
        vavis_tools::selection::select_named(&guard.registry, user_message, budget)
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>()
    };

    if router_model.trim().is_empty() {
        return keywords();
    }

    // A greeting needs no router call: keywords already answer "no tools",
    // and paying a model to confirm that on every "merhaba" is waste.
    if keywords().is_empty() {
        return Vec::new();
    }

    let (prompt, catalogue_len) = {
        let guard = AppState::lock(agent);
        let catalogue = vavis_tools::router::catalog(&guard.registry);
        (
            vavis_tools::router::prompt(user_message, &catalogue),
            catalogue.len(),
        )
    };

    // The router runs on the same provider and key as the conversation --
    // only the model differs, so there is nothing extra to set up.
    let router_cfg = ChatConfig {
        model: router_model.to_string(),
        // Picking tools is not a creative task.
        temperature: 0.0,
        ..cfg.clone()
    };

    let reply = client
        .chat_stream(
            &router_cfg,
            vec![vavis_brain::Message::user(&prompt)],
            |_| {},
        )
        .await;

    match reply {
        Ok(text) => {
            let guard = AppState::lock(agent);
            let mut picked: Vec<String> = vavis_tools::router::parse_reply(&text, &guard.registry)
                .into_iter()
                .map(str::to_string)
                .collect();
            drop(guard);

            if picked.is_empty() {
                tracing::debug!("router named nothing usable; using keywords");
                return keywords();
            }

            picked.truncate(budget);
            tracing::info!(
                picked = picked.len(),
                catalogue = catalogue_len,
                router = %router_cfg.model,
                "router chose the tools for this request"
            );
            picked
        }
        Err(e) => {
            tracing::warn!(error = %e, "router unavailable; using keywords");
            keywords()
        }
    }
}

/// One full turn: model → tools → model → … → reply.
#[allow(clippy::too_many_arguments)]
async fn run_turn(
    app: &tauri::AppHandle,
    client: &vavis_brain::BrainClient,
    agent: &std::sync::Arc<std::sync::Mutex<vavis_tools::Agent>>,
    // Speech starts on the first finished sentence rather than on the whole
    // answer, so the voice keeps up with the text instead of trailing it.
    voice: &std::sync::Arc<std::sync::Mutex<crate::voice::VoiceState>>,
    approval_rx: &std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<Approval>>>,
    cfg: &ChatConfig,
    router_model: &str,
    full_authority: bool,
    system: Message,
    history: Vec<Message>,
    user_message: &str,
) -> Result<String, TurnError> {
    // How many tools this model can choose between well -- a small model
    // loses its way among thirty schemas where a large one does not. A
    // ceiling, not a target: domain matching still decides what is relevant.
    let budget = vavis_brain::ModelCaps::for_model(&cfg.model).tool_budget;

    // A cheap model picks what is needed; the expensive one does the work.
    // Configured off, in which case this is keyword matching as before.
    let picked = route_tools(client, cfg, router_model, agent, user_message, budget).await;

    // Mutable because the model can ask for more mid-turn -- see the
    // `request_tools` handling further down. The starting set is what the router
    // (or the keyword table) chose from the message alone.
    // Some models bring their own web search, run on the provider's servers.
    // Offering ours alongside it opens two doors onto the same job, and the
    // model cannot tell which one the user can actually see the results of.
    // Theirs needs no key of ours, so it wins; ours is dropped for this turn.
    let picked: Vec<String> = if vavis_brain::builtin::covers_web_search(cfg.provider, &cfg.model) {
        picked.into_iter().filter(|n| n != "web_search").collect()
    } else {
        picked
    };

    let mut offered: Vec<String> = picked.clone();
    let mut tools = {
        let names: Vec<&str> = picked.iter().map(String::as_str).collect();
        let mut guard = AppState::lock(agent);
        guard.start_run();
        // Read fresh each turn, so switching it in settings takes effect on
        // the next message rather than the next launch.
        guard.gate.set_full_authority(full_authority);
        vavis_tools::builtin::request_tools::reset();
        guard.schemas_for(&names)
    };
    tracing::info!(
        count = tools.len(),
        budget,
        model = %cfg.model,
        "tools offered for this request"
    );

    let mut messages = vec![system];
    messages.extend(history);
    let mut final_text = String::new();
    // Whether history has already been cut back after a size refusal. Once
    // only, so a provider that refuses for some other reason it happens to
    // describe as "too long" cannot walk the conversation down to nothing.
    let mut shrunk = false;
    // How many times a rate limit has been waited out this turn. Separate from
    // `shrunk` because the two failures are unrelated and a turn can hit both.
    let mut rate_limit_waits = 0u8;

    // A fresh turn: drop any half-sentence left buffered by an abandoned one.
    AppState::lock(voice).begin_stream();

    for step in 0..MAX_STEPS {
        let emit = app.clone();

        let response = match client
            .chat_stream_with_tools(cfg, messages.clone(), &tools, {
                let emit = emit.clone();
                let voice = voice.clone();
                move |event| {
                    if let StreamEvent::Delta(text) = event {
                        // Speak first, then paint: synthesis has to start as
                        // early as possible, and emitting is the cheap half.
                        AppState::lock(&voice).push_stream(&text);
                        let _ = emit.emit("chat:delta", DeltaPayload { text });
                    }
                }
            })
            .await
        {
            Ok(response) => response,

            // The provider says the request is too big even though the budget
            // module thought it would fit. That happens because the window is
            // read from a table of model names, and the same name is served
            // with different limits by different providers -- so the number
            // can be wrong, and being wrong costs the user their turn.
            //
            // Rather than trust the table, shrink and ask again. Half the
            // conversation goes, oldest first, and the question itself is
            // never touched. One attempt only: if half was not enough, the
            // size is coming from something a second halving will not fix,
            // and the interface offers the same thing as a button by then.
            Err(e) if error_is_too_long(&e) && !shrunk => {
                shrunk = true;

                let kept = drop_oldest_half(&mut messages);
                if kept == 0 {
                    return Err(TurnError::from(&e));
                }

                tracing::info!(
                    dropped = kept,
                    "provider refused for size; retrying with less history"
                );
                continue;
            }

            // The free tiers refuse on a per-minute budget, and a turn that
            // calls two or three tools spends that budget in a few seconds.
            // The provider names the wait it wants; honouring it turns a dead
            // turn into a slow one.
            //
            // Bounded, and only when a wait was actually named: an unbounded
            // retry against a daily quota would hang the turn until the user
            // gave up, and the message they get instead ("try again in ...")
            // is at least true.
            Err(e) if rate_limit_waits < MAX_RATE_LIMIT_WAITS => {
                let Some(secs) = wait_before_retry(&e) else {
                    return Err(TurnError::from(&e));
                };

                rate_limit_waits += 1;
                tracing::info!(secs, attempt = rate_limit_waits, "rate limited; waiting");

                // Said out loud: several seconds of silence with no
                // explanation reads as the app having hung.
                let _ = app.emit(
                    "chat:notice",
                    NoticePayload {
                        text: format!("Rate limited — waiting {secs}s."),
                    },
                );

                tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
                continue;
            }

            Err(e) => return Err(TurnError::from(&e)),
        };

        if !response.text.is_empty() {
            final_text = response.text.clone();
        }

        if response.tool_calls.is_empty() {
            return Ok(final_text);
        }

        // Echo the model's tool request back, so the provider can match
        // results to calls on the next turn.
        messages.push(Message {
            role: vavis_brain::Role::Assistant,
            content: response.text.clone(),
            tool_call_id: None,
            tool_calls: Some(response.tool_calls.clone()),
            image: None,
        });

        let mut host = EventHost {
            app: app.clone(),
            approval_rx: approval_rx.clone(),
        };

        let results = {
            let mut guard = AppState::lock(agent);
            guard.execute_calls(&response.tool_calls, &mut host)
        };
        messages.extend(results);

        // A screenshot tool puts its image in a side channel — tool
        // results must be text, so it cannot ride along with them.
        if let Some(image) = vavis_tools::builtin::vision::take_pending_image() {
            messages.push(Message::user_with_image("(screenshot attached)", image));
        }

        // The model said it needs something it was not given. Neither the
        // keyword table nor the router can see this coming: both read the
        // user's message, and the need often only becomes clear once the
        // model is halfway through the job.
        //
        // The new tools are *added*, so nothing it is already holding
        // disappears mid-turn.
        for need in vavis_tools::builtin::request_tools::take() {
            let added = {
                let guard = AppState::lock(agent);
                let names = vavis_tools::selection::select_named(&guard.registry, &need, budget);
                // The same exclusion as at the top of the turn: a tool the
                // provider already runs server-side must not slip back in
                // through a mid-turn request either.
                let suppressed =
                    vavis_brain::builtin::covers_web_search(cfg.provider, &cfg.model);
                let fresh: Vec<&str> = names
                    .into_iter()
                    .filter(|n| !offered.iter().any(|had| had == n))
                    .filter(|n| !(suppressed && *n == "web_search"))
                    .collect();
                let schemas = guard.schemas_for(&fresh);
                let fresh: Vec<String> = fresh.into_iter().map(String::from).collect();
                (fresh, schemas)
            };

            let (names, schemas) = added;
            tracing::info!(need = %need, added = names.len(), "model asked for more tools");
            offered.extend(names);
            tools.extend(schemas);
        }

        if step == MAX_STEPS - 1 {
            return Err(format!("no answer after {MAX_STEPS} steps").into());
        }
    }

    Ok(final_text)
}

/// Bridges the agent to the interface: emits events, waits for approvals.
struct EventHost {
    app: tauri::AppHandle,
    approval_rx: std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<Approval>>>,
}

impl AgentHost for EventHost {
    fn ask_approval(&mut self, tool: &str, args: &str, reason: ApprovalReason) -> Approval {
        let _ = self.app.emit(
            "chat:approval",
            ApprovalPayload {
                tool: tool.to_string(),
                args: args.to_string(),
                reason: match reason {
                    ApprovalReason::RiskLevel => "risk".into(),
                    ApprovalReason::BudgetExceeded => "budget".into(),
                    ApprovalReason::TaintedContext => "tainted".into(),
                },
            },
        );

        // Block this thread — not the interface's. The dialog is drawn by
        // the frontend, which is a separate process entirely.
        let Ok(rx) = self.approval_rx.lock() else {
            return Approval::Deny;
        };
        rx.recv().unwrap_or(Approval::Deny)
    }

    fn on_tool_start(&mut self, tool: &str, args: &str) {
        let _ = self.app.emit(
            "chat:tool-start",
            ToolStartPayload {
                tool: tool.to_string(),
                args: clip(args, MAX_TOOL_DETAIL),
            },
        );
    }

    fn on_tool_result(&mut self, tool: &str, outcome: &ToolOutcome) {
        // Two lengths, because the interface shows two things: a one-line
        // note in the feed, and the detail behind it when it is opened. The
        // model still gets the whole output; only the display is trimmed.
        // The framing around untrusted content is addressed to the model, not
        // to the reader -- a feed line saying "DIŞ İÇERİK BAŞLANGICI" tells
        // them nothing about what the tool found. Strip it for display; the
        // model still receives it intact.
        let shown = vavis_tools::untrusted::strip_framing(&outcome.content);

        let _ = self.app.emit(
            "chat:tool-done",
            ToolDonePayload {
                tool: tool.to_string(),
                ok: outcome.ok,
                summary: one_line(shown, MAX_TOOL_SUMMARY),
                detail: clip(shown, MAX_TOOL_DETAIL),
            },
        );
    }
}

/// Whether a provider refused a request for its size.
///
/// Providers disagree on how to say this: some use 413, some return 400 with
/// an explanation. Both spellings of the phrase appear in the wild, hence the
/// two substrings.
/// Whether a provider refused because the user is sending too fast, rather
/// than because the request itself is too big.
///
/// The distinction matters because the remedies are opposites: a quota clears
/// by waiting, and shortening the conversation does nothing for it.
///
/// Several providers word a quota refusal as though it were about size. Groq
/// answers a per-minute token overage with "Request too large for model ...
/// on tokens per minute (TPM)" and sends it as **413** -- which read
/// literally is a size refusal. That is why a two-letter message came back
/// saying the conversation had grown too large to read.
fn is_quota_refusal(status: u16, body: &str) -> bool {
    let body = body.to_ascii_lowercase();
    status == 429
        || body.contains("per minute")
        || body.contains("per day")
        || body.contains("tpm")
        || body.contains("rpm")
        || body.contains("rate limit")
        || body.contains("rate_limit")
        || body.contains("quota")
        || body.contains("try again in")
}

/// The wait a provider asked for, in whole seconds, when it named one.
///
/// "Please try again in 1.2s" is more useful rounded up to 2 than reported to
/// the decimal: it is a hint about when to retry, not a measurement.
fn retry_after_seconds(body: &str) -> Option<u64> {
    let body = body.to_ascii_lowercase();
    // Groq words its input-token overage without naming a delay at all:
    //
    //   "... on input tokens per minute (ITPM): Limit 7000, Requested 60012,
    //    please reduce your message size and try again."
    //
    // Note "try again" with no "in": splitting on "try again in" finds the
    // phrase inside it and then parses an empty string. Requiring a digit
    // right after keeps that from reading as a zero-second wait.
    let rest = body.split("try again in").nth(1)?;
    let digits: String = rest
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let secs: f64 = digits.parse().ok()?;
    Some(secs.ceil().max(1.0) as u64)
}

/// A per-minute limit the request can never satisfy, however long we wait.
///
/// Measured against Groq on 2026-09-16. A request bigger than the whole
/// per-minute token allowance comes back as a quota refusal:
///
/// ```text
/// 413  on input tokens per minute (ITPM): Limit 7000, Requested 60012,
///      please reduce your message size and try again.
/// ```
///
/// Waiting does not help — the next minute grants 7000 again, and the
/// request still needs 60012. The honest advice is to shorten the message,
/// which is the opposite of what a rate-limit message tells the user to do.
fn quota_too_small_for_request(body: &str) -> bool {
    let body = body.to_ascii_lowercase();
    let Some(limit) = number_after(&body, "limit ") else {
        return false;
    };
    let Some(requested) = number_after(&body, "requested ") else {
        return false;
    };
    requested > limit
}

/// The first run of digits after `label`, ignoring separators inside it.
fn number_after(body: &str, label: &str) -> Option<u64> {
    let rest = body.split(label).nth(1)?;
    let digits: String = rest
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == ',')
        .filter(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

fn is_too_long(status: u16, body: &str) -> bool {
    // Checked first, and ahead of the status code, because the status is the
    // part that lies: a quota refusal can arrive as 413.
    if is_quota_refusal(status, body) {
        return false;
    }

    let body = body.to_ascii_lowercase();
    status == 413
        || body.contains("too long")
        || body.contains("too large")
        || body.contains("context_length_exceeded")
        || body.contains("maximum context length")
        // Groq says it without any of the words above, and as a 400:
        //   "Please reduce the length of the messages or completion."
        // Measured against the live API. Matching none of these patterns
        // meant the turn failed outright with a raw "Provider error 400"
        // instead of trimming the history and trying again.
        || body.contains("reduce the length")
}

/// The model a retirement notice tells you to move to.
///
/// Google phrases it as "Please use ... use models/gemini-3.6-flash for the
/// latest features", so the name is taken from after the last `models/` and
/// stops at the first character a model id cannot contain. `None` when the
/// sentence does not name one, rather than a guess.
fn replacement_model(body: &str) -> Option<String> {
    let after = body.rsplit_once("models/")?.1;
    let name: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '.')
        .collect();
    // A trailing dot is sentence punctuation, not part of the name.
    let name = name.trim_end_matches('.').to_string();
    (!name.is_empty()).then_some(name)
}

/// How long to wait before trying this request again, if waiting would help.
///
/// `None` means do not retry — either the refusal is not about pace, or the
/// provider named no delay, or the delay it named is long enough that the user
/// should be told rather than left watching a spinner. A quota refusal with no
/// stated delay falls here on purpose: guessing a number would turn a clear
/// message into an unexplained pause.
fn wait_before_retry(err: &vavis_brain::BrainError) -> Option<u64> {
    let vavis_brain::BrainError::Api { status, body } = err else {
        return None;
    };
    if !is_quota_refusal(*status, body) {
        return None;
    }
    retry_after_seconds(body).filter(|secs| *secs <= MAX_RATE_LIMIT_WAIT_SECS)
}

/// Whether this error is one the user can fix by shortening the conversation.
fn error_is_too_long(err: &vavis_brain::BrainError) -> bool {
    matches!(
        err,
        vavis_brain::BrainError::Api { status, body } if is_too_long(*status, body)
    )
}

/// Turns a provider error into something the user can act on.
fn friendly_error(err: &vavis_brain::BrainError) -> String {
    use vavis_brain::BrainError as E;
    match err {
        E::MissingKey { provider } => format!("No API key for {provider}."),
        E::Api { status: 401, .. } => "API key rejected — update it in settings.".into(),
        // A retired model names its own replacement, and that sentence is
        // the most useful thing we can show. Google keeps such models in
        // `/models` after retiring them, so this is reachable by picking a
        // model straight out of the list:
        //
        //   This model models/gemini-2.5-flash is no longer available to new
        //   users. Please update your code to use models/gemini-3.6-flash
        E::Api { status: 404, body } if body.contains("no longer available") => {
            match replacement_model(body) {
                Some(m) => format!("This model has been retired — use {m} instead."),
                None => "This model has been retired — pick another one.".into(),
            }
        }
        E::Api { status: 404, .. } => "Model not found — pick another one.".into(),

        // Before the size check, and matched on the body rather than the
        // status, because a provider can send a quota refusal as 413 and its
        // wording ("Request too large ... per minute") reads like a size
        // problem. Getting this order wrong told the user to shorten a
        // two-word conversation.
        // A single request larger than the whole per-minute allowance. It is
        // shaped like a rate limit but waiting cannot fix it, so saying
        // "try again in a moment" sends the user round a loop that never
        // ends. Checked ahead of the general quota case for that reason.
        E::Api { status, body }
            if is_quota_refusal(*status, body) && quota_too_small_for_request(body) =>
        {
            "This message is larger than your per-minute allowance — shorten it, \
             or start a new chat."
                .into()
        }
        E::Api { status, body } if is_quota_refusal(*status, body) => {
            match retry_after_seconds(body) {
                Some(secs) => format!("Sending too fast — try again in about {secs}s."),
                None => "Sending too fast — wait a moment and try again.".into(),
            }
        }
        E::Api { status, body } if is_too_long(*status, body) => {
            // No instruction in the text: the interface puts a button on this
            // message, and telling someone to do a thing they can be handed
            // instead is the worst of both.
            "This conversation has grown past what the model can read at once.".into()
        }
        E::Api { status, body } => format!("Provider error {status}: {body}"),
        E::Network(e) if e.is_timeout() => "Timed out — the provider did not respond.".into(),
        E::Network(e) if e.is_connect() => "Could not connect — check your network.".into(),
        E::Network(e) => format!("Network error: {e}"),
        E::Parse(e) => format!("Could not read the response: {e}"),
    }
}

/// Answers a pending approval dialog.
#[tauri::command]
pub fn answer_approval(state: State<AppState>, decision: String) {
    let approval = match decision.as_str() {
        "allow" => Approval::Allow,
        "always" => Approval::AllowAlways,
        _ => Approval::Deny,
    };
    let _ = state.approval_tx.send(approval);
}

/// Clears the conversation. Remembered facts survive — a user who saved
/// something with "remember this" must not lose it to a clear.
#[tauri::command]
pub fn clear_conversation(state: State<AppState>) -> Result<(), String> {
    AppState::lock(&state.history).clear();
    AppState::lock(&state.store)
        .clear_messages()
        .map_err(|e| e.to_string())
}

/// How much of the conversation `forget_oldest` keeps.
///
/// A fraction rather than a count, because the point is to make room, and a
/// fixed number of messages means something different in a conversation of
/// twenty than in one of four hundred. Half is enough to get under a window
/// in one press for any conversation that grew there gradually.
const KEEP_FRACTION: usize = 2;

/// Drops the oldest half of the conversation, keeping the recent part.
///
/// This is what the interface offers when a request comes back too long. The
/// alternative it replaces was "clear the conversation", which is a strange
/// thing to ask of someone whose complaint is that they have said too much:
/// it throws away the recent context — the part being talked about — along
/// with the old, and there is no undo.
///
/// Facts are untouched, as with a clear. The stored transcript is untouched
/// too: this trims what the *model* is sent, not what the user can scroll
/// back through, and losing the record of a conversation to a request-size
/// problem would be a poor trade.
#[tauri::command]
pub fn forget_oldest(state: State<AppState>) -> Result<usize, String> {
    let mut history = AppState::lock(&state.history);
    let drop = how_many_to_forget(history.len());
    history.drain(..drop);
    Ok(drop)
}

/// How many messages `forget_oldest` should drop from a history of `len`.
///
/// Separate from the command so the boundary is testable: the command itself
/// needs a Tauri `State` to call.
fn how_many_to_forget(len: usize) -> usize {
    // Nothing to gain below this. Dropping one of three messages will not
    // bring a request under a window, and it would lose context to no end --
    // a conversation this short that will not fit is one long message, which
    // trimming cannot help.
    if len < 4 {
        return 0;
    }
    len / KEEP_FRACTION
}

/// Stores an API key, encrypted.
#[tauri::command]
pub fn set_key(state: State<AppState>, provider: String, key: String) -> Result<(), String> {
    let Some(provider) = Provider::parse(&provider) else {
        return Err(format!("unknown provider: {provider}"));
    };

    let mut keys = AppState::lock(&state.keys);
    keys.set(provider.key_name(), key);

    // Speech recognition runs on Groq too — keep it in step.
    if provider == Provider::Groq {
        if let Some(k) = keys.get("groq") {
            AppState::lock(&state.voice).set_api_key(k.to_string());
        }
    }

    let root = AppState::lock(&state.core).paths.root().to_path_buf();
    keys.save(&root).map_err(|e| e.to_string())?;
    drop(keys);

    // OpenAI TTS reads the same key as OpenAI chat, so pasting it once is
    // enough -- but the speech engine holds its own copy and has to be told.
    if provider == Provider::OpenAI {
        state.refresh_voice();
    }
    Ok(())
}

/// Stores the ElevenLabs key.
///
/// Separate from `set_key` because ElevenLabs is not a chat provider: it does
/// speech only, so it has no place in the provider list the model picker
/// reads. The key still goes to the same encrypted store.
#[tauri::command]
pub fn set_voice_key(state: State<AppState>, key: String) -> Result<(), String> {
    {
        let mut keys = AppState::lock(&state.keys);
        keys.set("elevenlabs", key.trim());
        let root = AppState::lock(&state.core).paths.root().to_path_buf();
        keys.save(&root).map_err(|e| e.to_string())?;
    }
    state.refresh_voice();
    Ok(())
}

/// What the speech section of settings shows.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceSettings {
    /// Engine ids, in the order the interface should list them.
    pub engines: Vec<VoiceEngineInfo>,
    pub engine: String,
    pub rate: i32,
    pub volume: u32,
    pub sapi_voice: String,
    pub edge_voice: String,
    pub kokoro_url: String,
    pub kokoro_voice: String,
    pub eleven_voice: String,
    pub openai_voice: String,
    pub gemini_voice: String,
    /// True when an ElevenLabs key is stored. The key never crosses this bridge.
    pub has_eleven_key: bool,
    pub has_openai_key: bool,
    pub has_gemini_key: bool,
    /// Whether to speak with the chat provider's own voice when it has one.
    pub match_provider: bool,
    /// The engine actually speaking right now. Differs from `engine` when
    /// `match_provider` swapped it, and the interface says so rather than
    /// leaving the user wondering why the picker disagrees with what they
    /// hear.
    pub effective_engine: String,
    /// Voices offered per engine, so the interface can show a real list
    /// rather than making the user guess an identifier.
    pub sapi_voices: Vec<String>,
    pub edge_voices: Vec<[String; 2]>,
    pub kokoro_voices: Vec<[String; 2]>,
    pub eleven_voices: Vec<[String; 2]>,
    pub openai_voices: Vec<[String; 2]>,
    pub gemini_voices: Vec<[String; 2]>,
    /// The address Kokoro listens on out of the box, shown as the placeholder.
    pub kokoro_default_url: String,
    /// Which Edge voice an empty choice resolves to, for the current
    /// language. The interface names it rather than showing a blank
    /// "default" the user cannot interpret.
    pub default_edge_voice: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceEngineInfo {
    pub id: String,
    pub label: String,
    pub needs_key: bool,
}

fn pairs(list: &[(&'static str, &'static str)]) -> Vec<[String; 2]> {
    list.iter()
        .map(|(id, label)| [id.to_string(), label.to_string()])
        .collect()
}

#[tauri::command]
pub fn get_voice_settings(state: State<AppState>) -> VoiceSettings {
    let core = AppState::lock(&state.core);
    let keys = AppState::lock(&state.keys);
    let v = &core.config.voice;

    VoiceSettings {
        engines: vavis_audio::TtsEngineKind::ALL
            .iter()
            .map(|e| VoiceEngineInfo {
                id: e.id().to_string(),
                label: e.label().to_string(),
                needs_key: e.needs_key(),
            })
            .collect(),
        engine: v.engine.clone(),
        rate: v.rate,
        volume: v.volume,
        sapi_voice: v.sapi_voice.clone(),
        edge_voice: v.edge_voice.clone(),
        kokoro_url: v.kokoro_url.clone(),
        kokoro_voice: v.kokoro_voice.clone(),
        eleven_voice: v.eleven_voice.clone(),
        openai_voice: v.openai_voice.clone(),
        gemini_voice: v.gemini_voice.clone(),
        has_eleven_key: keys.get("elevenlabs").is_some_and(|k| !k.trim().is_empty()),
        has_openai_key: keys.get("openai").is_some_and(|k| !k.trim().is_empty()),
        has_gemini_key: keys.get("gemini").is_some_and(|k| !k.trim().is_empty()),
        match_provider: v.match_provider,
        // Asked of the same function the voice layer uses, so the interface
        // cannot drift from what actually speaks.
        effective_engine: crate::state::effective_engine(&core.config, &keys)
            .id()
            .to_string(),
        sapi_voices: vavis_audio::TtsEngine::available_voices(),
        edge_voices: pairs(vavis_audio::edge_tts::voices()),
        kokoro_voices: pairs(vavis_audio::kokoro::voices()),
        eleven_voices: pairs(vavis_audio::elevenlabs::voices()),
        openai_voices: pairs(vavis_audio::openai_tts::voices()),
        gemini_voices: pairs(vavis_audio::gemini_tts::voices()),
        kokoro_default_url: vavis_audio::kokoro::DEFAULT_URL.to_string(),
        default_edge_voice: vavis_audio::edge_tts::default_voice(&core.config.general.language)
            .to_string(),
    }
}

/// Speaks a sample line with the current settings.
///
/// The only honest test of a voice is hearing it: a key can be valid and the
/// engine still unreachable, and a voice id can be accepted and still be the
/// wrong voice.
#[tauri::command]
pub fn preview_voice(state: State<AppState>) -> Result<(), String> {
    let text = {
        let core = AppState::lock(&state.core);
        match core.config.general.language.as_str() {
            "en" => "Hello, this is how I sound.",
            _ => "Merhaba, sesim böyle çıkıyor.",
        }
    };
    AppState::lock(&state.voice).preview(text);
    Ok(())
}

#[tauri::command]
pub fn set_provider(state: State<AppState>, provider: String) -> Result<String, String> {
    let Some(provider) = Provider::parse(&provider) else {
        return Err(format!("unknown provider: {provider}"));
    };

    let mut core = AppState::lock(&state.core);
    core.config.llm.provider = provider.key_name().to_string();
    // Models are provider-specific: keeping the old name would 404.
    core.config.llm.model = provider.default_model().to_string();
    core.config.save(&core.paths).map_err(|e| e.to_string())?;

    Ok(provider.default_model().to_string())
}

#[tauri::command]
pub fn set_model(state: State<AppState>, model: String) -> Result<(), String> {
    let mut core = AppState::lock(&state.core);
    core.config.llm.model = model;
    core.config.save(&core.paths).map_err(|e| e.to_string())
}

/// Fetches the provider's live model list.
#[tauri::command]
pub async fn list_models(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let (provider, key, client) = {
        let core = AppState::lock(&state.core);
        let keys = AppState::lock(&state.keys);
        let provider = Provider::parse(&core.config.llm.provider).unwrap_or(Provider::Groq);
        (
            provider,
            keys.get(provider.key_name())
                .unwrap_or_default()
                .to_string(),
            state.client.clone(),
        )
    };

    client
        .list_models(provider, &key)
        .await
        .map_err(|e| friendly_error(&e))
}

/// Updates one setting.
#[tauri::command]
pub fn set_setting(state: State<AppState>, field: String, value: String) -> Result<(), String> {
    let mut core = AppState::lock(&state.core);
    let mut language_changed = false;

    match field.as_str() {
        "name" => core.config.general.assistant_name = value,
        "language" => {
            if vavis_core::Lang::parse(&value).is_none() {
                return Err("language must be one of: en, tr, de, fr, es".into());
            }
            core.config.general.language = value.clone();
            AppState::lock(&state.voice).set_language(value);
            // Which voice speaks follows the language, so the engine has to
            // be rebuilt too -- see the refresh below.
            language_changed = true;
        }
        "fontSize" => {
            let size: f32 = value.parse().map_err(|_| "font size must be a number")?;
            if !(8.0..=32.0).contains(&size) {
                return Err("font size must be between 8 and 32".into());
            }
            core.config.ui.font_size = size;
        }
        "windowMode" => {
            if !["windowed", "borderless", "fullscreen"].contains(&value.as_str()) {
                return Err("window mode: windowed, borderless or fullscreen".into());
            }
            core.config.ui.window_mode = value;
        }
        // The cheap model that picks tools. Empty turns routing off, which
        // is the default -- so clearing the box is a supported answer, not
        // an error.
        "routerModel" => core.config.llm.router_model = value.trim().to_string(),
        // Full authority: every approval off, every budget off. The warning
        // belongs in the interface, once, at the moment it is switched on --
        // re-asking here on every turn is the thing being switched off.
        "fullAuthority" => {
            let on = match value.as_str() {
                "true" => true,
                "false" => false,
                _ => return Err("full authority must be true or false".into()),
            };
            core.config.security.full_authority = on;
            tracing::warn!(on, "full authority changed");
        }
        // ── Speech ──────────────────────────────────────────────────────
        //
        // The engine and the voices. Keys are not here: those go through
        // `set_key`, into the encrypted store, and never touch this file.
        "voiceEngine" => {
            let Some(engine) = vavis_audio::TtsEngineKind::parse(&value) else {
                return Err("voice engine: sapi, edge, kokoro, elevenlabs or openai".into());
            };
            core.config.voice.engine = engine.id().to_string();
        }
        "voiceRate" => {
            let rate: i32 = value.parse().map_err(|_| "rate must be a number")?;
            if !(-10..=10).contains(&rate) {
                return Err("rate must be between -10 and 10".into());
            }
            core.config.voice.rate = rate;
        }
        "voiceVolume" => {
            let volume: u32 = value.parse().map_err(|_| "volume must be a number")?;
            if volume > 100 {
                return Err("volume must be between 0 and 100".into());
            }
            core.config.voice.volume = volume;
        }
        "sapiVoice" => core.config.voice.sapi_voice = value.trim().to_string(),
        "edgeVoice" => core.config.voice.edge_voice = value.trim().to_string(),
        // Blank means "use the default address", which is what the Kokoro
        // container listens on -- so most users never type anything here.
        "kokoroUrl" => core.config.voice.kokoro_url = value.trim().to_string(),
        "kokoroVoice" => core.config.voice.kokoro_voice = value.trim().to_string(),
        "elevenVoice" => core.config.voice.eleven_voice = value.trim().to_string(),
        "elevenModel" => core.config.voice.eleven_model = value.trim().to_string(),
        "openaiVoice" => core.config.voice.openai_voice = value.trim().to_string(),
        "openaiModel" => core.config.voice.openai_model = value.trim().to_string(),
        "geminiVoice" => core.config.voice.gemini_voice = value.trim().to_string(),
        "geminiModel" => core.config.voice.gemini_model = value.trim().to_string(),
        // Anything other than "true" reads as off, so a malformed value
        // pins the engine the user picked rather than silently moving them.
        "matchProvider" => core.config.voice.match_provider = value.trim() == "true",

        other => return Err(format!("unknown setting: {other}")),
    }

    // Listed rather than pattern-matched: "routerModel" ends in "Model" and
    // has nothing to do with speech, so a suffix rule would quietly rebuild
    // the speech engine every time the router changed.
    const VOICE_FIELDS: [&str; 14] = [
        "voiceEngine",
        "voiceRate",
        "voiceVolume",
        "sapiVoice",
        "edgeVoice",
        "kokoroUrl",
        "kokoroVoice",
        "elevenVoice",
        "elevenModel",
        "openaiVoice",
        "openaiModel",
        "geminiVoice",
        "geminiModel",
        // Changes which engine speaks, so the voice layer has to be rebuilt.
        "matchProvider",
    ];
    let is_voice = VOICE_FIELDS.contains(&field.as_str()) || language_changed;

    core.config.save(&core.paths).map_err(|e| e.to_string())?;

    // The lock has to go before refreshing: `refresh_voice` takes it again,
    // and holding it here would deadlock the moment anyone changed a voice.
    drop(core);
    if is_voice {
        state.refresh_voice();
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Web search chain
// ---------------------------------------------------------------------------

/// Search providers that take an API key.
///
/// `duckduckgo` is absent on purpose: it needs no key, which is what makes it
/// the fallback everyone gets.
const KEYED_SEARCH_PROVIDERS: [&str; 3] = ["tavily", "brave", "custom"];

/// Maps a provider id onto its name in the encrypted key store.
fn search_key_name(provider: &str) -> Option<&'static str> {
    match provider {
        "tavily" => Some("tavily"),
        "brave" => Some("brave"),
        "custom" => Some("search_custom"),
        _ => None,
    }
}

/// What the settings panel shows for the search chain.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchSettings {
    /// Provider ids in the order they are tried.
    pub order: Vec<String>,
    /// Ids that currently have a key stored — never the keys themselves.
    pub configured: Vec<String>,
    pub custom: CustomSearchInfo,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomSearchInfo {
    pub url: String,
    pub header_name: String,
    pub header_value: String,
    pub results_path: String,
    pub title_key: String,
    pub url_key: String,
    pub snippet_key: String,
}

#[tauri::command]
pub fn get_search_settings(state: State<AppState>) -> SearchSettings {
    let core = AppState::lock(&state.core);
    let keys = AppState::lock(&state.keys);
    let custom = &core.config.search.custom;

    SearchSettings {
        order: core.config.search.order.clone(),
        configured: KEYED_SEARCH_PROVIDERS
            .iter()
            .filter(|id| {
                // `custom` runs on its address alone -- its key only fills
                // `{key}` in an optional header. Judging it by the key said
                // "no key -- skipped" about a provider that was about to
                // answer, which is the image side's rule and the opposite of
                // what the search chain actually does:
                //
                //   fn is_usable(&self) -> bool {
                //       !self.url.trim().is_empty() && self.url.contains("{query}")
                //   }
                if **id == "custom" {
                    return custom.url.contains("{query}") && !custom.url.trim().is_empty();
                }
                search_key_name(id)
                    .and_then(|name| keys.get(name))
                    .is_some()
            })
            .map(|id| id.to_string())
            .collect(),
        custom: CustomSearchInfo {
            url: custom.url.clone(),
            header_name: custom.header_name.clone(),
            header_value: custom.header_value.clone(),
            results_path: custom.results_path.clone(),
            title_key: custom.title_key.clone(),
            url_key: custom.url_key.clone(),
            snippet_key: custom.snippet_key.clone(),
        },
    }
}

/// Stores a search provider key, encrypted. An empty value removes it.
#[tauri::command]
pub fn set_search_key(state: State<AppState>, provider: String, key: String) -> Result<(), String> {
    let Some(name) = search_key_name(&provider) else {
        return Err(format!("unknown search provider: {provider}"));
    };

    {
        // Lock core before keys, matching `refresh_search`. Taking these two
        // in opposite orders on different threads would deadlock.
        let core = AppState::lock(&state.core);
        let mut keys = AppState::lock(&state.keys);
        keys.set(name, key);
        keys.save(core.paths.root()).map_err(|e| e.to_string())?;
    }

    // The chain caches its keys, so it has to be told about the new one.
    state.refresh_search();
    Ok(())
}

/// Reorders the chain. Unknown ids are rejected rather than silently dropped,
/// so a typo surfaces here instead of as a provider that never runs.
#[tauri::command]
pub fn set_search_order(state: State<AppState>, order: Vec<String>) -> Result<(), String> {
    let known = ["tavily", "brave", "custom", "duckduckgo"];
    for id in &order {
        if !known.contains(&id.as_str()) {
            return Err(format!("unknown search provider: {id}"));
        }
    }
    if order.is_empty() {
        return Err("the chain needs at least one provider".into());
    }

    {
        let mut core = AppState::lock(&state.core);
        core.config.search.order = order;
        core.config.save(&core.paths).map_err(|e| e.to_string())?;
    }

    state.refresh_search();
    Ok(())
}

/// Saves the user-described JSON search endpoint.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn set_custom_search(
    state: State<AppState>,
    url: String,
    header_name: String,
    header_value: String,
    results_path: String,
    title_key: String,
    url_key: String,
    snippet_key: String,
) -> Result<(), String> {
    // Without the placeholder the query could not be substituted, so the
    // endpoint would silently return the same results for every search.
    if !url.trim().is_empty() && !url.contains("{query}") {
        return Err("the address must contain the {query} placeholder".into());
    }

    {
        let mut core = AppState::lock(&state.core);
        core.config.search.custom = vavis_core::CustomSearch {
            url: url.trim().to_string(),
            header_name: header_name.trim().to_string(),
            header_value: header_value.trim().to_string(),
            results_path: results_path.trim().to_string(),
            title_key: title_key.trim().to_string(),
            url_key: url_key.trim().to_string(),
            snippet_key: snippet_key.trim().to_string(),
        };
        core.config.save(&core.paths).map_err(|e| e.to_string())?;
    }

    state.refresh_search();
    Ok(())
}

// ---------------------------------------------------------------------------
// Code interface — workspace
// ---------------------------------------------------------------------------

/// Opens a folder in the code view.
#[tauri::command]
pub fn open_workspace(path: String) -> Result<String, String> {
    let path = std::path::PathBuf::from(path.trim());
    if !path.is_dir() {
        return Err(format!("no folder at {}", path.display()));
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());

    crate::workspace::set_root(Some(path));
    Ok(name)
}

/// The folder currently open, if any.
#[tauri::command]
pub fn current_workspace() -> Option<String> {
    crate::workspace::current_root().map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
pub fn list_workspace(path: String) -> Result<Vec<crate::workspace::Entry>, String> {
    crate::workspace::list(&path)
}

#[tauri::command]
pub fn read_workspace_file(path: String) -> Result<String, String> {
    crate::workspace::read(&path)
}

#[tauri::command]
pub fn write_workspace_file(path: String, content: String) -> Result<(), String> {
    crate::workspace::write(&path, &content)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub path: String,
    pub line: usize,
    pub text: String,
}

#[tauri::command]
pub fn search_workspace(query: String) -> Result<Vec<SearchHit>, String> {
    Ok(crate::workspace::grep(&query, 100)?
        .into_iter()
        .map(|(path, line, text)| SearchHit { path, line, text })
        .collect())
}

// ---------------------------------------------------------------------------
// MCP servers
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerInfo {
    pub id: String,
    pub transport: String,
    /// Exactly what would be run, so the user can see it before allowing it.
    pub command_line: String,
    pub enabled: bool,
    pub connected: bool,
    /// Every tool the server publishes.
    pub tools: Vec<String>,
    /// The subset the user switched off.
    pub disabled: Vec<String>,
    pub error: Option<String>,
    pub has_secret: bool,
}

/// Configured servers and their current state.
#[tauri::command]
pub fn list_mcp_servers(state: State<AppState>) -> Vec<McpServerInfo> {
    let connected = vavis_tools::mcp::connected_ids();
    let core = AppState::lock(&state.core);
    let keys = AppState::lock(&state.keys);

    core.config
        .mcp
        .servers
        .iter()
        .map(|s| McpServerInfo {
            id: s.id.clone(),
            transport: s.transport.clone(),
            command_line: if s.transport.eq_ignore_ascii_case("http") {
                s.url.clone()
            } else {
                format!("{} {}", s.command, s.args.join(" "))
                    .trim()
                    .to_string()
            },
            enabled: s.enabled,
            connected: connected.contains(&s.id),
            // Tool names are only known while connected; the registry has them.
            tools: {
                let agent = AppState::lock(&state.agent);
                let prefix = format!("{}_", s.id);
                agent
                    .registry
                    .iter()
                    .filter_map(|t| t.name().strip_prefix(&prefix).map(str::to_string))
                    .collect()
            },
            disabled: s.disabled.clone(),
            error: None,
            has_secret: keys.get(&format!("mcp_{}", s.id)).is_some(),
        })
        .collect()
}

/// Adds or replaces a server, then reconnects everything.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn save_mcp_server(
    state: State<AppState>,
    id: String,
    transport: String,
    command: String,
    args: String,
    url: String,
    header_name: String,
    header_value: String,
    secret: String,
) -> Result<String, String> {
    let id = id.trim().to_string();
    // The id prefixes every tool name and names the selection domain, so it
    // has to be a plain identifier.
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("The id has to be letters, digits and dashes.".into());
    }
    if !["stdio", "http"].contains(&transport.as_str()) {
        return Err("The transport has to be stdio or http.".into());
    }
    if transport == "stdio" && command.trim().is_empty() {
        return Err("stdio needs a command to run.".into());
    }
    if transport == "http" && !url.starts_with("http") {
        return Err("http needs an address.".into());
    }

    {
        let mut core = AppState::lock(&state.core);
        let entry = vavis_core::McpServer {
            id: id.clone(),
            transport,
            command: command.trim().to_string(),
            // Split on whitespace: enough for the `npx -y pkg` shape that
            // nearly every server uses.
            args: args.split_whitespace().map(str::to_string).collect(),
            env: Vec::new(),
            url: url.trim().to_string(),
            header_name: header_name.trim().to_string(),
            header_value: header_value.trim().to_string(),
            // Replacing an existing entry keeps whatever the user switched off.
            disabled: core
                .config
                .mcp
                .servers
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.disabled.clone())
                .unwrap_or_default(),
            enabled: true,
        };

        match core.config.mcp.servers.iter_mut().find(|s| s.id == id) {
            Some(existing) => *existing = entry,
            None => core.config.mcp.servers.push(entry),
        }
        core.config.save(&core.paths).map_err(|e| e.to_string())?;

        if !secret.trim().is_empty() {
            let mut keys = AppState::lock(&state.keys);
            keys.set(format!("mcp_{id}"), secret.trim());
            keys.save(core.paths.root()).map_err(|e| e.to_string())?;
        }
    }

    let statuses = state.reload_mcp();
    Ok(match statuses.iter().find(|s| s.id == id) {
        Some(status) if status.connected => {
            format!("{} connected — {} tools.", id, status.tools.len())
        }
        Some(status) => format!(
            "Saved, but could not connect: {}",
            status.error.clone().unwrap_or_default()
        ),
        None => "Saved.".to_string(),
    })
}

/// Removes a server and its stored secret.
#[tauri::command]
pub fn remove_mcp_server(state: State<AppState>, id: String) -> Result<(), String> {
    {
        let mut core = AppState::lock(&state.core);
        core.config.mcp.servers.retain(|s| s.id != id);
        core.config.save(&core.paths).map_err(|e| e.to_string())?;

        let mut keys = AppState::lock(&state.keys);
        keys.remove(&format!("mcp_{id}"));
        keys.save(core.paths.root()).map_err(|e| e.to_string())?;
    }
    state.reload_mcp();
    Ok(())
}

/// Enables or disables a whole server.
#[tauri::command]
pub fn toggle_mcp_server(state: State<AppState>, id: String, enabled: bool) -> Result<(), String> {
    {
        let mut core = AppState::lock(&state.core);
        let Some(server) = core.config.mcp.servers.iter_mut().find(|s| s.id == id) else {
            return Err(format!("no server called {id}"));
        };
        server.enabled = enabled;
        core.config.save(&core.paths).map_err(|e| e.to_string())?;
    }
    state.reload_mcp();
    Ok(())
}

/// Switches one tool on or off.
///
/// A server that brings fourteen tools when three are wanted would otherwise
/// spend the whole per-request budget.
#[tauri::command]
pub fn toggle_mcp_tool(
    state: State<AppState>,
    id: String,
    tool: String,
    enabled: bool,
) -> Result<(), String> {
    {
        let mut core = AppState::lock(&state.core);
        let Some(server) = core.config.mcp.servers.iter_mut().find(|s| s.id == id) else {
            return Err(format!("no server called {id}"));
        };
        if enabled {
            server.disabled.retain(|t| t != &tool);
        } else if !server.disabled.contains(&tool) {
            server.disabled.push(tool);
        }
        core.config.save(&core.paths).map_err(|e| e.to_string())?;
    }
    state.reload_mcp();
    Ok(())
}

// ---------------------------------------------------------------------------
// Spotify
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotifySettings {
    /// The user's own id, if they set one. Empty means the built-in app.
    pub client_id: String,
    pub connected: bool,
    /// Only needed by someone registering an app of their own; the built-in
    /// one already has this URI on it.
    pub redirect_uri: String,
}

#[tauri::command]
pub fn get_spotify_settings(state: State<AppState>) -> SpotifySettings {
    let core = AppState::lock(&state.core);
    SpotifySettings {
        client_id: core.config.spotify.client_id.clone(),
        connected: vavis_tools::spotify::current().is_connected(),
        redirect_uri: vavis_tools::spotify::auth::redirect_uri(),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NowPlayingInfo {
    pub track: String,
    pub artist: String,
    pub album_art: Option<String>,
    pub duration_ms: u64,
    pub progress_ms: u64,
    pub playing: bool,
    pub device: Option<String>,
}

/// What Spotify is playing, or `None`.
///
/// Kept out of `get_status` on purpose: status is polled every second and
/// must never wait on the network, while this can. The tool layer caches the
/// answer for a few seconds and the interface counts the progress bar
/// forward locally, so Spotify is not asked on every tick.
#[tauri::command]
pub fn spotify_now_playing() -> Option<NowPlayingInfo> {
    match vavis_tools::spotify::now_playing() {
        Ok(Some(np)) => Some(NowPlayingInfo {
            track: np.track,
            artist: np.artist,
            album_art: np.album_art,
            duration_ms: np.duration_ms,
            progress_ms: np.progress_ms,
            playing: np.playing,
            device: np.device,
        }),
        // Not connected or nothing playing both mean "show nothing".
        _ => None,
    }
}

/// Album art as a `data:` URI, cached on disk.
///
/// Two reasons not to point the `<img>` straight at Spotify's CDN: the
/// window's content-security policy allows no remote images, and re-fetching
/// the same cover on every poll would be wasteful. Fetching once here and
/// keeping the bytes means the art survives restarts too.
#[tauri::command]
pub fn spotify_album_art(state: State<AppState>, url: String) -> Result<String, String> {
    if !url.starts_with("https://") {
        return Err("only https artwork is fetched".into());
    }

    let cache_dir = AppState::lock(&state.core).paths.root().join("art");
    std::fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;

    // The CDN path is already a content hash; its last segment is a stable,
    // filesystem-safe name.
    let name: String = url
        .rsplit('/')
        .next()
        .unwrap_or("cover")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    let file = cache_dir.join(format!("{name}.jpg"));

    let bytes = match std::fs::read(&file) {
        Ok(bytes) => bytes,
        Err(_) => {
            let fetched = vavis_tools::spotify::fetch_album_art(&url).map_err(|e| e.to_string())?;
            // A failed write is not fatal; the art still displays this time.
            if let Err(e) = std::fs::write(&file, &fetched) {
                tracing::debug!(%e, "album art not cached");
            }
            fetched
        }
    };

    Ok(format!("data:image/jpeg;base64,{}", base64(&bytes)))
}

/// Standard base64 with padding, for the data URI.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);

    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;

        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }

    out
}

/// Transport control from the now-playing panel.
///
/// Direct rather than routed through the model: pressing pause is not a
/// request for the assistant to think about.
#[tauri::command]
pub fn spotify_control(action: String) -> Result<(), String> {
    let mapped = match action.as_str() {
        "play" | "pause" | "next" | "previous" => action.as_str(),
        other => return Err(format!("unknown action: {other}")),
    };
    vavis_tools::spotify::transport(mapped).map_err(|e| e.to_string())
}

/// Saves a Spotify client id of the user's own.
///
/// Optional. Empty clears it and puts the built-in application back, which is
/// the path almost everyone stays on; only someone who wants their own name on
/// the consent screen sets one.
///
/// A non-empty id is validated before it is stored: an id that is really the
/// redirect URI gets no error from Spotify, just a blank page, so the check
/// has to happen here.
#[tauri::command]
pub fn set_spotify_client_id(state: State<AppState>, client_id: String) -> Result<(), String> {
    if !client_id.trim().is_empty() {
        vavis_tools::spotify::auth::check_client_id(&client_id)?;
    }
    {
        let mut core = AppState::lock(&state.core);
        core.config.spotify.client_id = client_id.trim().to_string();
        core.config.save(&core.paths).map_err(|e| e.to_string())?;
    }
    state.refresh_spotify();
    Ok(())
}

/// Starts the authorisation flow.
///
/// Returns as soon as the browser is opened; the rest happens on a worker
/// thread and lands as a `spotify:auth` event. Blocking a command for the
/// ninety seconds someone might spend logging in would freeze the interface.
#[tauri::command]
pub fn connect_spotify(app: tauri::AppHandle, state: State<AppState>) -> Result<(), String> {
    let configured = AppState::lock(&state.core).config.spotify.client_id.clone();
    // Empty is the ordinary case and means the built-in application, so there
    // is nothing to set up before pressing connect.
    let client_id = vavis_tools::spotify::auth::client_id_or_default(&configured).to_string();
    // Checked here as well as on save: a config file edited by hand, or written
    // by an older build without the check, would otherwise open a browser tab
    // that can only fail.
    vavis_tools::spotify::auth::check_client_id(&client_id)?;

    let pkce = vavis_tools::spotify::auth::Pkce::generate();
    let url = vavis_tools::spotify::auth::authorize_url(&client_id, &pkce);

    // Bind the listener before opening the browser, so a very fast redirect
    // cannot arrive before anything is listening.
    let expected_state = pkce.state.clone();
    let verifier = pkce.verifier.clone();

    std::thread::spawn(move || {
        let outcome = match vavis_tools::spotify::auth::wait_for_callback(&expected_state) {
            Ok(vavis_tools::spotify::auth::Callback::Code(code)) => {
                vavis_tools::spotify::exchange_code(&client_id, &code, &verifier)
                    .map_err(|e| e.to_string())
            }
            Ok(vavis_tools::spotify::auth::Callback::Denied(reason)) => Err(reason),
            Err(e) => Err(e),
        };

        let payload = match outcome {
            Ok(token) => {
                // Persisting is the shell's job; the tool layer only holds it
                // in memory.
                if let Some(state) = app.try_state::<AppState>() {
                    state.save_spotify_token(&token);
                }
                serde_json::json!({ "ok": true, "message": "Spotify connected." })
            }
            Err(message) => serde_json::json!({ "ok": false, "message": message }),
        };
        let _ = app.emit("spotify:auth", payload);
    });

    open_in_browser(&url)
}

/// Disconnects Spotify by forgetting the stored token.
#[tauri::command]
pub fn disconnect_spotify(state: State<AppState>) -> Result<(), String> {
    {
        let core = AppState::lock(&state.core);
        let mut keys = AppState::lock(&state.keys);
        keys.remove("spotify_token");
        keys.save(core.paths.root()).map_err(|e| e.to_string())?;
    }
    state.refresh_spotify();
    Ok(())
}

/// Opens a URL in the user's default browser.
///
/// Never through `cmd /C start`, which is the obvious way and silently
/// mangles exactly the URLs this is for. `&` separates commands in `cmd`, and
/// an argument is only quoted on the way out if it contains a space, so an
/// authorisation URL arrives at the browser truncated at its first parameter
/// separator. Spotify then answers with a bare `client_id: Not present` page
/// and nothing anywhere reports an error: the process spawned fine, the
/// browser opened fine, and the request was simply not the one that was
/// built. Connecting could not work on Windows at all.
///
/// `url.dll,FileProtocolHandler` is the protocol handler itself, so the URL
/// reaches the browser exactly as built, query string and all. `explorer` is
/// the other obvious candidate and is worse: given a URL it does not
/// recognise as one it opens Documents instead, with no error either.
fn open_in_browser(url: &str) -> Result<(), String> {
    use std::process::Command;

    #[cfg(windows)]
    let result = Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn();

    #[cfg(not(windows))]
    let result = Command::new("xdg-open").arg(url).spawn();

    result
        .map(|_| ())
        .map_err(|e| format!("could not open the browser: {e}"))
}

// ---------------------------------------------------------------------------
// Steam
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamSettings {
    pub steam_id: String,
    /// Whether a key is stored — never the key itself.
    pub has_key: bool,
}

#[tauri::command]
pub fn get_steam_settings(state: State<AppState>) -> SteamSettings {
    let core = AppState::lock(&state.core);
    let keys = AppState::lock(&state.keys);
    SteamSettings {
        steam_id: core.config.steam.steam_id.clone(),
        has_key: keys.get("steam").is_some(),
    }
}

/// Saves Steam credentials and checks them.
///
/// The check matters more than it looks: a private profile answers HTTP 200
/// with an empty list, so without it every later question would come back
/// "you own no games" and the user would have no idea why.
#[tauri::command]
pub fn set_steam(state: State<AppState>, steam_id: String, key: String) -> Result<String, String> {
    let id = steam_id.trim().to_string();
    if !id.is_empty() && (id.len() != 17 || !id.chars().all(|c| c.is_ascii_digit())) {
        return Err("A SteamID64 is 17 digits.".into());
    }

    {
        let mut core = AppState::lock(&state.core);
        core.config.steam.steam_id = id.clone();
        core.config.save(&core.paths).map_err(|e| e.to_string())?;

        let mut keys = AppState::lock(&state.keys);
        // An empty key means "leave what is stored" rather than "erase it":
        // the field is a password input and comes back blank on every reload.
        if !key.trim().is_empty() {
            keys.set("steam", key.trim());
            keys.save(core.paths.root()).map_err(|e| e.to_string())?;
        }
    }

    state.refresh_steam();

    if !vavis_tools::steam::current().is_configured() {
        return Ok("Saved. The library needs both a key and a SteamID.".into());
    }

    // Verify once, now, while the user is looking at the settings panel.
    match vavis_tools::steam::library() {
        Ok(games) => Ok(format!("Connected — {} games visible.", games.len())),
        Err(e) => Ok(format!("Saved, but: {e}")),
    }
}

// ---------------------------------------------------------------------------
// Obsidian vault
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultInfo {
    /// Absolute path on disk.
    pub path: String,
    /// Folder name — what the user calls the vault.
    pub name: String,
    pub active: bool,
}

/// Vaults Obsidian knows about, plus whichever one is active.
#[tauri::command]
pub fn list_vaults(state: State<AppState>) -> Vec<VaultInfo> {
    let active = vavis_tools::obsidian::current().map(|v| v.root);
    let configured = AppState::lock(&state.core).config.obsidian.vault.clone();

    let mut paths = vavis_tools::obsidian::discover();
    // A vault chosen by hand may not be in Obsidian's own list.
    let configured_path = std::path::PathBuf::from(configured.trim());
    if !configured.trim().is_empty() && !paths.contains(&configured_path) {
        paths.insert(0, configured_path);
    }

    paths
        .into_iter()
        .map(|path| VaultInfo {
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string()),
            active: active.as_ref() == Some(&path),
            path: path.to_string_lossy().to_string(),
        })
        .collect()
}

/// Switches the active vault. An empty path falls back to auto-detection.
#[tauri::command]
pub fn set_vault(state: State<AppState>, path: String) -> Result<(), String> {
    let trimmed = path.trim().to_string();
    if !trimmed.is_empty() && !std::path::Path::new(&trimmed).is_dir() {
        return Err(format!("no folder at {trimmed}"));
    }

    {
        let mut core = AppState::lock(&state.core);
        core.config.obsidian.vault = trimmed.clone();
        core.config.save(&core.paths).map_err(|e| e.to_string())?;
    }

    vavis_tools::obsidian::set_active(vavis_tools::obsidian::autoselect(&trimmed));
    Ok(())
}

/// Cycles the voice mode, returning the new one.
#[tauri::command]
pub fn cycle_voice(state: State<AppState>) -> Result<String, String> {
    let mode = AppState::lock(&state.voice).cycle_mode()?;
    Ok(crate::voice::mode_name(mode).to_string())
}

/// Barge-in: stops speech immediately.
#[tauri::command]
pub fn stop_speaking(state: State<AppState>) {
    AppState::lock(&state.voice).stop_speaking();
}

/// Drains queued voice events.
#[tauri::command]
pub fn poll_voice(state: State<AppState>) -> Vec<crate::voice::VoiceEvent> {
    AppState::lock(&state.voice).poll()
}

/// A remembered fact.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FactView {
    pub id: i64,
    pub text: String,
}

#[tauri::command]
pub fn list_facts(state: State<AppState>) -> Vec<FactView> {
    AppState::lock(&state.store)
        .all_facts()
        .unwrap_or_default()
        .into_iter()
        .map(|f| FactView {
            id: f.id,
            text: f.text,
        })
        .collect()
}

#[tauri::command]
pub fn forget_fact(state: State<AppState>, id: i64) -> Result<bool, String> {
    AppState::lock(&state.store)
        .delete_fact(id)
        .map_err(|e| e.to_string())
}

/// A scheduled or conditional automation.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationView {
    pub id: i64,
    pub prompt: String,
    pub trigger: String,
    pub enabled: bool,
}

#[tauri::command]
pub fn list_automations(state: State<AppState>) -> Vec<AutomationView> {
    AppState::lock(&state.store)
        .all_automations()
        .unwrap_or_default()
        .into_iter()
        .map(|a| AutomationView {
            id: a.id,
            prompt: a.prompt,
            trigger: a.trigger.describe(),
            enabled: a.enabled,
        })
        .collect()
}

#[tauri::command]
pub fn delete_automation(state: State<AppState>, id: i64) -> Result<bool, String> {
    AppState::lock(&state.store)
        .delete_automation(id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn toggle_automation(state: State<AppState>, id: i64, enabled: bool) -> Result<bool, String> {
    AppState::lock(&state.store)
        .set_automation_enabled(id, enabled)
        .map_err(|e| e.to_string())
}

/// A registered tool, for the tools panel.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolView {
    pub name: String,
    pub description: String,
    pub domain: String,
    pub risk: String,
}

#[tauri::command]
pub fn list_tools(state: State<AppState>) -> Vec<ToolView> {
    AppState::lock(&state.agent)
        .registry
        .iter()
        .map(|t| ToolView {
            name: t.name().to_string(),
            description: t.description().to_string(),
            domain: t.domain().name().to_string(),
            risk: match t.risk() {
                vavis_tools::Risk::Safe => "safe",
                vavis_tools::Risk::Moderate => "moderate",
                vavis_tools::Risk::Destructive => "destructive",
            }
            .to_string(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Canvas interface — image and video generation
// ---------------------------------------------------------------------------

/// One row of the gallery, as the grid needs it.
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GalleryItem {
    pub id: i64,
    pub kind: String,
    /// Absolute path. The frontend turns it into an `asset:` URL rather than
    /// pulling megabytes of base64 through IPC for every tile.
    pub path: String,
    pub prompt: String,
    pub provider: String,
    pub model: String,
    pub params: String,
    /// Absent when the provider did not report one, which means this result
    /// cannot be reproduced exactly — the interface says so.
    pub seed: Option<i64>,
    pub width: i64,
    pub height: i64,
    pub bytes: i64,
    pub parent_id: Option<i64>,
    pub favourite: bool,
    pub created_at: i64,
}

fn to_gallery_item(item: &vavis_core::GalleryItem, media_dir: &std::path::Path) -> GalleryItem {
    GalleryItem {
        id: item.id,
        kind: item.kind.as_str().to_string(),
        path: media_dir.join(&item.path).to_string_lossy().to_string(),
        prompt: item.prompt.clone(),
        provider: item.provider.clone(),
        model: item.model.clone(),
        params: item.params.clone(),
        seed: item.seed,
        width: item.width,
        height: item.height,
        bytes: item.bytes,
        parent_id: item.parent_id,
        favourite: item.favourite,
        created_at: item.created_at,
    }
}

/// What the canvas needs to draw itself before anything is generated.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasSettings {
    pub image_order: Vec<String>,
    pub video_order: Vec<String>,
    pub image_model: String,
    pub video_model: String,
    pub size: String,
    pub count: u32,
    /// Which provider ids have a key — so the interface can say "add a key"
    /// instead of failing after the user has typed a prompt.
    pub configured: Vec<String>,
    pub can_image: bool,
    pub can_video: bool,
    pub can_upscale: bool,
    pub custom_url: String,
    pub custom_header_name: String,
    pub custom_header_value: String,
    pub custom_model: String,
    pub items: i64,
    pub bytes: i64,
}

#[tauri::command]
pub fn get_canvas_settings(state: State<AppState>) -> CanvasSettings {
    let core = AppState::lock(&state.core);
    let keys = AppState::lock(&state.keys);
    let canvas = &core.config.canvas;

    let mut configured = Vec::new();
    // The chat key doubles as the image key unless a separate one was set.
    if keys.get("canvas_openai").is_some() || keys.get("openai").is_some() {
        configured.push("openai".to_string());
    }
    for id in ["stability", "replicate"] {
        if keys.get(&format!("canvas_{id}")).is_some() {
            configured.push(id.to_string());
        }
    }
    if !canvas.custom.url.trim().is_empty() {
        configured.push("custom".to_string());
    }

    let usage = AppState::lock(&state.store)
        .gallery_usage()
        .unwrap_or_default();

    CanvasSettings {
        image_order: canvas.image_order.clone(),
        video_order: canvas.video_order.clone(),
        image_model: canvas.image_model.clone(),
        video_model: canvas.video_model.clone(),
        size: canvas.size.clone(),
        count: canvas.count,
        configured,
        can_image: vavis_tools::canvas::is_ready(vavis_tools::canvas::Kind::Image),
        can_video: vavis_tools::canvas::is_ready(vavis_tools::canvas::Kind::Video),
        can_upscale: vavis_tools::canvas::can_upscale(),
        custom_url: canvas.custom.url.clone(),
        custom_header_name: canvas.custom.header_name.clone(),
        custom_header_value: canvas.custom.header_value.clone(),
        custom_model: canvas.custom.model.clone(),
        items: usage.items,
        bytes: usage.bytes,
    }
}

/// Stores a generation key. Same rule as everywhere else: secrets go in the
/// encrypted store, never in the settings file.
#[tauri::command]
pub fn set_canvas_key(state: State<AppState>, provider: String, key: String) -> Result<(), String> {
    let name = format!("canvas_{}", provider.trim());
    {
        let core = AppState::lock(&state.core);
        let mut keys = AppState::lock(&state.keys);
        keys.set(name, key.trim().to_string());
        keys.save(core.paths.root()).map_err(|e| e.to_string())?;
    }
    // Locks released first: refresh takes core and keys in that order, and
    // holding them here would deadlock against it.
    state.refresh_canvas();
    Ok(())
}

/// Reorders a provider chain. `kind` is "image" or "video".
#[tauri::command]
pub fn set_canvas_order(
    state: State<AppState>,
    kind: String,
    order: Vec<String>,
) -> Result<(), String> {
    {
        let mut core = AppState::lock(&state.core);
        if kind == "video" {
            core.config.canvas.video_order = order;
        } else {
            core.config.canvas.image_order = order;
        }
        let paths = core.paths.clone();
        core.config.save(&paths).map_err(|e| e.to_string())?;
    }
    state.refresh_canvas();
    Ok(())
}

/// Saves the defaults and the custom endpoint.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn set_canvas_defaults(
    state: State<AppState>,
    image_model: String,
    video_model: String,
    size: String,
    count: u32,
    custom_url: String,
    custom_header_name: String,
    custom_header_value: String,
    custom_model: String,
) -> Result<(), String> {
    {
        let mut core = AppState::lock(&state.core);
        let canvas = &mut core.config.canvas;
        canvas.image_model = image_model.trim().to_string();
        canvas.video_model = video_model.trim().to_string();
        canvas.size = size.trim().to_string();
        canvas.count = count.clamp(1, 8);
        canvas.custom.url = custom_url.trim().to_string();
        canvas.custom.header_name = custom_header_name.trim().to_string();
        canvas.custom.header_value = custom_header_value.trim().to_string();
        canvas.custom.model = custom_model.trim().to_string();

        let paths = core.paths.clone();
        core.config.save(&paths).map_err(|e| e.to_string())?;
    }
    state.refresh_canvas();
    Ok(())
}

#[tauri::command]
pub fn list_gallery(state: State<AppState>, limit: Option<usize>) -> Vec<GalleryItem> {
    let media_dir = AppState::lock(&state.core).paths.media_dir();
    let store = AppState::lock(&state.store);
    store
        .gallery_items(limit.unwrap_or(200))
        .unwrap_or_default()
        .iter()
        .map(|item| to_gallery_item(item, &media_dir))
        .collect()
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CanvasDonePayload {
    items: Vec<GalleryItem>,
    provider: String,
    /// Providers that failed before the one that worked — a note, not an error.
    notes: Vec<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CanvasErrorPayload {
    message: String,
}

/// Starts a generation.
///
/// Returns as soon as the work is handed to a thread: an image takes seconds
/// and a video takes minutes, and neither should freeze the window. The result
/// arrives as a `canvas:done` or `canvas:error` event.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn canvas_generate(
    app: tauri::AppHandle,
    state: State<AppState>,
    prompt: String,
    kind: String,
    model: String,
    width: u32,
    height: u32,
    count: u32,
    seed: Option<i64>,
    negative: String,
    duration_secs: u32,
    from_id: Option<i64>,
    strength: f32,
    upscale: bool,
) -> Result<(), String> {
    use vavis_tools::canvas;

    let prompt = prompt.trim().to_string();
    // An enlargement has a source instead of a prompt, so only a fresh
    // generation needs one.
    if prompt.is_empty() && !upscale {
        return Err("nothing to draw — write a prompt first".into());
    }

    let kind = canvas::Kind::parse(&kind);
    if upscale {
        if from_id.is_none() {
            return Err("pick a result to enlarge".into());
        }
        if !canvas::can_upscale() {
            return Err("enlarging needs a Stability or Replicate key".into());
        }
    } else if !canvas::is_ready(kind) {
        return Err(format!(
            "no {} provider has a key yet — add one in settings",
            kind.as_str()
        ));
    }

    let media_dir = AppState::lock(&state.core).paths.media_dir();

    // Continuing from an existing result: its bytes become the starting image,
    // which is what makes "another like this" and "animate this" possible.
    let init = match from_id {
        Some(id) => {
            let store = AppState::lock(&state.store);
            let parent = store
                .gallery_item(id)
                .map_err(|e| e.to_string())?
                .ok_or("that result is no longer in the gallery")?;
            Some(canvas::storage::read(&media_dir, &parent.path).map_err(|e| e.to_string())?)
        }
        None => None,
    };

    let request = canvas::Request {
        prompt,
        kind,
        model: model.trim().to_string(),
        width: if width == 0 { 1024 } else { width },
        height: if height == 0 { 1024 } else { height },
        count: count.clamp(1, 8),
        seed,
        negative: negative.trim().to_string(),
        duration_secs: duration_secs.clamp(1, 60),
        init,
        strength: strength.clamp(0.0, 1.0),
        upscale,
    };

    let store = state.store.clone();

    std::thread::spawn(move || {
        let generated = match canvas::generate(request.clone()) {
            Ok(generated) => generated,
            Err(attempts) => {
                let message = if attempts.is_empty() {
                    "no provider was able to take this request".to_string()
                } else {
                    attempts
                        .iter()
                        .map(|a| format!("{}: {}", a.provider, a.error))
                        .collect::<Vec<_>>()
                        .join(" · ")
                };
                let _ = app.emit("canvas:error", CanvasErrorPayload { message });
                return;
            }
        };

        // Saved before it is announced: a result the interface can see but the
        // gallery does not have is a result that vanishes on the next restart.
        let mut saved = Vec::new();
        for asset in &generated.assets {
            let (name, bytes) = match canvas::storage::save(&media_dir, asset, request.kind) {
                Ok(result) => result,
                Err(e) => {
                    let _ = app.emit(
                        "canvas:error",
                        CanvasErrorPayload {
                            message: format!("generated, but could not be saved: {e}"),
                        },
                    );
                    return;
                }
            };

            let params = serde_json::json!({
                "size": request.size_label(),
                "aspect": request.aspect_ratio(),
                "count": request.count,
                "negative": request.negative,
                "duration": request.duration_secs,
                "strength": request.strength,
                "upscale": request.upscale,
            })
            .to_string();

            let row = vavis_core::NewGalleryItem {
                kind: Some(match request.kind {
                    canvas::Kind::Image => vavis_core::GalleryKind::Image,
                    canvas::Kind::Video => vavis_core::GalleryKind::Video,
                }),
                path: name,
                prompt: request.prompt.clone(),
                provider: generated.provider.clone(),
                model: generated.model.clone(),
                params,
                // The provider's seed, not the one that was asked for: they
                // differ whenever none was given, and only the real one
                // reproduces the result.
                seed: asset.seed.or(request.seed),
                width: i64::from(if asset.width > 0 {
                    asset.width
                } else {
                    request.width
                }),
                height: i64::from(if asset.height > 0 {
                    asset.height
                } else {
                    request.height
                }),
                bytes: bytes as i64,
                parent_id: from_id,
            };

            let inserted = {
                let store = AppState::lock(&store);
                store
                    .add_gallery_item(&row)
                    .and_then(|id| store.gallery_item(id))
            };

            match inserted {
                Ok(Some(item)) => saved.push(to_gallery_item(&item, &media_dir)),
                Ok(None) => {}
                Err(e) => tracing::warn!(%e, "generated file was not indexed"),
            }
        }

        let notes = generated
            .attempts
            .iter()
            .map(|a| format!("{}: {}", a.provider, a.error))
            .collect();

        let _ = app.emit(
            "canvas:done",
            CanvasDonePayload {
                items: saved,
                provider: generated.provider,
                notes,
            },
        );
    });

    Ok(())
}

/// Removes one result, file and row.
#[tauri::command]
pub fn delete_gallery_item(state: State<AppState>, id: i64) -> Result<(), String> {
    let media_dir = AppState::lock(&state.core).paths.media_dir();
    let removed = AppState::lock(&state.store)
        .delete_gallery_item(id)
        .map_err(|e| e.to_string())?;

    if let Some(name) = removed {
        // A file that will not delete (open in a viewer, say) is not worth
        // failing over: the row is gone, and the next sweep catches the file.
        if let Err(e) = vavis_tools::canvas::storage::remove(&media_dir, &name) {
            tracing::warn!(%e, "gallery file could not be deleted");
        }
    }
    Ok(())
}

#[tauri::command]
pub fn favourite_gallery_item(
    state: State<AppState>,
    id: i64,
    favourite: bool,
) -> Result<(), String> {
    AppState::lock(&state.store)
        .set_gallery_favourite(id, favourite)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Empties the gallery, optionally sparing what the user starred.
///
/// Also sweeps files no row points at — they accumulate from interrupted
/// generations and are otherwise invisible disk usage.
#[tauri::command]
pub fn clear_gallery(state: State<AppState>, keep_favourites: bool) -> Result<u64, String> {
    let media_dir = AppState::lock(&state.core).paths.media_dir();

    let (removed, kept) = {
        let store = AppState::lock(&state.store);
        let removed = store
            .clear_gallery(keep_favourites)
            .map_err(|e| e.to_string())?;
        let kept: Vec<String> = store
            .gallery_items(usize::MAX)
            .unwrap_or_default()
            .into_iter()
            .map(|i| i.path)
            .collect();
        (removed, kept)
    };

    let mut freed = 0u64;
    for name in removed
        .iter()
        .cloned()
        .chain(vavis_tools::canvas::storage::orphans(&media_dir, &kept))
    {
        if let Some(path) = vavis_tools::canvas::storage::resolve(&media_dir, &name) {
            freed += std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        }
        if let Err(e) = vavis_tools::canvas::storage::remove(&media_dir, &name) {
            tracing::warn!(%e, "gallery file could not be deleted");
        }
    }
    Ok(freed)
}

/// Opens the media folder in the system file manager.
///
/// The note asks that the user be able to see what this is costing them in
/// disk; a number in settings answers "how much", this answers "where".
#[tauri::command]
pub fn open_media_folder(state: State<AppState>) -> Result<(), String> {
    let dir = AppState::lock(&state.core).paths.media_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer").arg(&dir).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(&dir).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(&dir).spawn();

    // explorer.exe returns a non-zero exit code even when it worked, so only
    // a spawn failure counts as a failure here.
    result.map(|_| ()).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Updates
// ---------------------------------------------------------------------------

/// Asks GitHub whether a newer release exists.
///
/// Nothing about the user goes out with this request: no key, no identifier,
/// no settings. It reads a public release list, unauthenticated.
#[tauri::command]
pub async fn check_update() -> vavis_core::version::UpdateCheck {
    crate::update::check().await
}

/// Opens the release page in the browser.
///
/// The address is the compiled-in constant, never a value that came back over
/// the network. A URL from a response is data, and handing data straight to
/// the shell's "open this" verb is how a compromised or spoofed release feed
/// would get to open anything it liked on the user's machine.
#[tauri::command]
pub fn open_release_page() -> Result<(), String> {
    let url = crate::update::RELEASES_PAGE;

    #[cfg(target_os = "windows")]
    // Through `explorer`, not `cmd /c start`: `start` runs its argument
    // through the command interpreter.
    let result = std::process::Command::new("explorer").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(url).spawn();

    // explorer.exe reports a non-zero exit code even on success, so only a
    // failure to spawn counts.
    result.map(|_| ()).map_err(|e| e.to_string())
}

/// The version this build reports.
#[tauri::command]
pub fn app_version() -> String {
    vavis_core::version::CURRENT.to_string()
}

// ---------------------------------------------------------------------------
// Council interface — several models on one question
// ---------------------------------------------------------------------------

/// A seat as the interface describes it.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeatInput {
    pub id: String,
    pub provider: String,
    pub model: String,
    pub sees_others: bool,
    pub brief: String,
}

impl From<SeatInput> for crate::council::Seat {
    fn from(s: SeatInput) -> Self {
        Self {
            id: s.id,
            provider: s.provider,
            model: s.model,
            sees_others: s.sees_others,
            brief: s.brief,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastView {
    pub requests: usize,
    pub tokens: usize,
    pub dollars: f64,
    /// Seats whose model has no published price. Shown separately so a
    /// partial total is never presented as a complete one.
    pub unpriced: usize,
}

/// What a run would cost, before it runs.
#[tauri::command]
pub fn council_forecast(task: String, seats: Vec<SeatInput>) -> ForecastView {
    let seats: Vec<crate::council::Seat> = seats.into_iter().map(Into::into).collect();
    let f = crate::council::forecast(&task, &seats);
    ForecastView {
        requests: f.requests,
        tokens: f.tokens,
        dollars: f.dollars,
        unpriced: f.unpriced,
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CouncilDeltaPayload {
    seat: String,
    text: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CouncilSeatDonePayload {
    seat: String,
    text: String,
    label: String,
    input_tokens: usize,
    output_tokens: usize,
    /// Absent for a model with no published price, including local ones.
    dollars: Option<f64>,
    elapsed_ms: u128,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CouncilSeatFailedPayload {
    seat: String,
    message: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CouncilDonePayload {
    /// Seats that produced an answer.
    answered: usize,
    failed: usize,
    dollars: f64,
    unpriced: usize,
}

/// Runs a council.
///
/// Returns as soon as the work is handed off. Progress arrives as events:
///
/// | Event | Payload |
/// |---|---|
/// | `council:delta` | `{ seat, text }` |
/// | `council:seat-done` | `{ seat, text, label, tokens, dollars, elapsedMs }` |
/// | `council:seat-failed` | `{ seat, message }` |
/// | `council:done` | `{ answered, failed, dollars, unpriced }` |
#[tauri::command]
pub fn council_run(
    app: tauri::AppHandle,
    state: State<AppState>,
    task: String,
    seats: Vec<SeatInput>,
) -> Result<(), String> {
    let task = task.trim().to_string();
    if task.is_empty() {
        return Err("write a task for the council first".into());
    }
    if seats.is_empty() {
        return Err("add at least one seat".into());
    }
    // A ceiling the user cannot accidentally cross. Not a technical limit —
    // a bill limit. Eight parallel requests on one question is already a lot.
    if seats.len() > 8 {
        return Err("eight seats is the most this will run at once".into());
    }

    let seats: Vec<crate::council::Seat> = seats.into_iter().map(Into::into).collect();

    // Configs are built up front, while the key store is at hand — and so a
    // misconfigured seat is reported before any paid request goes out.
    let configs: Vec<Result<ChatConfig, String>> = {
        let keys = AppState::lock(&state.keys);
        seats
            .iter()
            .map(|seat| {
                let key = Provider::parse(&seat.provider)
                    .and_then(|p| keys.get(p.key_name()))
                    .unwrap_or_default()
                    .to_string();
                crate::council::config_for(seat, &key)
            })
            .collect()
    };

    let client = state.client.clone();

    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
        {
            Ok(r) => r,
            Err(e) => {
                let _ = app.emit(
                    "council:done",
                    CouncilDonePayload {
                        answered: 0,
                        failed: seats.len(),
                        dollars: 0.0,
                        unpriced: 0,
                    },
                );
                tracing::error!(%e, "council runtime could not be built");
                return;
            }
        };

        runtime.block_on(run_council(&app, &client, &task, &seats, &configs));
    });

    Ok(())
}

/// One seat's outcome.
struct SeatResult {
    text: String,
    dollars: Option<f64>,
}

/// Runs both waves, reporting as it goes.
async fn run_council(
    app: &tauri::AppHandle,
    client: &std::sync::Arc<vavis_brain::BrainClient>,
    task: &str,
    seats: &[crate::council::Seat],
    configs: &[Result<ChatConfig, String>],
) {
    let (independent, informed) = crate::council::plan(seats);

    let mut answered = 0usize;
    let mut failed = 0usize;
    let mut dollars = 0.0;
    let mut unpriced = 0usize;
    let mut prior: Vec<(String, String)> = Vec::new();

    for wave in [independent, informed] {
        if wave.is_empty() {
            continue;
        }

        // Every seat in the wave is spawned before any is awaited. Awaiting
        // them one at a time inside the loop would serialise the whole thing
        // and quietly turn this into four conversations in a row.
        let mut running = Vec::new();
        for index in wave {
            let seat = seats[index].clone();
            let config = match &configs[index] {
                Ok(config) => config.clone(),
                Err(message) => {
                    let _ = app.emit(
                        "council:seat-failed",
                        CouncilSeatFailedPayload {
                            seat: seat.id.clone(),
                            message: message.clone(),
                        },
                    );
                    failed += 1;
                    continue;
                }
            };

            let messages = crate::council::build_messages(task, &seat, &prior);
            let app = app.clone();
            let client = client.clone();

            running.push(tokio::spawn(async move {
                let started = std::time::Instant::now();
                let input_tokens: usize = messages
                    .iter()
                    .map(|m| vavis_brain::estimate_tokens(&m.content))
                    .sum();

                let seat_id = seat.id.clone();
                let stream = client
                    .chat_stream(&config, messages, |event| {
                        if let vavis_brain::StreamEvent::Delta(text) = event {
                            let _ = app.emit(
                                "council:delta",
                                CouncilDeltaPayload {
                                    seat: seat_id.clone(),
                                    text,
                                },
                            );
                        }
                    })
                    .await;

                match stream {
                    Ok(text) => {
                        let output_tokens = vavis_brain::estimate_tokens(&text);
                        let cost =
                            vavis_brain::estimate_cost(&config.model, input_tokens, output_tokens);
                        let _ = app.emit(
                            "council:seat-done",
                            CouncilSeatDonePayload {
                                seat: seat.id.clone(),
                                text: text.clone(),
                                label: crate::council::label(&seat),
                                input_tokens,
                                output_tokens,
                                dollars: cost,
                                elapsed_ms: started.elapsed().as_millis(),
                            },
                        );
                        (
                            seat,
                            Some(SeatResult {
                                text,
                                dollars: cost,
                            }),
                        )
                    }
                    Err(e) => {
                        // This seat is done; the others carry on. Having other
                        // answers is the entire reason for this interface.
                        let _ = app.emit(
                            "council:seat-failed",
                            CouncilSeatFailedPayload {
                                seat: seat.id.clone(),
                                message: e.to_string(),
                            },
                        );
                        (seat, None)
                    }
                }
            }));
        }

        for handle in running {
            match handle.await {
                Ok((seat, Some(result))) => {
                    answered += 1;
                    match result.dollars {
                        Some(cost) => dollars += cost,
                        None => unpriced += 1,
                    }
                    prior.push((crate::council::label(&seat), result.text));
                }
                Ok((_, None)) => failed += 1,
                // A panicking task must not take the run down with it.
                Err(e) => {
                    tracing::warn!(%e, "a council seat panicked");
                    failed += 1;
                }
            }
        }
    }

    let _ = app.emit(
        "council:done",
        CouncilDonePayload {
            answered,
            failed,
            dollars,
            unpriced,
        },
    );
}

/// Sends one seat's answer into the conversation, so a council can end
/// somewhere useful rather than in a panel nobody reads again.
#[tauri::command]
pub fn council_keep(state: State<AppState>, text: String) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("nothing to keep".into());
    }

    AppState::lock(&state.history).push(Message::assistant(text.clone()));
    AppState::lock(&state.store)
        .add_message("assistant", &text)
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Connection tests
// ---------------------------------------------------------------------------

/// The result of actually trying something.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTest {
    pub ok: bool,
    /// One line, fit to show next to the setting it tested.
    pub detail: String,
}

impl ConnectionTest {
    fn ok(detail: impl Into<String>) -> Self {
        Self {
            ok: true,
            detail: detail.into(),
        }
    }
    fn bad(detail: impl Into<String>) -> Self {
        Self {
            ok: false,
            detail: detail.into(),
        }
    }
}

/// Tries a target for real and reports what happened.
///
/// A real request, not a "is there a key" check: the point is that the user
/// finds out here rather than by starting a conversation and watching it fail.
///
/// Nothing here spends meaningful money. Image generation is deliberately
/// absent — a test that costs a few cents per press is not a test, and
/// pretending a key check is a connection test would be worse.
#[tauri::command]
pub fn test_connection(state: State<AppState>, target: String) -> ConnectionTest {
    match target.as_str() {
        "obsidian" => match vavis_tools::obsidian::current() {
            Some(vault) => match vault.scan() {
                Ok(notes) => {
                    ConnectionTest::ok(format!("{} notes in {}", notes.len(), vault.root.display()))
                }
                Err(e) => ConnectionTest::bad(e.to_string()),
            },
            None => ConnectionTest::bad("no vault selected"),
        },

        "search" => match vavis_tools::websearch::search("vavis connection test", 1) {
            Ok((response, _)) => ConnectionTest::ok(format!(
                "{} answered with {} result(s)",
                response.provider,
                response.hits.len()
            )),
            Err(attempts) if attempts.is_empty() => {
                ConnectionTest::bad("no provider is configured")
            }
            Err(attempts) => ConnectionTest::bad(
                attempts
                    .iter()
                    .map(|a| format!("{}: {}", a.provider, a.error))
                    .collect::<Vec<_>>()
                    .join(" · "),
            ),
        },

        "steam" => match vavis_tools::steam::library() {
            Ok(games) => ConnectionTest::ok(format!("{} games in the library", games.len())),
            Err(e) => ConnectionTest::bad(e.to_string()),
        },

        "spotify" => match vavis_tools::spotify::now_playing() {
            Ok(Some(now)) => ConnectionTest::ok(format!("playing {} — {}", now.track, now.artist)),
            // Connected and idle is a pass: the account answered.
            Ok(None) => ConnectionTest::ok("connected, nothing playing"),
            Err(e) => ConnectionTest::bad(e.to_string()),
        },

        "canvas" => {
            // Generating an image to prove a key works would charge the user
            // for a test, every time they pressed it.
            let image = vavis_tools::canvas::is_ready(vavis_tools::canvas::Kind::Image);
            let video = vavis_tools::canvas::is_ready(vavis_tools::canvas::Kind::Video);
            match (image, video) {
                (true, true) => ConnectionTest::ok("image and video providers configured"),
                (true, false) => ConnectionTest::ok("image ready · no video provider"),
                (false, true) => ConnectionTest::ok("video ready · no image provider"),
                (false, false) => ConnectionTest::bad("no generation key configured"),
            }
        }

        // Anything else is a chat provider id. Listing models is the cheapest
        // request that still proves the key is accepted.
        provider => {
            let Some(parsed) = Provider::parse(provider) else {
                return ConnectionTest::bad(format!("unknown target: {provider}"));
            };
            let key = AppState::lock(&state.keys)
                .get(parsed.key_name())
                .unwrap_or_default()
                .to_string();

            if parsed.needs_key() && key.is_empty() {
                return ConnectionTest::bad("no key stored");
            }

            let client = state.client.clone();
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(r) => r,
                Err(e) => return ConnectionTest::bad(e.to_string()),
            };

            match runtime.block_on(client.list_models(parsed, &key)) {
                Ok(models) => ConnectionTest::ok(format!("{} models available", models.len())),
                Err(e) => ConnectionTest::bad(e.to_string()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_error_event_names_its_fields_the_way_the_interface_reads_them() {
        // The interface reads `tooLong`. Rust writes `too_long` unless the
        // struct is told otherwise, and nothing warns when it is not: the
        // event goes out, the field arrives undefined, and the recovery
        // button silently never renders. That shipped once -- the flag was
        // added and the rename was not, so the fix was invisible in the
        // build that was supposed to carry it.
        let json = serde_json::to_value(ErrorPayload {
            message: "too big".into(),
            too_long: true,
        })
        .unwrap();

        assert_eq!(json["tooLong"], true, "serialised as {json}");
        assert!(
            json.get("too_long").is_none(),
            "the snake_case spelling must not be what goes out"
        );
    }

    fn user(text: &str) -> Message {
        Message::user(text)
    }

    /// An assistant turn that asked for a tool, and the result answering it.
    fn tool_pair(id: &str) -> [Message; 2] {
        [
            Message {
                role: vavis_brain::Role::Assistant,
                content: String::new(),
                tool_call_id: None,
                tool_calls: Some(Vec::new()),
                image: None,
            },
            Message {
                role: vavis_brain::Role::Tool,
                content: "result".into(),
                tool_call_id: Some(id.to_string()),
                tool_calls: None,
                image: None,
            },
        ]
    }

    #[test]
    fn shrinking_keeps_the_identity_and_the_question() {
        let mut messages = vec![Message::system("you are vavis")];
        for i in 0..8 {
            messages.push(user(&format!("old {i}")));
        }
        messages.push(user("THE QUESTION"));

        let dropped = drop_oldest_half(&mut messages);

        assert!(dropped > 0);
        assert_eq!(messages[0].role, vavis_brain::Role::System);
        assert_eq!(
            messages.last().unwrap().content,
            "THE QUESTION",
            "the turn being asked must survive -- dropping it answers \
             something the user never sent"
        );
    }

    #[test]
    fn shrinking_never_orphans_a_tool_result() {
        // Providers reject a tool result whose originating call is not in the
        // same request. A blind halving lands in the middle of such a pair
        // about half the time.
        let mut messages = vec![Message::system("you are vavis")];
        for i in 0..4 {
            messages.push(user(&format!("q{i}")));
            messages.extend(tool_pair(&format!("call{i}")));
        }
        messages.push(user("THE QUESTION"));

        drop_oldest_half(&mut messages);

        // Index 0 is the system message; the survivors start after it.
        assert!(
            messages[1].tool_call_id.is_none(),
            "the first surviving message answers a call that was cut"
        );
    }

    #[test]
    fn shrinking_a_conversation_with_nothing_spare_reports_nothing() {
        // system + question only: retrying would send an identical request.
        let mut messages = vec![Message::system("you are vavis"), user("hello")];
        assert_eq!(drop_oldest_half(&mut messages), 0);
        assert_eq!(messages.len(), 2, "nothing may be taken");
    }

    #[test]
    fn a_short_conversation_is_not_worth_trimming() {
        // Half of three is one, and dropping one message will not bring a
        // request under a window -- it would lose context for nothing. The
        // interface says so rather than offering a button that does nothing.
        for len in 0..4 {
            assert_eq!(how_many_to_forget(len), 0, "len {len}");
        }
    }

    #[test]
    fn trimming_halves_a_long_conversation() {
        assert_eq!(how_many_to_forget(4), 2);
        assert_eq!(how_many_to_forget(101), 50);
    }

    #[test]
    fn trimming_always_leaves_something_behind() {
        // The recent half is the part being talked about. Dropping all of it
        // would be a clear, which is the thing this exists to avoid.
        for len in 4..200 {
            assert!(how_many_to_forget(len) < len, "len {len}");
        }
    }

    #[test]
    fn a_per_minute_quota_is_not_mistaken_for_a_long_conversation() {
        // The bug this guards, in the words the provider actually used. Groq
        // sends this as 413, and it reads like a size problem -- so a
        // two-letter message on a freshly cleared conversation came back
        // saying the conversation had grown too large, offering to shorten
        // something that was already as short as it gets.
        let groq = "Request too large for model `qwen/qwen3-32b` in organization \
                    `org_x` service tier `on_demand` on tokens per minute (TPM): \
                    Limit 6000, Used 5980, Requested 90. Please try again in 1.2s.";

        assert!(is_quota_refusal(413, groq));
        assert!(
            !is_too_long(413, groq),
            "a per-minute quota is not a conversation that got too long"
        );

        // And the user is told the thing that actually helps.
        let err = vavis_brain::BrainError::Api {
            status: 413,
            body: groq.to_string(),
        };
        let text = friendly_error(&err);
        assert!(text.contains("too fast"), "said: {text}");
        assert!(
            !text.contains("grown past"),
            "must not blame the conversation: {text}"
        );
    }

    /// A request bigger than the whole per-minute allowance cannot be fixed
    /// by waiting, so it must not be described as sending too fast.
    ///
    /// Measured on 2026-09-16: 60012 tokens against an ITPM limit of 7000.
    /// The next minute grants 7000 again and the request still needs 60012 --
    /// the advice has to be "shorten it", the opposite of "wait".
    #[test]
    fn a_request_larger_than_the_whole_quota_is_not_a_pace_problem() {
        let groq = "Request too large for model `qwen/qwen3.8-27b` in organization \
                    `org_x` service tier `on_demand` on input tokens per minute \
                    (ITPM): Limit 7000, Requested 60012, please reduce your message \
                    size and try again.";

        assert!(quota_too_small_for_request(groq));
        let err = vavis_brain::BrainError::Api {
            status: 413,
            body: groq.to_string(),
        };
        let text = friendly_error(&err);
        assert!(text.contains("shorten"), "said: {text}");
        assert!(
            !text.contains("too fast"),
            "waiting cannot fix this: {text}"
        );

        // And nothing should sit waiting on it.
        assert_eq!(wait_before_retry(&err), None);
    }

    /// The ordinary pace refusal must keep its old advice.
    #[test]
    fn a_plain_rate_limit_still_says_wait() {
        let groq = "Rate limit reached for model in organization `org_x` on tokens \
                    per minute (TPM): Limit 6000, Used 5980, Requested 90. \
                    Please try again in 1.2s.";
        // Used + requested exceeds the limit, but the single request does not.
        assert!(!quota_too_small_for_request(groq));
        let err = vavis_brain::BrainError::Api {
            status: 429,
            body: groq.to_string(),
        };
        assert!(friendly_error(&err).contains("too fast"));
        assert_eq!(wait_before_retry(&err), Some(2));
    }

    /// "try again" with no delay after it must not read as a zero-second wait.
    #[test]
    fn a_refusal_that_names_no_delay_yields_none() {
        assert_eq!(
            retry_after_seconds("please reduce your message size and try again."),
            None
        );
    }

    /// A retired model names its successor, and that is what the user needs.
    ///
    /// Measured 2026-09-16: `gemini-2.5-flash` is still in Google's model
    /// list and still returns this when used.
    #[test]
    fn a_retired_model_tells_the_user_what_to_switch_to() {
        let body = "{\"error\":{\"code\":404,\"message\":\"This model \
                    models/gemini-2.5-flash is no longer available to new users. \
                    Please update your code to use models/gemini-3.6-flash for \
                    the latest features and improvements.\"}}";

        assert_eq!(replacement_model(body).as_deref(), Some("gemini-3.6-flash"));

        let err = vavis_brain::BrainError::Api {
            status: 404,
            body: body.to_string(),
        };
        let text = friendly_error(&err);
        assert!(text.contains("gemini-3.6-flash"), "said: {text}");
        assert!(text.contains("retired"), "said: {text}");
    }

    /// An ordinary 404 keeps its old wording.
    #[test]
    fn a_plain_missing_model_is_not_called_retired() {
        let err = vavis_brain::BrainError::Api {
            status: 404,
            body: "{\"error\":{\"message\":\"model not found\"}}".into(),
        };
        let text = friendly_error(&err);
        assert!(text.contains("not found"), "said: {text}");
        assert!(!text.contains("retired"), "said: {text}");
    }

    /// A notice with no replacement named must not invent one.
    #[test]
    fn a_retirement_without_a_named_successor_says_so_plainly() {
        let err = vavis_brain::BrainError::Api {
            status: 404,
            body: "this model is no longer available".into(),
        };
        let text = friendly_error(&err);
        assert!(text.contains("retired"), "said: {text}");
        assert!(text.contains("pick another"), "said: {text}");
    }

    /// Groq announces a context overflow without any of the words the size
    /// check looked for, and as a 400 rather than a 413.
    ///
    /// Measured against the live API on 2026-09-16 by sending 200k words:
    ///
    /// ```text
    /// 400 {"message": "Please reduce the length of the messages or completion.",
    ///      "param": "messages"}
    /// ```
    ///
    /// Matching nothing meant the turn died with a raw "Provider error 400"
    /// rather than trimming the history and retrying — the recovery path
    /// existed and simply never ran.
    #[test]
    fn groq_says_the_conversation_is_too_long_in_its_own_words() {
        let groq = "Please reduce the length of the messages or completion.";

        assert!(is_too_long(400, groq), "boyut reddi tanınmadı");
        assert!(
            !is_quota_refusal(400, groq),
            "bu bir hız sınırı değil, gerçekten uzun"
        );

        let err = vavis_brain::BrainError::Api {
            status: 400,
            body: groq.to_string(),
        };
        let text = friendly_error(&err);
        assert!(text.contains("grown past"), "said: {text}");
        assert!(
            !text.contains("Provider error"),
            "ham hata gösterildi: {text}"
        );
    }

    #[test]
    fn a_named_retry_delay_is_passed_on() {
        assert_eq!(retry_after_seconds("please try again in 1.2s"), Some(2));
        assert_eq!(retry_after_seconds("try again in 30s"), Some(30));
        // Never zero: "try again in 0s" as advice is worse than no advice.
        assert_eq!(retry_after_seconds("try again in 0.4s"), Some(1));
        assert_eq!(retry_after_seconds("no delay mentioned"), None);
    }

    /// Hangi reddedilmeler beklenerek çözülür, hangileri kullanıcıya söylenir.
    #[test]
    fn only_a_short_named_pace_limit_is_waited_out() {
        let api = |status: u16, body: &str| vavis_brain::BrainError::Api {
            status,
            body: body.to_string(),
        };

        // Süre söylenmiş ve kısa — beklenir.
        assert_eq!(
            wait_before_retry(&api(429, "rate limit, please try again in 12s")),
            Some(12)
        );

        // Süre söylenmemiş: tahmin yürütmek yerine kullanıcıya söyle.
        assert_eq!(wait_before_retry(&api(429, "rate limit exceeded")), None);

        // Günlük kota kılığındaki uzun bekleme — kullanıcı spinner izlemesin.
        assert_eq!(
            wait_before_retry(&api(429, "quota, try again in 3600s")),
            None
        );

        // Hız sorunu değil: beklemek bunu çözmez.
        assert_eq!(
            wait_before_retry(&api(413, "context length exceeded")),
            None
        );
        assert_eq!(wait_before_retry(&api(401, "bad key")), None);
        assert_eq!(
            wait_before_retry(&vavis_brain::BrainError::MissingKey {
                provider: Provider::Groq
            }),
            None
        );
    }

    #[test]
    fn a_size_refusal_is_recognised_however_it_is_worded() {
        // Providers disagree: some 413, some 400 with an explanation.
        assert!(is_too_long(413, ""));
        assert!(is_too_long(400, "prompt is too long"));
        assert!(is_too_long(400, r#"{"code":"context_length_exceeded"}"#));
        assert!(is_too_long(
            400,
            "This model's maximum context length is 128000"
        ));
        assert!(is_too_long(413, "Request Entity Too Large"));

        // And things that are not a size problem must not offer the fix,
        // since shortening the conversation would not help.
        assert!(!is_too_long(401, "invalid api key"));
        assert!(!is_too_long(429, "rate limit exceeded"));
        assert!(!is_too_long(500, "internal error"));
    }

    #[test]
    fn a_summary_fits_on_one_line() {
        let messy = "read notes.md\n\n  1,204 bytes\ttext/markdown";
        assert_eq!(
            one_line(messy, 120),
            "read notes.md 1,204 bytes text/markdown"
        );
    }

    #[test]
    fn a_long_summary_is_cut_and_says_so() {
        let long = "x".repeat(500);
        let out = one_line(&long, 120);
        assert_eq!(out.chars().count(), 121);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn short_output_is_left_alone() {
        assert_eq!(clip("hello", 120), "hello");
        // Exactly at the limit is not truncated — an off-by-one here would
        // add an ellipsis to output that was complete.
        assert_eq!(clip("hello", 5), "hello");
    }

    #[test]
    fn clipping_counts_characters_not_bytes() {
        // Turkish tool output is normal here; cutting at a byte boundary
        // would produce broken text rather than shorter text.
        let text = "çğıöşü".repeat(10);
        let out = clip(&text, 6);
        assert_eq!(out, "çğıöşü…");
    }

    #[test]
    fn empty_output_stays_empty() {
        assert_eq!(one_line("", 120), "");
        assert_eq!(clip("", 120), "");
    }
}

/// Just the microphone level.
///
/// Its own command because the meter wants ten readings a second and
/// `get_status` does far too much to be asked that often — CPU sampling,
/// battery, three database counts. This reads one atomic.
#[tauri::command]
pub fn mic_level(state: State<AppState>) -> f32 {
    AppState::lock(&state.voice).mic_level()
}

// ---------------------------------------------------------------------------
// Window mode
// ---------------------------------------------------------------------------

/// Puts the window into `mode`.
///
/// # Why this is not two lines in the frontend
///
/// It used to be, and startup had its own copy — which is how the two drifted
/// apart. The frontend called `maximize()` and the window stayed the size it
/// was, because of two things `maximize()` alone does not handle:
///
/// * **Leaving fullscreen is not instant.** Asking to maximize in the same
///   breath as `set_fullscreen(false)` races the window manager, and the
///   maximize is the one that loses.
/// * **An undecorated window maximizes oddly.** With the OS title bar off,
///   "maximized" is not reliably the whole work area, which is what the user
///   means by borderless.
///
/// So borderless is applied by measuring the monitor and setting the size
/// directly. One function, called from startup and from the F11 toggle.
pub fn apply_window_mode(
    window: &tauri::WebviewWindow,
    mode: vavis_core::WindowMode,
) -> Result<(), String> {
    use vavis_core::WindowMode;

    // The interface draws its own title strip, so the OS decorations stay off
    // in every mode — turning them on gives two title bars.
    let was_fullscreen = window.is_fullscreen().unwrap_or(false);
    if was_fullscreen && mode != WindowMode::Fullscreen {
        window.set_fullscreen(false).map_err(|e| e.to_string())?;
    }

    match mode {
        WindowMode::Fullscreen => window.set_fullscreen(true).map_err(|e| e.to_string())?,

        WindowMode::Borderless => {
            // Unmaximize first: a window that is already maximized ignores a
            // set_size, and would keep whatever bounds it had.
            let _ = window.unmaximize();

            match window.current_monitor() {
                Ok(Some(monitor)) => {
                    let size = *monitor.size();
                    let position = *monitor.position();
                    window.set_position(position).map_err(|e| e.to_string())?;
                    window.set_size(size).map_err(|e| e.to_string())?;
                }
                // No monitor information — maximize is the honest fallback,
                // and is what this always did before.
                _ => window.maximize().map_err(|e| e.to_string())?,
            }
        }

        WindowMode::Windowed => {
            let _ = window.unmaximize();
        }
    }

    Ok(())
}

/// Sets the window mode and saves it.
///
/// Replaces the frontend doing the move itself: the two paths had drifted, and
/// only one of them worked.
#[tauri::command]
pub fn set_window_mode(
    state: State<AppState>,
    window: tauri::WebviewWindow,
    mode: String,
) -> Result<(), String> {
    if !["windowed", "borderless", "fullscreen"].contains(&mode.as_str()) {
        return Err("window mode: windowed, borderless or fullscreen".into());
    }

    apply_window_mode(&window, vavis_core::WindowMode::parse(&mode))?;

    let mut core = AppState::lock(&state.core);
    core.config.ui.window_mode = mode;
    core.config.save(&core.paths).map_err(|e| e.to_string())
}
