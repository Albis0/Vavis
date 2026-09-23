//! Reading provider refusals: what went wrong, whether waiting helps, and
//! what to tell the user.

use super::*;

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
pub(super) fn wait_before_retry(err: &vavis_brain::BrainError) -> Option<u64> {
    let vavis_brain::BrainError::Api { status, body } = err else {
        return None;
    };
    if !is_quota_refusal(*status, body) {
        return None;
    }
    retry_after_seconds(body).filter(|secs| *secs <= MAX_RATE_LIMIT_WAIT_SECS)
}

/// Whether this error is one the user can fix by shortening the conversation.
pub(super) fn error_is_too_long(err: &vavis_brain::BrainError) -> bool {
    matches!(
        err,
        vavis_brain::BrainError::Api { status, body } if is_too_long(*status, body)
    )
}

/// Turns a provider error into something the user can act on.
pub(super) fn friendly_error(err: &vavis_brain::BrainError) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
