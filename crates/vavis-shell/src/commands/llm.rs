//! Which provider and model answer, and the commands that change them.

use super::*;

/// The model to send, given what is saved.
///
/// Filtering the picker is not enough on its own: the saved value outlives
/// the list it was chosen from. A model that was offered yesterday, picked,
/// and retired or reclassified today is still sitting in `vavis.toml`, and
/// every request keeps going to it.
///
/// That is exactly what happened. `gemini-2.5-flash-native-audio-latest` was
/// selected while the picker still offered it, and afterwards every single
/// message came back:
///
/// ```text
/// 400  only supports real-time bidirectional streaming via WebSocket
///      (bidiGenerateContent). Please use the Gemini Live API
/// ```
///
/// Cleaning the list did nothing for it, because nothing re-reads the list.
/// So the check belongs here, where the value is used: a saved model the
/// provider filter rejects is treated as no choice at all, and the provider's
/// default answers instead. A working assistant on the wrong model beats a
/// broken one on the right name.
pub(super) fn model_for(config: &vavis_core::Config, provider: Provider) -> String {
    let saved = config.llm.model.trim();
    // Names come back from Google qualified; the filter works on bare ones.
    let bare = saved.trim_start_matches("models/");

    if bare.is_empty() {
        return provider.default_model().to_string();
    }
    if !vavis_brain::provider::is_useful_model(provider, bare) {
        tracing::warn!(
            model = saved,
            default = provider.default_model(),
            "saved model is one we would not offer; using the default"
        );
        return provider.default_model().to_string();
    }
    // Stored qualified, sent bare: the path builder adds the prefix back, and
    // adding it twice is a 404.
    bare.to_string()
}

/// Which model a turn goes to, given what kind of work it is.
///
/// Chat and code want different things from a model. Chat wants an answer
/// back before the thought is gone; code wants the answer to be right, and
/// will wait. With one slot the user had to pick a loser: either chat runs
/// on a model priced for refactors, or code runs on one that cannot do them.
///
/// So code gets its own slot, and the slot is optional. Empty -- the default,
/// and what every existing config has -- means chat's choice answers for
/// both, so nobody who has not asked for this sees any change at all.
///
/// The retired-model guard in `model_for` applies to whichever slot wins:
/// a stale name in the code slot fails exactly the way a stale name in the
/// chat slot does, and is caught the same way.
///
/// `has_key` answers whether a provider is actually usable, so a code
/// provider whose key was never added falls back to chat instead of failing
/// every code turn. Picking a provider and not getting round to the key is
/// an ordinary half-finished setup, not a reason to break the app.
pub(super) fn llm_for(
    config: &vavis_core::Config,
    code: bool,
    has_key: impl Fn(Provider) -> bool,
) -> (Provider, String) {
    if code {
        let configured = config.llm.code_provider.trim();
        if !configured.is_empty() {
            if let Some(provider) = Provider::parse(configured) {
                if provider.needs_key() && !has_key(provider) {
                    tracing::warn!(
                        provider = configured,
                        "code provider has no key; using the chat provider"
                    );
                    let chat = Provider::parse(&config.llm.provider).unwrap_or(Provider::Groq);
                    return (chat, model_for(config, chat));
                }
                let saved = config.llm.code_model.trim();
                let bare = saved.trim_start_matches("models/");
                // Same two checks `model_for` makes, against the code slot:
                // unset, or a name the provider filter would not offer.
                let model =
                    if bare.is_empty() || !vavis_brain::provider::is_useful_model(provider, bare) {
                        if !bare.is_empty() {
                            tracing::warn!(
                                model = saved,
                                "saved code model is one we would not offer; using the default"
                            );
                        }
                        provider.default_model().to_string()
                    } else {
                        bare.to_string()
                    };
                return (provider, model);
            }
            // A provider name we do not recognise is a config that was hand
            // edited or written by an older build. Falling back to chat is
            // better than refusing the turn.
            tracing::warn!(
                provider = configured,
                "unknown code provider; using the chat provider"
            );
        }
    }

    let provider = Provider::parse(&config.llm.provider).unwrap_or(Provider::Groq);
    (provider, model_for(config, provider))
}

