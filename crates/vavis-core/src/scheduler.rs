//! Zamanlanmış görevler ve koşullu tetikleyiciler.
//!
//! Üç tür otomasyon:
//!
//! - **Zamanlanmış**: "her sabah 9'da hava durumunu söyle"
//! - **Koşullu**: "pil %20'nin altına inince uyar"
//! - **Olay**: "indirilenlere yeni dosya gelince VirusTotal'a sor",
//!   "Steam açılınca arkadaşlarımdan kim çevrimiçi söyle", "bilgisayar
//!   başına dönünce ne kaçırdığımı özetle". These are noticed by the
//!   shell's watcher, which polls the world; `should_fire` never fires them.
//!
//! # Tasarım kararı: kendi zamanlayıcımız
//!
//! Cron kütüphanesi eklemek yerine basit bir "her dakika kontrol et" döngüsü
//! kullanıyoruz. Saniye hassasiyeti gerekmiyor (bir asistan için dakika yeter)
//! ve bu, bağımlılığı ve karmaşıklığı düşürüyor.
//!
//! **Tetikleme kaydı:** Bir görev tetiklendiğinde `last_fired` güncellenir.
//! Böylece uygulama kapanıp açılsa bile aynı görev iki kez çalışmaz.

use crate::error::Result;
use crate::store::Store;
use serde::{Deserialize, Serialize};

/// Bir otomasyonun ne zaman tetikleneceği.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tip", rename_all = "lowercase")]
pub enum Trigger {
    /// Her gün belirli saatte. `hour`: 0-23, `minute`: 0-59.
    Daily { hour: u32, minute: u32 },
    /// Belirli aralıklarla (dakika).
    Every { minutes: u32 },
    /// Bir kez, belirli bir zamanda (unix zaman damgası).
    Once { at: i64 },
    /// Pil yüzdesi bu değerin altına inince.
    BatteryBelow { percent: u32 },
    /// CPU kullanımı bu değerin üstüne çıkınca.
    CpuAbove { percent: u32 },
    /// A finished file appears in a folder. `folder` is a path, or one of
    /// the names `downloads`, `desktop`, `documents`. The prompt may say
    /// `{file}` where the file's path should go.
    #[serde(rename = "fileadded")]
    FileAdded { folder: String },
    /// A program starts, matched by process name (`steam`, `chrome`).
    #[serde(rename = "appstarted")]
    AppStarted { app: String },
    /// A program that was running is no longer running.
    #[serde(rename = "appstopped")]
    AppStopped { app: String },
    /// Once each time Vavis starts: the morning briefing.
    Startup,
    /// The user comes back after at least `minutes` without touching the
    /// keyboard or mouse.
    Returned { minutes: u32 },
}

impl Trigger {
    /// İnsan okunur açıklama.
    pub fn describe(&self) -> String {
        match self {
            Self::Daily { hour, minute } => format!("her gün {hour:02}:{minute:02}"),
            Self::Every { minutes } if *minutes >= 60 => {
                format!("her {} saatte", minutes / 60)
            }
            Self::Every { minutes } => format!("her {minutes} dakikada"),
            Self::Once { at } => format!("bir kez ({at})"),
            Self::BatteryBelow { percent } => format!("pil %{percent} altına inince"),
            Self::CpuAbove { percent } => format!("cpu %{percent} üstüne çıkınca"),
            Self::FileAdded { folder } => format!("{folder} klasörüne yeni dosya gelince"),
            Self::AppStarted { app } => format!("{app} açılınca"),
            Self::AppStopped { app } => format!("{app} kapanınca"),
            Self::Startup => "Vavis her açıldığında".to_string(),
            Self::Returned { minutes } => {
                format!("{minutes} dakikadan uzun ara verip dönünce")
            }
        }
    }

    /// Bu tetikleyici zamana mı yoksa duruma mı bağlı?
    pub fn is_conditional(&self) -> bool {
        matches!(self, Self::BatteryBelow { .. } | Self::CpuAbove { .. })
    }

    /// Fired by something happening rather than by the clock -- the
    /// watcher's job, not `should_fire`'s.
    pub fn is_event(&self) -> bool {
        matches!(
            self,
            Self::FileAdded { .. }
                | Self::AppStarted { .. }
                | Self::AppStopped { .. }
                | Self::Startup
                | Self::Returned { .. }
        )
    }
}

/// Kayıtlı bir otomasyon.
#[derive(Debug, Clone, PartialEq)]
pub struct Automation {
    pub id: i64,
    /// Tetiklendiğinde asistana gönderilecek istek.
    pub prompt: String,
    pub trigger: Trigger,
    pub enabled: bool,
    /// En son ne zaman tetiklendi (unix). 0 = hiç.
    pub last_fired: i64,
}

