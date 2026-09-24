# VAVIS

> Telaffuz: **veyvis**
> Windows için kişisel AI asistanı. Tek `.exe`, kurulum yok.

AEGIS/RealJarvis'in (TypeScript + Electron, ~1 GB) yerine sıfırdan yazıldı:
arka taraf Rust, pencere Tauri, arayüz React + TypeScript.

---

## Hızlı başlangıç

```bash
cd ui && bun install && bun run build && cd ..
cargo run --release
```

İlk açılışta **Ayarlar → Model & keys** (`Ctrl+,`) ekranından bir sağlayıcı
seç. Para vermeden çalışan seçenekler:

| Seçenek | Ne gerekiyor |
|---|---|
| **Claude Code** | Claude Pro/Max aboneliğin. [Claude Code](https://claude.com/code)'u kur, bir kez terminalde `claude` yazıp giriş yap. Anahtar yok — Vavis kurulu olduğunu görürse kendisi seçer. |
| **Gemini** | Ücretsiz anahtar: [aistudio.google.com](https://aistudio.google.com). Canlı sesli konuşma ve anlamsal hafıza da bununla çalışır. |
| **Groq** | Ücretsiz anahtar: [console.groq.com](https://console.groq.com). Ses tanıma (Whisper) için de en iyisi. |
| **Cerebras** · **OpenRouter** (`:free` modeller) · **GitHub Models** · **Mistral** · **NVIDIA** | Hepsinin ücretsiz katmanı var. |
| **Local** | Ollama / LM Studio — kendi bilgisayarın. |

Bir sağlayıcının kotası dolarsa mesaj yarıda kalmasın diye **Fallback**
bölümüne yedek sağlayıcılar ekleyebilirsin.

---

## Neler yapabiliyor

**Sohbet ve hafıza**
- Birden çok sohbet: sağ paneldeki saat simgesi; ara, aç, yeniden adlandır, sil.
- Seni tanıyan hafıza: kendinden bahsettiğinde kalıcı bilgileri kendisi çıkarıp
  hatırlar, her mesajda ilgili olanları modele verir. Kelimeye değil anlama
  göre arar ("kahve" deyince "espresso içer"i bulur). Hepsi bu bilgisayarda
  durur; Ayarlar → Memory'den görülür ve silinir.

**Ses**
- Uyandırma kelimesi **bu bilgisayarda** tanınır (Ayarlar → Voice → Wake word:
  adını üç kez söyle). Adın geçmeyen hiçbir cümle buluta gitmez.
- **Canlı konuşma** (Gemini Live): kesintisiz konuşur, sözünü kesebilirsin,
  araçları kullanmaya devam eder. Sohbet panelinin başındaki mikrofon.
- Ses tanıma Groq'ta Whisper, Groq anahtarı yoksa Gemini.

**Bilgisayar**
- Pencerelerdeki düğmelere, alanlara, menülere **adıyla** ulaşır (Windows UI
  Automation) — ekran görüntüsü tahmini yerine. Kaydırma ve sürükleme de var.
- Dosyalar, uygulamalar, ses/parlaklık, pano, PowerShell, medya, Spotify,
  Steam, Obsidian, VirusTotal, web araması, görsel üretimi, MCP sunucuları.

**Kendiliğinden**
- Otomasyonlar saate, pile, CPU'ya ve **olaylara** bağlanabilir: indirilenlere
  dosya düşünce ("VirusTotal ile kontrol et: {file}"), bir program açılınca ya
  da kapanınca, Vavis açılınca (sabah özeti), uzun aradan dönünce.

**Telefon**
- Kendi Telegram botunla dışarıdan yaz (Ayarlar → Phone). Yalnızca eşleştirdiğin
  hesap cevap alır; yıkıcı her işlem telefonda butonla onayına gelir.

**Kod**
- Kod ekranında (`Ctrl+K` → Code) bir klasör aç ve iste: projeyi okur, arar,
  birebir metin değiştirerek düzenler, testleri çalıştırır, hata varsa düzeltir.
  Her değişiklik onaydan geçer; ekrandaki dosya güncellenir.

**Konsey** — aynı soruyu birkaç modele birden sor, cevapları yan yana gör.

---

## Güvenlik

- **Yıkıcı işlemler onay ister** (dosya yazma, komut, tıklama…). Bir turda
  üçten fazla yıkıcı işlem olursa "hep izin ver" bile yeniden sorar.
- Okunan bir sayfa, dosya ya da indirilen bir dosyanın adı modele talimat
  vermeye çalışırsa, o turda bir şeyi değiştiren her işlem yeniden sorulur —
  **tam yetki açıkken bile**. Claude Code'un kendi okuduğu sayfalar da buna
  dahil.
- Telefondan gelen istekler her zaman sorar; bot yalnızca özel sohbette cevap
  verir.
- Anahtarlar Windows DPAPI ile şifreli; ayar dosyasına yazılmaz.
- Claude Code'un kendi yazma/kabuk araçları kapalıdır; bilgisayarda yapılan
  her şey Vavis'in araçlarından ve onay kapısından geçer. Kod turunda yalnızca
  açık projenin içini okuyabilir; projenin kendi `.claude` ayarları (hook'lar)
  ve CLAUDE.md'si yüklenmez.
- PowerShell'e giden her metin (sesli okunan cümle, pano, pencere adı) tek bir
  yerden, her tırnak türü kaçırılarak geçer; "Uygulama aç" komut
  çalıştırıcılarını açmaz.

---

## Kısayollar

| Tuş | Ne yapar |
|---|---|
| `Ctrl+K` | Komut paleti — her eylem burada |
| `Ctrl+,` | Ayarlar |
| `Ctrl+L` | Yeni sohbet (eskisi listede kalır) |
| `Ctrl+B` | Sohbet panelini gizle / göster |
| `Ctrl+M` | Ses modunu değiştir |
| `Ctrl+S` | Açık dosyayı kaydet (kod ekranı) |
| `Esc` | Konuşmayı kes |
| `F11` | Pencere modu |

---

## Mimari

```
┌──────────────────────────────────┐
│  ui/           ARAYÜZ            │  React + TypeScript (Vite)
├──────────────────────────────────┤
│  vavis-shell   KABUK             │  Tauri penceresi, komutlar, sohbet turu,
│                                  │  hafıza, olay izleyici, Telegram
├──────────────────────────────────┤
│  vavis-tools   ELLER             │  araçlar, izin kapısı, ajan döngüsü,
│                                  │  MCP (istemci + Claude Code köprüsü)
├──────────────────────────────────┤
│  vavis-brain   BEYİN             │  sağlayıcılar, Claude Code CLI,
│                                  │  embedding, bağlam bütçesi, anahtarlar
├──────────────────────────────────┤
│  vavis-audio   DUYULAR           │  mikrofon, VAD, uyandırma kelimesi,
│                                  │  STT, TTS, Gemini Live, akışlı çıkış
├──────────────────────────────────┤
│  vavis-core    ÇEKİRDEK          │  ayarlar, SQLite (sohbetler, hafıza,
│                                  │  otomasyonlar, galeri), arama
└──────────────────────────────────┘
```

**Bağımlılık tek yönlüdür.** Alt katman üstünü tanımaz.

### Tasarımın kritik kararları

- **Modele her istekte bütün araçlar gönderilmez.** Mesajdan alan çıkarılır,
  o alanın araçları sunulur; büyük alanlar mesajın andığı araçları öne alıp
  altıda kesilir. Sohbet mesajlarına hiç araç gitmez. Model elinde olmayan bir
  araca ihtiyaç duyarsa `request_tools` ile ister. (Claude Code ve kod turları
  istisna: onlara işin gerektirdiği araçların hepsi verilir.)
- **Claude Code bir program, API değil.** Her tur `claude -p` ile çalışır, cevap
  stream-json ile kelime kelime gelir. Vavis'in araçları ona yerel bir MCP
  sunucusundan verilir (token'lı, yalnızca 127.0.0.1, yalnızca tur sürerken
  açık); her çağrı aynı izin kapısından geçer.
- **Barge-in yapısal olarak doğru.** Kuyruk ve oynatma tek kilit altında;
  durdurulmuş bir konuşmanın geç gelen sesi yenisine karışamaz.
- **Bağlam bütçesinde her şey sayılır** — araç şemaları ve görüntüler dahil.

---

## Geliştirme

```bash
cargo test --all                         # Rust testleri (1000+)
cd ui && bun run check && bun run test   # tip kontrolü + arayüz testleri
cargo clippy --all-targets -- -D warnings

# Tool seçim kalitesi (%100 kalmalı)
cargo test -p vavis-tools --test selection_eval -- --nocapture

# Gerçek Claude Code'a karşı uçtan uca (kurulu ve giriş yapılmış olmalı)
cargo test -p vavis-tools --test claude_code_agent_e2e -- --ignored --nocapture
```

Uygulama yalnızca Windows için; ama çekirdek, beyin, araç ve ses katmanları
Linux'ta da derlenip test ediliyor (Windows'a özel kod orada saplanıyor).
Windows hedefi için çapraz kontrol:
`cargo clippy --target x86_64-pc-windows-gnu --all-targets -- -D warnings`.

### Veri konumu

`%APPDATA%\vavis\data\`

| Dosya | İçerik |
|---|---|
| `vavis.toml` | Ayarlar |
| `vavis.db` | Sohbetler, hafıza, otomasyonlar, galeri dizini (SQLite) |
| `keys.dat` | API anahtarları (DPAPI ile şifreli) |
| `wake.json` | Eğitilmiş uyandırma kelimesi |
| `media/` | Üretilen görseller ve videolar |
| `logs/` | Günlük log dosyaları + çökme kaydı |

---

## Bilinen sınırlar

- **Edge TTS** servisi zaman zaman 403 döndürüyor; başarısızlıkta otomatik
  olarak SAPI'ye düşülüyor.
- **Canlı konuşmada yankı engelleme yok**: hoparlörle kullanırken asistan
  konuşurken sessiz girdi tutuluyor, net konuşarak sözünü kesebilirsin.
  Kulaklıkla bu sorun olmaz.
- **Uyandırma kelimesi** onu eğiten sese göre ayarlı; başka biri söyleyince
  uyanmayabilir.
- **Parlaklık** sadece dizüstü panelinde çalışır, harici monitörde değil.

---

*Apache-2.0*
