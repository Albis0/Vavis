//! The conversation: sending a message, running a turn, approvals, history.

use super::*;

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
    let conversation = *AppState::lock(&state.conversation);
    restore(&state, conversation)
}

/// Loads a conversation's tail into the model's history and returns it for
/// the interface to draw.
pub(super) fn restore(state: &AppState, conversation: i64) -> Vec<StoredLine> {
    const RESTORE: usize = 20;

    let stored = AppState::lock(&state.store).messages_in(conversation, RESTORE);
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
///
/// `code` says whether this turn is code work, so it can go to the code
/// model when one is set. The interface decides, because it is the only side
/// that knows which pane the message was typed in -- a folder being open is
/// not the same question, and routing on it would send an unrelated aside to
/// the code model. Omitted means chat, which is what every turn was before.
#[tauri::command]
pub fn send_message(
    app: tauri::AppHandle,
    state: State<AppState>,
    text: String,
    code: Option<bool>,
) -> Result<(), String> {
    let code = code.unwrap_or(false);
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("empty message".into());
    }

    if !state.try_claim() {
        return Err("a reply is already in progress".into());
    }

    let conversation = *AppState::lock(&state.conversation);
    let (chain, router_model, full_authority, mut identity, history, plan) = {
        let core = AppState::lock(&state.core);
        let keys = AppState::lock(&state.keys);

        let chain = match build_chain(&core.config, &keys, code) {
            Ok(chain) => chain,
            Err(e) => {
                state.release();
                return Err(e);
            }
        };

        // Rebuilt each turn so a settings change takes effect on the very
        // next message.
        let identity = Identity {
            name: core.config.general.assistant_name.clone(),
            language: core.config.general.language.clone(),
            memory: String::new(),
        };
        let plan = MemoryPlan {
            inject: core.config.memory.inject,
            extract: core.config.memory.auto_extract,
            embed: crate::recall::embed_config(&core.config, &keys),
        };

        let mut history = AppState::lock(&state.history);
        history.push(Message::user(text.clone()));

        (
            chain,
            core.config.llm.router_model.clone(),
            core.config.security.full_authority,
            identity,
            history.clone(),
            plan,
        )
    };

    // Claude Code reaches Vavis's tools over MCP; the server that serves
    // them starts the first time it is needed and stays up after.
    let bridge = if chain
        .iter()
        .any(|c| c.provider == vavis_brain::Provider::ClaudeCode)
    {
        match state.claude_bridge(&app) {
            Ok(b) => Some(b),
            Err(e) => {
                tracing::warn!(%e, "could not start the tool bridge; Claude Code runs without tools");
                None
            }
        }
    } else {
        None
    };

    // Persist the user's turn before the request goes out — if the app
    // dies mid-reply the question is still on record.
    if let Err(e) = AppState::lock(&state.store).add_message_to(conversation, "user", &text) {
        tracing::warn!(%e, "could not persist message");
    }

    let client = state.client.clone();
    let agent = state.agent.clone();
    let busy = state.busy.clone();
    let approval_rx = state.approval_rx.clone();
    let voice = state.voice.clone();
    let store = state.store.clone();
    let history_handle = state.history.clone();
    let current = state.conversation.clone();

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

        // What the assistant knows that bears on this message goes into
        // the prompt. Looked up once here, not per provider: the answer
        // does not depend on who is asked.
        if plan.inject {
            identity.memory = runtime.block_on(crate::recall::relevant_block(
                &client,
                &store,
                plan.embed.as_ref(),
                &text,
            ));
        }

        let turn = Turn {
            app: &app,
            client: &client,
            agent: &agent,
            voice: &voice,
            approval_rx: &approval_rx,
            full_authority,
            identity: &identity,
            bridge: bridge.as_deref(),
            quiet: false,
        };
        let result =
            runtime.block_on(turn.run_with_failover(&chain, &router_model, history, &text));

        let mut learn_from: Option<String> = None;
        match result {
            Ok(reply) => {
                if !reply.trim().is_empty() {
                    // The reply belongs to the conversation it was asked in.
                    // Switching is refused while a reply runs, so this only
                    // guards against that rule changing.
                    if *AppState::lock(&current) == conversation {
                        AppState::lock(&history_handle).push(Message::assistant(reply.clone()));
                    }
                    if let Err(e) =
                        AppState::lock(&store).add_message_to(conversation, "assistant", &reply)
                    {
                        tracing::warn!(%e, "could not persist reply");
                    }
                    learn_from = Some(reply.clone());
                    // Most of the answer has already been spoken as it
                    // streamed; this is the last, unfinished sentence.
                    AppState::lock(&voice).finish_stream();
                }
                let _ = app.emit("chat:done", DonePayload { text: reply });
            }
            Err(TurnError {
                message, too_long, ..
            }) => {
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

        // After the turn, and after the user has their input back: nothing
        // below is worth making them wait for.
        if let Some(embed) = plan.embed.as_ref() {
            runtime.block_on(crate::recall::backfill(&client, &store, embed));
        }
        if let (true, Some(reply), Some(primary)) = (plan.extract, learn_from, chain.first()) {
            if crate::recall::worth_extracting(&text) {
                let learned = runtime.block_on(crate::recall::extract(
                    &client,
                    primary,
                    &store,
                    plan.embed.as_ref(),
                    &text,
                    &reply,
                ));
                if !learned.is_empty() {
                    let _ = app.emit("memory:learned", LearnedPayload { facts: learned });
                }
            }
        }
    });

    Ok(())
}

