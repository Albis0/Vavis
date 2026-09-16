//! Bağlam bütçesi — istek modelin penceresine sığdırılır.
//!
//! Eski projedeki 413 ("message too long") hatasının kök nedeni: geçmiş
//! pencereye sığdırılıyordu ama **tool şemaları sayılmıyordu** (64 şema ≈ 8-15k
//! token). Sistem + tool'lar tek başına pencereyi doldurunca istek reddediliyordu.
//!
//! Buradaki kural: **her şey sayılır.** Sığmıyorsa sırayla küçültülür:
//!   1. en alakasız tool'lar atılır (tool'lar alaka sırasında gelir),
//!   2. geçmiş eskiden yeniye budanır,
//!   3. son çare: sadece son kullanıcı mesajı.
//!
//! Sığmayacağı bilinen istek **asla gönderilmez.**

use crate::message::{Message, Role};

/// Kaba token tahmini. Gerçek tokenizer yerine karakter/4 — sağlayıcılar arası
/// farklıdır, o yüzden zaten güvenlik payı bırakıyoruz.
///
/// Türkçe karakterler UTF-8'de 2 bayt ama ~1 token, bu yüzden bayt değil
/// **karakter** sayıyoruz (eski projede bayt sayılıyordu, Türkçe metinde
/// bütçeyi olduğundan büyük gösteriyordu).
pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count() / 4 + 1
}

/// Pencerenin doldurulacak en fazla oranı.
const SAFETY_FACTOR: f64 = 0.9;
/// Modelin kendi ek yükü için ayrılan pay.
const RESERVE: usize = 512;

/// Reply limit for a model that is not in the table below.
///
/// This used to be 1024 for *every* model, table entry or not, which is about
/// 700 words: long answers were cut off mid-sentence, and a code block that
/// ran past it simply stopped in the middle of a line with nothing to say it
/// had been truncated. Providers stop at this number silently, so the bug
/// looked like the model losing its train of thought.
///
/// 8192 is a length no ordinary answer reaches, and it costs nothing when it
/// is not used -- output is billed per token produced, not per token allowed.
const DEFAULT_MAX_OUTPUT: usize = 8_192;

/// Assumed context window for a model that is not in the table.
///
/// Deliberately below what any current model offers, because underestimating
/// costs some trimmed history while overestimating costs a 413 and a lost
/// turn. It is not *tiny*, though: this was 8192, and since the reply limit
/// is held to a quarter of the window, that quietly capped unknown models at
/// a 2048-token answer -- the same truncation this constant's neighbour
/// exists to prevent. Anything a user is likely to have configured by hand
/// (a local model, a new hosted one) is at least 32k.
const UNKNOWN_WINDOW: usize = 32_768;

/// Tanınmayan bir modele kaç tool sunulur.
///
/// Temkinli: model bilinmiyorsa seçim kalitesi de bilinmiyor demektir.
/// Tabloya bir satır eklemek bunu istediğin yere çeker.
const DEFAULT_TOOL_BUDGET: usize = 12;

/// Bütçe bunun altına inemez.
///
/// Çekirdek tool'lar (saat, hesap) her istekte sunuluyor; bütçe onların
/// altına inerse alan tool'una hiç yer kalmaz ve asistan alan işi yapamaz
/// hâle gelir.
const MIN_TOOL_BUDGET: usize = 6;

#[derive(Debug, Clone, Copy)]
pub struct ModelCaps {
    /// Modelin bağlam penceresi (token).
    pub context_window: usize,
    /// Cevap için ayrılacak en fazla token.
    pub max_output: usize,
    /// Bu modele bir istekte sunulabilecek en fazla tool sayısı.
    ///
    /// **Tavan, hedef değil.** Bütçenin yüksek olması "hepsini gönder"
    /// demek değil; sadece gerekenler gider. Fazlası hem faturayı şişirir
    /// hem modeli kışkırtır.
    ///
    /// Neden model başına: seçim kalitesi modele göre değişiyor. Küçük bir
    /// Llama otuz şema arasında kaybolurken Opus rahatça seçiyor. Tek bir
    /// sabit, zayıf modelin sınırını güçlü olana da dayatıyordu.
    pub tool_budget: usize,
}

