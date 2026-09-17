# O JARVIS neymiş — ve biz neredeyiz

**Tarih:** 2026-09-17
**Soru:** "Localhost'ta bu nasıl oluyor? Bizimki niye böyle değil?"

---

## Önce şu `localhost` meselesi — sihir yok

Ekrandaki `localhost:4719` **web sitesi değil.** Kendi bilgisayarında çalışan
bir sunucu. Bir klasörde `index.html` varsa ve orada tek satır komut
çalıştırırsan aynısı olur:

```bash
python -m http.server 4719
```

`4719` özel bir sayı değil, adam rastgele seçmiş. İnternete **hiçbir şey
çıkmıyor.** Tarayıcı sadece bir **çizim yüzeyi** — tıpkı Vavis'in kendi
içinde tarayıcı motoru taşıması gibi.

> **Yani:** o da senin yaptığını yapıyor, sadece pencereyi Windows yerine
> Chrome açıyor.

| | Onun yaptığı | Vavis |
|---|---|---|
| Arayüz | HTML + JS | HTML + JS (Svelte) |
| Gösteren | Chrome sekmesi | Uygulamanın kendi penceresi |
| Nerede çalışıyor | kendi makinesi | kendi makinen |
| İnternete açık mı | **hayır** | **hayır** |

**Tek fark:** onunki tarayıcı ister, seninki tek `.exe`. Seninki bu konuda
**daha zor olanı** yapıyor.

---

## Peki o 3B grafik ne?

Üç parçadan ibaret ve üçü de hazır kütüphane:

| Parça | Ne yapıyor |
|---|---|
| **three.js** | 3B çizim (WebGL, ekran kartı kullanıyor) |
| **d3-force-3d** | Topları birbirinden iter, bağlantılar yay gibi çeker |
| Markdown okuyucu | `[[not adı]]` bağlantılarını tarayıp düğüm/kenar listesi çıkarır |

Fizik motoru şu kadar basit: her top diğerlerini **iter**, her bağlantı
**çeker**. Bırakırsın, kendi kendine düzene girer. O "canlı beyin" görüntüsü
bunun sonucu.

