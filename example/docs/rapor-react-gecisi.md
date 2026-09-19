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

## Geçişten sonra çıkan dört hata

Uygulama açılıp kullanılınca üç görsel hata daha çıktı. **Hepsi 149 test
geçerken ve tip denetimi tertemizken duruyordu** — ve hepsi aynı kökten:
Svelte'in kapsamlamasının sessizce çözdüğü şeyler.

### 1. Ayarlar penceresi çerçevesini doldurmuyordu

`.content` → `.modal-content` yeniden adlandırması CSS'i değiştirdi ama
işaretlemeyi kaçırdı (koşullu ifadenin içindeydi). Panel gövdesi `flex: 1`
kuralını kaybetti: **1039 px'lik panelde 730 px içerik**, altta 300 px'lik
delikten reaktör görünüyordu.

Aynı hata `.chip` → `.settings-chip`'te de vardı (araç risk süzgeçleri).

### 2. Ayarların üstünde dönen daire

`reactor.css`'te `.fallback` **220 px'lik dönen bir halka** (reaktör
çizilemezse gösterilen yedek). `settings.css`'te `.setting .fallback` ise
küçük bir "varsayılan: …" yazısı. Aynı ad, tamamen farklı iş.

Sonuç: sağlayıcı kartlarının üstünde dönen bir çember ve kenarına kıvrılmış
"default: off — keyword matching" yazısı.

> **Çakışma denetimim bunu kaçırdı.** Sadece tek sınıflı kuralları
> karşılaştırıyordu; `.setting .fallback` iki parçalı olduğu için listeye
> girmemişti. Denetim **iki kez yanlış çıktı**, o yüzden artık doğru soruyu
> soruyor: *bir kuralın seçicisi tek bir bileşene bağlı mı?* Bağlı değilse
> (`.fallback {}`) her yere ulaşır ve benzersiz olmak zorundadır; bağlıysa
> (`.setting .fallback`, `.code-entry.active`) kendi bileşeninden çıkamaz.
>
> Bu kurala göre 9 sınıf daha kapsandı — aralarında `.md` de vardı: Modal'ın
> `size="md"` değeri Message'ın markdown gövdesiyle çarpışıyordu.

### 3. 58 araçlık liste sayfadan taşıyordu

ToolsPane'in listesinde Svelte'te `max-height: 380px` vardı, taşınırken
düştü. Süzgeç kutusu ekrandan kayıyordu. Geri kondu (`.tool-list`), ölçüldü:
380 px kutu, 3252 px içerik, kaydırılabilir.

### 4. `cargo build` eski arayüzü paketliyordu

**Bu en önemlisi, çünkü beni bir tur yanılttı.** CSS düzeltmesini tarayıcıda
doğruladım, exe'yi yeniden derledim — hata ekranda duruyordu. Kaynak
doğruydu, paket bir saat eskiydi.

Sebep: `tauri_build` `ui/dist` klasörünü **gömüyor ama derlemiyor**, ve
`cargo build` `beforeBuildCommand`'ı çalıştırmıyor (o `tauri build`'e ait,
üstelik boştu).

İki düzeltme: `build.rs` artık `ui/dist`'i girdi olarak tanıyor, ve iki
`before*Command` dolduruldu.

> **Ders:** "düzelttim ama çalışmıyor" dediğinde önce **neyin çalıştığını**
> kontrol et. Derlenen şey ile çalışan şey aynı olmayabilir.

### Kalıcı koruma

İki test eklendi:

- `styles/scoping.test.ts` — bir kural başka bileşene ulaşabiliyorsa kırılır
- `styles/orphans.test.ts` — hiçbir bileşenin kullanmadığı CSS sınıfı varsa
  kırılır (ilk iki hatanın bıraktığı iz tam olarak buydu)

**Doğrulama:** 14 panelin hepsi ölçüldü (senin ekran boyutun ve yazı
tipinle) — hepsi panelini dolduruyor, yatay taşma yok, çerçeveden taşan öğe
yok, konsol temiz. 129 test, 17 Rust paketi, tip denetimi temiz.

---

## Ayarlar ekranının görünümü

Dört hata kapandıktan sonra ekran çalışıyordu ama **bitmemiş görünüyordu.**
Hepsi aynı cinsten: tarayıcının ya da işletim sisteminin varsayılanı, biz
bir şey söylemediğimiz için olduğu gibi kalmış.

### 1. Açılır listeyi işletim sistemi çiziyordu

`<select>`'in **açılan kutusu** CSS'in ulaşamadığı tek yer — onu pencere
yöneticisi çizer. Hiçbir yerde `color-scheme` tanımlı değildi, dolayısıyla
liste **beyaz zemin, gri yazı** olarak açılıyordu: koyu arayüzün ortasında
bembeyaz bir kutu.

Kapalı hâli de öyleydi: sistemin kendi çerçevesi ve kendi oku. Artık
`color-scheme` iki temada da tanımlı (açık temada `light`, yoksa aynı hata
ters yönde olur), kapalı kutu da diğer alanlar gibi çiziliyor — ok bir
arka plan görseli.

### 2. Etiket ve açıklaması tek satırda

`Assistant name` ile `spoken aloud, so pick something sayable` aynı
taban çizgisindeydi ve tek bir cümle gibi okunuyordu. Alt alta kondu,
etiket kalınlığı aldı.

### 3. Aynı sütunda üç farklı sol kenar

Sağa yaslı kontroller kendi içeriklerine göre boyutlanıyordu: aynı panelde
**102 px, 116 px, 106 px.** Sağ kenarları hizalı olduğu için sol kenarları
bozuk görünüyordu. Ortak bir `min-width: 160px` kondu — `width` değil, uzun
bir seçenek hâlâ büyüyebilsin diye.

### 4. Düğme gibi görünmeyen düğmeler

`cycle mode` ve `hear this voice` uygulamanın varsayılan düğmesini
kullanıyordu: şeffaf zemin, soluk yazı. Araç çubuğundaki bir simge için
doğru, formda tek başına duran bir eylem için yanlış — fare üstüne gelene
kadar başıboş bir yazı gibi duruyordu.

### 5. Ayıracak bir şeyi olmayan ayraç

`.stat-row` her satırın altına çizgi çekiyordu. Listede doğru; Ses
panelindeki tek başına duran "Mode" satırının altında, hiçbir şeye
benzemeyen bir çizgi.

### 6. Sütunun yarısına sıkışmış metin

Bölüm açıklamaları `62ch` ölçüsündeydi — doğru bir okuma genişliği — ama
11.5 px yazıyla bu **380 px** ediyor, sütun ise 760 px. Ölçü korundu,
punto bir kademe büyütüldü.

### Doğrulama

14 panelin hepsi senin ekran boyutunda (1831×1103) tarandı: yatay taşma
yok, panel dışına taşan öğe yok, sistemin kendi çizdiği açılır liste
kalmadı, konsol temiz. 129 test, tip denetimi 0 hata. Üç düzeltmenin de
derlenen pakette olduğu doğrulandı.

> Taramanın **Data** panelinde "çerçevesiz düğme" işaretlemesi yanlış
> alarmdı: `Clear the conversation` kasıtlı olarak kırmızı yazı, dolu
> düğme değil. Denetim fazla genişti, arayüz değil — dokunulmadı.

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