/// Runs one turn asked from outside the window (the phone) and returns the
/// answer. Blocks; call it from its own thread.
///
/// Shares the busy flag with the window, so the two never interleave.
/// Approvals go to `asker` -- and always go there: full authority is off
/// for a remote turn whatever the setting, because it is a statement about
/// the person at this keyboard.
pub(crate) fn run_remote(
    app: &tauri::AppHandle,
    history: &std::sync::Arc<std::sync::Mutex<Vec<Message>>>,
    text: &str,
    asker: Asker,
) -> Result<String, String> {
    let state = app.state::<AppState>();
    if !state.try_claim() {
        return Err("busy".into());
    }
    let prepared = (|| {
        let core = AppState::lock(&state.core);
        let keys = AppState::lock(&state.keys);
        let chain = build_chain(&core.config, &keys, false)?;
        let identity = Identity {
            name: core.config.general.assistant_name.clone(),
            language: core.config.general.language.clone(),
            memory: String::new(),
        };
        let embed = core
            .config
            .memory
            .inject
            .then(|| crate::recall::embed_config(&core.config, &keys))
            .flatten();
        let inject = core.config.memory.inject;
        Ok::<_, String>((chain, identity, embed, inject))
    })();
    let (chain, mut identity, embed, inject) = match prepared {
        Ok(p) => p,
        Err(e) => {
            state.release();
            return Err(e);
        }
    };

    let bridge = if chain
        .iter()
        .any(|c| c.provider == vavis_brain::Provider::ClaudeCode)
    {
        state.claude_bridge(app).ok()
    } else {
        None
    };

    let past = {
        let mut h = AppState::lock(history);
        h.push(Message::user(text));
        // A phone conversation stays short; the window's has its own budget.
        let excess = h.len().saturating_sub(30);
        h.drain(..excess);
        h.clone()
    };

    let result = (|| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        if inject {
            identity.memory = runtime.block_on(crate::recall::relevant_block(
                &state.client,
                &state.store,
                embed.as_ref(),
                text,
            ));
        }
        let _route = RemoteRoute::install(asker);
        let turn = Turn {
            app,
            client: &state.client,
            agent: &state.agent,
            voice: &state.voice,
            approval_rx: &state.approval_rx,
            full_authority: false,
            identity: &identity,
            bridge: bridge.as_deref(),
            quiet: true,
        };
        runtime
            .block_on(turn.run_with_failover(&chain, "", past, text))
            .map_err(|e| e.message)
    })();

    {
        let mut h = AppState::lock(history);
        match &result {
            Ok(reply) if !reply.trim().is_empty() => h.push(Message::assistant(reply.clone())),
            // Drop the unanswered question, as the window does.
            _ => {
                if h.last().map(|m| m.role) == Some(vavis_brain::Role::User) {
                    h.pop();
                }
            }
        }
    }
    state.release();
    result
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct LearnedPayload {
    facts: Vec<String>,
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
pub(super) const MAX_RATE_LIMIT_WAIT_SECS: u64 = 60;

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
    /// Another provider could take this turn over from the start: the
    /// failure came on the very first request, before anything was shown or
    /// any tool ran. Past that point a retry elsewhere would repeat work the
    /// user already watched happen -- a file written twice, a message sent
    /// twice -- so the error stands.
    elsewhere: bool,
}

