//! Steam tool'ları.
//!
//! Steam okuma ağırlıklı: kontrol edilecek pek bir şey yok, bilinecek çok şey
//! var. Tek yazma işlemi oyun başlatmak, o da izin kapısından geçiyor.
//!
//! Sarmalanmayanlar: takas, market, workshop, grup yönetimi. Sesli
//! sorulmuyorlar ve bazıları asistanın tutmaması gereken bilgi istiyor.

use crate::steam::{self, SteamError};
use crate::tool::{arg_num, arg_str, Domain, Param, Risk, Tool, ToolOutcome};
use serde_json::Value;

const VAULT_WORDS: &[&str] = &[
    "steam",
    "game",
    "game",
    "oynadım",
    "oynadim",
    "kütüphane",
    "kutuphane",
];

/// Bir oyunu ada göre bulur — model AppID bilmiyor, isim biliyor.
///
/// Tam eşleşme, sonra başlangıç, sonra içerme sırasıyla bakılıyor: "Fallout"
/// yazınca "Fallout 4" ile "Fallout 76" arasında sessizce seçim yapılmamalı.
fn find_game(query: &str) -> Result<Vec<steam::Game>, SteamError> {
    let games = steam::library()?;
    let needle = query.trim().to_lowercase();

    let exact: Vec<_> = games
        .iter()
        .filter(|g| g.name.to_lowercase() == needle)
        .cloned()
        .collect();
    if !exact.is_empty() {
        return Ok(exact);
    }

    let starts: Vec<_> = games
        .iter()
        .filter(|g| g.name.to_lowercase().starts_with(&needle))
        .cloned()
        .collect();
    if !starts.is_empty() {
        return Ok(starts);
    }

    Ok(games
        .into_iter()
        .filter(|g| g.name.to_lowercase().contains(&needle))
        .collect())
}

/// Kütüphane.
pub struct Library;

impl Tool for Library {
    fn name(&self) -> &'static str {
        "steam_library"
    }

    fn description(&self) -> &'static str {
        "Steam kütüphanesini listeler. filtre='oynanmamis' hiç oynanmayanları, \
         'en_cok' en çok oynananları verir; başka bir metin ada göre arar."
    }

    fn domain(&self) -> Domain {
        Domain::Steam
    }

    fn params(&self) -> Vec<Param> {
        vec![Param::optional(
            "filter",
            "unplayed | most_played | a game name to search for",
        )]
    }

    fn keywords(&self) -> &'static [&'static str] {
        VAULT_WORDS
    }

    fn run(&self, args: &Value) -> ToolOutcome {
        let games = match steam::library() {
            Ok(g) => g,
            Err(e) => return ToolOutcome::err(e.to_string()),
        };

        let filter = arg_str(args, "filter").unwrap_or("en_cok");
        let selected: Vec<steam::Game> = match filter {
            "oynanmamis" | "oynanmamış" | "unplayed" => {
                games.into_iter().filter(|g| g.minutes == 0).collect()
            }
            "en_cok" | "en_çok" | "most" => games.into_iter().filter(|g| g.minutes > 0).collect(),
            name => match find_game(name) {
                Ok(g) => g,
                Err(e) => return ToolOutcome::err(e.to_string()),
            },
        };

        if selected.is_empty() {
            return ToolOutcome::ok("Eşleşen oyun yok.".to_string());
        }

        let total = selected.len();
        let listed: Vec<String> = selected
            .iter()
            .take(30)
            .map(|g| format!("{} — {}", g.name, g.playtime()))
            .collect();

        let mut out = format!("{total} oyun:\n{}", listed.join("\n"));
        if total > 30 {
            out.push_str(&format!("\n…({} oyun daha)", total - 30));
        }
        ToolOutcome::ok(out)
    }
}

/// Şu an oynanan oyun.
pub struct NowPlaying;

