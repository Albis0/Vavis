//! Tool seçimi — projenin en kritik kararı.
//!
//! # Neden bu dosya var
//!
//! Eski projede 353 tool tanımlıydı ve modele her istekte **64 tanesi**
//! gönderiliyordu. Hiçbir LLM 64 seçenek arasından güvenilir seçim yapamaz;
//! kullanıcının "tool'lar aşırı gidiyor" şikayetinin sebebi buydu.
//!
//! Seçim mantığı da yamalar zinciriydi: kelime kökü eşleştirme → STICKY ESCAPE
//! → STICKY CONTEXT → BM25 kurtarma → CHATTER listesi → priorityCore →
//! `slice(0, 64)`. Her yama bir öncekinin açtığı deliği kapatıyordu.
//!
//! # Buradaki kural
//!
//! **Sadece gerekenler gider — bütçe kadarı değil.**
//!
//! İki kademe:
//!   1. Mesajdan **alan** seçilir (dosya mı, sistem mi, web mi…).
//!   2. Sadece o alanın + çekirdeğin tool'ları sunulur.
//!
//! Alan bulunamazsa hiç tool gönderilmez (sohbet mesajı) — bu bilinçli:
//! "merhaba" için tool listesi göndermek modeli boş yere kışkırtıyor.
//!
//! # Bütçe neden model başına
//!
//! Bir zamanlar burada tek bir sabit vardı: modele asla 12'den fazla tool
//! gönderilmezdi. O sabit bir kalite kapısı gibi görünüyordu ama aslında
//! **en zayıf modelin sınırını herkese dayatıyordu.** Otuz şema arasında
//! kaybolan küçük bir Llama ile rahatça seçen bir Opus aynı kafesteydi.
//!
//! Artık bütçe modelden geliyor (`ModelCaps::tool_budget`). İki şeyi
//! birbirinden ayırmak gerekiyor:
//!
//! * **Bütçe bir tavan, hedef değil.** Yüksek bütçe "listeyi doldur" demek
//!   değil. Gereksiz tool hem faturayı şişirir (API başına ödeyen için
//!   gerçek para) hem modeli kışkırtır.
//! * **Alan eşleşmesi yine de daraltır.** Bütçe 48 olsa bile "dosyaları
//!   listele" mesajına Spotify tool'u gitmez.

use crate::tool::{Domain, Registry};

use serde_json::Value;

/// Bütçe bilinmediğinde kullanılacak temkinli varsayılan.
///
/// Gerçek bütçe `ModelCaps::tool_budget`'tan gelir; bu sabit yalnızca
/// model bilgisi olmayan çağrılar (testler, günlük kaydı) içindir.
pub const DEFAULT_TOOL_BUDGET: usize = 12;

/// Bir alanın eşleştiği anahtar kelimeler (Türkçe + İngilizce).
///
/// Kök eşleştirme yerine **tam kelime içerme** kullanılıyor: Türkçe sondan
/// eklemeli olduğu için "dosyayı", "dosyada", "dosyalar" hepsi "dosya" içerir.
struct DomainKeywords {
    domain: Domain,
    words: &'static [&'static str],
}

/// Tek başına bir alan tetiklemeye yetmeyen genel kelimeler.
///
/// **Neden gerekli:** "bana bir şiir yaz" cümlesindeki "yaz", dosya yazma
/// isteği değil. Eski projede tam olarak bu tür yanlış eşleşmeler modele
/// alakasız tool'lar sunuyordu — kullanıcının "tool'lar aşırı gidiyor"
/// şikayetinin bir kaynağı buydu.
///
/// Bu kelimeler ancak **alana özgü bir kelimeyle birlikte** sayılır:
/// "dosya yaz" → Files ✓ · "şiir yaz" → eşleşme yok ✓
///
/// Fiillerin yanında birkaç **zaman ismi** de burada: "bugün" ve "gün"
/// çoğu zaman soru değil, sohbet dolgusu ("bugün nasılsın", "iyi günler").
/// Gerçek bir tarih sorusunda yanlarında "saat", "date" ya da "kaçı"
/// gibi güçlü bir kelime bulunuyor.
const WEAK_VERBS: &[&str] = &[
    "yaz", "oku", "kaydet", "sil", "listele", "ara", "bul", "ac", "kapat", "goster", "write",
    "read", "save", "delete", "list", "search", "find", "open", "close", "show", "bugun", "gun",
    "zaman", "today",
];

