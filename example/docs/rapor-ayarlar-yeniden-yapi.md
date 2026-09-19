# Ayarlar ekranı: yeniden yapılandırma raporu

**Tarih:** 2026-09-19
**Commit'ler:** `5755a29`, `fb51852`, `f8fe36b`
**Durum:** tamamlandı — 14 panelin 14'ü taşındı

---

## Özet

`Settings.svelte` **2189 → 712 satır.** Dosyada artık hiç panel içeriği yok:
bir kenar çubuğu, bir arama kutusu ve kategori başına bir dal. Tüm arayüz
kodunun dörtte biri tek dosyadaydı, şimdi 14 bileşene bölündü.

Hiçbir işlev kaybolmadı. Her panel çalışan ekranda tek tek açılıp kontrol
edildi — tip denetleyicisine güvenilmedi.

---

## Bu oturumda taşınanlar

| Panel | Dosya |
|---|---|
| Genel | `panes/GeneralPane.svelte` |
| Model & anahtarlar | `panes/ProviderPane.svelte` |
| Ses | `panes/VoicePane.svelte` |
| Hafıza | `panes/MemoryPane.svelte` |
| Obsidian | `panes/ObsidianPane.svelte` |
| Spotify | `panes/SpotifyPane.svelte` |
| Steam | `panes/SteamPane.svelte` |
| MCP sunucuları | `panes/McpPane.svelte` |
| Kısayollar | `panes/ShortcutsPane.svelte` |
| Veri | `panes/DataPane.svelte` |
| Güncellemeler | `panes/UpdatesPane.svelte` |

Daha önce taşınmıştı: Web arama, Görsel & video, Araçlar.

---

## Ölçerek bulunan üç hata

Bunların üçü de **tip denetleyicisinden geçti.** Çalıştırmadan görünmezlerdi.

### 1. Panellerin stilleri arkada kaldı

En sinsisi buydu. Svelte'de bir bileşenin stilleri **kendi işaretlemesine
kilitlidir.** Paneller `Settings.svelte`'ten çıkarken `class="row"`,
`class="entry"`, `class="tiny"` yazmaya devam ettiler — ama o kuralların
tanımı geride kaldı.

Sonuç: sınıf adı duruyor, stil uygulanmıyor. Hata yok, uyarı yok.

**Çözüm:** ortak olanlar `styles.css`'e bir kez taşındı. Aynı bloğu 14
dosyaya yapıştırıp kopyaların zamanla birbirinden ayrılmasını beklemektense.

### 2. Aynı isim, farklı iş — `.entry` çakışması

Genel `.entry` bir **liste satırı**: alt çizgili, iki ucu ayrık.
`KeyInput.svelte`'teki `.entry` ise bir **şifre kutusu sarmalayıcısı**:
esnek kutu, `flex: 1 1 320px`.

İsimleri aynıydı, işleri değil. Genel kural yereli eziyordu — anahtar kutusu
yeniden daralıyordu, yani **düzeltilen ilk hatanın ta kendisi geri geliyordu.**

**Çözüm:** `.key-entry` olarak ayrıldı.

### 3. 58 aracın yükseklik sınırı silindi

`ToolsPane`'in listesi `max-height: 380px` taşıyordu. Genel `.list`'te böyle
bir sınır yok — çoğu liste kısa olduğu için doğru karar. Ama araç kayıtı
58 satır: sınır gidince liste panelin dışına taşıyor, süzgeç kutusu yukarı
kayıyor.

Stub'da 2 araç olduğu için **ilk ekran görüntüsünde görünmedi.** 58 araçla
tekrar çalıştırılınca çıktı.

**Ölçüm:** liste yüksekliği 380 px, içerik 3252 px, kaydırılabilir ✓

---

## Ayrıca düzeltilenler

**`CustomEndpoint` tek eksik alanda çöküyordu.** `url.trim()` korumasızdı;
`url` tanımsız gelirse tüm ayarlar ekranı kararıyor ve **gezinme kilitleniyor**
— sonraki her panel eski içeriği gösteriyordu. Arka uç bu alanı her zaman
gönderiyor, ama tek bir eksik alanın koca ekranı düşürmesi kabul edilebilir
değil. `(url ?? "")` ile korundu.

**`.gitignore` bozuktu.** `example` tek başına git'in klasöre girmesini
engelliyor, dolayısıyla altındaki `!example/docs/` **hiç çalışmıyordu.**
Yani oraya yazılacak **yeni bir rapor sessizce yok sayılacaktı** — bu rapor
dahil. Takip edilen dosyalar etkilenmediği için fark edilmesi zordu.
`example/*` ile düzeltildi.

**Hafıza süzgeci.** Kelimeler **ayrı ayrı** aranıyor, sıra önemsiz. Gerçekler
cümle olarak yazıldığı için ("kahvesini sade içer") düz alt dizi araması iki
kelime yazan herkese boş liste döndürürdü — kelimeler metinde yan yana
değil.

---

## Doğrulama

**Çalışan ekranda** (Playwright, gerçek tarayıcı):

| Kontrol | Sonuç |
|---|---|
| 14 panelin hepsi açılıyor, kendi başlığıyla | ✓ |
| Hiçbir panel yatay taşmıyor | ✓ |
| Sağlayıcı kartları, etiketler hizalı | ✓ |
| Araç listesi 380 px'te duruyor, kayıyor | ✓ |
| Hafıza süzgeci — 7 ayrı durum | ✓ |
| MCP katlanır bölümler, kaydet düğmesi kilidi | ✓ |
| Steam 17 hane doğrulaması | ✓ |
| Kenar çubuğu araması, boş durum | ✓ |

Toplam **17 etkileşim iddiası**, hepsi geçti.

**Derleme ve testler:** svelte-check 0 hata 0 uyarı · 50 arayüz testi ·
17 Rust paketi (çıkış kodu 0, FAILED yok) · clippy temiz · üretim derlemesi
başarılı.

---

## Öğrenilen: stub'ın eksikliği hatayı gizler

Tarayıcıda çalıştırmak için sahte bir veri katmanı gerekti. İlk hâli eksikti
ve **iki kez yanlış yöne götürdü:**

1. Komut adları yanlış yazıldığında paneller boş göründü — kod sanıldı,
   stub'dı.
2. Araç sayısı 2 olduğu için yükseklik sınırının silindiği görünmedi —
   58'e çıkarılınca ortaya çıktı.

**Kural:** stub gerçeğe benzemeli. Az veriyle test etmek, az veriyle çalışan
hataları görünmez kılar.

---

## Açık kalanlar

- `budget.rs` → `PRICES` tablosunda Gemini/Groq fiyatları yok
- `ModelCaps` tablosu elle yazılmış; `/models` gerçeği veriyor
- Compound seçilince Vavis'in araçları çalışmıyor, arayüzde not yok
- **Mark LIII'ten:** mikrofon seçici (`capture.rs:196` hâlâ
  `default_input_device()`), geri alma, anında karşılık, model ısıtma
- **JARVIS raporundan:** görünürlük ekranı, kişilik seçeneği, tek tıkla kurulum
