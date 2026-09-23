//! Otomasyon tool'ları — zamanlanmış görev ve koşullu tetikleyici kurma.
//!
//! Eski projede bu iş 4 ayrı alan ve 15+ tool'a dağılmıştı (`schedule_task`,
//! `if_then`, `watch_condition`, `routine_*`, `list_*`, `cancel_*`,
//! `toggle_*`…). Burada **3 tool**: kur, listele, sil.
//!
//! Tetikleyici türü doğal dilden ayrıştırılıyor; model ayrı tool seçmek
//! zorunda değil.

use crate::tool::{arg_num, arg_str, Domain, Param, Risk, Tool, ToolOutcome};
use serde_json::Value;
use std::sync::{Arc, Mutex, OnceLock};
use vavis_core::{Store, Trigger};

static STORE: OnceLock<Arc<Mutex<Store>>> = OnceLock::new();

/// Otomasyon deposunu bağlar. İkinci çağrı yok sayılır.
pub fn attach_store(store: Arc<Mutex<Store>>) {
    let _ = STORE.set(store);
}

fn with_store<T>(f: impl FnOnce(&Store) -> T) -> Option<T> {
    let store = STORE.get()?;
    let guard = store.lock().unwrap_or_else(|e| e.into_inner());
    Some(f(&guard))
}

/// Doğal dil zaman ifadesini tetikleyiciye çevirir.
///
/// Kabul edilen biçimler:
/// - `"09:00"` · `"9:30"` → her gün o saatte
/// - `"30 dakika"` · `"2 saat"` → aralıklı
/// - `"pil 20"` → pil %20 altına inince
/// - `"cpu 80"` → CPU %80 üstüne çıkınca
/// - `"dosya: indirilenler"` → o klasöre yeni dosya gelince
/// - `"açılınca: steam"` / `"kapanınca: steam"` → program açılınca / kapanınca
/// - `"açılışta"` → Vavis her açıldığında
/// - `"dönünce 30"` → 30 dakikadan uzun aradan sonra dönünce
pub fn parse_trigger(input: &str) -> Option<Trigger> {
    let s = input.trim().to_lowercase();

    if let Some(t) = parse_event(&s) {
        return Some(t);
    }

    // Koşullu: "pil 20" / "batarya 15"
    if s.starts_with("pil") || s.starts_with("batarya") {
        let percent = extract_number(&s)?;
        if percent > 100 {
            return None;
        }
        return Some(Trigger::BatteryBelow { percent });
    }
    if s.starts_with("cpu") || s.starts_with("islemci") || s.starts_with("işlemci") {
        let percent = extract_number(&s)?;
        if percent > 100 {
            return None;
        }
        return Some(Trigger::CpuAbove { percent });
    }

    // Saat: "09:00" veya "9.30"
    if let Some((h, m)) = parse_clock(&s) {
        return Some(Trigger::Daily { hour: h, minute: m });
    }

    // Aralık: "30 dakika", "2 saat"
    let number = extract_number(&s)?;
    if s.contains("saat") || s.contains("hour") {
        // 0 saat anlamsız.
        if number == 0 {
            return None;
        }
        return Some(Trigger::Every {
            minutes: number * 60,
        });
    }
    if s.contains("dakika") || s.contains("dk") || s.contains("minute") {
        if number == 0 {
            return None;
        }
        return Some(Trigger::Every { minutes: number });
    }

    None
}