const DOMAIN_KEYWORDS: &[DomainKeywords] = &[
    DomainKeywords {
        domain: Domain::Files,
        words: &[
            "dosya",
            "klasör",
            "klasor",
            "dizin",
            "oku",
            "yaz",
            "kaydet",
            "sil",
            "listele",
            "file",
            "folder",
            "directory",
            "read",
            "write",
            "save",
            "delete",
            "list",
            "path",
            "yol",
            "belge",
            "txt",
            "içerik",
            "icerik",
        ],
    },
    // OKUMA — durum sorgulama.
    DomainKeywords {
        domain: Domain::System,
        words: &[
            "cpu",
            "ram",
            "bellek",
            "memory",
            "disk",
            "pil",
            "batarya",
            "battery",
            "sistem",
            "system",
            "işlem",
            "islem",
            "süreç",
            "surec",
            "process",
            "performans",
            "durum",
            "telemetri",
            "sarj",
            "şarj",
            "prize",
            "pencere",
            "window",
            "calisan",
            "çalışan",
            "calisiyor",
            "çalışıyor",
            "acik",
            "açık",
            "kullanan",
            "yiyen",
            "tuketiyor",
            "tüketiyor",
        ],
    },
    // DEĞİŞTİRME — ayar yapma, uygulama, komut, pano.
    //
    // Okumadan ayrı: tek alanda 11 tool birikince çekirdek tool'lar
    // sınırın dışına itiliyordu (eval %95'e düşmüştü).
    DomainKeywords {
        domain: Domain::Control,
        words: &[
            "ses",
            "sesi",
            "volume",
            "sessiz",
            "kis",
            "kıs",
            "parlak",
            "brightness",
            "karart",
            "aydinlat",
            "aydınlat",
            "isik",
            "ışık",
            "uygulama",
            "program",
            "başlat",
            "baslat",
            "kapat",
            "sonlandir",
            "sonlandır",
            "komut",
            "command",
            "powershell",
            "script",
            "pano",
            "clipboard",
            "kopyala",
            "yapistir",
            "yapıştır",
            // Cihazın kendisi. "kapat" / "başlat" zayıf fiil olduğu için
            // tek başına alan tetiklemiyor; onlara eşlik edecek güçlü bir
            // kelime gerekiyor ve kullanıcının söylediği kelime bu.
            "bilgisayar",
            "makine",
            "computer",
            "pc",
            // Tarayıcı ve sık açılan uygulamalar: "aç" da zayıf fiil.
            "tarayıcı",
            "tarayici",
            "browser",
            "notepad",
            "chrome",
            "spotify",
            "explorer",
        ],
    },
    DomainKeywords {
        domain: Domain::Vision,
        words: &[
            // "bak" ve "gör" bilinçli olarak YOK: "şunu oku bakalım" cümlesi
            // ekran görüntüsü isteği değil. Ekrana özgü kelimeler gerekli.
            "ekran",
            "goruntu",
            "görüntü",
            "screenshot",
            "gorunuyor",
            "görünüyor",
            "ekranda",
            "ekranima",
            "ekranıma",
        ],
    },
    DomainKeywords {
        domain: Domain::Web,
        words: &[
            "web",
            "internet",
            "ara",
            "arama",
            "search",
            "google",
            "site",
            "sayfa",
            "page",
            "url",
            "link",
            "haber",
            "news",
            "hava",
            "weather",
            "fiyat",
            "price",
            "güncel",
            "guncel",
            "bul",
            "araştır",
            "arastir",
            "indir",
            "download",
        ],
    },
    DomainKeywords {
        domain: Domain::Media,
        words: &[
            "muzik", "müzik", "sarki", "şarkı", "parca", "parça", "spotify", "cal", "çal", "oynat",
            "duraklat", "sonraki", "onceki", "önceki", "medya", "video", "youtube", "vlc", "album",
            "albüm", "calan", "çalan",
        ],
    },
    DomainKeywords {
        domain: Domain::Automation,
        words: &[
            "otomasyon",
            "zamanla",
            "zamanlanmis",
            "zamanlanmış",
            "otomatik",
            // "gece" YOK: "iyi geceler" selamlaması otomasyon tetikliyordu.
            // Saat ifadeleri zaten "09:00" biçiminde geliyor.
            // "sabah" burada güvenli: "iyi geceler"i bozmuyor, "her sabah X yap"
            // cümlesini yakalıyor. "gece" ise selamlamaya çarptığı için YOK.
            "periyodik",
            "tekrarla",
            "sabah",
            "azalinca",
            "azalınca",
            "olunca",
            "inince",
            "cikinca",
            "çıkınca",
            "hatirlat",
            "hatırlat",
            "uyar",
            "alarm",
            "gorev",
            "görev",
        ],
    },
    DomainKeywords {
        domain: Domain::Memory,
        words: &[
            "hatırla",
            "hatirla",
            "hatırlat",
            "hatirlat",
            "unut",
            "hafıza",
            "hafiza",
            "remember",
            "forget",
            "memory",
            "not",
            "note",
            "kaydettiğim",
            "kaydettigim",
            "biliyor",
            "söylemiştim",
            "soylemistim",
            // "bilgi" **bilerek yok**: eşleşme ek toleranslı olduğu için
            // "bilgisayar" kelimesini de yakalıyordu. Sonucu sessizdi —
            // "bilgisayarı kapat" hafıza alanına düşüyor, kontrol araçları
            // modele hiç ulaşmıyordu. Bir asistanda "bilgisayar" en sık
            // geçen kelimelerden biri, yani bu tek satır çok sayıda isteği
            // yanlış yere gönderiyordu.
            //
            // Hafıza isteğini belli eden kelimeler zaten yukarıda:
            // "hatırla", "unut", "hafıza", "söylemiştim".
            "hatırladığın",
            "hatirladigin",
        ],
    },
    DomainKeywords {
        domain: Domain::Obsidian,
        // Kasaya özgü kelimeler seçildi: "not" tek başına Memory alanına ait
        // (asistanın kullanıcı hakkında hatırladıkları). Buradaki kelimeler
        // kullanıcının **kendi** not kasasını işaret ediyor.
        words: &[
            "obsidian",
            "kasa",
            "vault",
            "notlar",
            "notum",
            "notuma",
            "notumda",
            "notlarim",
            "notlarım",
            "günlük",
            "gunluk",
            "daily",
            "etiket",
            "tag",
            "backlink",
            "wikilink",
            "markdown",
            // "md" bilinçli olarak yok: eşleşme "içerir" mantığıyla çalıştığı
            // için "durumda", "hakkımda", "ekranımda" gibi kelimeleri
            // yakalıyor ve alakasız isteklere 9 tool ekliyordu.
        ],
    },
    DomainKeywords {
        domain: Domain::Steam,
        words: &[
            "steam",
            "oyun",
            "oyunu",
            "oyunlar",
            "oynadığım",
            "oynadigim",
            "kütüphane",
            "kutuphane",
            "başarım",
            "basarim",
            "achievement",
            "wishlist",
            "indirim",
            "arkadaş",
            "arkadas",
        ],
    },
    DomainKeywords {
        domain: Domain::Spotify,
        // "cal"/"çal" bilinçli olarak yok: "içerir" eşleşmesi yüzünden
        // "çalışıyor", "çalıştır" kelimelerini de yakalar ve 7 tool'u
        // alakasız isteklere sokar. Medya alanı zaten o kelimeleri
        // kapsıyor ve yalnızca 2 tool taşıyor.
        words: &[
            "spotify",
            "şarkı",
            "sarki",
            "müzik",
            "muzik",
            "playlist",
            "kuyruk",
            "shuffle",
            "karıştır",
            "karistir",
            "sanatçı",
            "sanatci",
        ],
    },
    DomainKeywords {
        domain: Domain::Canvas,
        // "çiz" ve "resim çiz" gibi kısa kökler bilinçli olarak yok:
        // "çizelge", "çizgi" gibi kelimeleri yakalıyor. Alan tek tool
        // taşıdığı için yanlış eşleşmenin bedeli küçük ama sıfır değil.
        words: crate::builtin::canvas::KEYWORDS,
    },
    // ÇEKİRDEK — saat/tarih ve hesap.
    //
    // Bu satır uzun süre eksikti ve sonucu şuydu: "saat kaç" hiçbir alanı
    // tetiklemiyor, alan listesi boş kalınca `select_named` erkenden dönüyor,
    // `get_current_time` modele **hiç** sunulmuyordu. Model de saati uyduruyordu.
    //
    // Çekirdek tool'lar başka bir alan tetiklendiğinde zaten ekleniyor; bu
    // satır onları **tek başlarına** da erişilebilir yapıyor.
    DomainKeywords {
        domain: Domain::Core,
        words: &[
            "saat",
            "tarih",
            // "bugün"/"gün"/"zaman" WEAK_VERBS'te: tek başlarına sohbet
            // dolgusu ("bugün nasılsın"). Aşağıdaki güçlü kelimelerden biri
            // yanlarındaysa istek gerçekten tarih/saat sorusudur.
            "bugun",
            "bugün",
            "gun",
            "gün",
            "zaman",
            "gunlerden",
            "günlerden",
            "kaci",
            "kaçı",
            "hesapla",
            "hesap",
            "topla",
            "carp",
            "çarp",
            "bol",
            "böl",
            "cikar",
            "çıkar",
            "yuzde",
            "yüzde",
            "time",
            "date",
            "today",
            "calculate",
            "compute",
        ],
    },
];