/// The request configuration for one provider: its key, and its URL when
/// the user chose one.
///
/// Every place that builds a request goes through here, so a custom or
/// relocated local endpoint is honoured by chat, council and the model list
/// alike rather than by whichever of them remembered to look.
pub(crate) fn chat_config(
    config: &vavis_core::Config,
    keys: &vavis_brain::KeyStore,
    provider: Provider,
    model: String,
) -> ChatConfig {
    let key = keys
        .get(provider.key_name())
        .unwrap_or_default()
        .to_string();
    let mut cfg = ChatConfig::new(provider, model, key);
    cfg.url_override = endpoint_for(config, provider);
    cfg
}

/// The chat URL a user set for a provider, if any.
pub(crate) fn endpoint_for(config: &vavis_core::Config, provider: Provider) -> Option<String> {
    let url = match provider {
        Provider::Custom => config.llm.custom_url.trim(),
        Provider::Local => config.llm.local_url.trim(),
        _ => "",
    };
    (!url.is_empty()).then(|| vavis_brain::chat_url_from_user(url))
}

/// Whether a provider could answer right now: it has what it needs to
/// send a request.
pub(crate) fn is_usable(
    config: &vavis_core::Config,
    keys: &vavis_brain::KeyStore,
    provider: Provider,
) -> bool {
    match provider {
        Provider::Custom => !config.llm.custom_url.trim().is_empty(),
        p if p.needs_key() => keys.get(p.key_name()).is_some(),
        _ => true,
    }
}

/// The failover chain after `first`: configured providers that are usable,
/// each on its default model, in the order the user put them.
pub(crate) fn fallbacks_for(
    config: &vavis_core::Config,
    keys: &vavis_brain::KeyStore,
    first: Provider,
) -> Vec<ChatConfig> {
    let mut seen = vec![first];
    let mut out = Vec::new();
    for name in &config.llm.fallback {
        let Some(p) = Provider::parse(name) else {
            continue;
        };
        if seen.contains(&p) || !is_usable(config, keys, p) {
            continue;
        }
        seen.push(p);
        out.push(chat_config(config, keys, p, p.default_model().to_string()));
    }
    out
}