impl Tool for NowPlaying {
    fn name(&self) -> &'static str {
        "steam_current_game"
    }

    fn description(&self) -> &'static str {
        "Şu an çalışan Steam oyununu söyler. Anahtar gerektirmez, gizli \
         profilde de çalışır."
    }

    fn domain(&self) -> Domain {
        Domain::Steam
    }

    fn keywords(&self) -> &'static [&'static str] {
        VAULT_WORDS
    }

    fn run(&self, _args: &Value) -> ToolOutcome {
        // Tespit tamamen lokal: Web API bunu geç ve sadece açık profilde
        // söylüyor, çalışan süreçlere bakmak anında ve her zaman çalışıyor.
        match steam::running_game() {
            Some(game) => ToolOutcome::ok(format!("{} çalışıyor.", game.name)),
            None => ToolOutcome::ok("Şu an çalışan bir Steam oyunu yok.".to_string()),
        }
    }
}

/// Oyun başlatır.
pub struct LaunchGame;

impl Tool for LaunchGame {
    fn name(&self) -> &'static str {
        "steam_launch_game"
    }

    fn description(&self) -> &'static str {
        "Launches a Steam game. Give the full game name."
    }

    fn domain(&self) -> Domain {
        Domain::Steam
    }

    fn risk(&self) -> Risk {
        // Dünyayı değiştiren tek Steam tool'u ve ekranı komple kaplıyor.
        // Yanlış eşleşme (Fallout 4 / Fallout 76) tam da izin kapısının
        // engellemek için var olduğu şey.
        Risk::Destructive
    }

    fn params(&self) -> Vec<Param> {
        vec![Param::required("game", "Name of the game to launch")]
    }

    fn keywords(&self) -> &'static [&'static str] {
        &[
            "steam", "oyun", "başlat", "baslat", "aç", "ac", "launch", "play",
        ]
    }

    fn run(&self, args: &Value) -> ToolOutcome {
        let Some(query) = arg_str(args, "game") else {
            return ToolOutcome::err("game is required");
        };

        let matches = match find_game(query) {
            Ok(m) => m,
            Err(e) => return ToolOutcome::err(e.to_string()),
        };

        match matches.len() {
            0 => ToolOutcome::err(format!("'{query}' kütüphanede bulunamadı")),
            1 => {
                let game = &matches[0];
                match launch(game.appid) {
                    Ok(()) => ToolOutcome::ok(format!("{} başlatılıyor.", game.name)),
                    Err(e) => ToolOutcome::err(e),
                }
            }
            _ => {
                // Emin değilse sorar, tahmin etmez.
                let names: Vec<&str> = matches.iter().take(6).map(|g| g.name.as_str()).collect();
                ToolOutcome::err(format!(
                    "'{query}' birden fazla oyunla eşleşti: {}. Hangisi?",
                    names.join(", ")
                ))
            }
        }
    }
}

/// Steam protokolüyle oyunu başlatır.
fn launch(appid: u32) -> Result<(), String> {
    use std::process::Command;

    let url = format!("steam://rungameid/{appid}");

    #[cfg(windows)]
    {
        // `cmd /C start` steam: protokolünü kayıtlı uygulamaya yönlendirir.
        // İlk boş argüman `start`ın pencere başlığı beklentisi için.
        vavis_core::process::hidden(&mut Command::new("cmd"))
            .args(["/C", "start", "", &url])
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("başlatılamadı: {e}"))
    }

    #[cfg(not(windows))]
    {
        Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("başlatılamadı: {e}"))
    }
}

/// Başarımlar.
pub struct Achievements;

impl Tool for Achievements {
    fn name(&self) -> &'static str {
        "steam_achievements"
    }

    fn description(&self) -> &'static str {
        "Reports achievement progress for a game."
    }

    fn domain(&self) -> Domain {
        Domain::Steam
    }

    fn params(&self) -> Vec<Param> {
        vec![Param::required("game", "Name of the game")]
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["başarım", "basarim", "achievement", "steam", "oyun"]
    }

    fn run(&self, args: &Value) -> ToolOutcome {
        let Some(query) = arg_str(args, "game") else {
            return ToolOutcome::err("game is required");
        };

        let matches = match find_game(query) {
            Ok(m) => m,
            Err(e) => return ToolOutcome::err(e.to_string()),
        };
        let Some(game) = matches.first() else {
            return ToolOutcome::err(format!("'{query}' kütüphanede bulunamadı"));
        };

        match steam::achievements(game.appid) {
            Ok((unlocked, total, recent)) => {
                let percent = unlocked
                    .checked_mul(100)
                    .and_then(|n| n.checked_div(total))
                    .unwrap_or(0);
                let mut out = format!("{}: {unlocked}/{total} (%{percent})", game.name);
                if !recent.is_empty() {
                    out.push_str(&format!("\nson açılanlar: {}", recent.join(", ")));
                }
                ToolOutcome::ok(out)
            }
            Err(e) => ToolOutcome::err(e.to_string()),
        }
    }
}