/// Sohbet kapatıcıları — bunlara tool sunulmaz.
const CHATTER: &[&str] = &[
    "merhaba",
    "selam",
    "teşekkür",
    "tesekkur",
    "teşekkürler",
    "tesekkurler",
    "sağol",
    "sagol",
    "tamam",
    "ok",
    "okey",
    "peki",
    "evet",
    "hayır",
    "hayir",
    "güzel",
    "guzel",
    "harika",
    "süper",
    "super",
    "hello",
    "hi",
    "thanks",
    "thank",
    "yes",
    "no",
    "cool",
    "nice",
    "great",
    "bye",
    "görüşürüz",
    "gorusuruz",
];

/// Metni karşılaştırma için normalleştirir: küçük harf + Türkçe sadeleştirme.
///
/// Türkçe'de `İ`'nin küçüğü `i` değil `i̇` olabildiği için `to_lowercase()`
/// tek başına yetmiyor; ayrıca kullanıcılar sıklıkla ASCII yazıyor ("dosya"
/// yerine "dosya", "açık" yerine "acik").
fn normalize(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'ı' | 'İ' | 'î' => 'i',
            'ş' => 's',
            'ğ' => 'g',
            'ü' => 'u',
            'ö' => 'o',
            'ç' => 'c',
            'â' => 'a',
            other => other,
        })
        .collect()
}