/// The event forms. Checked first: "açılışta" contains no number and would
/// otherwise fall through to nothing, and "dosya: rapor 2024" would be
/// read as an interval.
fn parse_event(s: &str) -> Option<Trigger> {
    // "önek: değer" or "önek değer", with the value trimmed of quotes.
    let after = |prefixes: &[&str]| -> Option<String> {
        prefixes.iter().find_map(|p| {
            s.strip_prefix(p).map(|rest| {
                rest.trim_start_matches([':', ' '])
                    .trim()
                    .trim_matches(['\'', '"'])
                    .to_string()
            })
        })
    };
    // "steam açılınca" -- the value before the keyword.
    let before = |suffixes: &[&str]| -> Option<String> {
        suffixes.iter().find_map(|x| {
            s.strip_suffix(x)
                .map(|rest| rest.trim().trim_matches(['\'', '"']).to_string())
                .filter(|v| !v.is_empty())
        })
    };

    if [
        "açılışta",
        "acilista",
        "başlangıçta",
        "baslangicta",
        "startup",
        "on startup",
    ]
    .contains(&s)
    {
        return Some(Trigger::Startup);
    }

    if let Some(rest) = after(&["dönünce", "donunce", "geri dönünce", "returned", "away"]) {
        let minutes = extract_number(&rest).unwrap_or(30);
        return (minutes > 0).then_some(Trigger::Returned { minutes });
    }

    if let Some(folder) = after(&["yeni dosya", "dosya", "klasör", "klasor", "folder", "file"]) {
        let folder = if folder.is_empty() {
            "downloads".into()
        } else {
            folder
        };
        return Some(Trigger::FileAdded { folder });
    }
    if s.contains("indirilen") || s.contains("download") {
        return Some(Trigger::FileAdded {
            folder: "downloads".into(),
        });
    }

    if let Some(app) = after(&[
        "açılınca",
        "acilinca",
        "başlayınca",
        "baslayinca",
        "app start",
        "starts",
    ])
    .or_else(|| before(&["açılınca", "acilinca", "başlayınca", "baslayinca", "starts"]))
    .filter(|a| !a.is_empty())
    {
        return Some(Trigger::AppStarted { app });
    }
    if let Some(app) = after(&["kapanınca", "kapaninca", "app stop", "stops"])
        .or_else(|| before(&["kapanınca", "kapaninca", "stops"]))
        .filter(|a| !a.is_empty())
    {
        return Some(Trigger::AppStopped { app });
    }
    None
}

/// "09:30" / "9.30" / "9 30" → (9, 30)
fn parse_clock(s: &str) -> Option<(u32, u32)> {
    let separator = s.find([':', '.'])?;
    let (head, tail) = s.split_at(separator);

    let hour: u32 = head.trim().parse().ok()?;
    let minute: u32 = tail[1..].split_whitespace().next()?.parse().ok()?;

    if hour > 23 || minute > 59 {
        return None;
    }
    Some((hour, minute))
}

/// Metindeki ilk sayıyı çıkarır.
fn extract_number(s: &str) -> Option<u32> {
    let digits: String = s
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// Otomasyon kurma.
pub struct CreateAutomation;

impl Tool for CreateAutomation {
    fn name(&self) -> &'static str {
        "create_automation"
    }

    fn description(&self) -> &'static str {
        "Sets up a scheduled, conditional or event task. 'when' formats: '09:00' \
         (daily), '30 dakika' (every 30 min), 'pil 20' (battery under 20%), 'cpu 80' \
         (cpu over 80%), 'dosya: indirilenler' (a new file lands in that folder; \
         write {file} in the task where its path goes), 'açılınca: steam' / \
         'kapanınca: steam' (a program starts / stops), 'açılışta' (each time Vavis \
         starts), 'dönünce 30' (the user comes back after 30+ minutes away)."
    }

    fn domain(&self) -> Domain {
        Domain::Automation
    }

    /// Arka planda kendiliğinden çalışacak bir şey kuruyoruz — kullanıcı bilmeli.
    fn risk(&self) -> Risk {
        Risk::Moderate
    }

    fn params(&self) -> Vec<Param> {
        vec![
            Param::required("when", "When it fires: a time or a condition"),
            Param::required("task", "What to do when it fires"),
        ]
    }

    fn keywords(&self) -> &'static [&'static str] {
        &[
            "hatırlat",
            "her",
            "zamanla",
            "otomatik",
            "uyar",
            "açılınca",
            "gelince",
            "dönünce",
        ]
    }

    fn run(&self, args: &Value) -> ToolOutcome {
        let (Some(when), Some(task)) = (arg_str(args, "when"), arg_str(args, "task")) else {
            return ToolOutcome::err("ne_zaman ve gorev parametreleri gerekli");
        };

        let Some(trigger) = parse_trigger(when) else {
            return ToolOutcome::err(format!(
                "'{when}' anlaşılmadı. Örnekler: '09:00', '30 dakika', '2 saat', \
                 'pil 20', 'cpu 80', 'dosya: indirilenler', 'açılınca: steam', \
                 'kapanınca: steam', 'açılışta', 'dönünce 30'"
            ));
        };

        match with_store(|s| s.add_automation(task, &trigger)) {
            Some(Ok(id)) => ToolOutcome::ok(format!(
                "Otomasyon #{id} kuruldu — {} : {task}",
                trigger.describe()
            )),
            Some(Err(e)) => ToolOutcome::err(format!("kaydedilemedi: {e}")),
            None => ToolOutcome::err("otomasyon deposu hazır değil"),
        }
    }
}

/// Otomasyonları listeleme.
pub struct ListAutomations;

