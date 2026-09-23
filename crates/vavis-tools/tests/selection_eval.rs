//! Tool seçimi değerlendirmesi.
//!
//! Eski projede `tool-selection-eval.mjs` vardı ve %100 skoru korunuyordu.
//! Buradaki karşılığı: gerçekçi kullanıcı cümleleri → doğru tool sunuldu mu?
//!
//! **Bu testin amacı birim testten farklı:** birim testler tek tek davranışı
//! doğrular, bu ise seçim kalitesini bir bütün olarak ölçer. Skor düşerse
//! seçim mantığında bir gerileme var demektir.

use vavis_tools::{default_registry, selection, DEFAULT_TOOL_BUDGET};

/// (kullanıcı cümlesi, sunulması BEKLENEN tool)
const EXPECT_OFFERED: &[(&str, &str)] = &[
    // Sistem
    ("cpu kullanımı ne durumda", "get_system_status"),
    ("ram ne kadar dolu", "get_system_status"),
    ("bilgisayarın performansı nasıl", "get_system_status"),
    ("pil yüzde kaç", "get_battery"),
    ("şarj var mı", "get_battery"),
    ("sesi 30 yap", "set_volume"),
    ("ses seviyesini kıs", "set_volume"),
    ("hangi uygulamalar çalışıyor", "list_processes"),
    ("en çok ram yiyen program hangisi", "list_processes"),
    // Dosya
    ("masaüstündeki dosyaları listele", "list_directory"),
    ("indirilenler klasöründe neler var", "list_directory"),
    ("şu dosyayı oku", "read_file"),
    ("belgenin içeriğini göster", "read_file"),
    ("bir dosyaya not kaydet", "write_file"),
    ("rapor geçen dosyaları bul", "search_files"),
    // Hafıza
    ("beni hatırla: kahveyi sade içerim", "remember"),
    ("şunu unutma: toplantı salı günü", "remember"),
    ("hakkımda ne biliyorsun", "search_memory"),
    ("sana ne söylemiştim", "search_memory"),
    // Kontrol (sistem degistirme)
    ("parlaklığı azalt", "set_brightness"),
    ("ekranı karart", "set_brightness"),
    ("notepad aç", "launch_app"),
    ("chrome başlat", "launch_app"),
    ("spotify uygulamasını kapat", "close_app"),
    ("panodaki metni oku", "read_clipboard"),
    ("şunu panoya kopyala", "write_clipboard"),
    ("bir powershell komutu çalıştır", "run_command"),
    // Medya
    ("müziği duraklat", "media_control"),
    ("sonraki şarkıya geç", "media_control"),
    ("spotify'da ne çalıyor", "get_now_playing"),
    // Otomasyon
    ("her sabah 9da hava durumunu söyle", "create_automation"),
    ("pil azalınca beni uyar", "create_automation"),
    ("kurulu otomasyonları listele", "list_automations"),
    // Görü
    ("ekranımda ne var", "take_screenshot"),
    ("ekran görüntüsü al", "take_screenshot"),
    // Görü — kontrollere adıyla ulaşmak
    ("kaydet düğmesine bas", "click_element"),
    ("bu penceredeki butonları listele", "list_ui_elements"),
    ("arama kutusuna vavis yaz ve doldur", "set_element_text"),
    ("sayfayı aşağı kaydır", "scroll"),
    ("dosyayı klasöre sürükle", "drag"),
    // Cekirdek (alan tetiklendiginde birlikte gelir)
    ("cpu durumu ve saat", "get_current_time"),
    // Cekirdek TEK BASINA — bir zamanlar hicbiri sunulmuyordu ve model
    // saati uyduruyordu. Bu satirlar o hatanin geri gelmemesi icin.
    ("saat kaç", "get_current_time"),
    ("saat", "get_current_time"),
    ("bugün ayın kaçı", "get_current_time"),
    ("bugün günlerden ne", "get_current_time"),
    ("tarih ne", "get_current_time"),
    ("what time is it", "get_current_time"),
    ("15 * 3 kaç eder", "calculate"),
    ("şunu hesapla: 120 / 4", "calculate"),
    ("240 ın yüzde 18 i ne kadar", "calculate"),
];

/// Bu cümlelerde **hiç** tool sunulmamalı — sohbet.
const EXPECT_NO_TOOLS: &[&str] = &[
    "merhaba",
    "selam nasılsın",
    "teşekkürler",
    "tamam",
    "bana bir şiir yaz",
    "bir fıkra anlat",
    "python nedir",
    "ne düşünüyorsun",
    "kendini tanıt",
    "iyi geceler",
];

#[test]
fn offered_tool_eval_scores_100_percent() {
    let reg = default_registry();
    let mut failures = Vec::new();

    for (msg, expected) in EXPECT_OFFERED {
        let offered = selection::select_named(&reg, msg, DEFAULT_TOOL_BUDGET);
        if !offered.contains(expected) {
            failures.push(format!(
                "  '{msg}' → {expected} bekleniyordu, sunulan: {offered:?}"
            ));
        }
    }

    let total = EXPECT_OFFERED.len();
    let passed = total - failures.len();
    let score = passed as f64 / total as f64 * 100.0;

    assert!(
        failures.is_empty(),
        "\nTool sunum skoru: {score:.0}% ({passed}/{total})\nBaşarısızlar:\n{}",
        failures.join("\n")
    );
}

