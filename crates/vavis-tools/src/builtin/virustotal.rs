//! VirusTotal tool'ları.
//!
//! Hepsi okuma: bu alanda değiştirilecek bir şey yok, sorulacak şey var.
//!
//! **Dosya hiçbir zaman yüklenmiyor.** Dosya sorulduğunda SHA-256'sı yerel
//! hesaplanıp yalnızca o gönderiliyor. VirusTotal'a dosya yüklemek onu
//! kalıcı olarak dışarı çıkarır ve premium hesaplar indirebilir — kullanıcının
//! makinesindeki bir şeyi habersiz oraya göndermek bu projede en baştan
//! çizilen çizgiyi aşar. Hash bilinmiyorsa cevap "bilinmiyor" olur.

use crate::tool::{arg_str, Domain, Param, Risk, Tool, ToolOutcome};
use crate::virustotal::{self, Verdict};
use serde_json::Value;

const WORDS: &[&str] = &[
    "virüs",
    "virus",
    "zararlı",
    "zararli",
    "malware",
    "güvenli",
    "guvenli",
    "tara",
    "virustotal",
    "şüpheli",
    "supheli",
];

/// Bir sonucu modele verilecek metne çevirir.
///
/// Özet en başta: modelin ilk satırda cevabı görmesi, uzun bir listeyi
/// yanlış özetlemesinden iyi.
fn render(v: &Verdict) -> String {
    let mut out = String::new();
    out.push_str(&v.summary());

    if !v.name.is_empty() {
        out.push_str(&format!("\n\nDosya: {}", v.name));
    }

    out.push_str(&format!(
        "\n\nSayılar: {} zararlı · {} şüpheli · {} temiz · {} görüş yok",
        v.malicious, v.suspicious, v.harmless, v.undetected
    ));

    if !v.flagged_by.is_empty() {
        // Hepsini değil: 45 motor adı modele gönderilecek en faydasız
        // metin olurdu. İlk birkaçı hangi tür motorların işaretlediğini
        // göstermeye yetiyor.
        let shown: Vec<&str> = v.flagged_by.iter().take(8).map(|s| s.as_str()).collect();
        out.push_str(&format!("\n\nİşaretleyenler: {}", shown.join(", ")));
        if v.flagged_by.len() > shown.len() {
            out.push_str(&format!(
                " (+{} tane daha)",
                v.flagged_by.len() - shown.len()
            ));
        }
    }

    if !v.permalink.is_empty() {
        out.push_str(&format!("\n\nAyrıntı: {}", v.permalink));
    }

    out
}

/// Bir dosyayı tarar — dosyayı göndermeden.
pub struct ScanFile;

impl Tool for ScanFile {
    fn name(&self) -> &'static str {
        "virus_tara_dosya"
    }

    fn description(&self) -> &'static str {
        "Bir dosyanın VirusTotal'daki itibarını sorar. Dosya YÜKLENMEZ — \
         yalnızca yerel hesaplanan SHA-256 özeti sorulur. Dosya daha önce \
         görülmemişse 'bilinmiyor' döner, ki bu 'temiz' demek değildir."
    }

    fn domain(&self) -> Domain {
        Domain::Security
    }

    fn params(&self) -> Vec<Param> {
        vec![Param::required("path", "taranacak dosyanın tam yolu")]
    }

    fn keywords(&self) -> &'static [&'static str] {
        WORDS
    }

    /// Okuma: dosya diskten okunuyor ama hiçbir yere gitmiyor, ve hiçbir
    /// şey değişmiyor.
    fn risk(&self) -> Risk {
        Risk::Safe
    }

    fn run(&self, args: &Value) -> ToolOutcome {
        let Some(path) = arg_str(args, "path") else {
            return ToolOutcome::err("dosya yolu gerekli".to_string());
        };
        let path = std::path::Path::new(path.trim());
        if !path.exists() {
            return ToolOutcome::err(format!("dosya bulunamadı: {}", path.display()));
        }
        if path.is_dir() {
            return ToolOutcome::err("bu bir klasör — tek bir dosya ver".to_string());
        }

        match virustotal::scan_file(path) {
            Ok(v) => ToolOutcome::ok(render(&v)),
            Err(e) => ToolOutcome::err(e.to_string()),
        }
    }
}

/// Bilinen bir hash'i sorar.
pub struct LookupHash;

impl Tool for LookupHash {
    fn name(&self) -> &'static str {
        "virus_tara_hash"
    }

    fn description(&self) -> &'static str {
        "Bir MD5/SHA-1/SHA-256 özetinin VirusTotal'daki itibarını sorar."
    }

    fn domain(&self) -> Domain {
        Domain::Security
    }

    fn params(&self) -> Vec<Param> {
        vec![Param::required("hash", "MD5, SHA-1 veya SHA-256 özeti")]
    }

    fn keywords(&self) -> &'static [&'static str] {
        WORDS
    }

    fn risk(&self) -> Risk {
        Risk::Safe
    }

    fn run(&self, args: &Value) -> ToolOutcome {
        let Some(hash) = arg_str(args, "hash") else {
            return ToolOutcome::err("hash gerekli".to_string());
        };
        match virustotal::lookup_hash(hash) {
            Ok(v) => ToolOutcome::ok(render(&v)),
            Err(e) => ToolOutcome::err(e.to_string()),
        }
    }
}