impl From<&vavis_brain::BrainError> for TurnError {
    fn from(err: &vavis_brain::BrainError) -> Self {
        Self {
            message: friendly_error(err),
            too_long: error_is_too_long(err),
            elsewhere: false,
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
            elsewhere: false,
        }
    }
}

/// The providers a turn may use, chosen first: the configured one if it is
/// usable, then the failover chain.
fn build_chain(
    config: &vavis_core::Config,
    keys: &vavis_brain::KeyStore,
    code: bool,
) -> Result<Vec<ChatConfig>, String> {
    let (provider, model) = llm_for(config, code, |p| is_usable(config, keys, p));
    let first = chat_config(config, keys, provider, model);
    let fallbacks = fallbacks_for(config, keys, provider);

    // The chosen provider cannot answer, but a fallback can: start there
    // rather than refusing the message outright.
    let mut chain = Vec::new();
    if is_usable(config, keys, provider) {
        chain.push(first);
    }
    chain.extend(fallbacks);
    if chain.is_empty() {
        return Err(if provider == vavis_brain::Provider::Custom {
            "the custom provider has no URL — set one in settings".to_string()
        } else {
            format!("no API key for {provider}")
        });
    }
    Ok(chain)
}

/// Who the assistant is: rebuilt into a system prompt for whichever
/// provider ends up answering, since the prompt differs between them.
struct Identity {
    name: String,
    language: String,
    /// What the assistant remembers that bears on this message, already
    /// formatted for the prompt. Empty when nothing does.
    memory: String,
}

impl Identity {
    fn system_for(&self, provider: Provider) -> Message {
        let mut prompt = vavis_brain::system_prompt_for(provider, &self.name, &self.language);
        prompt.push_str(&self.memory);
        Message::system(prompt)
    }
}

/// Memory settings for one turn, read once at send time.
struct MemoryPlan {
    inject: bool,
    extract: bool,
    embed: Option<vavis_brain::embeddings::EmbedConfig>,
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

/// Tool runs since the app started, across every turn and path. A turn
/// compares it before and after to know whether anything has actually
/// happened on the machine -- the bridge runs tools on its own threads, so
/// counting them where they start is the one place that sees them all.
static TOOL_RUNS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Everything a turn needs that does not change between providers.
struct Turn<'a> {
    app: &'a tauri::AppHandle,
    client: &'a vavis_brain::BrainClient,
    agent: &'a std::sync::Arc<std::sync::Mutex<vavis_tools::Agent>>,
    // Speech starts on the first finished sentence rather than on the whole
    // answer, so the voice keeps up with the text instead of trailing it.
    voice: &'a std::sync::Arc<std::sync::Mutex<crate::voice::VoiceState>>,
    approval_rx: &'a std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<Approval>>>,
    full_authority: bool,
    identity: &'a Identity,
    /// Where Claude Code finds Vavis's tools. `None` for every other
    /// provider, and for Claude Code if the server could not start.
    bridge: Option<&'a vavis_tools::mcp::bridge::Bridge>,
    /// A turn asked from elsewhere (the phone): nothing is drawn in the
    /// window or spoken aloud, since nobody there asked.
    quiet: bool,
}

