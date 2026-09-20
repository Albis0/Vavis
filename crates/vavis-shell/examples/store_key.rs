//! Bir API anahtarını şifreli depoya yazar.
//!
//! Arayüz açmadan anahtar kaydetmek için. Anahtar **argüman olarak değil**,
//! ortam değişkeniyle geçiyor: komut satırı argümanları süreç listesinde ve
//! kabuk geçmişinde görünür.
//!
//! ```text
//! KEY_NAME=virustotal KEY_VALUE=... cargo run -p vavis-shell --example store_key
//! ```

fn main() {
    let name = std::env::var("KEY_NAME").unwrap_or_default();
    let value = std::env::var("KEY_VALUE").unwrap_or_default();

    if name.trim().is_empty() || value.trim().is_empty() {
        eprintln!("KEY_NAME ve KEY_VALUE gerekli");
        std::process::exit(1);
    }

    let paths = match vavis_core::Paths::discover() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("veri klasörü bulunamadı: {e}");
            std::process::exit(1);
        }
    };

    let root = paths.root();
    let mut store = vavis_brain::KeyStore::load(root);
    store.set(name.trim(), value.trim());

    if let Err(e) = store.save(root) {
        eprintln!("kaydedilemedi: {e}");
        std::process::exit(1);
    }

    // Anahtarın kendisi değil, yalnızca hangi adların kayıtlı olduğu.
    println!("kayıtlı: {:?}", store.configured());
}