impl Automation {
    /// Şu an tetiklenmeli mi?
    ///
    /// `now`: şu anki unix zaman damgası.
    /// `local_hour`/`local_minute`: yerel saat (zaman dilimi çağırana ait).
    /// `condition_value`: koşullu tetikleyiciler için ölçülen değer
    /// (pil yüzdesi, CPU yüzdesi…). Yoksa koşullu olanlar tetiklenmez.
    pub fn should_fire(
        &self,
        now: i64,
        local_hour: u32,
        local_minute: u32,
        condition_value: Option<u32>,
    ) -> bool {
        if !self.enabled {
            return false;
        }

        match &self.trigger {
            Trigger::Daily { hour, minute } => {
                if local_hour != *hour || local_minute != *minute {
                    return false;
                }
                // Aynı dakika içinde iki kez tetiklenmesin.
                now - self.last_fired > 90
            }

            Trigger::Every { minutes } => {
                let interval = (*minutes).max(1) as i64 * 60;
                // İlk kez: hemen değil, bir aralık sonra.
                if self.last_fired == 0 {
                    return false;
                }
                now - self.last_fired >= interval
            }

            Trigger::Once { at } => {
                // Zamanı geldi ve hiç tetiklenmedi.
                self.last_fired == 0 && now >= *at
            }

            Trigger::BatteryBelow { percent } => {
                let Some(value) = condition_value else {
                    return false;
                };
                if value >= *percent {
                    return false;
                }
                // Koşul sürerken sürekli tetiklenmesin — saatte bir yeter.
                now - self.last_fired > 3600
            }

            Trigger::CpuAbove { percent } => {
                let Some(value) = condition_value else {
                    return false;
                };
                if value <= *percent {
                    return false;
                }
                // Yüksek CPU dalgalanır; 10 dakikada birden sık uyarma.
                now - self.last_fired > 600
            }

            // The watcher decides these; the clock never does.
            Trigger::FileAdded { .. }
            | Trigger::AppStarted { .. }
            | Trigger::AppStopped { .. }
            | Trigger::Startup
            | Trigger::Returned { .. } => false,
        }
    }
}