impl Default for ModelCaps {
    fn default() -> Self {
        // A conservative window for an unknown model, but a reply length that
        // is still usable: see `DEFAULT_MAX_OUTPUT`. `new` caps the reply
        // against the window, which for a window this small is what binds.
        Self::new(UNKNOWN_WINDOW, DEFAULT_MAX_OUTPUT)
    }
}

impl ModelCaps {
    /// Window and reply limit from the model name. Safe defaults when unknown.
    ///
    /// A table, so a new family is one line. Order matters -- first match
    /// wins, so specific names come before general ones.
    pub fn for_model(model: &str) -> Self {
        /// (fragment of the model name, context window, max reply tokens,
        /// tool budget)
        ///
        /// The tool budget is a judgement about how many choices the model
        /// handles well, not about its context window: a small model with a
        /// large window still picks badly from thirty options.
        const MODELS: &[(&str, usize, usize, usize)] = &[
            ("gemini", 1_000_000, 8_192, 32),
            ("claude-3-haiku", 200_000, 4_096, 16),
            ("claude", 200_000, 8_192, 48),
            ("gpt-4o", 128_000, 16_384, 32),
            ("gpt-4.1", 128_000, 16_384, 32),
            ("gpt-5", 128_000, 16_384, 48),
            ("o1", 128_000, 16_384, 32),
            // Measured against Groq's own /models endpoint, 2026-09-16.
            // gpt-oss reports a 65536 reply ceiling, but `new` holds the reply
            // to a quarter of the window anyway, so the table states the real
            // figure and lets that rule bind.
            ("gpt-oss", 131_072, 65_536, 24),
            ("qwen", 131_042, 16_384, 16),
            ("kimi", 128_000, 8_192, 16),
            // Groq's own agentic systems. They refuse our schemas outright
            // (see `crate::builtin`), so the tool budget is moot -- but the
            // window is real and used for trimming history.
            ("compound", 131_072, 8_192, 6),
            // Guard/classifier models, not chat models: a 512-token window.
            // Ahead of the general llama row, which would otherwise hand them
            // a 32000 window and let every request overflow.
            ("prompt-guard", 512, 512, 6),
            ("llama-guard", 512, 512, 6),
            ("llama-3.3", 128_000, 8_192, 14),
            ("llama-3.1", 128_000, 8_192, 14),
            ("llama", 32_000, 4_096, 12),
            // Speech and audio models reached through the chat table only by
            // accident; a tiny window keeps a mistaken pick from sending a
            // request that cannot be answered.
            ("whisper", 448, 448, 6),
            ("orpheus", 4_000, 4_096, 6),
            ("allam", 4_096, 4_096, 8),
            ("grok", 131_072, 8_192, 24),
            ("deepseek", 64_000, 8_192, 16),
            ("mistral", 32_000, 4_096, 12),
            ("gemma", 8_192, 4_096, 8),
        ];

        let m = model.to_ascii_lowercase();
        let (context_window, max_output, tool_budget) = MODELS
            .iter()
            .find(|(name, _, _, _)| m.contains(name))
            .map(|(_, window, output, tools)| (*window, *output, *tools))
            .unwrap_or((UNKNOWN_WINDOW, DEFAULT_MAX_OUTPUT, DEFAULT_TOOL_BUDGET));

        Self::with_tools(context_window, max_output, tool_budget)
    }

    /// Caps to a window, holding the reply limit to something the window can
    /// actually accommodate.
    ///
    /// A reply limit near the window size starves the input: `input_budget`
    /// subtracts it, so 8192-of-8192 leaves nothing and every request arrives
    /// with its history stripped. A quarter of the window is the ceiling --
    /// generous for the reply, and it still leaves roughly two thirds for the
    /// conversation after the safety margin.
    fn new(context_window: usize, max_output: usize) -> Self {
        Self::with_tools(context_window, max_output, DEFAULT_TOOL_BUDGET)
    }

    /// As [`Self::new`], with an explicit tool budget.
    fn with_tools(context_window: usize, max_output: usize, tool_budget: usize) -> Self {
        Self {
            context_window,
            max_output: max_output.min(context_window / 4),
            tool_budget: tool_budget.max(MIN_TOOL_BUDGET),
        }
    }