/// Mağaza fiyatı.
pub struct StorePrice;

impl Tool for StorePrice {
    fn name(&self) -> &'static str {
        "steam_price"
    }

    fn description(&self) -> &'static str {
        "Reports a game's price and discount on the Steam store."
    }

    fn domain(&self) -> Domain {
        Domain::Steam
    }

    fn params(&self) -> Vec<Param> {
        vec![
            Param::required("game", "Name of the game"),
            Param::optional("country", "Country code, default tr"),
        ]
    }

    fn keywords(&self) -> &'static [&'static str] {
        &[
            "fiyat",
            "indirim",
            "kaç para",
            "kac para",
            "steam",
            "price",
            "sale",
        ]
    }

    fn run(&self, args: &Value) -> ToolOutcome {
        let Some(query) = arg_str(args, "game") else {
            return ToolOutcome::err("game is required");
        };
        let country = arg_str(args, "country").unwrap_or("tr");

        // Önce kütüphanede ara; yoksa istek listesinde. İkisi de AppID veriyor,
        // mağaza API'si isimle arama yapmıyor.
        let appid = find_game(query)
            .ok()
            .and_then(|m| m.first().map(|g| g.appid))
            .or_else(|| {
                steam::wishlist().ok().and_then(|list| {
                    list.into_iter()
                        .find(|(_, name)| name.to_lowercase().contains(&query.to_lowercase()))
                        .map(|(id, _)| id)
                })
            });

        let Some(appid) = appid else {
            return ToolOutcome::err(format!(
                "'{query}' kütüphanende veya istek listende bulunamadı — \
                 mağaza araması için oyunun tam adı gerekiyor"
            ));
        };

        match steam::store(appid, country) {
            Ok((name, price, discount)) => {
                if discount > 0 {
                    ToolOutcome::ok(format!("{name}: {price} (%{discount} indirimde)"))
                } else {
                    ToolOutcome::ok(format!("{name}: {price}"))
                }
            }
            Err(e) => ToolOutcome::err(e.to_string()),
        }
    }
}

/// İstek listesi.
pub struct Wishlist;

impl Tool for Wishlist {
    fn name(&self) -> &'static str {
        "steam_wishlist"
    }

    fn description(&self) -> &'static str {
        "Lists the Steam wishlist, including which items are discounted."
    }

    fn domain(&self) -> Domain {
        Domain::Steam
    }

    fn params(&self) -> Vec<Param> {
        vec![Param::optional(
            "on_sale_only",
            "'yes' to list only discounted items",
        )]
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["istek listesi", "wishlist", "indirim", "steam"]
    }

    fn run(&self, args: &Value) -> ToolOutcome {
        let list = match steam::wishlist() {
            Ok(l) => l,
            Err(e) => return ToolOutcome::err(e.to_string()),
        };

        if list.is_empty() {
            return ToolOutcome::ok("İstek listen boş.".to_string());
        }

        let only_sales = arg_str(args, "on_sale_only")
            .is_some_and(|v| matches!(v.to_lowercase().as_str(), "evet" | "true" | "yes"));

        if !only_sales {
            let names: Vec<&str> = list.iter().take(40).map(|(_, n)| n.as_str()).collect();
            let mut out = format!("{} oyun:\n{}", list.len(), names.join("\n"));
            if list.len() > 40 {
                out.push_str(&format!("\n…({} oyun daha)", list.len() - 40));
            }
            return ToolOutcome::ok(out);
        }

        // İndirim için her oyunun mağaza kaydına bakmak gerekiyor; liste uzun
        // olabildiği için ilk 25 ile sınırlanıyor (her biri bir istek).
        let mut discounted = Vec::new();
        for (appid, _) in list.iter().take(25) {
            if let Ok((name, price, discount)) = steam::store(*appid, "tr") {
                if discount > 0 {
                    discounted.push(format!("{name}: {price} (%{discount})"));
                }
            }
        }

        if discounted.is_empty() {
            ToolOutcome::ok("İstek listendekilerin ilk 25'inde indirim yok.".to_string())
        } else {
            ToolOutcome::ok(format!("İndirimdekiler:\n{}", discounted.join("\n")))
        }
    }
}

