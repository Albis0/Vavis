//! "Test" buttons: a real request against each configured service.

use super::*;

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
///
/// Async so the window stays responsive while it waits: a sync command runs
/// on the interface thread, and a test that takes seconds froze the whole
/// window for those seconds.
#[tauri::command]
pub async fn test_connection(
    state: State<'_, AppState>,
    target: String,
) -> Result<ConnectionTest, String> {
    Ok(test(&state, &target).await)
}

async fn test(state: &AppState, target: &str) -> ConnectionTest {
    match target {
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
            let (key, url) = {
                let core = AppState::lock(&state.core);
                let keys = AppState::lock(&state.keys);
                (
                    keys.get(parsed.key_name()).unwrap_or_default().to_string(),
                    super::llm::endpoint_for(&core.config, parsed),
                )
            };

            if parsed.needs_key() && key.is_empty() {
                return ConnectionTest::bad("no key stored");
            }

            let client = state.client.clone();

            // Claude Code has no model list to ask for: being installed says
            // nothing about being logged in, and only a real answer proves
            // both. One word from the smallest model costs next to nothing
            // against a subscription.
            if parsed == Provider::ClaudeCode {
                return async {
                    let version = match vavis_brain::claude_code::version().await {
                        Ok(v) => v,
                        Err(e) => return ConnectionTest::bad(super::friendly_error(&e)),
                    };
                    let cfg = ChatConfig::new(Provider::ClaudeCode, "haiku", "");
                    let reply = client
                        .chat_stream(
                            &cfg,
                            vec![Message::user("Reply with the single word: ready")],
                            |_| {},
                        )
                        .await;
                    match reply {
                        Ok(text) => ConnectionTest::ok(format!(
                            "{version} · logged in · answered \"{}\"",
                            text.trim().chars().take(20).collect::<String>()
                        )),
                        Err(e) => ConnectionTest::bad(super::friendly_error(&e)),
                    }
                }
                .await;
            }

            match client.list_models_at(parsed, &key, url.as_deref()).await {
                Ok(models) => ConnectionTest::ok(format!("{} models available", models.len())),
                Err(e) => ConnectionTest::bad(super::friendly_error(&e)),
            }
        }
    }
}