fn tokenize(text: &str) -> Vec<String> {
    normalize(text)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(String::from)
        .collect()
}

/// Bir kelime, anahtar kelimenin **ekli hâli** mi.
///
/// Türkçe sondan eklemeli: "dosya" araması "dosyayı", "dosyada",
/// "dosyalarım" kelimelerini de yakalamalı. Bunun için kelimenin anahtar
/// kelimeyle **başlaması** yetiyor.
///
/// # Neden `contains` değil
///
/// Önceden `w.contains(kw)` yazıyordu, yani anahtar kelime **kelimenin
/// herhangi bir yerinde** eşleşiyordu. Sonucu sessiz ve yaygındı:
///
/// - `"bilgisayar"` içinde `"bilgi"` var → **"bilgisayarı kapat"** cümlesi
///   hafıza alanına düşüyor, kontrol araçları modele hiç ulaşmıyordu.
/// - Aynı sebeple "bilgisayar" geçen her cümle yanlış alana gidiyordu, ve
///   bir asistanda bu kelime en sık geçenlerden biri.
///
/// Sonek eşleşmesi Türkçe için doğru olanı yapıyor: ek **sonda** gelir.
/// Bir ön ek yüzünden eşleşme gerekiyorsa (nadir), anahtar kelimenin
/// kendisi tabloya eklenmeli — sessiz bir alt dizge kuralı değil.
fn matches_with_suffix(word: &str, keyword: &str) -> bool {
    word == keyword || word.starts_with(keyword)
}

/// Mesaj sadece nezaket/onay ifadesi mi?
fn is_chatter(words: &[String]) -> bool {
    if words.is_empty() || words.len() > 3 {
        return false;
    }
    let normalized_chatter: Vec<String> = CHATTER.iter().map(|c| normalize(c)).collect();
    words
        .iter()
        .all(|w| normalized_chatter.iter().any(|c| c == w))
}

/// Mesaj bir aritmetik ifade taşıyor mu — "15 * 3", "(120+8)/4".
///
/// Anahtar kelime tablosu bunu yakalayamıyor: "15 * 3 kaç eder" cümlesinde
/// hesap isteğini belli eden şey kelimeler değil, **iki sayı arasındaki
/// operatör**. Tek bir sayı ("3 dosya sil") ya da tek bir işaret ("a * b")
/// yetmiyor; ikisi birden gerekiyor.
fn has_arithmetic(message: &str) -> bool {
    let bytes = message.as_bytes();
    let operator_at = |i: usize| matches!(bytes[i], b'+' | b'*' | b'/' | b'%' | b'-');

    (0..bytes.len()).filter(|&i| operator_at(i)).any(|i| {
        let before = message[..i].trim_end();
        let after = message[i + 1..].trim_start();
        before.ends_with(|c: char| c.is_ascii_digit())
            && after.starts_with(|c: char| c.is_ascii_digit())
    })
}

/// Mesaja uyan alanlar, en çok eşleşenden aza doğru.
pub fn match_domains(message: &str) -> Vec<Domain> {
    let words = tokenize(message);
    if is_chatter(&words) {
        return Vec::new();
    }

    let weak: Vec<String> = WEAK_VERBS.iter().map(|v| normalize(v)).collect();

    let mut scored: Vec<(Domain, usize)> = DOMAIN_KEYWORDS
        .iter()
        .map(|dk| {
            // Güçlü ve zayıf eşleşmeleri ayrı say: zayıf olanlar (genel fiiller)
            // tek başına alan tetikleyemez.
            let (mut strong, mut weak_hits) = (0usize, 0usize);

            for kw in dk.words {
                let kw = normalize(kw);
                // Kelime tam eşleşir ya da anahtar kelimeyle **başlar**
                // ("dosyayı" ⊃ "dosya") — Türkçe ekler için.
                if words.iter().any(|w| matches_with_suffix(w, &kw)) {
                    if weak.contains(&kw) {
                        weak_hits += 1;
                    } else {
                        strong += 1;
                    }
                }
            }

            // Sadece zayıf fiil eşleşti → bu alan değil ("şiir yaz").
            let mut score = if strong == 0 {
                0
            } else {
                strong * 2 + weak_hits
            };

            // Aritmetik ifade, kelimeye ihtiyaç duymadan çekirdeği tetikler:
            // "15 * 3 kaç eder" cümlesinde niyeti belli eden şey operatör.
            if dk.domain == Domain::Core && has_arithmetic(message) {
                score += 2;
            }

            // Otomasyon niyeti diğer alanları GÖLGELER.
            //
            // "her sabah 9'da hava durumunu söyle" cümlesinde "hava" (Web) ve
            // "durum" (System) de eşleşiyor — ama kullanıcı bir otomasyon
            // kuruyor, hava durumu sormuyor. İç istek (hava) otomasyon
            // tetiklendiğinde ayrıca çalışacak.
            if dk.domain == Domain::Automation && score > 0 {
                score += 6;
            }

            (dk.domain, score)
        })
        .filter(|(_, score)| *score > 0)
        .collect();

    scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    // En fazla 2 alan — üçüncüsü zaten alakasızdır ve tool sayısını şişirir.
    scored.into_iter().take(2).map(|(d, _)| d).collect()
}