/// Arkadaşlar.
pub struct Friends;

impl Tool for Friends {
    fn name(&self) -> &'static str {
        "steam_friends"
    }

    fn description(&self) -> &'static str {
        "Lists Steam friends and what each of them is playing."
    }

    fn domain(&self) -> Domain {
        Domain::Steam
    }

    fn params(&self) -> Vec<Param> {
        vec![Param::optional(
            "count",
            "How many people to show (default 15)",
        )]
    }

    fn keywords(&self) -> &'static [&'static str] {
        &[
            "arkadaş",
            "arkadas",
            "friend",
            "steam",
            "online",
            "çevrimiçi",
        ]
    }

    fn run(&self, args: &Value) -> ToolOutcome {
        let limit = arg_num(args, "count")
            .map(|n| (n as usize).clamp(1, 50))
            .unwrap_or(15);

        match steam::friends() {
            Ok(list) if list.is_empty() => ToolOutcome::ok("Arkadaş listen boş.".to_string()),
            Ok(list) => {
                let shown: Vec<&str> = list.iter().take(limit).map(String::as_str).collect();
                ToolOutcome::ok(format!("{} arkadaş:\n{}", list.len(), shown.join("\n")))
            }
            Err(e) => ToolOutcome::err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steam::Settings;

    /// Steam ayarları süreç geneli; ona dokunan testler serileştiriliyor.
    fn test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn tools_explain_that_steam_is_not_set_up() {
        let _guard = test_lock();
        steam::configure(Settings::default());

        for (name, out) in [
            ("kutuphane", Library.run(&serde_json::json!({}))),
            (
                "basarimlar",
                Achievements.run(&serde_json::json!({"game": "x"})),
            ),
            ("arkadaslar", Friends.run(&serde_json::json!({}))),
        ] {
            assert!(!out.ok, "{name} ayarsız başarılı olmamalı");
            assert!(
                out.content.contains("ayarlanmamış"),
                "{name}: {}",
                out.content
            );
        }
    }

    #[test]
    fn launching_a_game_always_needs_approval() {
        // Ekranı kaplayan, geri alınamayan tek işlem.
        assert_eq!(LaunchGame.risk(), Risk::Destructive);
        assert_eq!(Library.risk(), Risk::Safe);
        assert_eq!(NowPlaying.risk(), Risk::Safe);
        assert_eq!(StorePrice.risk(), Risk::Safe);
        assert_eq!(Wishlist.risk(), Risk::Safe);
        assert_eq!(Friends.risk(), Risk::Safe);
    }

    #[test]
    fn now_playing_works_without_any_credentials() {
        let _guard = test_lock();
        steam::configure(Settings::default());

        // Tespit lokal — anahtar olmadan da cevap vermeli.
        let out = NowPlaying.run(&serde_json::json!({}));
        assert!(out.ok, "{}", out.content);
    }

    #[test]
    fn missing_parameters_are_rejected() {
        assert!(!LaunchGame.run(&serde_json::json!({})).ok);
        assert!(!Achievements.run(&serde_json::json!({})).ok);
        assert!(!StorePrice.run(&serde_json::json!({})).ok);
    }

    #[test]
    fn schemas_declare_their_required_parameters() {
        let schema = LaunchGame.schema();
        let required = schema["function"]["parameters"]["required"]
            .as_array()
            .unwrap();
        assert!(required.contains(&Value::String("game".into())));
    }
}