    /// Girdi için gerçekten kullanılabilir token sayısı.
    pub fn input_budget(&self) -> usize {
        let usable = (self.context_window as f64 * SAFETY_FACTOR) as usize;
        usable
            .saturating_sub(self.max_output)
            .saturating_sub(RESERVE)
    }
}

/// Indicative price per million tokens, as (input, output) in US dollars.
///
/// Running four agents on one question costs four times as much, and the
/// council interface has to say so before the user presses go — a quota burnt
/// without warning is the worst outcome that screen can produce.
///
/// **These prices go stale.** They are published figures, not a billing feed,
/// so everything derived from them is labelled an estimate and an unknown
/// model returns `None` rather than a confident wrong number.
const PRICES: &[(&str, f64, f64)] = &[
    ("claude-opus", 15.0, 75.0),
    ("claude-sonnet", 3.0, 15.0),
    ("claude-haiku", 0.80, 4.0),
    ("gpt-5", 1.25, 10.0),
    ("gpt-4o-mini", 0.15, 0.60),
    ("gpt-4o", 2.50, 10.0),
    ("gpt-4.1", 2.0, 8.0),
    ("gemini-2.5-pro", 1.25, 10.0),
    ("gemini-2.5-flash", 0.30, 2.50),
    ("gemini", 0.30, 2.50),
    ("deepseek", 0.28, 0.42),
    ("grok", 3.0, 15.0),
    ("mistral", 0.40, 2.0),
    ("llama", 0.20, 0.20),
];