/// MCP alanlarını mesaja göre eşleştirir.
///
/// Yerleşik alanların anahtar kelimeleri derleme zamanında yazılı; MCP
/// sunucuları çalışma anında geliyor, o yüzden kelimeler sunucunun **kendi
/// kimliğinden ve tool adlarından** çıkarılıyor.
///
/// En fazla iki sunucu dönüyor: üç sunucu bağlayan kullanıcının 60 tool'u
/// modele hep birden gitmesin diye (bütçe zaten üstte kesiyor, ama
/// kesilen tool'lar rastgele olmasın).
pub fn match_mcp_domains(registry: &Registry, message: &str) -> Vec<Domain> {
    let words = tokenize(message);
    if words.is_empty() || is_chatter(&words) {
        return Vec::new();
    }

    let mut scores: std::collections::BTreeMap<&'static str, usize> = Default::default();

    for tool in registry.iter() {
        let Domain::Mcp(server) = tool.domain() else {
            continue;
        };

        // Sunucu adı geçiyorsa niyet açık.
        let server_word = normalize(server);
        let mut score = if server_word.len() > 2 && words.iter().any(|w| w.contains(&server_word)) {
            3
        } else {
            0
        };

        // Tool adındaki parçalar ("create_issue" → "create", "issue").
        // Önekli ad kullanıldığı için sunucu kimliği baştan atılıyor.
        let bare = tool.name().strip_prefix(server).unwrap_or(tool.name());
        for part in bare.split(['_', '-']).filter(|p| p.len() > 3) {
            let part = normalize(part);
            if words.iter().any(|w| w == &part) {
                score += 1;
            }
        }

        if score > 0 {
            let entry = scores.entry(server).or_insert(0);
            *entry = (*entry).max(score);
        }
    }

    let mut ranked: Vec<(&'static str, usize)> = scores.into_iter().collect();
    ranked.sort_by_key(|(server, score)| (std::cmp::Reverse(*score), *server));
    ranked
        .into_iter()
        .take(2)
        .map(|(server, _)| Domain::Mcp(server))
        .collect()
}

#[cfg(test)]
mod obsidian_selection_tests {
    use super::*;

    /// Kasa kelimeleri kasa isteklerini yakalamalı.
    #[test]
    fn vault_requests_reach_the_obsidian_domain() {
        for msg in [
            "obsidian notlarımda ara",
            "notlarımda tool seçimi hakkında ne yazmıştım",
            "bugünün günlük notuna ekle",
            "bu etiketteki notları listele",
        ] {
            assert!(
                match_domains(msg).contains(&Domain::Obsidian),
                "'{msg}' Obsidian alanını tetiklemeliydi: {:?}",
                match_domains(msg)
            );
        }
    }

    /// Kısa kelimeler "içerir" mantığıyla eşleştiği için buradaki cümleler
    /// bir kere yanlışlıkla kasa tool'larını çekiyordu ("durumda" ⊃ "md").
    /// Regresyon olmasın diye sabitlendi.
    #[test]
    fn everyday_words_do_not_drag_in_the_vault() {
        for msg in [
            "cpu kullanımı ne durumda",
            "hakkımda ne biliyorsun",
            "ekranımda ne var",
            "bir dosyaya not kaydet",
        ] {
            assert!(
                !match_domains(msg).contains(&Domain::Obsidian),
                "'{msg}' Obsidian alanını tetiklememeliydi: {:?}",
                match_domains(msg)
            );
        }
    }
}

/// Bu istek için modele sunulacak tool şemaları.
///
/// `budget` modelden gelir (`ModelCaps::tool_budget`) — bkz. [`select_named`].
pub fn select_tools(registry: &Registry, message: &str, budget: usize) -> Vec<Value> {
    select_named(registry, message, budget)
        .into_iter()
        .filter_map(|name| registry.get(name).map(|t| t.schema()))
        .collect()
}

/// Seçilen tool adları — testler ve günlük kaydı için.
///
/// `budget` bir **tavan**, hedef değil: alan eşleşmesi az tool getiriyorsa
/// liste kısa kalır. Bütçenin yüksek olması listeyi doldurmak için sebep
/// değildir — gereksiz tool hem faturayı büyütür hem modeli kışkırtır.
pub fn select_named<'a>(registry: &'a Registry, message: &str, budget: usize) -> Vec<&'a str> {
    let mut domains = match_domains(message);
    // MCP alanları çalışma anında oluşuyor, statik tabloda yer alamıyor —
    // ayrı eşleştiriliyor ve yerleşik alanların önüne geçiyor: kullanıcı bir
    // sunucunun adını anıyorsa kastettiği odur.
    let mcp = match_mcp_domains(registry, message);
    if !mcp.is_empty() {
        domains.truncate(1);
        let mut combined = mcp;
        combined.extend(domains);
        domains = combined;
    }

    if domains.is_empty() {
        // Sohbet — tool yok. Modeli boş yere kışkırtma.
        return Vec::new();
    }

    let mut names: Vec<&str> = Vec::new();

    // 1) Eşleşen alanların tool'ları (en alakalı olan önce).
    for domain in &domains {
        for tool in registry.in_domain(*domain) {
            if names.len() >= budget {
                break;
            }
            if !names.contains(&tool.name()) {
                names.push(tool.name());
            }
        }
    }

    // 2) Çekirdek tool'lar — kalan yere sığdığı kadar.
    for tool in registry.in_domain(Domain::Core) {
        if names.len() >= budget {
            break;
        }
        if !names.contains(&tool.name()) {
            names.push(tool.name());
        }
    }

    debug_assert!(names.len() <= budget);
    names
}