impl Tool for ListAutomations {
    fn name(&self) -> &'static str {
        "list_automations"
    }

    fn description(&self) -> &'static str {
        "Lists the scheduled tasks and conditional alerts that are set up."
    }

    fn domain(&self) -> Domain {
        Domain::Automation
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["otomasyon", "zamanlanmış", "hatırlatma", "görev"]
    }

    fn run(&self, _args: &Value) -> ToolOutcome {
        let Some(result) = with_store(|s| s.all_automations()) else {
            return ToolOutcome::err("otomasyon deposu hazır değil");
        };
        let list = match result {
            Ok(l) => l,
            Err(e) => return ToolOutcome::err(format!("okunamadı: {e}")),
        };

        if list.is_empty() {
            return ToolOutcome::ok("kurulu otomasyon yok");
        }

        let text = list
            .iter()
            .map(|a| {
                let state = if a.enabled { "" } else { " (kapalı)" };
                format!("#{} {} : {}{state}", a.id, a.trigger.describe(), a.prompt)
            })
            .collect::<Vec<_>>()
            .join("\n");
        ToolOutcome::ok(text)
    }
}

/// Otomasyon silme.
pub struct DeleteAutomation;

impl Tool for DeleteAutomation {
    fn name(&self) -> &'static str {
        "delete_automation"
    }

    fn description(&self) -> &'static str {
        "Deletes an automation. Use list_automations first to find its number."
    }

    fn domain(&self) -> Domain {
        Domain::Automation
    }

    fn risk(&self) -> Risk {
        Risk::Destructive
    }

    fn params(&self) -> Vec<Param> {
        vec![Param::required(
            "number",
            "Number of the automation to delete",
        )]
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["otomasyon", "sil", "iptal", "kaldır"]
    }

    fn run(&self, args: &Value) -> ToolOutcome {
        let Some(id) = arg_num(args, "number") else {
            return ToolOutcome::err("number is required");
        };
        let id = id as i64;

        match with_store(|s| s.delete_automation(id)) {
            Some(Ok(true)) => ToolOutcome::ok(format!("Otomasyon #{id} silindi")),
            Some(Ok(false)) => ToolOutcome::err(format!("#{id} bulunamadı")),
            Some(Err(e)) => ToolOutcome::err(format!("silinemedi: {e}")),
            None => ToolOutcome::err("otomasyon deposu hazır değil"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_are_understood() {
        assert_eq!(parse_trigger("açılışta"), Some(Trigger::Startup));
        assert_eq!(parse_trigger("Startup"), Some(Trigger::Startup));
        assert_eq!(
            parse_trigger("dosya: indirilenler"),
            Some(Trigger::FileAdded {
                folder: "indirilenler".into()
            })
        );
        assert_eq!(
            parse_trigger("indirilenlere yeni bir şey gelince"),
            Some(Trigger::FileAdded {
                folder: "downloads".into()
            })
        );
        assert_eq!(
            parse_trigger("dosya: C:\\Users\\ali\\Desktop"),
            Some(Trigger::FileAdded {
                folder: "c:\\users\\ali\\desktop".into()
            })
        );
        assert_eq!(
            parse_trigger("açılınca: steam"),
            Some(Trigger::AppStarted {
                app: "steam".into()
            })
        );
        assert_eq!(
            parse_trigger("steam açılınca"),
            Some(Trigger::AppStarted {
                app: "steam".into()
            })
        );
        assert_eq!(
            parse_trigger("kapanınca: chrome"),
            Some(Trigger::AppStopped {
                app: "chrome".into()
            })
        );
        assert_eq!(
            parse_trigger("dönünce 45"),
            Some(Trigger::Returned { minutes: 45 })
        );
        assert_eq!(
            parse_trigger("geri dönünce"),
            Some(Trigger::Returned { minutes: 30 })
        );
    }

    #[test]
    fn a_bare_event_word_with_nothing_to_watch_is_refused() {
        assert_eq!(parse_trigger("açılınca"), None);
        assert_eq!(parse_trigger("kapanınca:"), None);
    }

    fn ensure_store() {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            let store = Store::open_in_memory().expect("bellek içi depo");
            attach_store(Arc::new(Mutex::new(store)));
        });
    }

    // ── Tetikleyici ayrıştırma ────────────────────────────────────────────

    #[test]
    fn clock_times_parse_to_daily() {
        assert_eq!(
            parse_trigger("09:00"),
            Some(Trigger::Daily { hour: 9, minute: 0 })
        );
        assert_eq!(
            parse_trigger("21:45"),
            Some(Trigger::Daily {
                hour: 21,
                minute: 45
            })
        );
        // Nokta da kabul.
        assert_eq!(
            parse_trigger("7.30"),
            Some(Trigger::Daily {
                hour: 7,
                minute: 30
            })
        );
    }

    #[test]
    fn invalid_clock_times_are_rejected() {
        assert_eq!(parse_trigger("25:00"), None, "25. saat yok");
        assert_eq!(parse_trigger("09:99"), None, "99. dakika yok");
    }

    #[test]
    fn intervals_parse_to_every() {
        assert_eq!(
            parse_trigger("30 dakika"),
            Some(Trigger::Every { minutes: 30 })
        );
        assert_eq!(
            parse_trigger("2 saat"),
            Some(Trigger::Every { minutes: 120 })
        );
        assert_eq!(parse_trigger("15 dk"), Some(Trigger::Every { minutes: 15 }));
    }

    #[test]
    fn zero_intervals_are_rejected() {
        // "0 dakikada bir" sonsuz döngü olurdu.
        assert_eq!(parse_trigger("0 dakika"), None);
        assert_eq!(parse_trigger("0 saat"), None);
    }

    #[test]
    fn battery_conditions_parse() {
        assert_eq!(
            parse_trigger("pil 20"),
            Some(Trigger::BatteryBelow { percent: 20 })
        );
        assert_eq!(
            parse_trigger("batarya 15"),
            Some(Trigger::BatteryBelow { percent: 15 })
        );
    }

    #[test]
    fn cpu_conditions_parse() {
        assert_eq!(
            parse_trigger("cpu 80"),
            Some(Trigger::CpuAbove { percent: 80 })
        );
        assert_eq!(
            parse_trigger("işlemci 90"),
            Some(Trigger::CpuAbove { percent: 90 })
        );
    }

    #[test]
    fn percentages_above_hundred_are_rejected() {
        assert_eq!(parse_trigger("pil 150"), None);
        assert_eq!(parse_trigger("cpu 200"), None);
    }

    #[test]
    fn unparseable_input_returns_none() {
        for bad in ["", "yarın", "bir ara", "saçmalık", "çok sonra"] {
            assert_eq!(parse_trigger(bad), None, "'{bad}' ayrıştırılmamalıydı");
        }
    }

    #[test]
    fn number_extraction_finds_the_first_digits() {
        assert_eq!(extract_number("pil 20 olunca"), Some(20));
        assert_eq!(extract_number("her 5 dakika"), Some(5));
        assert_eq!(extract_number("sayı yok"), None);
    }

    // ── Tool davranışı ────────────────────────────────────────────────────

    #[test]
    fn create_requires_both_parameters() {
        ensure_store();
        assert!(!CreateAutomation.run(&serde_json::json!({})).ok);
        assert!(
            !CreateAutomation
                .run(&serde_json::json!({"when": "09:00"}))
                .ok
        );
    }

    #[test]
    fn create_explains_accepted_formats_on_failure() {
        ensure_store();
        let out = CreateAutomation.run(&serde_json::json!({
            "when": "bir ara",
            "task": "x"
        }));
        assert!(!out.ok);
        assert!(out.content.contains("09:00"), "örnekler gösterilmeli");
    }

    #[test]
    fn create_and_list_round_trip() {
        ensure_store();
        let out = CreateAutomation.run(&serde_json::json!({
            "when": "08:15",
            "task": "benzersiz-gorev-aaa hatirlatt"
        }));
        assert!(out.ok, "{}", out.content);
        assert!(out.content.contains("08:15"));

        let list = ListAutomations.run(&Value::Null);
        assert!(list.ok);
        assert!(list.content.contains("benzersiz-gorev-aaa"));
    }

    #[test]
    fn delete_removes_the_automation() {
        ensure_store();
        let created = CreateAutomation.run(&serde_json::json!({
            "when": "45 dakika",
            "task": "silinecek-gorev-bbb"
        }));

        let id: i64 = created
            .content
            .split('#')
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse().ok())
            .expect("numara ayrıştırılmalı");

        let deleted = DeleteAutomation.run(&serde_json::json!({"number": id.to_string()}));
        assert!(deleted.ok, "{}", deleted.content);

        let list = ListAutomations.run(&Value::Null);
        assert!(!list.content.contains("silinecek-gorev-bbb"));
    }

    #[test]
    fn delete_reports_missing_id() {
        ensure_store();
        let out = DeleteAutomation.run(&serde_json::json!({"number": "99999"}));
        assert!(!out.ok);
        assert!(out.content.contains("bulunamadı"));
    }

    #[test]
    fn risk_levels_are_correct() {
        assert_eq!(DeleteAutomation.risk(), Risk::Destructive);
        assert_eq!(CreateAutomation.risk(), Risk::Moderate);
        assert_eq!(ListAutomations.risk(), Risk::Safe);
    }
}
