# Svelte → React geçişi

**Tarih:** 2026-09-19
**Commit'ler:** `4ff2c06` … `7507a2c` (6 commit)
**Sonuç:** tamamlandı, Svelte depodan kaldırıldı

---

## Özet

13.000 satırlık arayüz React 19'a taşındı. Next.js **elendi** — Tauri'de
sunucu yok, tek `.exe` var; Vite zaten kuruluydu ve kaldı. Tailwind sonraya
bırakıldı, mevcut 67 tasarım değişkeni korundu.

Uygulama hiçbir an kırık kalmadı: React ve Svelte bir süre yan yana çalıştı,
geçiş tek commit'te yapıldı.

---

## Neden bu kadar hata çıktı

Üç subagent paralel çeviri yaptı. **Çıktıları körlemesine kabul edilmedi** —
ve iyi ki edilmedi. Çıkan hataların hiçbiri "çeviri yanlış" cinsinden
değildi; hepsi **çerçeve değişiminin yan etkisiydi** ve üçü de testten
geçiyordu.

### 1. Store'lar: sessizce çizmeyi bırakan 6 nokta

Svelte'in `$state`'i React'te çalışmaz. State katmanı çerçevesiz hâle
getirildi: düz nesneler + yazmaları duyuran bir vekil (proxy).

Vekil **alanı** izler, nesneyi değil. Yani:

```js
this.messages.push(mesaj);   // derlenir, tip denetiminden geçer, ÇİZMEZ
this.messages = [...this.messages, mesaj];   // doğru
```

Altı yerde bu vardı. En kritiği `appendDelta` — saniyede onlarca kez çalışan
akış yolu.

**Kanıt:** eski Svelte store testleri **aynen** yeni store'a karşı koşturuldu,
üstelik vekilin içinden. Davranış değişmedi.

### 2. CSS kapsamlaması: 31 çakışma

En sinsisi buydu. Svelte her `<style>` bloğunu **kendi bileşenine
kilitliyordu.** İki dosya da `.panel` diyebilirdi, karşılaşmazlardı.

Düz CSS olunca karşılaştılar ve **son yüklenen kazandı:**

| `.panel` kimdi | Nerede | Ne diyordu |
|---|---|---|
| Sohbet sütunu | chatPanel.css | `position: relative` |
| Modal pencere | modal.css | `position: fixed` |
| Konsey koltuğu | councilview.css | başka bir şey |

Sonuç: **ayarlar penceresi ekranın altına yarı yarıya taşıyordu.** 149 test
geçerken, tip denetimi temizken.

> **İlk taramam yanlıştı.** "Aynı özelliği farklı değere ayarlayanlar" diye
> aradım, 31'den 17'sini zararsız ilan ettim. Yanlış: `codeview.css`'teki
> `.section` ayarların dikey sütununa `justify-content` **ekliyordu** —
> ayarlarda o özellik hiç yoktu, dolayısıyla "çakışma" saymadım. Doğru kural
> künt olanmış: **bir sınıf adı tek bir dosyaya ait.**

Her bileşenin özel sınıfları kendi adıyla öneklendi. Ve kalıcı koruma:
`styles/scoping.test.ts` bir sınıf iki dosyada tanımlanırsa **testi
kırıyor.**

### 3. Test ortamı: dört ayrı engel

Hiçbiri kod hatası değildi, ama hiçbiri çözülmeden tek test koşmuyordu.

| Sorun | Gerçek sebep |
|---|---|
| Her matcher "Invalid Chai property" | `jest-dom/vitest` alt yolu CJS'e çözülüyor, matcher'ları **başka bir** expect'e bağlıyordu. Bir subagent buna "sürüm uyumsuzluğu" dedi — **yanlış teşhis**, çözümleme sorunuydu |
| 45 tip hatası | Aynı matcher'lar TypeScript'e tanıtılmamıştı |
| `scrollTo is not a function` | jsdom'da düzen motoru yok; `ResizeObserver` da yok |
| Bir test **24 dakika** asılı kaldı | Sahte zamanlayıcı + `userEvent`: userEvent kendi saatini bekliyor, kimse ilerletmiyor |