/// Seçilen tool'ların yaklaşık token maliyeti — bağlam bütçesi için.
pub fn schema_tokens(schemas: &[Value]) -> usize {
    schemas
        .iter()
        .map(|s| vavis_brain::estimate_tokens(&s.to_string()))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::{Tool, ToolOutcome};

    struct Fake(&'static str, Domain);
    impl Tool for Fake {
        fn name(&self) -> &'static str {
            self.0
        }
        fn description(&self) -> &'static str {
            "sahte tool"
        }
        fn domain(&self) -> Domain {
            self.1
        }
        fn run(&self, _: &Value) -> ToolOutcome {
            ToolOutcome::ok("")
        }
    }

    fn registry_with(tools: Vec<(&'static str, Domain)>) -> Registry {
        let mut reg = Registry::new();
        for (name, domain) in tools {
            reg.register(Box::new(Fake(name, domain)));
        }
        reg
    }

    fn big_registry() -> Registry {
        // 30 tool: gerçekçi bir yük.
        let names: Vec<(&'static str, Domain)> = vec![
            ("read_file", Domain::Files),
            ("write_file", Domain::Files),
            ("list_dir", Domain::Files),
            ("delete_file", Domain::Files),
            ("move_file", Domain::Files),
            ("find_file", Domain::Files),
            ("sys_info", Domain::System),
            ("set_volume", Domain::Control),
            ("set_brightness", Domain::Control),
            ("list_processes", Domain::System),
            ("run_command", Domain::Control),
            ("battery", Domain::System),
            ("web_search", Domain::Web),
            ("fetch_url", Domain::Web),
            ("remember", Domain::Memory),
            ("recall", Domain::Memory),
            ("forget", Domain::Memory),
            ("now", Domain::Core),
            ("remind", Domain::Core),
        ];
        registry_with(names)
    }

    #[test]
    fn never_exceeds_budget() {
        let reg = big_registry();
        // Her alandan kelime içeren kötü niyetli mesaj.
        let msg = "dosya sistem ses web ara hatırla cpu disk klasör oku yaz sil";

        // Bütçe ne verilirse verilsin aşılmamalı — dar da olsa geniş de olsa.
        for budget in [DEFAULT_TOOL_BUDGET, 6, 8, 24, 48] {
            let selected = select_named(&reg, msg, budget);
            assert!(
                selected.len() <= budget,
                "{} tool seçildi, bütçe {budget}",
                selected.len()
            );
        }
    }

    /// Geniş bütçe listeyi doldurmak için sebep değil.
    ///
    /// Bütçe bir tavan: alan eşleşmesi az tool getiriyorsa liste kısa kalır.
    /// Aksi hâlde "bütçe 48" demek her isteğe 48 şema göndermek olurdu.
    #[test]
    fn wide_budget_does_not_pad_the_list() {
        let reg = big_registry();
        let narrow = select_named(&reg, "masaüstündeki dosyaları listele", 12);
        let wide = select_named(&reg, "masaüstündeki dosyaları listele", 48);

        assert_eq!(narrow, wide, "bütçe genişleyince alakasız tool eklenmemeli");
        assert!(wide.len() < 48, "liste bütçeye kadar doldurulmuş");
    }

    #[test]
    fn greeting_gets_no_tools() {
        let reg = big_registry();
        for msg in ["merhaba", "selam", "teşekkürler", "tamam", "ok"] {
            assert!(
                select_named(&reg, msg, DEFAULT_TOOL_BUDGET).is_empty(),
                "'{msg}' için tool sunulmamalı"
            );
        }
    }

    #[test]
    fn file_request_offers_file_tools() {
        let reg = big_registry();
        let selected = select_named(&reg, "masaüstündeki dosyaları listele", DEFAULT_TOOL_BUDGET);
        assert!(selected.contains(&"list_dir"), "seçilenler: {selected:?}");
    }

    #[test]
    fn system_request_offers_system_tools() {
        let reg = big_registry();
        let selected = select_named(&reg, "cpu kullanımı ne durumda", DEFAULT_TOOL_BUDGET);
        assert!(selected.contains(&"sys_info"), "seçilenler: {selected:?}");
    }

    #[test]
    fn volume_request_offers_volume_tool() {
        let reg = big_registry();
        let selected = select_named(&reg, "sesi %30 yap", DEFAULT_TOOL_BUDGET);
        assert!(selected.contains(&"set_volume"), "seçilenler: {selected:?}");
    }

    #[test]
    fn web_request_offers_web_tools() {
        let reg = big_registry();
        let selected = select_named(&reg, "bugünkü haberleri ara", DEFAULT_TOOL_BUDGET);
        assert!(selected.contains(&"web_search"), "seçilenler: {selected:?}");
    }

    #[test]
    fn memory_request_offers_memory_tools() {
        let reg = big_registry();
        let selected = select_named(
            &reg,
            "beni hatırla: kahveyi sade içerim",
            DEFAULT_TOOL_BUDGET,
        );
        assert!(selected.contains(&"remember"), "seçilenler: {selected:?}");
    }

    #[test]
    fn turkish_suffixes_are_handled() {
        let _reg = big_registry();
        // "dosyayı", "dosyada", "dosyalar" hepsi eşleşmeli.
        for msg in ["dosyayı oku", "dosyada ara", "dosyalarımı listele"] {
            let d = match_domains(msg);
            assert!(d.contains(&Domain::Files), "'{msg}' → {d:?}");
        }
    }

    #[test]
    fn ascii_turkish_also_matches() {
        // Kullanıcılar sıklıkla şapkasız yazıyor.
        let d = match_domains("parlaklik arttir");
        assert!(
            d.contains(&Domain::Control),
            "ASCII yazım da eşleşmeli: {d:?}"
        );
    }

    #[test]
    fn unrelated_text_yields_no_domain() {
        assert!(match_domains("bana bir şiir yaz").is_empty());
        assert!(match_domains("nasılsın bugün").is_empty());
    }

    /// "bilgisayar" kontrol alanına gitmeli, hafızaya değil.
    ///
    /// Eşleşme ek toleranslı olduğu için `"bilgi"` anahtar kelimesi
    /// `"bilgisayar"` kelimesini de yakalıyordu. Sonucu sessizdi: kullanıcı
    /// "bilgisayarı kapat" dediğinde modele kontrol araçları hiç
    /// ulaşmıyor, hafıza araçları gidiyordu. Bir asistanda bu kelime en sık
    /// geçenlerden biri, yani tek bir anahtar kelime çok sayıda isteği
    /// yanlış yere gönderiyordu.
    #[test]
    fn the_word_computer_reaches_the_control_tools() {
        for msg in [
            "bilgisayarı kapat",
            "bilgisayari kapat",
            "bilgisayarımı yeniden başlat",
            "bilgisayarda ne var",
        ] {
            let d = match_domains(msg);
            assert!(
                d.contains(&Domain::Control),
                "'{msg}' kontrol alanına gitmeli: {d:?}"
            );
            assert!(
                !d.contains(&Domain::Memory),
                "'{msg}' hafızaya gitmemeli: {d:?}"
            );
        }
    }

    /// Zayıf fiillere eşlik edecek güçlü bir kelime olmalı.
    ///
    /// "aç" ve "kapat" tek başına alan tetiklemiyor (doğru: "gözlerini aç").
    /// Kullanıcının söylediği nesne o eşliği sağlamalı.
    #[test]
    fn a_named_target_pairs_with_a_weak_verb() {
        for msg in ["tarayıcı aç", "chrome aç", "spotify kapat"] {
            assert!(
                match_domains(msg).contains(&Domain::Control),
                "'{msg}' kontrol alanına gitmeli"
            );
        }
    }

    /// **Regresyon testi.** Zayıf fiil tek başına alan tetiklememeli.
    ///
    /// Bu tam olarak kullanıcının "tool'lar aşırı gidiyor" şikayetinin
    /// kaynağı: "şiir yaz" cümlesindeki "yaz" dosya tool'larını çağırıyordu.
    #[test]
    fn weak_verbs_alone_do_not_trigger_a_domain() {
        for msg in [
            "bana bir şiir yaz",
            "bir hikaye yaz",
            "şunu oku bakalım", // nesne yok
            // ("bir şarkı bul" artık Media alanını tetikliyor — DOĞRU davranış,
            //  medya tool'ları eklendikten sonra. Bu yüzden listeden çıkarıldı.)
            "espri yap ve yaz",
        ] {
            let d = match_domains(msg);
            assert!(d.is_empty(), "'{msg}' alan tetiklememeli, tetikledi: {d:?}");
        }
    }

    /// Zayıf fiil + alan kelimesi **birlikte** çalışmalı.
    #[test]
    fn weak_verb_with_domain_noun_does_trigger() {
        for (msg, expected) in [
            ("dosya yaz", Domain::Files),
            ("bu dosyayı oku", Domain::Files),
            ("klasörü listele", Domain::Files),
            ("internette ara", Domain::Web),
        ] {
            let d = match_domains(msg);
            assert!(
                d.contains(&expected),
                "'{msg}' → {expected:?} bekleniyordu, gelen: {d:?}"
            );
        }
    }

    /// Görsel üretim istekleri canvas alanını tetiklemeli.
    #[test]
    fn image_requests_reach_the_canvas_domain() {
        for msg in [
            "bana bir kedi resmi çiz",
            "gün batımı görseli üret",
            "generate an image of a robot",
            "bir logo tasarla",
        ] {
            let d = match_domains(msg);
            assert!(d.contains(&Domain::Canvas), "'{msg}' → {d:?}");
        }
    }

    /// Canvas kelimeleri alakasız isteklere sızmamalı.
    ///
    /// Alan kısa köklerle ("çiz") kurulsaydı bu cümleler de eşleşirdi —
    /// "md" olayının aynısı, bir tur daha.
    #[test]
    fn canvas_keywords_do_not_leak_into_unrelated_requests() {
        for msg in [
            "haftalık çizelgeyi göster",
            "ekranımda ne var",
            "bir çizgi roman öner",
            "cpu durumu nasıl",
        ] {
            let d = match_domains(msg);
            assert!(!d.contains(&Domain::Canvas), "'{msg}' → {d:?}");
        }
    }

    /// Sohbet mesajlarında hiçbir koşulda tool sunulmamalı.
    #[test]
    fn conversational_requests_stay_tool_free() {
        for msg in [
            "bugün nasılsın",
            "bana kendini tanıt",
            "python nedir",
            "bir fıkra anlat",
            "ne düşünüyorsun bu konuda",
        ] {
            let d = match_domains(msg);
            assert!(d.is_empty(), "'{msg}' → {d:?}");
        }
    }

    /// **Regresyon testi.** Çekirdek tool'lar tek başlarına da sunulmalı.
    ///
    /// Uzun süre `DOMAIN_KEYWORDS` içinde Core satırı yoktu. Sonuç: "saat kaç"
    /// hiçbir alanı tetiklemiyor, alan listesi boş kalınca `select_named`
    /// erkenden dönüyor, `get_current_time` modele hiç ulaşmıyordu — ve model
    /// saati uyduruyordu ("2023-10-27 14:30").
    #[test]
    fn a_bare_time_question_offers_the_clock() {
        let reg = big_registry();
        for msg in [
            "saat kaç",
            "saat",
            "tarih ne",
            "bugün ayın kaçı",
            "bugün günlerden ne",
            "what time is it",
        ] {
            let selected = select_named(&reg, msg, DEFAULT_TOOL_BUDGET);
            assert!(
                selected.contains(&"now"),
                "'{msg}' saat tool'unu sunmalıydı: {selected:?}"
            );
        }
    }

    /// Zaman isimleri sohbete sızmamalı — düzeltmenin diğer yarısı.
    ///
    /// "bugün" tek başına bir tarih sorusu değil; Core alanını her "bugün"
    /// geçen cümlede açmak, sohbeti tool listesiyle kışkırtmak olurdu.
    #[test]
    fn everyday_time_words_do_not_open_the_core_domain() {
        for msg in [
            "bugün nasılsın",
            "nasılsın bugün",
            "iyi günler",
            "bugün canım sıkkın",
        ] {
            let d = match_domains(msg);
            assert!(
                !d.contains(&Domain::Core),
                "'{msg}' çekirdeği tetiklememeliydi: {d:?}"
            );
        }
    }

    /// Aritmetik ifade kelimeye ihtiyaç duymadan hesap makinesini getirmeli.
    #[test]
    fn an_arithmetic_expression_offers_the_calculator() {
        assert!(has_arithmetic("15 * 3 kaç eder"));
        assert!(has_arithmetic("(120+8)/4"));
        assert!(has_arithmetic("100-25"));

        // Tek sayı ya da tek işaret yetmez.
        assert!(!has_arithmetic("3 dosya sil"));
        assert!(!has_arithmetic("a * b"));
        assert!(!has_arithmetic("merhaba"));
        assert!(!has_arithmetic("dosya-adi.txt oku"));
    }

    #[test]
    fn core_tools_are_included_when_a_domain_matches() {
        let reg = big_registry();
        let selected = select_named(&reg, "cpu durumu", DEFAULT_TOOL_BUDGET);
        assert!(selected.contains(&"now"), "çekirdek tool'lar da sunulmalı");
    }

    #[test]
    fn most_relevant_domain_comes_first() {
        let reg = big_registry();
        // Baskın olarak dosya mesajı.
        let selected = select_named(&reg, "dosya klasör oku yaz listele", DEFAULT_TOOL_BUDGET);
        let first = selected.first().copied().unwrap_or("");
        let file_tools = [
            "read_file",
            "write_file",
            "list_dir",
            "delete_file",
            "move_file",
            "find_file",
        ];
        assert!(
            file_tools.contains(&first),
            "en alakalı alan başta olmalı, ilk: {first}"
        );
    }

    #[test]
    fn schemas_are_valid_json_and_counted() {
        let reg = big_registry();
        let schemas = select_tools(&reg, "dosyaları listele", DEFAULT_TOOL_BUDGET);
        assert!(!schemas.is_empty());
        for s in &schemas {
            assert_eq!(s["type"], "function");
            assert!(s["function"]["name"].is_string());
        }
        assert!(schema_tokens(&schemas) > 0);
    }

    #[test]
    fn empty_message_gets_no_tools() {
        let reg = big_registry();
        assert!(select_named(&reg, "", DEFAULT_TOOL_BUDGET).is_empty());
        assert!(select_named(&reg, "   ", DEFAULT_TOOL_BUDGET).is_empty());
    }
}
