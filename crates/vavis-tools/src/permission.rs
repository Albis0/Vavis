//! İzin kapısı — yıkıcı işlemler onaysız çalışmaz.
//!
//! Eski projeden taşınan iki fikir:
//!
//! 1. **Yıkıcı bütçe**: Tek bir çalıştırmada çok sayıda yıkıcı işlem yapılırsa
//!    (8 farklı dosya silmek gibi), "hep izin ver" seçilmiş olsa bile sonrakiler
//!    yeniden onay ister. Tekrar koruması (loop guard) tekrarı yakalar, **çeşidi**
//!    yakalamaz — bu onu yakalar.
//!
//! 2. **Kalıcı izin**: Kullanıcı "bu tool'a hep izin ver" derse o oturum
//!    boyunca sorulmaz — ama sadece `Moderate` için. `Destructive` her zaman sorar.

use crate::tool::Risk;
use std::collections::HashSet;

/// Tek çalıştırmada onaysız geçebilecek yıkıcı işlem sayısı.
pub const DESTRUCTIVE_BUDGET: usize = 3;

/// Onay neden isteniyor?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalReason {
    /// Tool'un kendi risk seviyesi. Kalıcı izin bunu susturabilir.
    RiskLevel,
    /// Bütçe aşıldı. Kalıcı izin **susturamaz**.
    BudgetExceeded,
    /// Bu turda okunan dış içerik modele talimat vermeye çalışıyordu.
    ///
    /// Kalıcı izin **susturamaz**: saldırının işe yaraması için tam olarak
    /// kullanıcının daha önce "hep izin ver" demiş olması gerekiyor.
    TaintedContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Sorulmadan çalıştır.
    Allow,
    /// Kullanıcıya sor.
    Ask(ApprovalReason),
}

#[derive(Debug, Default)]
pub struct PermissionGate {
    /// "Hep izin ver" denen tool'lar (oturum boyunca).
    granted: HashSet<String>,
    /// Bu çalıştırmada kaç yıkıcı işlem yapıldı.
    destructive_used: usize,
    /// Bu turda enjeksiyon şüphesi taşıyan dış içerik okundu mu.
    ///
    /// Tur başına sıfırlanıyor: bir sayfanın şüpheli olması sonraki isteği
    /// cezalandırmamalı.
    tainted: bool,
    /// Tam yetki — hiçbir şey sorulmuyor.
    ///
    /// Kullanıcının açık tercihi, ayarlarda tek anahtar. Varsayılan kapalı ve
    /// açarken bir kez uyarılıyor; ondan sonra bir daha sorulmuyor, çünkü
    /// "emin misin" diye tekrar tekrar sormak zaten kapatmak istediği şeyin
    /// ta kendisi.
    full_authority: bool,
}