impl Store {
    /// Otomasyon tablosunu hazırlar (şema v3 göçü).
    pub(crate) fn migrate_automations(&self) -> Result<()> {
        self.connection().execute_batch(
            "CREATE TABLE IF NOT EXISTS automations (
                 id         INTEGER PRIMARY KEY AUTOINCREMENT,
                 prompt     TEXT    NOT NULL,
                 trigger    TEXT    NOT NULL,
                 enabled    INTEGER NOT NULL DEFAULT 1,
                 last_fired INTEGER NOT NULL DEFAULT 0,
                 created_at INTEGER NOT NULL DEFAULT (unixepoch())
             );",
        )?;
        Ok(())
    }

    /// Yeni otomasyon kaydeder.
    pub fn add_automation(&self, prompt: &str, trigger: &Trigger) -> Result<i64> {
        let json = serde_json::to_string(trigger).unwrap_or_else(|_| "{}".into());
        self.connection().execute(
            "INSERT INTO automations (prompt, trigger) VALUES (?1, ?2)",
            [prompt, json.as_str()],
        )?;
        Ok(self.connection().last_insert_rowid())
    }

    pub fn all_automations(&self) -> Result<Vec<Automation>> {
        let conn = self.connection();
        let mut stmt = conn.prepare(
            "SELECT id, prompt, trigger, enabled, last_fired FROM automations ORDER BY id",
        )?;

        let rows = stmt.query_map([], |r| {
            let trigger_json: String = r.get(2)?;
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                trigger_json,
                r.get::<_, i64>(3)? != 0,
                r.get::<_, i64>(4)?,
            ))
        })?;

        Ok(rows
            .filter_map(std::result::Result::ok)
            .filter_map(|(id, prompt, json, enabled, last_fired)| {
                // Bozuk tetikleyici kaydı tüm listeyi düşürmesin — atla.
                let trigger = serde_json::from_str(&json).ok()?;
                Some(Automation {
                    id,
                    prompt,
                    trigger,
                    enabled,
                    last_fired,
                })
            })
            .collect())
    }

    pub fn delete_automation(&self, id: i64) -> Result<bool> {
        let n = self
            .connection()
            .execute("DELETE FROM automations WHERE id = ?1", [id])?;
        Ok(n > 0)
    }

    pub fn set_automation_enabled(&self, id: i64, enabled: bool) -> Result<bool> {
        let n = self.connection().execute(
            "UPDATE automations SET enabled = ?1 WHERE id = ?2",
            [i64::from(enabled), id],
        )?;
        Ok(n > 0)
    }

    /// Tetiklendi olarak işaretler.
    pub fn mark_automation_fired(&self, id: i64, when: i64) -> Result<()> {
        self.connection().execute(
            "UPDATE automations SET last_fired = ?1 WHERE id = ?2",
            [when, id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_triggers_never_fire_on_the_clock() {
        for t in [
            Trigger::FileAdded {
                folder: "downloads".into(),
            },
            Trigger::AppStarted {
                app: "steam".into(),
            },
            Trigger::Startup,
            Trigger::Returned { minutes: 30 },
        ] {
            assert!(t.is_event());
            assert!(!automation(t, 0).should_fire(NOW, 9, 0, Some(99)));
        }
    }

    #[test]
    fn event_triggers_survive_the_database() {
        let store = Store::open_in_memory().unwrap();
        let t = Trigger::FileAdded {
            folder: "downloads".into(),
        };
        store.add_automation("tara: {file}", &t).unwrap();
        store.add_automation("günaydın", &Trigger::Startup).unwrap();
        let all = store.all_automations().unwrap();
        assert_eq!(all[0].trigger, t);
        assert_eq!(all[1].trigger, Trigger::Startup);
    }

    fn automation(trigger: Trigger, last_fired: i64) -> Automation {
        Automation {
            id: 1,
            prompt: "test".into(),
            trigger,
            enabled: true,
            last_fired,
        }
    }

    const NOW: i64 = 1_800_000_000;

    #[test]
    fn disabled_automation_never_fires() {
        let mut a = automation(Trigger::Daily { hour: 9, minute: 0 }, 0);
        a.enabled = false;
        assert!(!a.should_fire(NOW, 9, 0, None));
    }

    #[test]
    fn daily_fires_at_the_right_minute() {
        let a = automation(
            Trigger::Daily {
                hour: 9,
                minute: 30,
            },
            0,
        );
        assert!(a.should_fire(NOW, 9, 30, None));
        assert!(!a.should_fire(NOW, 9, 31, None), "yanlış dakika");
        assert!(!a.should_fire(NOW, 10, 30, None), "yanlış saat");
    }

    #[test]
    fn daily_does_not_fire_twice_in_the_same_minute() {
        // Kontrol döngüsü dakikada birkaç kez çalışabilir.
        let a = automation(Trigger::Daily { hour: 9, minute: 0 }, NOW - 30);
        assert!(!a.should_fire(NOW, 9, 0, None), "30 saniye önce tetiklendi");

        let a = automation(Trigger::Daily { hour: 9, minute: 0 }, NOW - 86_400);
        assert!(
            a.should_fire(NOW, 9, 0, None),
            "dün tetiklendi, bugün tetiklenmeli"
        );
    }

    #[test]
    fn interval_respects_the_gap() {
        let a = automation(Trigger::Every { minutes: 30 }, NOW - 1800);
        assert!(a.should_fire(NOW, 0, 0, None), "tam 30 dakika geçti");

        let a = automation(Trigger::Every { minutes: 30 }, NOW - 600);
        assert!(!a.should_fire(NOW, 0, 0, None), "10 dakika yetmez");
    }

    #[test]
    fn interval_does_not_fire_immediately_after_creation() {
        // "Her 30 dakikada" diyen kullanıcı hemen tetiklenmesini beklemez.
        let a = automation(Trigger::Every { minutes: 30 }, 0);
        assert!(!a.should_fire(NOW, 0, 0, None));
    }

    #[test]
    fn zero_interval_is_treated_as_one_minute() {
        // Bölme hatası veya sonsuz döngü olmasın.
        let a = automation(Trigger::Every { minutes: 0 }, NOW - 120);
        assert!(a.should_fire(NOW, 0, 0, None));
    }

    #[test]
    fn once_fires_exactly_once() {
        let a = automation(Trigger::Once { at: NOW - 10 }, 0);
        assert!(a.should_fire(NOW, 0, 0, None), "zamanı geldi");

        let fired = automation(Trigger::Once { at: NOW - 10 }, NOW - 5);
        assert!(!fired.should_fire(NOW, 0, 0, None), "zaten tetiklendi");
    }

    #[test]
    fn once_does_not_fire_before_its_time() {
        let a = automation(Trigger::Once { at: NOW + 3600 }, 0);
        assert!(!a.should_fire(NOW, 0, 0, None));
    }

    #[test]
    fn battery_condition_needs_a_measurement() {
        let a = automation(Trigger::BatteryBelow { percent: 20 }, 0);
        assert!(!a.should_fire(NOW, 0, 0, None), "ölçüm yoksa tetiklenmez");
        assert!(a.should_fire(NOW, 0, 0, Some(15)), "%15 < %20");
        assert!(!a.should_fire(NOW, 0, 0, Some(50)), "%50 > %20");
    }

    #[test]
    fn battery_alert_does_not_repeat_constantly() {
        // Pil %15'te kalırsa her dakika uyarmasın.
        let a = automation(Trigger::BatteryBelow { percent: 20 }, NOW - 60);
        assert!(!a.should_fire(NOW, 0, 0, Some(15)), "1 dakika önce uyardı");

        let a = automation(Trigger::BatteryBelow { percent: 20 }, NOW - 7200);
        assert!(a.should_fire(NOW, 0, 0, Some(15)), "2 saat geçti");
    }

    #[test]
    fn cpu_condition_fires_above_threshold() {
        let a = automation(Trigger::CpuAbove { percent: 80 }, 0);
        assert!(a.should_fire(NOW, 0, 0, Some(95)));
        assert!(!a.should_fire(NOW, 0, 0, Some(50)));
    }

    #[test]
    fn triggers_describe_themselves_readably() {
        assert_eq!(
            Trigger::Daily { hour: 9, minute: 5 }.describe(),
            "her gün 09:05"
        );
        assert_eq!(Trigger::Every { minutes: 30 }.describe(), "her 30 dakikada");
        assert_eq!(Trigger::Every { minutes: 120 }.describe(), "her 2 saatte");
        assert!(Trigger::BatteryBelow { percent: 20 }
            .describe()
            .contains("pil"));
    }

    #[test]
    fn conditional_triggers_are_identified() {
        assert!(Trigger::BatteryBelow { percent: 20 }.is_conditional());
        assert!(Trigger::CpuAbove { percent: 80 }.is_conditional());
        assert!(!Trigger::Daily { hour: 9, minute: 0 }.is_conditional());
        assert!(!Trigger::Every { minutes: 5 }.is_conditional());
    }

    #[test]
    fn triggers_survive_serialisation() {
        for t in [
            Trigger::Daily {
                hour: 9,
                minute: 30,
            },
            Trigger::Every { minutes: 45 },
            Trigger::Once { at: NOW },
            Trigger::BatteryBelow { percent: 20 },
            Trigger::CpuAbove { percent: 80 },
        ] {
            let json = serde_json::to_string(&t).unwrap();
            let back: Trigger = serde_json::from_str(&json).unwrap();
            assert_eq!(t, back, "json: {json}");
        }
    }

    // ── Depolama ──────────────────────────────────────────────────────────

    #[test]
    fn automations_round_trip_through_the_store() {
        let store = Store::open_in_memory().unwrap();
        let id = store
            .add_automation(
                "hava durumunu söyle",
                &Trigger::Daily { hour: 9, minute: 0 },
            )
            .unwrap();

        let all = store.all_automations().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, id);
        assert_eq!(all[0].prompt, "hava durumunu söyle");
        assert_eq!(all[0].trigger, Trigger::Daily { hour: 9, minute: 0 });
        assert!(all[0].enabled, "yeni otomasyon açık olmalı");
        assert_eq!(all[0].last_fired, 0);
    }

    #[test]
    fn firing_is_recorded() {
        let store = Store::open_in_memory().unwrap();
        let id = store
            .add_automation("x", &Trigger::Every { minutes: 10 })
            .unwrap();

        store.mark_automation_fired(id, NOW).unwrap();
        assert_eq!(store.all_automations().unwrap()[0].last_fired, NOW);
    }

    #[test]
    fn automations_can_be_disabled_and_deleted() {
        let store = Store::open_in_memory().unwrap();
        let id = store
            .add_automation("x", &Trigger::Every { minutes: 10 })
            .unwrap();

        assert!(store.set_automation_enabled(id, false).unwrap());
        assert!(!store.all_automations().unwrap()[0].enabled);

        assert!(store.delete_automation(id).unwrap());
        assert!(store.all_automations().unwrap().is_empty());
    }

    #[test]
    fn missing_automation_reports_false() {
        let store = Store::open_in_memory().unwrap();
        assert!(!store.delete_automation(999).unwrap());
        assert!(!store.set_automation_enabled(999, true).unwrap());
    }

    #[test]
    fn corrupt_trigger_row_is_skipped_not_fatal() {
        let store = Store::open_in_memory().unwrap();
        store
            .add_automation("iyi", &Trigger::Every { minutes: 5 })
            .unwrap();
        // Bozuk kayıt elle ekleniyor.
        store
            .connection()
            .execute(
                "INSERT INTO automations (prompt, trigger) VALUES ('bozuk', 'gecersiz json')",
                [],
            )
            .unwrap();

        let all = store.all_automations().unwrap();
        assert_eq!(all.len(), 1, "bozuk kayıt atlanmalı, liste düşmemeli");
        assert_eq!(all[0].prompt, "iyi");
    }
}