/// Roughly what a turn cost, in US dollars.
///
/// `None` for a model with no published price here — including every local
/// model, where the answer is genuinely zero but the interface should say
/// "local" rather than "$0.00" as though it had looked it up.
pub fn estimate_cost(model: &str, input_tokens: usize, output_tokens: usize) -> Option<f64> {
    let m = model.to_ascii_lowercase();
    let (_, input, output) = PRICES.iter().find(|(name, _, _)| m.contains(name))?;
    Some((input_tokens as f64 * input + output_tokens as f64 * output) / 1_000_000.0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct FitResult {
    pub messages: Vec<Message>,
    /// The tools that survived, in the order they were offered.
    ///
    /// The caller must send *these* rather than the list it passed in. When
    /// this is shorter than that list, the difference is the whole point:
    /// leaving the full set on the request is what produced the 413 this
    /// module exists to prevent, and schemas are the largest fixed cost in it.
    pub tools: Vec<serde_json::Value>,
    /// How many tools were dropped to make the request fit.
    pub tools_dropped: usize,
    /// Kaç geçmiş mesajı atıldı.
    pub history_dropped: usize,
    /// Gönderilen tahmini girdi token'ı.
    pub est_tokens: usize,
}

/// Bir görüntünün yaklaşık token maliyeti.
///
/// Sağlayıcılar görüntüyü karo karo tokenleştirir, base64 uzunluğuyla
/// orantılı değil. ~1100 token tipik bir ekran görüntüsü için makul bir
/// üst tahmin (eski projedeki değerle aynı).
///
/// **Sayılmazsa ne olur:** 2 MB'lık bir PNG base64'te ~2.7 milyon karakter
/// eder; karakter/4 saysaydık bütçe anında patlar, sığdırma tüm geçmişi
/// atardı. Sabit maliyet doğru davranış.
const IMAGE_TOKENS: usize = 1_100;

fn message_tokens(m: &Message) -> usize {
    // +4: rol ve ayraç token'ları için sabit pay.
    let mut tokens = estimate_tokens(&m.content) + 4;
    if m.image.is_some() {
        tokens += IMAGE_TOKENS;
    }
    tokens
}

/// The fewest tools worth sending.
///
/// Dropping schemas beats dropping the conversation, but a model with no
/// tools at all is a different assistant: it cannot read a file or run
/// anything, and it does not announce that -- it answers as though it had
/// tried. Tools arrive most-relevant first, so the leading few are the ones
/// the request is most likely to need. If even these will not fit, the system
/// prompt is the problem and trimming further would not save the request.
const MIN_TOOLS: usize = 4;

/// İstek gövdesini bütçeye sığdırır.
///
/// `messages[0]` sistem mesajıysa **asla atılmaz** — kimliği taşır.
/// Son kullanıcı mesajı da asla atılmaz — isteğin kendisi odur.
///
/// Tools are trimmed before history, because a schema is worth less than a
/// turn: the model copes with a smaller toolbox, but it cannot answer a
/// question whose context has been thrown away.
pub fn fit_request(
    messages: Vec<Message>,
    tools: Vec<serde_json::Value>,
    caps: ModelCaps,
) -> FitResult {
    let budget = caps.input_budget();

    // Sistem mesajını ayır (varsa).
    let (system, rest): (Vec<_>, Vec<_>) =
        messages.into_iter().partition(|m| m.role == Role::System);

    let system_tokens: usize = system.iter().map(message_tokens).sum();

    // What the conversation may claim before tools start being dropped.
    // Without a floor, one long paste would strip the toolbox to `MIN_TOOLS`
    // and the next question would be answered with no way to act on it.
    let history_floor = budget.saturating_sub(system_tokens) / 3;

    let mut costs: Vec<usize> = tools
        .iter()
        .map(|t| estimate_tokens(&t.to_string()))
        .collect();
    let mut tool_tokens: usize = costs.iter().sum();
    let offered = tools.len();

    // Drop the least relevant tools -- from the end, since they arrive most
    // relevant first -- until the fixed cost leaves the conversation its
    // floor.
    //
    // This is the part that used to be a `tools_dropped = 1` marker nobody
    // read. The count was reported while the *full* schema list still went
    // out on the request, so the 413 this module was written to prevent
    // happened anyway, and the trimming it did instead came out of history --
    // the expensive place to take it from.
    let mut kept_tools = tools;
    while kept_tools.len() > MIN_TOOLS && system_tokens + tool_tokens + history_floor > budget {
        kept_tools.pop();
        tool_tokens -= costs.pop().unwrap_or(0);
    }
    let tools_dropped = offered - kept_tools.len();

    let fixed = system_tokens + tool_tokens;
    let mut available = budget.saturating_sub(fixed);

    // Geçmişi YENİDEN ESKİYE doldur — en yeni mesajlar en değerlisi.
    let mut kept: Vec<Message> = Vec::new();
    let mut history_dropped = 0;
    let total = rest.len();

    for (idx, m) in rest.into_iter().enumerate().rev() {
        let cost = message_tokens(&m);
        if cost <= available {
            available -= cost;
            kept.push(m);
        } else if idx == total - 1 {
            // Son mesaj (kullanıcının isteği) her hâlükârda kalır — gerekirse kırpılır.
            let mut m = m;
            let max_chars = available.saturating_mul(4);
            if max_chars < m.content.chars().count() {
                m.content = m.content.chars().take(max_chars).collect();
            }
            kept.push(m);
            available = 0;
        } else {
            history_dropped += 1;
        }
    }

    kept.reverse();

    let mut out = system;
    out.extend(kept);

    let est_tokens: usize = out.iter().map(message_tokens).sum::<usize>() + tool_tokens;
    FitResult {
        messages: out,
        tools: kept_tools,
        tools_dropped,
        history_dropped,
        est_tokens,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_scales_with_both_directions() {
        // One million in, one million out, at Sonnet's published rate.
        let cost = estimate_cost("claude-sonnet-4-5", 1_000_000, 1_000_000).unwrap();
        assert!((cost - 18.0).abs() < 0.01, "{cost}");
    }

    #[test]
    fn a_more_specific_model_wins_over_a_general_one() {
        let mini = estimate_cost("gpt-4o-mini", 1_000_000, 0).unwrap();
        let full = estimate_cost("gpt-4o", 1_000_000, 0).unwrap();
        // Listed before "gpt-4o", so the cheap model is not priced as the
        // expensive one — the mistake that would matter most here.
        assert!(mini < full, "mini {mini} should be under full {full}");
    }

    #[test]
    fn an_unpriced_model_reports_nothing_rather_than_zero() {
        // A local model genuinely costs nothing, but the interface should say
        // "local", not print a confident "$0.00" it never looked up.
        assert_eq!(estimate_cost("my-local-mistrial-thing", 1000, 1000), None);
        assert_eq!(estimate_cost("", 1000, 1000), None);
    }

    #[test]
    fn model_names_are_matched_regardless_of_case() {
        assert!(estimate_cost("Claude-Opus-4-1", 1000, 1000).is_some());
    }

    fn msg(role: Role, len: usize) -> Message {
        let content = "a".repeat(len);
        match role {
            Role::System => Message::system(content),
            Role::User => Message::user(content),
            _ => Message::assistant(content),
        }
    }

    fn no_tools() -> Vec<serde_json::Value> {
        Vec::new()
    }

    /// `count` schemas of roughly `each` tokens apiece.
    fn tools(count: usize, each: usize) -> Vec<serde_json::Value> {
        (0..count)
            .map(|i| serde_json::json!({ "name": format!("t{i}"), "schema": "x".repeat(each * 4) }))
            .collect()
    }

    #[test]
    fn short_conversation_passes_through_untouched() {
        let msgs = vec![Message::system("sen vavis'sin"), Message::user("selam")];
        let r = fit_request(msgs.clone(), no_tools(), ModelCaps::for_model("gpt-4o"));
        assert_eq!(r.messages, msgs);
        assert_eq!(r.history_dropped, 0);
    }

    #[test]
    fn system_message_is_never_dropped() {
        let mut msgs = vec![msg(Role::System, 200)];
        for _ in 0..200 {
            msgs.push(msg(Role::User, 400));
        }
        let r = fit_request(msgs, no_tools(), ModelCaps::default());
        assert_eq!(r.messages[0].role, Role::System);
        assert!(r.history_dropped > 0, "bir şeyler atılmalıydı");
    }

    #[test]
    fn last_user_message_always_survives() {
        let mut msgs = vec![msg(Role::System, 100)];
        for _ in 0..100 {
            msgs.push(msg(Role::Assistant, 500));
        }
        msgs.push(Message::user("BU KALMALI"));

        let r = fit_request(msgs, no_tools(), ModelCaps::default());
        let last = r.messages.last().unwrap();
        assert_eq!(last.role, Role::User);
        assert!(last.content.starts_with("BU KALMALI"));
    }

    #[test]
    fn result_fits_within_budget() {
        let caps = ModelCaps::default();
        let mut msgs = vec![msg(Role::System, 500)];
        for _ in 0..300 {
            msgs.push(msg(Role::User, 300));
        }
        let r = fit_request(msgs, no_tools(), caps);
        assert!(
            r.est_tokens <= caps.input_budget(),
            "bütçe aşıldı: {} > {}",
            r.est_tokens,
            caps.input_budget()
        );
    }

    #[test]
    fn tool_tokens_are_counted_against_budget() {
        // Asıl 413 bugu buydu: tool'lar sayılmıyordu.
        let caps = ModelCaps::default();
        let msgs = vec![msg(Role::System, 100), msg(Role::User, 2000)];

        let without = fit_request(msgs.clone(), no_tools(), caps);
        let with = fit_request(msgs, tools(8, caps.input_budget() / 16), caps);

        assert!(
            with.est_tokens > without.est_tokens,
            "tool token'ları toplama katılmalı"
        );
    }

    #[test]
    fn tools_that_do_not_fit_are_actually_dropped() {
        // The second half of the 413 bug. Counting the schemas was only ever
        // half a fix: the count was reported, the *full* list still went on
        // the request, and the trimming came out of history instead.
        let caps = ModelCaps::default();
        let budget = caps.input_budget();
        let msgs = vec![msg(Role::System, 100), msg(Role::User, 200)];

        // Sixteen schemas at a sixth of the budget each: wildly over.
        let offered = tools(16, budget / 6);
        let r = fit_request(msgs, offered.clone(), caps);

        assert!(r.tools_dropped > 0, "sığmayan tool'lar atılmalıydı");
        assert_eq!(
            r.tools.len(),
            offered.len() - r.tools_dropped,
            "the returned list has to match the reported count"
        );
        assert!(
            r.est_tokens <= budget,
            "bütçe aşıldı: {} > {budget}",
            r.est_tokens
        );
    }

    #[test]
    fn a_few_tools_always_survive() {
        // A model with an empty toolbox is a different assistant: it cannot
        // act and does not say so. Better to trim history than to strip it.
        let caps = ModelCaps::default();
        let msgs = vec![msg(Role::System, 100), msg(Role::User, 200)];

        let r = fit_request(msgs, tools(40, caps.input_budget()), caps);
        assert_eq!(r.tools.len(), MIN_TOOLS);
    }

    #[test]
    fn tools_survive_a_long_conversation() {
        // History is trimmed before the toolbox is: an assistant that has
        // forgotten the start of the conversation is still useful, one that
        // cannot run anything is not.
        let caps = ModelCaps::default();
        let mut msgs = vec![msg(Role::System, 100)];
        for _ in 0..300 {
            msgs.push(msg(Role::User, 400));
        }

        let offered = tools(8, 200);
        let r = fit_request(msgs, offered.clone(), caps);

        assert_eq!(r.tools.len(), offered.len(), "tool'lar korunmalıydı");
        assert!(r.history_dropped > 0, "geçmiş budanmalıydı");
    }

    #[test]
    fn turkish_text_is_not_overcounted() {
        // Türkçe karakterler 2 bayt ama ~1 token — bayt sayarsak bütçe şişer.
        let turkish = "çğıöşü".repeat(100); // 600 karakter, 1200 bayt
        assert!(
            estimate_tokens(&turkish) < 200,
            "karakter sayılmalı, bayt değil"
        );
    }

    /// A model whose real window is tiny must not be handed a large one.
    ///
    /// Measured against Groq's `/models` endpoint on 2026-09-16: these are
    /// classifiers and speech models with windows between 448 and 4096
    /// tokens. The general `llama` row was claiming 32000 for the guard
    /// models, so every request to one overflowed at the provider with
    /// nothing on our side having tried to trim.
    #[test]
    fn a_tiny_window_is_not_rounded_up_to_a_chat_sized_one() {
        for (model, real) in [
            ("meta-llama/llama-prompt-guard-2-86m", 512),
            ("meta-llama/llama-prompt-guard-2-22m", 512),
            ("whisper-large-v3", 448),
            ("allam-2-7b", 4_096),
        ] {
            let ours = ModelCaps::for_model(model).context_window;
            assert!(
                ours <= real,
                "{model}: bize göre {ours}, gerçekte {real} — taşar"
            );
        }
    }

    /// The opposite error: claiming a model is smaller than it is costs
    /// history on every turn for no reason.
    #[test]
    fn groqs_large_models_get_the_window_they_actually_have() {
        for model in [
            "openai/gpt-oss-120b",
            "openai/gpt-oss-20b",
            "groq/compound",
            "groq/compound-mini",
        ] {
            let w = ModelCaps::for_model(model).context_window;
            assert!(w >= 131_000, "{model}: {w}, gerçekte 131072");
        }
    }

    #[test]
    fn model_caps_are_conservative_for_unknown_models() {
        // An unknown model is assumed smaller than any current model, so a
        // wrong guess trims history rather than earning a 413.
        let unknown = ModelCaps::for_model("bilinmeyen-model").context_window;
        assert!(
            unknown <= 32_768,
            "unknown models should not be assumed large: {unknown}"
        );
        assert!(ModelCaps::for_model("gemini-2.5-flash").context_window > 100_000);
    }

    /// The bug this guards: `max_output` was 1024 for every model, so a long
    /// answer stopped mid-sentence and a code block stopped mid-line. An
    /// unknown model must not inherit that -- the window is guessed low on
    /// purpose, but the reply limit is not.
    #[test]
    fn every_model_can_write_a_long_answer() {
        for model in [
            "bilinmeyen-model",
            "qwen/qwen3.8-27b",
            "gpt-4o",
            "claude-sonnet-5",
            "gemini-2.5-flash",
            "llama-3.3-70b",
        ] {
            let caps = ModelCaps::for_model(model);
            assert!(
                caps.max_output >= 4_096,
                "{model} would truncate long replies at {} tokens",
                caps.max_output
            );
        }
        assert!(ModelCaps::default().max_output >= 4_096);
    }

    /// A reply limit larger than the window would leave no room for the
    /// question, and providers reject that outright.
    #[test]
    fn the_reply_limit_always_leaves_room_for_input() {
        for model in ["gemma-7b", "mistral-7b", "gpt-4o", "bilinmeyen"] {
            let caps = ModelCaps::for_model(model);
            assert!(
                caps.max_output < caps.context_window,
                "{model}: reply limit {} does not fit in {}",
                caps.max_output,
                caps.context_window
            );
            assert!(
                caps.input_budget() > 0,
                "{model} has no room left for input"
            );
        }
    }

    /// Groq serves Qwen, and the name matched nothing before, so it fell to
    /// the 8k default while the model actually has 128k.
    #[test]
    fn qwen_is_recognised() {
        assert!(ModelCaps::for_model("qwen/qwen3.8-27b").context_window > 100_000);
    }

    #[test]
    fn newest_messages_are_kept_in_order() {
        let caps = ModelCaps::default();
        let msgs = vec![
            Message::system("s"),
            Message::user("bir"),
            Message::assistant("iki"),
            Message::user("üç"),
        ];
        let r = fit_request(msgs, no_tools(), caps);
        let texts: Vec<&str> = r.messages.iter().map(|m| m.content.as_str()).collect();
        assert_eq!(texts, vec!["s", "bir", "iki", "üç"], "sıra korunmalı");
    }
}

#[cfg(test)]
mod image_budget_tests {
    use super::*;

    fn no_tools() -> Vec<serde_json::Value> {
        Vec::new()
    }

    #[test]
    fn image_costs_a_fixed_amount_not_its_base64_length() {
        // 2 MB'lık PNG base64'te ~2.7M karakter — karakter sayarsak bütçe patlar.
        let huge_base64 = "A".repeat(2_700_000);
        let with_image = Message::user_with_image("bak", huge_base64);

        let tokens = message_tokens(&with_image);
        assert!(
            tokens < IMAGE_TOKENS + 100,
            "görüntü sabit maliyetli olmalı, hesaplanan: {tokens}"
        );
    }

    #[test]
    fn image_adds_to_the_budget() {
        let plain = Message::user("bak");
        let with_image = Message::user_with_image("bak", "kucuk");
        assert!(message_tokens(&with_image) > message_tokens(&plain) + 1000);
    }

    #[test]
    fn conversation_with_image_still_fits_the_budget() {
        let caps = ModelCaps::for_model("gpt-4o");
        let mut msgs = vec![Message::system("sen vavis'sin")];
        for _ in 0..5 {
            msgs.push(Message::user_with_image("bak", "A".repeat(100_000)));
        }

        let r = fit_request(msgs, no_tools(), caps);
        assert!(
            r.est_tokens <= caps.input_budget(),
            "görüntülü sohbet bütçeyi aşmamalı: {} > {}",
            r.est_tokens,
            caps.input_budget()
        );
    }

    #[test]
    fn small_context_model_drops_older_images() {
        // Ten images do not fit in an 8k window -- the old ones must go.
        // The window is stated here rather than taken from the default, which
        // is a guess for unknown models and free to change.
        let caps = ModelCaps::for_model("gemma-7b");
        let mut msgs = vec![Message::system("s")];
        for i in 0..10 {
            msgs.push(Message::user_with_image(format!("görüntü {i}"), "x"));
        }

        let r = fit_request(msgs, no_tools(), caps);
        assert!(r.history_dropped > 0, "eski görüntüler atılmalıydı");
        assert!(r.est_tokens <= caps.input_budget());
    }
}

#[cfg(test)]
mod caps_report {
    use super::*;

    /// Not an assertion -- prints the table so a model's real limits can be
    /// checked against what the app will send. Run with `--nocapture`.
    #[test]
    #[ignore = "reporting only"]
    fn print_caps_for_common_models() {
        for m in [
            "qwen/qwen3.8-27b",
            "bilinmeyen-model",
            "gpt-4o",
            "claude-sonnet-5",
            "gemma-7b",
        ] {
            let c = ModelCaps::for_model(m);
            println!(
                "{m}: window={} max_output={} input_budget={}",
                c.context_window,
                c.max_output,
                c.input_budget()
            );
        }
    }
}
