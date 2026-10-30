//! Modelin kendi aracını istemesi.
//!
//! # Neden gerekli
//!
//! Bir isteğe hangi araçların gideceğine iki katman karar veriyor: anahtar
//! kelime tablosu ([`crate::selection`]) ve isteğe bağlı ucuz yönlendirici
//! ([`crate::router`]). İkisi de mesaja bakıyor — mesajın **söylemediği**
//! şeyi göremiyorlar.
//!
//! "Şu ekrandakini bir yere kaydet" cümlesinde model ekran görüntüsünü alıp
//! sonra dosya yazmak istiyor olabilir; ama "dosya" kelimesi geçmediği için
//! dosya araçları hiç sunulmamış olur. Model o noktada elinde olmayan bir
//! aracı isteyemez — sadece uydurabilir, ki uydurduğunda "öyle bir araç yok"
//! cevabını alır ve tur boşa gider.
//!
//! Bu araç o kapıyı açıyor: model **ne yapmak istediğini** yazıyor, sonraki
//! adımda ilgili araçlar sunuluyor.
//!
//! # Neden döngüye girmiyor
//!
//! Üç sınır var:
//!
//! * Tur başına [`MAX_REQUESTS`] kez çağrılabiliyor. Sonrasında araç hâlâ
//!   listede ama "bu turda yeterince istedin" cevabı dönüyor.
//! * Aynı argümanla tekrarı zaten `LoopGuard` kesiyor.
//! * İstenen şeyler **eklenir**, mevcut araçların yerine geçmez — model bir
//!   önceki adımda elindeki aracı kaybetmez.

use crate::tool::{arg_str, Domain, Param, Risk, Tool, ToolOutcome};
use serde_json::Value;
use std::sync::Mutex;

/// Bir turda kaç kez araç istenebilir.
///
/// İki: birincisi olağan durum (model işin ikinci yarısını görüyor),
/// ikincisi düzeltme payı. Üçüncüsü artık arama değil, dolaşmadır.
pub const MAX_REQUESTS: usize = 2;

/// Bu turda istenmiş açıklamalar.
///
/// Süreç genelinde tek bir yerde tutuluyor, çünkü aracı çalıştıran katman
/// (`Agent`) ile bir sonraki adımın araçlarını seçen katman (kabuk) farklı
/// ve aralarında bu bilgiyi taşıyacak bir kanal yok. `vision`'daki bekleyen
/// ekran görüntüsü de aynı deseni kullanıyor.
static PENDING: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Turun başında çağrılır — önceki turdan kalan istek taşınmaz.
pub fn reset() {
    lock().clear();
}

/// Bu turda istenmiş açıklamaları alır ve listeyi boşaltır.
///
/// Alındıktan sonra silinmesi bilinçli: aynı istek her adımda yeniden
/// uygulanırsa araç listesi tur boyunca şişer.
pub fn take() -> Vec<String> {
    std::mem::take(&mut *lock())
}

/// Bu turda kaç kez istendi.
pub fn requested_count() -> usize {
    lock().len()
}

fn lock() -> std::sync::MutexGuard<'static, Vec<String>> {
    PENDING.lock().unwrap_or_else(|e| e.into_inner())
}

/// "Bana şunu yapacak araç lazım" diyebilen araç.
pub struct RequestTools;

impl Tool for RequestTools {
    fn name(&self) -> &'static str {
        "request_tools"
    }

    fn description(&self) -> &'static str {
        "İhtiyacın olan bir araç elinde yoksa araç adını (prompttaki 'Diğer araçlar' \
         listesinden) ya da ne yapmak istediğini yaz; araç bir sonraki adımda \
         sunulur. Örnek: 'launch_app' ya da 'dosyaya yazmam lazım'."
    }

    fn domain(&self) -> Domain {
        Domain::Core
    }

    fn risk(&self) -> Risk {
        // Hiçbir şeye dokunmuyor: sadece bir sonraki adımda ne sunulacağını
        // etkiliyor. Onay istemek kullanıcıyı anlamsız bir soruya boğar.
        Risk::Safe
    }

    fn params(&self) -> Vec<Param> {
        vec![Param::required(
            "need",
            "What you are trying to do, in your own words, e.g. 'I need to take a screenshot'",
        )]
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["araç", "arac", "tool", "yetenek"]
    }

    fn run(&self, args: &Value) -> ToolOutcome {
        let Some(need) = arg_str(args, "need") else {
            return ToolOutcome::err("need is required");
        };
        let need = need.trim();
        if need.is_empty() {
            return ToolOutcome::err("ne yapmak istediğini yaz");
        }

        let mut pending = lock();
        if pending.len() >= MAX_REQUESTS {
            // Hata değil: model yanlış bir şey yapmadı, sadece sınıra geldi.
            // Hata dönmek onu düzeltmeye çalışmaya iter; bu cevap durmasını
            // ve eldekiyle devam etmesini söylüyor.
            return ToolOutcome::ok(
                "Bu turda yeterince araç istendi. Elindeki araçlarla devam et, \
                 ya da kullanıcıya neye ihtiyacın olduğunu söyle.",
            );
        }

        pending.push(need.to_string());
        ToolOutcome::ok(format!(
            "Anlaşıldı: '{need}'. İlgili araçlar bir sonraki adımda elinde olacak."
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Testler aynı süreçte paralel koşuyor ve `PENDING` süreç genelinde tek.
    /// Bu kilit olmadan biri diğerinin listesini boşaltıyor.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn ask(need: &str) -> ToolOutcome {
        RequestTools.run(&serde_json::json!({ "need": need }))
    }

    #[test]
    fn a_request_is_recorded_for_the_next_step() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset();

        let out = ask("dosyaya yazmam lazım");
        assert!(out.ok, "{}", out.content);

        assert_eq!(take(), vec!["dosyaya yazmam lazım".to_string()]);
        // Alındıktan sonra boşalıyor: aynı istek her adımda tekrar uygulanmasın.
        assert!(take().is_empty());
    }

    #[test]
    fn asking_too_often_is_answered_not_punished() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset();

        for _ in 0..MAX_REQUESTS {
            assert!(ask("bir şey").ok);
        }

        // Sınırın ötesi hâlâ başarılı bir cevap — model hata düzeltmeye
        // çalışmasın, eldekiyle devam etsin.
        let out = ask("bir şey daha");
        assert!(out.ok);
        assert!(out.content.contains("yeterince"), "{}", out.content);
        assert_eq!(requested_count(), MAX_REQUESTS);
    }

    #[test]
    fn a_new_turn_starts_clean() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset();

        assert!(ask("eski tur").ok);
        reset();

        assert_eq!(requested_count(), 0);
        assert!(take().is_empty());
    }

    #[test]
    fn an_empty_need_is_rejected() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset();

        assert!(!ask("   ").ok);
        assert!(!RequestTools.run(&serde_json::json!({})).ok);
        assert_eq!(requested_count(), 0);
    }

    /// Hiçbir şeye dokunmuyor — onay sorulmamalı.
    #[test]
    fn asking_for_a_tool_needs_no_approval() {
        assert_eq!(RequestTools.risk(), Risk::Safe);
    }
}