impl Turn<'_> {
    /// Runs the turn on the first provider in `chain`, moving to the next
    /// when one cannot take it at all.
    ///
    /// Only a failure on the very first request moves on (see
    /// [`TurnError::elsewhere`]). The user is told each time, because an
    /// answer from a different model than the one they picked should never
    /// arrive unannounced.
    async fn run_with_failover(
        &self,
        chain: &[ChatConfig],
        router_model: &str,
        history: Vec<Message>,
        user_message: &str,
    ) -> Result<String, TurnError> {
        let mut last = TurnError::from("no provider could answer".to_string());
        for (i, cfg) in chain.iter().enumerate() {
            // The router belongs to the provider it was set up for: its model
            // name means nothing to the others.
            let router = if i == 0 { router_model } else { "" };
            match self.run(cfg, router, history.clone(), user_message).await {
                Ok(text) => return Ok(text),
                Err(e) if e.elsewhere && i + 1 < chain.len() => {
                    let next = &chain[i + 1];
                    tracing::warn!(
                        from = %cfg.provider,
                        to = %next.provider,
                        reason = %e.message,
                        "provider failed; moving to the next in the chain"
                    );
                    let _ = (!self.quiet).then(|| {
                        self.app.emit(
                            "chat:notice",
                            NoticePayload {
                                text: format!(
                                    "{} could not answer ({}) — trying {}.",
                                    cfg.provider,
                                    e.message.trim_end_matches('.'),
                                    next.provider
                                ),
                            },
                        )
                    });
                    last = e;
                }
                Err(e) => return Err(e),
            }
        }
        Err(last)
    }

    /// One full turn on one provider: model → tools → model → … → reply.
    async fn run(
        &self,
        cfg: &ChatConfig,
        router_model: &str,
        history: Vec<Message>,
        user_message: &str,
    ) -> Result<String, TurnError> {
        let Turn {
            app,
            client,
            agent,
            voice,
            approval_rx,
            full_authority,
            quiet,
            ..
        } = *self;
        let claude = cfg.provider == vavis_brain::Provider::ClaudeCode;

        // Claude Code gets its tools over MCP, so the config has to say
        // where; nothing else changes about the request.
        let bridged;
        let cfg = match (claude, self.bridge) {
            (true, Some(bridge)) => {
                let mut c = cfg.clone();
                c.tool_bridge = Some(vavis_brain::claude_code::ToolBridge {
                    url: bridge.url(),
                    token: bridge.token().to_string(),
                });
                bridged = c;
                &bridged
            }
            _ => cfg,
        };
        // Open only for this turn: a call arriving at any other time is
        // refused before it reaches the agent.
        let _open = self.bridge.filter(|_| claude).map(|b| b.open());

        let tool_runs_before = TOOL_RUNS.load(Ordering::SeqCst);
        let streamed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        // Nothing seen, nothing done: another provider may start over.
        let untouched = |step: usize| {
            step == 0
                && !streamed.load(Ordering::SeqCst)
                && TOOL_RUNS.load(Ordering::SeqCst) == tool_runs_before
        };
        let fail = |e: &vavis_brain::BrainError, step: usize| TurnError {
            elsewhere: untouched(step) && !error_is_too_long(e),
            ..TurnError::from(e)
        };

        // How many tools this model can choose between well -- a small model
        // loses its way among thirty schemas where a large one does not. A
        // ceiling, not a target: domain matching still decides what is relevant.
        let budget = vavis_brain::ModelCaps::for_model(&cfg.model).tool_budget;

        let picked: Vec<String> = if claude {
            // Claude handles the whole catalogue well, and it runs its own
            // loop -- there is no second step at which to hand it more. So it
            // gets everything, minus what it already has in a better form:
            // its own web search, and `request_tools`, which only makes sense
            // for a model that was given a subset.
            let guard = AppState::lock(agent);
            guard
                .registry
                .iter()
                .map(|t| t.name().to_string())
                .filter(|n| n != "request_tools" && n != "web_search")
                .collect()
        } else {
            // A cheap model picks what is needed; the expensive one does the
            // work. Configured off, in which case this is keyword matching.
            let picked = route_tools(client, cfg, router_model, agent, user_message, budget).await;

            // Some models bring their own web search, run on the provider's
            // servers. Offering ours alongside it opens two doors onto the
            // same job, and the model cannot tell which one the user can
            // actually see the results of. Theirs needs no key of ours, so it
            // wins; ours is dropped for this turn.
            if vavis_brain::builtin::covers_web_search(cfg.provider, &cfg.model) {
                picked.into_iter().filter(|n| n != "web_search").collect()
            } else {
                picked
            }
        };

        // Mutable because the model can ask for more mid-turn -- see the
        // `request_tools` handling further down. The starting set is what the
        // router (or the keyword table) chose from the message alone.
        let mut offered: Vec<String> = picked.clone();
        let mut tools = {
            let names: Vec<&str> = picked.iter().map(String::as_str).collect();
            let mut guard = AppState::lock(agent);
            guard.start_run();
            // Read fresh each turn, so switching it in settings takes effect
            // on the next message rather than the next launch.
            guard.gate.set_full_authority(full_authority);
            vavis_tools::builtin::request_tools::reset();
            guard.schemas_for(&names)
        };
        tracing::info!(
            count = tools.len(),
            budget,
            model = %cfg.model,
            provider = %cfg.provider,
            "tools offered for this request"
        );

        let mut messages = vec![self.identity.system_for(cfg.provider)];
        messages.extend(history);
        let mut final_text = String::new();
        // Whether history has already been cut back after a size refusal.
        // Once only, so a provider that refuses for some other reason it
        // happens to describe as "too long" cannot walk the conversation down
        // to nothing.
        let mut shrunk = false;
        // How many times a rate limit has been waited out this turn. Separate
        // from `shrunk` because the two failures are unrelated and a turn can
        // hit both.
        let mut rate_limit_waits = 0u8;

        // A fresh turn: drop any half-sentence left buffered by an abandoned
        // one.
        if !quiet {
            AppState::lock(voice).begin_stream();
        }

        for step in 0..MAX_STEPS {
            let emit = app.clone();

            let response = match client
                .chat_stream_with_tools(cfg, messages.clone(), &tools, {
                    let emit = emit.clone();
                    let voice = voice.clone();
                    let streamed = streamed.clone();
                    move |event| {
                        if let StreamEvent::Delta(text) = event {
                            streamed.store(true, Ordering::SeqCst);
                            if quiet {
                                return;
                            }
                            // Speak first, then paint: synthesis has to start
                            // as early as possible, and emitting is the cheap
                            // half.
                            AppState::lock(&voice).push_stream(&text);
                            let _ = emit.emit("chat:delta", DeltaPayload { text });
                        }
                    }
                })
                .await
            {
                Ok(response) => response,

                // The provider says the request is too big even though the
                // budget module thought it would fit. That happens because the
                // window is read from a table of model names, and the same
                // name is served with different limits by different providers
                // -- so the number can be wrong, and being wrong costs the
                // user their turn.
                //
                // Rather than trust the table, shrink and ask again. Half the
                // conversation goes, oldest first, and the question itself is
                // never touched. One attempt only: if half was not enough, the
                // size is coming from something a second halving will not
                // fix, and the interface offers the same thing as a button by
                // then.
                Err(e) if error_is_too_long(&e) && !shrunk => {
                    shrunk = true;

                    let kept = drop_oldest_half(&mut messages);
                    if kept == 0 {
                        return Err(fail(&e, step));
                    }

                    tracing::info!(
                        dropped = kept,
                        "provider refused for size; retrying with less history"
                    );
                    continue;
                }

                // The free tiers refuse on a per-minute budget, and a turn
                // that calls two or three tools spends that budget in a few
                // seconds. The provider names the wait it wants; honouring it
                // turns a dead turn into a slow one.
                //
                // Bounded, and only when a wait was actually named: an
                // unbounded retry against a daily quota would hang the turn
                // until the user gave up, and the message they get instead
                // ("try again in ...") is at least true.
                Err(e) if rate_limit_waits < MAX_RATE_LIMIT_WAITS => {
                    let Some(secs) = wait_before_retry(&e) else {
                        return Err(fail(&e, step));
                    };

                    rate_limit_waits += 1;
                    tracing::info!(secs, attempt = rate_limit_waits, "rate limited; waiting");

                    // Said out loud: several seconds of silence with no
                    // explanation reads as the app having hung.
                    if !quiet {
                        let _ = app.emit(
                            "chat:notice",
                            NoticePayload {
                                text: format!("Rate limited — waiting {secs}s."),
                            },
                        );
                    }

                    tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
                    continue;
                }

                Err(e) => return Err(fail(&e, step)),
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
                    let names =
                        vavis_tools::selection::select_named(&guard.registry, &need, budget);
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
}

/// Asks for approval somewhere other than the window: the phone.
pub(crate) type Asker =
    std::sync::Arc<dyn Fn(&str, &str, ApprovalReason) -> Approval + Send + Sync>;

/// Where approvals go while a remote turn runs. One turn runs at a time
/// (the busy flag), so one slot is enough -- and a slot rather than a
/// field on each host, because Claude Code's tool calls arrive through the
/// MCP bridge's own host, which must route the same way.
static REMOTE: std::sync::Mutex<Option<Asker>> = std::sync::Mutex::new(None);

fn remote_asker() -> Option<Asker> {
    REMOTE.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Sends approvals to `asker` until dropped.
pub(crate) struct RemoteRoute;

impl RemoteRoute {
    pub(crate) fn install(asker: Asker) -> Self {
        *REMOTE.lock().unwrap_or_else(|e| e.into_inner()) = Some(asker);
        Self
    }
}

impl Drop for RemoteRoute {
    fn drop(&mut self) {
        *REMOTE.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

/// Bridges the agent to the interface: emits events, waits for approvals.
pub(crate) struct EventHost {
    pub app: tauri::AppHandle,
    pub approval_rx: std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<Approval>>>,
}

impl AgentHost for EventHost {
    fn ask_approval(&mut self, tool: &str, args: &str, reason: ApprovalReason) -> Approval {
        if let Some(asker) = remote_asker() {
            return asker(tool, args, reason);
        }
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
        TOOL_RUNS.fetch_add(1, Ordering::SeqCst);
        if remote_asker().is_some() {
            return;
        }
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
        if remote_asker().is_some() {
            return;
        }
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
    if state.busy.load(Ordering::SeqCst) {
        return Err("a reply is in progress".into());
    }
    let conversation = *AppState::lock(&state.conversation);
    AppState::lock(&state.history).clear();
    AppState::lock(&state.store)
        .clear_conversation(conversation)
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

/// What Claude Code's tool calls run through.
///
/// The same `Agent::execute_calls` every other provider's calls go through,
/// with the same host: the permission gate asks in the same place, the
/// feed shows the call in the same way, the loop guard and the destructive
/// budget count it the same. The only difference is who is asking.
pub(crate) struct ToolBridgeHandler {
    pub agent: std::sync::Arc<std::sync::Mutex<vavis_tools::Agent>>,
    pub app: tauri::AppHandle,
    pub approval_rx: std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<Approval>>>,
}

impl vavis_tools::mcp::bridge::Handler for ToolBridgeHandler {
    fn tools(&self) -> Vec<serde_json::Value> {
        let guard = AppState::lock(&self.agent);
        let names: Vec<&str> = guard
            .registry
            .iter()
            .map(|t| t.name())
            .filter(|n| *n != "request_tools" && *n != "web_search")
            .collect();
        guard.schemas_for(&names)
    }

    fn call(
        &self,
        id: &str,
        name: &str,
        arguments: serde_json::Value,
    ) -> vavis_tools::mcp::bridge::CallResult {
        let call = vavis_brain::ToolCall {
            id: id.to_string(),
            kind: "function".into(),
            function: vavis_brain::FunctionCall {
                name: name.to_string(),
                arguments: arguments.to_string(),
            },
            provider_state: None,
        };
        let mut host = EventHost {
            app: self.app.clone(),
            approval_rx: self.approval_rx.clone(),
        };
        let results = {
            let mut guard = AppState::lock(&self.agent);
            guard.execute_calls(std::slice::from_ref(&call), &mut host)
        };
        let text = results
            .into_iter()
            .next()
            .map(|m| m.content)
            .unwrap_or_default();
        vavis_tools::mcp::bridge::CallResult {
            // The agent prefixes failures this way for every provider.
            is_error: text.starts_with("HATA:") || text == "Kullanıcı bu işlemi reddetti.",
            text,
            image: vavis_tools::builtin::vision::take_pending_image(),
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