Kalan 4 başarısız testin **hiçbiri bileşen hatası değildi** — üçü testin
yanlış sorgulaması (dosya içeriğini `<textarea>` yerine metin olarak aramak
gibi), biri jsdom eksiği.

---

## Svelte'te de bozuk olan bir şey

Beş panel etiket-değer satırı için global `.row`'u kullanıyordu — ama o bir
**liste satırı** (monospace, tıklanabilir). Sonuç: `Version0.7.4`, arada
boşluk yok.

Bu React gerilemesi **değil**; ayarlar işinde ortak sınıfları globale
taşırken benim yaptığım hataydı ve Svelte'te de aynı görünüyordu. `.stat-row`
eklendi, Shortcuts kendi satırını aldı.

---

## Kendi hatam: toplu metin değiştirme

Sınıfları yeniden adlandırırken iki hata yaptım, ikisi de aynı dersi veriyor:

1. **`CodeView.tsx` sıfırlandı** — aynı ifadede hem okuyup hem yazdım, okuma
   yazmadan sonra çalıştı. Git'ten geri alındı.
2. **`entry.path` → `code-entry.path`** — `entry` aynı zamanda bir değişken
   adıydı, regex onu da yakaladı.

> Körlemesine metin değiştirme, okumadan düzenlemenin bir türü. Her ikisi de
> derleyici tarafından yakalandı çünkü **her adımdan sonra ölçtüm.**

---

## Doğrulama

**Gerçek uygulamada** (tarayıcı değil, `vavis.exe`): açılıyor, reaktör
dönüyor, eski konuşma geri yükleniyor, markdown ve kod kopyalama çalışıyor.

**Tarayıcıda sürülerek:** kabuk biniyor, 14 ayar paneli de kendi başlığıyla
açılıyor, **hiçbiri yatay taşmıyor**, komut paleti açılıyor, konsol temiz.

**Otomatik:** 127 test (149'dan düştü çünkü Svelte store testleri silindi,
React karşılıkları duruyor) · tip denetimi 0 hata · üretim derlemesi başarılı.

---

## Sayılar

| | Önce | Sonra |
|---|---|---|
| Çerçeve | Svelte 5 | React 19 |
| Bileşen | 34 `.svelte` | 36 `.tsx` |
| Çerçevesiz kalan | — | 6 dosya (`api`, `markdown`, `reactor`, `registry`) |
| Test | 50 | 127 |
| JS paket | 217 KB | 364 KB (gzip 111 KB) |
| CSS paket | 60 KB | 51 KB |

JS büyüdü çünkü React çalışma zamanı pakete giriyor; Svelte derleme
aşamasında kayboluyordu. Tek `.exe` içinde yerel dosya olarak sunulduğu için
ağ maliyeti yok.

---

## Not: takılma sorunu React'le çözülmedi

Geçişin gerekçesi performanstı. Ama **ölçüm başka söylüyordu:** takılmanın
sebebi Rust tarafında saniyede bir çalışan 14 ms'lik bir kilitti ve daha önce
`refresh_cpu_usage()` ile 0.1 ms'ye indirilmişti.

React o hatayı çözmezdi, sadece yanına taşırdı. Geçiş kullanıcı tercihiyle
yapıldı ve düzgün yapıldı — ama kayda geçsin diye yazıyorum.

---

## Yedek

Silinen Svelte ağacı (35 dosya) depo dışında:
`…\scratchpad\svelte-removed\`

Bir eksik çıkmadığına emin olunca silinebilir.

---

## Açık kalanlar

- Tailwind (ileriye bırakıldı)
- `budget.rs` → `PRICES` tablosunda Gemini/Groq fiyatları yok
- Compound seçilince araçlar çalışmıyor, arayüzde not yok
- **Mark LIII'ten:** mikrofon seçici (`capture.rs:196`), geri alma, anında
  karşılık, model ısıtma
- **JARVIS raporundan:** görünürlük ekranı, kişilik seçeneği, tek tık kurulum