/// Sets the failover chain.
#[tauri::command]
pub fn set_fallback(state: State<AppState>, providers: Vec<String>) -> Result<(), String> {
    let mut clean = Vec::new();
    for name in providers {
        let Some(p) = Provider::parse(&name) else {
            return Err(format!("unknown provider: {name}"));
        };
        let key = p.key_name().to_string();
        if !clean.contains(&key) {
            clean.push(key);
        }
    }
    let mut core = AppState::lock(&state.core);
    core.config.llm.fallback = clean;
    core.config.save(&core.paths).map_err(|e| e.to_string())
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

/// Picks the provider for code work, or clears it.
///
/// An empty string is a supported answer, not an error: it means "use the
/// chat model for code too", which is the default and what most setups want.
#[tauri::command]
pub fn set_code_provider(state: State<AppState>, provider: String) -> Result<String, String> {
    let mut core = AppState::lock(&state.core);

    if provider.trim().is_empty() {
        core.config.llm.code_provider = String::new();
        core.config.llm.code_model = String::new();
        core.config.save(&core.paths).map_err(|e| e.to_string())?;
        return Ok(String::new());
    }

    let Some(provider) = Provider::parse(&provider) else {
        return Err(format!("unknown provider: {provider}"));
    };

    core.config.llm.code_provider = provider.key_name().to_string();
    // Models are provider-specific, so the old name would 404 -- same reason
    // `set_provider` resets it.
    core.config.llm.code_model = provider.default_model().to_string();
    core.config.save(&core.paths).map_err(|e| e.to_string())?;

    Ok(provider.default_model().to_string())
}

#[tauri::command]
pub fn set_code_model(state: State<AppState>, model: String) -> Result<(), String> {
    let mut core = AppState::lock(&state.core);
    core.config.llm.code_model = model;
    core.config.save(&core.paths).map_err(|e| e.to_string())
}

/// The code provider's live model list.
///
/// Separate from `list_models` because it has to ask a different provider
/// with a different key; reusing that one would list the chat provider's
/// models under the code picker.
#[tauri::command]
pub async fn list_code_models(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let (provider, key, url, client) = {
        let core = AppState::lock(&state.core);
        let keys = AppState::lock(&state.keys);
        let configured = core.config.llm.code_provider.trim();
        if configured.is_empty() {
            return Ok(Vec::new());
        }
        let Some(provider) = Provider::parse(configured) else {
            return Err(format!("unknown provider: {configured}"));
        };
        (
            provider,
            keys.get(provider.key_name())
                .unwrap_or_default()
                .to_string(),
            endpoint_for(&core.config, provider),
            state.client.clone(),
        )
    };

    client
        .list_models_at(provider, &key, url.as_deref())
        .await
        .map_err(|e| friendly_error(&e))
}

/// Fetches the provider's live model list.
#[tauri::command]
pub async fn list_models(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let (provider, key, url, client) = {
        let core = AppState::lock(&state.core);
        let keys = AppState::lock(&state.keys);
        let provider = Provider::parse(&core.config.llm.provider).unwrap_or(Provider::Groq);
        (
            provider,
            keys.get(provider.key_name())
                .unwrap_or_default()
                .to_string(),
            endpoint_for(&core.config, provider),
            state.client.clone(),
        )
    };

    client
        .list_models_at(provider, &key, url.as_deref())
        .await
        .map_err(|e| friendly_error(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A saved model outlives the list it was chosen from.
    ///
    /// This is the bug the user actually hit. The picker was cleaned, but
    /// `gemini-2.5-flash-native-audio-latest` was already in vavis.toml from
    /// before, so every message still went to it and came back 400. Nothing
    /// re-reads the list, so filtering the list could never have fixed it.
    #[test]
    fn a_saved_model_we_would_not_offer_falls_back_to_the_default() {
        for dead in [
            "models/gemini-2.5-flash-native-audio-latest",
            "gemini-2.5-flash-native-audio-latest",
            "gemini-3.8-live",
            // Retired but still listed by Google.
            "gemini-2.5-flash",
        ] {
            let mut c = vavis_core::Config::default();
            c.llm.provider = "gemini".into();
            c.llm.model = dead.into();
            assert_eq!(
                model_for(&c, Provider::Gemini),
                Provider::Gemini.default_model(),
                "{dead} hâlâ gönderiliyor"
            );
        }
    }

    /// A model that is fine must be left exactly as it is.
    #[test]
    fn a_working_saved_model_is_used_untouched() {
        let mut c = vavis_core::Config::default();
        c.llm.provider = "gemini".into();
        c.llm.model = "gemini-3.5-flash".into();
        assert_eq!(model_for(&c, Provider::Gemini), "gemini-3.5-flash");
    }

    /// Google publishes names with a `models/` prefix and the settings screen
    /// can store one that way. The path builder adds the prefix itself, so
    /// sending it again is a 404.
    #[test]
    fn a_qualified_name_is_sent_bare() {
        let mut c = vavis_core::Config::default();
        c.llm.provider = "gemini".into();
        c.llm.model = "models/gemini-3.6-flash".into();
        assert_eq!(model_for(&c, Provider::Gemini), "gemini-3.6-flash");
    }

    #[test]
    fn no_saved_model_means_the_providers_default() {
        let mut c = vavis_core::Config::default();
        c.llm.provider = "groq".into();
        c.llm.model = "   ".into();
        assert_eq!(
            model_for(&c, Provider::Groq),
            Provider::Groq.default_model()
        );
    }

    /// The guard must not reach past the provider whose rules it knows: a
    /// local Ollama model has no list to be judged against.
    #[test]
    fn a_local_model_is_never_second_guessed() {
        let mut c = vavis_core::Config::default();
        c.llm.provider = "local".into();
        c.llm.model = "my-own-finetune".into();
        assert_eq!(model_for(&c, Provider::Local), "my-own-finetune");
    }

    // ── The code slot ───────────────────────────────────────────────
    //
    // The whole point of the slot is that it is optional, so the cases that
    // matter most are the ones where it is unset or half set: every one of
    // those has to land back on the chat model rather than break the turn.

    /// Every config written before the code slot existed has it empty, and
    /// those users must see no change whatsoever.
    #[test]
    fn code_falls_back_to_chat_when_no_code_provider_is_set() {
        let mut c = vavis_core::Config::default();
        c.llm.provider = "groq".into();
        c.llm.model = String::new();

        let chat = llm_for(&c, false, |_| true);
        let code = llm_for(&c, true, |_| true);
        assert_eq!(chat, code, "an unset code slot must change nothing");
        assert_eq!(code.0, Provider::Groq);
    }

    /// The ordinary case: a code provider is set and has a key.
    #[test]
    fn code_uses_its_own_provider_and_model() {
        let mut c = vavis_core::Config::default();
        c.llm.provider = "groq".into();
        c.llm.code_provider = "local".into();
        c.llm.code_model = "my-own-finetune".into();

        assert_eq!(llm_for(&c, false, |_| true).0, Provider::Groq);
        assert_eq!(
            llm_for(&c, true, |_| true),
            (Provider::Local, "my-own-finetune".to_string()),
            "code work goes to the code slot"
        );
    }

    /// Picking a provider and not getting round to its key is an ordinary
    /// half-finished setup. It must not make every code turn fail.
    #[test]
    fn a_code_provider_with_no_key_falls_back_instead_of_failing() {
        let mut c = vavis_core::Config::default();
        c.llm.provider = "groq".into();
        c.llm.code_provider = "openai".into();
        c.llm.code_model = "gpt-4o".into();

        // No key stored for anything.
        let (provider, _) = llm_for(&c, true, |_| false);
        assert_eq!(
            provider,
            Provider::Groq,
            "without a key the code provider cannot answer, so chat does"
        );
    }

    /// A provider that needs no key is usable the moment it is picked.
    #[test]
    fn a_keyless_code_provider_needs_no_key_to_be_chosen() {
        let mut c = vavis_core::Config::default();
        c.llm.provider = "groq".into();
        c.llm.code_provider = "local".into();

        let (provider, model) = llm_for(&c, true, |_| false);
        assert_eq!(provider, Provider::Local);
        assert_eq!(
            model,
            Provider::Local.default_model(),
            "an empty code model means the provider's default"
        );
    }

    /// The retired-model guard has to cover the code slot too, or the exact
    /// bug it was written for comes back through the other door.
    #[test]
    fn a_retired_code_model_is_replaced_by_the_default() {
        let mut c = vavis_core::Config::default();
        c.llm.provider = "groq".into();
        c.llm.code_provider = "gemini".into();
        c.llm.code_model = "gemini-2.5-flash-native-audio-latest".into();

        let (provider, model) = llm_for(&c, true, |_| true);
        assert_eq!(provider, Provider::Gemini);
        assert_eq!(
            model,
            Provider::Gemini.default_model(),
            "a model we would not offer must not be sent"
        );
    }

    /// Google hands names back qualified; the path builder adds the prefix,
    /// so sending it qualified is a 404. Same rule, code slot.
    #[test]
    fn a_qualified_code_model_is_sent_bare() {
        let mut c = vavis_core::Config::default();
        c.llm.code_provider = "gemini".into();
        c.llm.code_model = "models/gemini-3.6-flash".into();

        assert_eq!(llm_for(&c, true, |_| true).1, "gemini-3.6-flash");
    }

    /// A hand-edited or older config can name a provider we do not know.
    /// Refusing the turn over it would be worse than answering on chat.
    #[test]
    fn an_unknown_code_provider_falls_back_to_chat() {
        let mut c = vavis_core::Config::default();
        c.llm.provider = "groq".into();
        c.llm.code_provider = "some-provider-we-dropped".into();

        assert_eq!(llm_for(&c, true, |_| true).0, Provider::Groq);
    }
}