/// Bir adresi ya da alan adını sorar.
pub struct CheckUrl;

impl Tool for CheckUrl {
    fn name(&self) -> &'static str {
        "virus_tara_adres"
    }

    fn description(&self) -> &'static str {
        "Bir adresin veya alan adının VirusTotal'daki itibarını sorar. \
         Bir bağlantıya tıklamadan önce sormak için."
    }

    fn domain(&self) -> Domain {
        Domain::Security
    }

    fn params(&self) -> Vec<Param> {
        vec![Param::required("url", "adres veya alan adı")]
    }

    fn keywords(&self) -> &'static [&'static str] {
        WORDS
    }

    fn risk(&self) -> Risk {
        Risk::Safe
    }

    fn run(&self, args: &Value) -> ToolOutcome {
        let Some(raw) = arg_str(args, "url") else {
            return ToolOutcome::err("adres gerekli".to_string());
        };
        let raw = raw.trim();

        // Şema yoksa bu bir alan adıdır: "example.com" yazan kullanıcı
        // alan adı sorusu soruyor, ve alan adı uç noktası daha faydalı
        // cevap veriyor (kayıt tarihi, itibar geçmişi).
        let looks_like_url = raw.starts_with("http://") || raw.starts_with("https://");
        let result = if looks_like_url {
            virustotal::lookup_url(raw)
        } else {
            virustotal::lookup_domain(raw)
        };

        match result {
            Ok(v) => ToolOutcome::ok(render(&v)),
            Err(e) => ToolOutcome::err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verdict(malicious: u32, flagged: Vec<&str>) -> Verdict {
        Verdict {
            subject: "x".into(),
            malicious,
            suspicious: 0,
            harmless: 60,
            undetected: 10u32.saturating_sub(flagged.len() as u32),
            name: "setup.exe".into(),
            flagged_by: flagged.into_iter().map(String::from).collect(),
            permalink: "https://example.invalid/x".into(),
        }
    }

    /// Özet ilk satırda olmalı: model uzun bir listeyi yanlış özetlemesin.
    #[test]
    fn the_summary_comes_first() {
        let text = render(&verdict(0, vec![]));
        let first = text.lines().next().unwrap();
        assert!(first.contains("Temiz"), "ilk satır: {first}");
    }

    /// 45 motor adı modele gönderilecek en faydasız metin olurdu.
    #[test]
    fn a_long_engine_list_is_trimmed_and_says_so() {
        let many: Vec<&str> = vec!["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L"];
        let text = render(&verdict(12, many));
        assert!(text.contains("+4 tane daha"), "dedi ki: {text}");
    }

    #[test]
    fn a_short_engine_list_is_shown_whole() {
        let text = render(&verdict(2, vec!["Kaspersky", "Microsoft"]));
        assert!(text.contains("Kaspersky, Microsoft"), "dedi ki: {text}");
        assert!(!text.contains("tane daha"), "kısaltmamalı: {text}");
    }

    /// Temiz sonuçta işaretleyen listesi hiç görünmemeli.
    #[test]
    fn a_clean_result_carries_no_engine_list() {
        let text = render(&verdict(0, vec![]));
        assert!(!text.contains("İşaretleyenler"), "dedi ki: {text}");
    }

    /// Var olmayan dosya ağa hiç çıkmamalı.
    #[test]
    fn a_missing_file_is_refused_before_any_request() {
        let out = ScanFile.run(&serde_json::json!({ "path": "C:/yok/olmayan-dosya.bin" }));
        assert!(!out.ok);
        assert!(
            out.content.contains("bulunamadı"),
            "dedi ki: {}",
            out.content
        );
    }

    /// Klasör verilince hash'lemeye kalkışmamalı.
    #[test]
    fn a_directory_is_refused() {
        let dir = std::env::temp_dir();
        let out = ScanFile.run(&serde_json::json!({ "path": dir.to_str().unwrap() }));
        assert!(!out.ok);
        assert!(out.content.contains("klasör"), "dedi ki: {}", out.content);
    }

    /// Hepsi okuma: hiçbiri onay kapısına takılmamalı.
    #[test]
    fn every_tool_here_is_read_only() {
        assert_eq!(ScanFile.risk(), Risk::Safe);
        assert_eq!(LookupHash.risk(), Risk::Safe);
        assert_eq!(CheckUrl.risk(), Risk::Safe);
    }
}
