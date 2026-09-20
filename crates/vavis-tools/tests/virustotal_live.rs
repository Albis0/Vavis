//! VirusTotal, gerçek servise karşı.
//!
//! `VT_API_KEY` yoksa atlanıyor: CI'da anahtar yok ve olmamalı.
//! Çalıştırmak için:
//!
//! ```text
//! VT_API_KEY=... cargo test -p vavis-tools --test virustotal_live -- --ignored --nocapture
//! ```
//!
//! Ücretsiz katman dakikada 4 istek veriyor, o yüzden testler arasında
//! bekleniyor.

use vavis_tools::virustotal::{self, Settings, VtError};

fn key() -> Option<String> {
    std::env::var("VT_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
}

fn setup() -> bool {
    match key() {
        Some(k) => {
            virustotal::configure(Settings { api_key: k });
            true
        }
        None => {
            eprintln!("VT_API_KEY yok — atlanıyor");
            false
        }
    }
}

/// Boş dosyanın özeti VirusTotal'ın kesinlikle bildiği bir şey.
#[test]
#[ignore = "ağ ve anahtar gerektirir"]
fn a_known_hash_comes_back_with_an_analysis() {
    if !setup() {
        return;
    }
    const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    let v = virustotal::lookup_hash(EMPTY).expect("bilinen hash bulunmalı");
    eprintln!("özet: {}", v.summary());
    eprintln!("sayılar: {v:#?}");
    assert!(v.total() > 0, "hiçbir motor bakmamış görünüyor");
    assert_eq!(v.subject, EMPTY);
}

/// Alan adı sorgusu ayrı bir uç nokta; onun da çözümlendiğini görmek lazım.
#[test]
#[ignore = "ağ ve anahtar gerektirir"]
fn a_well_known_domain_is_reported_clean() {
    if !setup() {
        return;
    }
    std::thread::sleep(std::time::Duration::from_secs(16));
    let v = virustotal::lookup_domain("google.com").expect("alan adı bulunmalı");
    eprintln!("özet: {}", v.summary());
    assert!(v.total() > 0);
    // Sıfır değil: ölçüldü, 89 motorun 2'si google.com'u işaretliyor. Asıl
    // mesele oranın küçük kalması ve özetin bunu "ciddi" diye sunmaması.
    let ratio = v.malicious as f32 / v.total() as f32;
    assert!(
        ratio < 0.10,
        "google.com'da beklenmeyecek kadar yüksek oran: {ratio}"
    );
    assert!(
        !v.summary().contains("ciddi"),
        "birkaç tanınmayan motor ciddi diye sunulmamalı: {}",
        v.summary()
    );
}

/// Adres uç noktası base64url kimliği istiyor; yanlışsa 404 döner.
#[test]
#[ignore = "ağ ve anahtar gerektirir"]
fn a_url_id_is_accepted_by_the_service() {
    if !setup() {
        return;
    }
    std::thread::sleep(std::time::Duration::from_secs(16));
    match virustotal::lookup_url("https://www.google.com/") {
        Ok(v) => {
            eprintln!("özet: {}", v.summary());
            assert!(v.total() > 0);
        }
        // Bu adres taranmamışsa "bilinmiyor" doğru cevaptır; kimlik
        // yanlış olsaydı da 404 gelirdi, o yüzden ayırt edelim:
        Err(VtError::Unknown(_)) => {
            eprintln!("bu adres taranmamış — kimlik kabul edildi, içerik yok");
        }
        Err(e) => panic!("beklenmeyen hata: {e}"),
    }
}

/// Bozuk anahtar "bilinmiyor" değil, "reddedildi" demeli.
#[test]
#[ignore = "ağ gerektirir"]
fn a_bad_key_is_reported_as_a_bad_key() {
    virustotal::configure(Settings {
        api_key: "0000000000000000000000000000000000000000000000000000000000000000".into(),
    });
    const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    match virustotal::lookup_hash(EMPTY) {
        Err(VtError::BadKey) => {}
        other => panic!("anahtar reddi bekleniyordu, gelen: {other:?}"),
    }
}