#[test]
fn conversational_messages_get_zero_tools() {
    let reg = default_registry();
    let mut failures = Vec::new();

    for msg in EXPECT_NO_TOOLS {
        let offered = selection::select_named(&reg, msg, DEFAULT_TOOL_BUDGET);
        if !offered.is_empty() {
            failures.push(format!("  '{msg}' → tool sunulmamalıydı: {offered:?}"));
        }
    }

    assert!(
        failures.is_empty(),
        "\n{} sohbet mesajında tool sunuldu:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// **Asıl garanti.** Hiçbir girdi sınırı aşamamalı.
#[test]
fn tool_count_never_exceeds_limit_on_any_input() {
    let reg = default_registry();

    let adversarial = [
        "dosya klasör sistem ses cpu ram disk web ara hatırla oku yaz sil listele bul",
        "dosya dosya dosya sistem sistem web web hafıza hafıza",
        &"dosya sistem web hatırla ".repeat(50),
    ];

    for msg in adversarial {
        let offered = selection::select_named(&reg, msg, DEFAULT_TOOL_BUDGET);
        assert!(
            offered.len() <= DEFAULT_TOOL_BUDGET,
            "'{}' için {} tool sunuldu (sınır {DEFAULT_TOOL_BUDGET})",
            &msg[..msg.len().min(40)],
            offered.len()
        );
    }

    // Tüm eval cümleleri için de geçerli olmalı.
    for (msg, _) in EXPECT_OFFERED {
        assert!(
            selection::select_named(&reg, msg, DEFAULT_TOOL_BUDGET).len() <= DEFAULT_TOOL_BUDGET
        );
    }
}

/// Sunulan tool sayısı gerçekte ne kadar? Ortalama düşük olmalı.
///
/// Eski projede model 64 tool görüyordu. Buradaki ortalama bunu somut kılar.
#[test]
fn average_offered_tool_count_stays_small() {
    let reg = default_registry();
    let counts: Vec<usize> = EXPECT_OFFERED
        .iter()
        .map(|(msg, _)| selection::select_named(&reg, msg, DEFAULT_TOOL_BUDGET).len())
        .collect();

    let avg = counts.iter().sum::<usize>() as f64 / counts.len() as f64;
    let max = counts.iter().copied().max().unwrap_or(0);

    println!("average tools offered: {avg:.1} · most: {max}");
    if std::env::var_os("VAVIS_EVAL_DETAIL").is_some() {
        for ((msg, _), n) in EXPECT_OFFERED.iter().zip(&counts) {
            println!("{n:>3}  {msg}");
        }
    }
    assert!(
        avg <= 8.0,
        "ortalama {avg:.1} tool sunuluyor — model şaşırabilir"
    );
}

/// Şemalar sağlayıcıya gönderilebilir biçimde olmalı.
#[test]
fn generated_schemas_are_provider_compatible() {
    let reg = default_registry();
    let schemas =
        selection::select_tools(&reg, "masaüstündeki dosyaları listele", DEFAULT_TOOL_BUDGET);
    assert!(!schemas.is_empty());

    for s in &schemas {
        assert_eq!(s["type"], "function", "tip 'function' olmalı");

        let f = &s["function"];
        assert!(f["name"].is_string(), "ad zorunlu");
        assert!(
            !f["description"].as_str().unwrap_or_default().is_empty(),
            "açıklama boş olmamalı — model neye yaradığını bilemez"
        );

        let params = &f["parameters"];
        assert_eq!(params["type"], "object");
        assert!(params["required"].is_array());

        // Sayısal parametreler dahil HER ŞEY string olmalı — sağlayıcılar
        // sayı beklerken string alınca tool_use_failed veriyor.
        if let Some(props) = params["properties"].as_object() {
            for (name, spec) in props {
                assert_eq!(spec["type"], "string", "{name} string olmalı");
            }
        }
    }
}

/// Aynı tool iki kez sunulmamalı — model kafası karışır.
#[test]
fn no_duplicate_tools_are_offered() {
    let reg = default_registry();
    for (msg, _) in EXPECT_OFFERED {
        let offered = selection::select_named(&reg, msg, DEFAULT_TOOL_BUDGET);
        let mut sorted = offered.clone();
        sorted.sort_unstable();
        let before = sorted.len();
        sorted.dedup();
        assert_eq!(
            before,
            sorted.len(),
            "'{msg}' tekrarlı tool içeriyor: {offered:?}"
        );
    }
}

/// What a two-word message actually costs before any history exists.
///
/// Reported, not asserted: this exists to answer "why was a request with two
/// letters in it refused for being too long", where the answer has to be a
/// number rather than a guess.
#[test]
#[ignore = "measurement only — run with --ignored"]
fn measure_fixed_overhead_of_a_trivial_message() {
    let reg = default_registry();

    for msg in ["sa", "merhaba", "su projeyi devam ettir"] {
        let schemas = selection::select_tools(&reg, msg, DEFAULT_TOOL_BUDGET);
        let tool_tokens: usize = schemas
            .iter()
            .map(|s| vavis_brain::estimate_tokens(&s.to_string()))
            .sum();

        println!("\n=== {msg:?} ===");
        println!("tools offered : {}", schemas.len());
        println!("tool tokens   : {tool_tokens}");

        let mut named: Vec<(String, usize)> = schemas
            .iter()
            .map(|s| {
                (
                    s["function"]["name"].as_str().unwrap_or("?").to_string(),
                    vavis_brain::estimate_tokens(&s.to_string()),
                )
            })
            .collect();
        named.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        for (name, n) in named {
            println!("  {name:<34} {n:>6}");
        }
    }
}
