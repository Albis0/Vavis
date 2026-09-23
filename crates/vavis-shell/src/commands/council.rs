//! Several models on one question.

use super::*;

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