impl PermissionGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// Yeni bir kullanıcı isteği başladı — yıkıcı bütçe sıfırlanır.
    /// Kalıcı izinler **korunur** (oturum boyunca geçerli).
    pub fn start_run(&mut self) {
        self.destructive_used = 0;
        self.tainted = false;
    }

    /// Bu turda okunan dış içerik modele talimat vermeye çalışıyordu.
    ///
    /// Bundan sonra yıkıcı işlemler kalıcı izne rağmen onay ister. Geri alma
    /// yolu yok: tur bitene kadar şüphe sürüyor.
    pub fn mark_tainted(&mut self) {
        self.tainted = true;
    }

    /// Bu turda şüpheli dış içerik görüldü mü.
    pub fn is_tainted(&self) -> bool {
        self.tainted
    }

    /// Bu tool çalıştırılabilir mi?
    pub fn check(&self, tool_name: &str, risk: Risk) -> Decision {
        // One guard outlives full authority: a page that tried to give the
        // model orders. Full authority says "I trust what I ask the assistant
        // to do" -- it cannot mean "I trust whatever a web page tells it to
        // do", because the user never sees that page. With the guard off, one
        // poisoned search result could run a command with nobody asked.
        //
        // Moderate changes count too, once the turn is suspect. Opening an
        // app can open `powershell` with arguments, and an automation runs
        // its prompt later in a clean turn where nothing is suspect any
        // more -- "moderate" is a statement about the user's own requests,
        // not about what a page can build out of them.
        if self.tainted && risk != Risk::Safe {
            return Decision::Ask(ApprovalReason::TaintedContext);
        }
        if self.full_authority {
            return Decision::Allow;
        }

        match risk {
            Risk::Safe => Decision::Allow,

            Risk::Moderate => {
                if self.granted.contains(tool_name) {
                    Decision::Allow
                } else {
                    Decision::Ask(ApprovalReason::RiskLevel)
                }
            }

            Risk::Destructive => {
                // Şüpheli dış içerik okunduysa kalıcı izin geçersiz. Bu tam
                // olarak saldırının hedeflediği delik: kullanıcı bir kez
                // "hep izin ver" demişse, sayfanın yazdığı komut sessizce
                // çalışırdı.
                if self.tainted {
                    Decision::Ask(ApprovalReason::TaintedContext)
                } else if self.destructive_used >= DESTRUCTIVE_BUDGET {
                    // Bütçe aşıldıysa kalıcı izin bile geçersiz.
                    Decision::Ask(ApprovalReason::BudgetExceeded)
                } else if self.granted.contains(tool_name) {
                    Decision::Allow
                } else {
                    Decision::Ask(ApprovalReason::RiskLevel)
                }
            }
        }
    }

    /// Kullanıcı "hep izin ver" dedi.
    pub fn grant_always(&mut self, tool_name: impl Into<String>) {
        self.granted.insert(tool_name.into());
    }

    /// Bir yıkıcı işlem çalıştırıldı — bütçeden düş.
    pub fn record_execution(&mut self, risk: Risk) {
        if risk == Risk::Destructive {
            self.destructive_used += 1;
        }
    }

    /// Tüm kalıcı izinleri kaldır.
    pub fn revoke_all(&mut self) {
        self.granted.clear();
    }

    /// Tam yetkiyi aç/kapat.
    ///
    /// Oturum boyunca sürüyor ve ayarlardan geliyor; `start_run` bunu
    /// sıfırlamıyor — her istekte yeniden açtırmak anahtarın anlamını
    /// ortadan kaldırırdı.
    pub fn set_full_authority(&mut self, on: bool) {
        self.full_authority = on;
    }

    /// Tam yetki açık mı — arayüzün göstergeyi çizmesi için.
    pub fn full_authority(&self) -> bool {
        self.full_authority
    }

    pub fn destructive_used(&self) -> usize {
        self.destructive_used
    }

    pub fn granted_tools(&self) -> Vec<&str> {
        let mut v: Vec<&str> = self.granted.iter().map(String::as_str).collect();
        v.sort_unstable();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_tools_never_ask() {
        let gate = PermissionGate::new();
        assert_eq!(gate.check("read_file", Risk::Safe), Decision::Allow);
    }

    #[test]
    fn moderate_asks_until_granted() {
        let mut gate = PermissionGate::new();
        assert!(matches!(
            gate.check("set_volume", Risk::Moderate),
            Decision::Ask(_)
        ));

        gate.grant_always("set_volume");
        assert_eq!(gate.check("set_volume", Risk::Moderate), Decision::Allow);
    }

    #[test]
    fn grant_applies_only_to_the_named_tool() {
        let mut gate = PermissionGate::new();
        gate.grant_always("set_volume");
        assert!(matches!(
            gate.check("set_brightness", Risk::Moderate),
            Decision::Ask(_)
        ));
    }

    #[test]
    fn destructive_budget_overrides_standing_grant() {
        let mut gate = PermissionGate::new();
        gate.grant_always("delete_file");

        // Bütçe içindeyken izin geçerli.
        for _ in 0..DESTRUCTIVE_BUDGET {
            assert_eq!(
                gate.check("delete_file", Risk::Destructive),
                Decision::Allow
            );
            gate.record_execution(Risk::Destructive);
        }

        // Bütçe dolunca izin ARTIK geçersiz — asıl korunan senaryo bu.
        assert_eq!(
            gate.check("delete_file", Risk::Destructive),
            Decision::Ask(ApprovalReason::BudgetExceeded)
        );
    }

    #[test]
    fn new_run_resets_budget_but_keeps_grants() {
        let mut gate = PermissionGate::new();
        gate.grant_always("delete_file");
        for _ in 0..DESTRUCTIVE_BUDGET {
            gate.record_execution(Risk::Destructive);
        }
        assert!(matches!(
            gate.check("delete_file", Risk::Destructive),
            Decision::Ask(_)
        ));

        gate.start_run();
        assert_eq!(gate.destructive_used(), 0);
        assert_eq!(
            gate.check("delete_file", Risk::Destructive),
            Decision::Allow,
            "kalıcı izin yeni çalıştırmada da geçerli olmalı"
        );
    }

    #[test]
    fn safe_tools_do_not_consume_budget() {
        let mut gate = PermissionGate::new();
        for _ in 0..10 {
            gate.record_execution(Risk::Safe);
        }
        assert_eq!(gate.destructive_used(), 0);
    }

    #[test]
    fn revoke_clears_all_grants() {
        let mut gate = PermissionGate::new();
        gate.grant_always("a");
        gate.grant_always("b");
        assert_eq!(gate.granted_tools(), vec!["a", "b"]);

        gate.revoke_all();
        assert!(gate.granted_tools().is_empty());
    }

    /// Tam yetki açıkken hiçbir şey sorulmuyor — anahtarın vaadi bu.
    #[test]
    fn full_authority_asks_nothing_at_all() {
        let mut gate = PermissionGate::new();
        gate.set_full_authority(true);

        for risk in [Risk::Safe, Risk::Moderate, Risk::Destructive] {
            assert_eq!(gate.check("herhangi_bir_arac", risk), Decision::Allow);
        }
    }

    /// Yıkıcı bütçe de kapanıyor. Yarısını açık bırakmak anahtarı yalancı
    /// yapardı: kullanıcı dördüncü silmede yine soru görürdü.
    #[test]
    fn full_authority_outlasts_the_destructive_budget() {
        let mut gate = PermissionGate::new();
        gate.set_full_authority(true);

        for _ in 0..DESTRUCTIVE_BUDGET + 5 {
            assert_eq!(gate.check("dosya_sil", Risk::Destructive), Decision::Allow);
            gate.record_execution(Risk::Destructive);
        }
    }

    /// Full authority trusts the user's requests, not a web page's. After
    /// content that tried to instruct the model, destructive work asks even
    /// with full authority on -- the settings screen says so where the
    /// switch is.
    #[test]
    fn full_authority_does_not_silence_the_injection_guard() {
        let mut gate = PermissionGate::new();
        gate.set_full_authority(true);
        gate.mark_tainted();

        assert_eq!(
            gate.check("run_command", Risk::Destructive),
            Decision::Ask(ApprovalReason::TaintedContext)
        );
        // Looking still goes through; changing anything asks.
        assert_eq!(gate.check("read_file", Risk::Safe), Decision::Allow);
        assert_eq!(
            gate.check("launch_app", Risk::Moderate),
            Decision::Ask(ApprovalReason::TaintedContext)
        );
    }

    /// A standing "always allow" for a moderate tool does not survive a
    /// page that gave orders either: opening an app or scheduling an
    /// automation is enough to turn the page's words into a command.
    #[test]
    fn a_suspect_turn_asks_before_moderate_changes_too() {
        let mut gate = PermissionGate::new();
        gate.grant_always("create_automation");
        assert_eq!(
            gate.check("create_automation", Risk::Moderate),
            Decision::Allow
        );

        gate.mark_tainted();
        assert_eq!(
            gate.check("create_automation", Risk::Moderate),
            Decision::Ask(ApprovalReason::TaintedContext)
        );
        assert_eq!(gate.check("read_file", Risk::Safe), Decision::Allow);
    }

    #[test]
    fn turning_full_authority_off_restores_every_guard() {
        let mut gate = PermissionGate::new();
        gate.set_full_authority(true);
        gate.set_full_authority(false);

        assert_eq!(
            gate.check("dosya_sil", Risk::Destructive),
            Decision::Ask(ApprovalReason::RiskLevel)
        );
    }

    /// Ayardan geliyor, istekten değil: her turda yeniden açtırmak anahtarı
    /// anlamsız kılardı.
    #[test]
    fn a_new_turn_does_not_switch_full_authority_off() {
        let mut gate = PermissionGate::new();
        gate.set_full_authority(true);
        gate.start_run();

        assert!(gate.full_authority());
        assert_eq!(gate.check("dosya_sil", Risk::Destructive), Decision::Allow);
    }

    /// Varsayılan kapalı. Korumalı olan, kullanıcının hiçbir şey seçmediği hâl.
    #[test]
    fn full_authority_is_off_until_it_is_asked_for() {
        let gate = PermissionGate::new();
        assert!(!gate.full_authority());
        assert_eq!(
            gate.check("dosya_sil", Risk::Destructive),
            Decision::Ask(ApprovalReason::RiskLevel)
        );
    }
}
