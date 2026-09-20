//! Kayıtlı tool'ları ve saklanan anahtarla gerçek bir taramayı doğrular.
//!
//! ```text
//! cargo run -p vavis-shell --example check_tools
//! ```

fn main() {
    let reg = vavis_tools::default_registry();
    println!("toplam tool: {}", reg.len());
    for tool in reg.iter() {
        if tool.name().starts_with("virus_") {
            println!("  kayıtlı: {} ({:?})", tool.name(), tool.risk());
        }
    }

    // Saklanan anahtarı üretimdeki yoldan yükle.
    let paths = vavis_core::Paths::discover().expect("paths");
    let store = vavis_brain::KeyStore::load(paths.root());
    let key = store.get("virustotal").unwrap_or_default().to_string();
    println!("anahtar saklı mı: {}", !key.is_empty());
    if key.is_empty() {
        return;
    }
    vavis_tools::virustotal::configure(vavis_tools::virustotal::Settings { api_key: key });

    // Bu dosyanın kendisini tara: yerel hash'lenecek, dosya gitmeyecek.
    let me = std::env::current_exe().expect("exe");
    println!("\ntaranan: {}", me.display());
    let tool = reg.get("virus_tara_dosya").expect("tool kayıtlı");
    let out = tool.run(&serde_json::json!({ "path": me.to_string_lossy() }));
    println!("ok={}\n{}", out.ok, out.content);
}