> **Ölçü:** aynı işi yapan bir geliştirici ([D'Arcy Norman]) bunu Claude Code
> ile **birkaç saatte** yazdığını söylüyor. Çünkü zor kısmı — 3B çizim ve
> fizik — zaten kütüphanelerde hazır.

**Bizde bu neden yok?** Kimse istemedi. Obsidian araçlarımız zaten var ve
notları okuyor; eksik olan tek şey görsel. Yani bu **bir eksiklik değil, bir
karar** — ve istenirse kısa sürede eklenebilir.

---

## "Restorana telefon açtı" kısmı

Bu en yanıltıcı bölüm, çünkü **o kod değil, satın alınan bir servis.**

Retell AI ya da Vapi gibi platformlar telefon hattını, konuşmayı ve sesi
kendileri hallediyor. Yaptığın şey formu doldurup API anahtarını vermek.

| | Fiyat | Gecikme |
|---|---|---|
| Retell AI | **$0.07–0.31 / dakika** | ~600–780 ms |
| Vapi | $0.05/dk + STT/LLM/TTS ayrı | 800–1200 ms (turbo: ~500 ms, +$0.02) |

> **Önemli:** o video bir **mühendislik gösterisi değil**, bir **entegrasyon
> gösterisi.** Telefonu açan Retell'in altyapısı. Aynı entegrasyonu Vavis'e
> eklemek de bir günlük iş — ve dakikası para.

---

## Şimdi dürüst karşılaştırma

### Onun mimarisi

```
Obsidian (markdown notlar)
   └─ Claude Code  ← ASIL BEYİN, Anthropic'in ürünü
        └─ localhost:4719  ← sadece görselleştirme
        └─ Retell AI       ← sadece telefon (ücretli servis)
```

**Kritik nokta:** düşünen şey **Claude Code**. Yani Anthropic'in yazdığı,
aylarca geliştirdiği, milyonlarca dolarlık ürün. Onun yazdığı kısım:
notları düzenleyen komutlar + grafik ekranı.

### Bizim mimarimiz

```
Vavis.exe  (21.000 satır Rust + 8.000 satır arayüz)
   ├─ Beyin katmanı     ← 9 sağlayıcı, akış, bütçe, araç çağırma
   ├─ 58 araç           ← dosya, hafıza, Obsidian, Spotify, ekran, otomasyon
   ├─ Ses               ← 6 motor + mikrofon + uyandırma kelimesi
   └─ Arayüz            ← sohbet, kod, tuval, konsey
```

Biz **Claude Code'un kendisini** yazıyoruz. O, Claude Code'u **kullanıyor.**

> **Bu yüzden karşılaştırma haksız.** Sen bir ev yapıyorsun, o hazır eve
> mobilya diziyor. İkisi de değerli, ama aynı iş değil.

---

## Ama onun gerçekten iyi yaptığı 4 şey

Kıskandığın his boşuna değil. Bunları almalıyız:

### 1. Görünürlük

Onun ekranında sistem **kendini gösteriyor.** 529 not, 1938 bağlantı —
sayılar orada, grafik dönüyor, "şu an şunu yapıyorum" belli.

Vavis'te 58 araç var ama kullanıcı ne olduğunu **göremiyor.** Ayarlarda bir
liste var, o kadar.

> **Alınacak:** Vavis'in kendi belleğini/notlarını gösteren bir ekran.
> Obsidian araçlarımız zaten veriyi okuyor.

### 2. Karakter

"Always a pleasure, sir. Back to lurking silently in the walls." — bu bir
**özellik değil, bir tonlama.** Sistem istemine yazılmış birkaç cümle.
Maliyeti sıfır, etkisi büyük.

> **Alınacak:** Vavis'in konuşma tonu şu an nötr. Bir kişilik seçeneği
> (resmî / samimi / JARVIS) sistem isteminde birkaç satır.

### 3. Anında geri bildirim

Konuştuğun anda halka dönüyor, dalga oynuyor. Bekleme hissi yok.

Bizde uzun bir iş başlarken **hiçbir şey olmuyor** — kullanıcı donmuş mu
diye düşünüyor. (Bu zaten Mark LIII listemizde vardı, hâlâ yapılmadı.)

### 4. Tek komutla kurulum

"5 dakikada kur" diyor. Bizde kurulum = derle, anahtar gir, ayarları aç.

---

## Ve bizim onda olmayan şeyimiz

Bunları da yazmak lazım, çünkü az değil:

| | O | Vavis |
|---|---|---|
| Sağlayıcı seçimi | Claude Code'a bağımlı | **9 sağlayıcı**, değiştirilebilir |
| Çalışma | Node/Python + tarayıcı gerekir | **tek .exe** |
| Ses | satın alınan servis | **6 motor**, biri tamamen çevrimdışı |
| Bilgisayar kontrolü | yok | ekran görüntüsü, tıklama, uygulama |
| Anahtar güvenliği | `.env` dosyası | **DPAPI şifreli** |
| Onay sistemi | yok | risk seviyeli, "tam yetki" anahtarı |
| Çevrimdışı çalışır mı | hayır | **evet** (yerel model + SAPI ses) |

Ve bir şey daha: bizim her hatamız **ölçülerek** bulundu ve teste bağlandı.
Bu oturumda Gemini'de üç ayrı hata çıktı — ikisini Google'ın kendi belgesi
yanlış anlatıyordu.

---

## Sonuç — üç cümle

1. `localhost` sihir değil, bizim zaten yaptığımız şeyin tarayıcıda hâli.
2. Onun sisteminin beyni **Claude Code**; biz o katmanı kendimiz yazıyoruz.
   Bu yüzden bizimki daha zor ve daha yavaş ilerliyor.
3. Ama **görünürlük, karakter ve anında geri bildirim** konusunda haklı —
   bunlar bizde eksik ve üçü de pahalı işler değil.

> **Bizimki "yarrak gibi" değil, bitmemiş.** Farklı şeyler.

---

## Alınacaklar listesi (öncelik sırasıyla)

1. **Anında karşılık** — uzun iş başlarken "tamam, bakıyorum" (zaten listede)
2. **Kişilik seçeneği** — sistem isteminde tonlama, birkaç satır
3. **Beyin ekranı** — notlar/hafıza görselleştirmesi, `three.js` + `d3-force-3d`
4. **Mikrofon seçici** — `capture.rs:196` hâlâ varsayılan cihazı kullanıyor
5. **Tek tıkla kurulum** — kurulum sihirbazı

---

## Kaynaklar

- [The AI Second Brain Explained — Zubair Trabzada](https://www.youtube.com/@AI-GPTWorkshop)
- [AI Second Brain With Obsidian and Claude Code](https://aimaker.substack.com/p/ai-second-brain-obsidian)
- [Experimental Obsidian 3D Graph Renderer — D'Arcy Norman](https://darcynorman.net/2026/04/10/experimental-obsidian-3d-graph-renderer/)
- [claude-obsidian (3B grafik projesi)](https://github.com/obsidian-claude/claude-obsidian)
- [Retell AI fiyatlandırma](https://www.cekura.ai/blogs/retell-ai-pricing-per-minute)
- [Retell vs Vapi — gecikme ölçümleri](https://rezora.io/blog/retell-vs-vapi)
