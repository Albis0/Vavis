# Kod için ayrı model — ve `my_ideas` dosyasından kalanlar

Tarih: 2026-09-20
Commit: `a4a847b`

---

## 1. Yapılan iş: kod işi kendi modeline gidiyor

### Sorun

`my_ideas.txt` satır 24:

> kişi code için ayrı chat için ayrı ai modeli provider seçebilmeli

Ve satır 26'daki şikâyet aslında aynı sorunun diğer yüzü:

> bizim gemini flash 10 saniyede falan tool kullanmaya başlıo

Sohbet ile kod modelden **zıt** şeyler ister:

| | ister |
|---|---|
| Sohbet | cevap, düşünce kaçmadan gelsin — hız |
| Kod | cevap **doğru** olsun — beklemeye razı |

Tek slot varken kullanıcı birini feda etmek zorundaydı: ya sohbet
refactor fiyatına çalışan bir modelde koşuyordu, ya kod refactor
yapamayan bir modelde.

### Çözüm

`Llm` yapısına iki alan eklendi — ikisi de **isteğe bağlı, varsayılanı
boş**:

```rust
pub code_provider: String,  // boşsa sohbetinki
pub code_model: String,     // boşsa sağlayıcının varsayılanı
```

Boş varsayılan kasıtlı: bu özelliği istemeyen hiç kimsenin davranışı
değişmiyor. Daha önce yazılmış her `vavis.toml` aynen çalışmaya devam
ediyor.

### Yönlendirme nereden karar veriliyor

İlk akla gelen "bir klasör açıksa kod işidir" idi. **Yanlış olurdu:**
proje açıkken sorulan alakasız bir soru hâlâ sohbettir.

Doğru sinyal arayüzde: mesajın hangi panelden geldiğini yalnızca o
biliyor. Ama burada bir tuzak var — kod ekranı yazı yazdırmak için seni
sohbet ekranına gönderiyor (`CodeView.tsx:163`), yani mesaj gönderildiği
anda `view` zaten `"chat"` diyor ve sorunun nereden geldiği kaybolmuş
oluyor.

Bu yüzden `view`'dan türetilmedi, ayrı bir işaret kondu:

- kod ekranı devrettiği turu işaretler
- mağaza o mesaj gönderilir gönderilmez işareti siler
- yani **tam olarak devredilen tur** işaretli; sonrasında kendi yazdığın
  tur yine sohbet

Yeni bir sohbet açmak da işareti siliyor: gönderilmemiş devredilmiş bir
istem yeni konuşmaya sızmıyor.

### Yarım kalmış kurulumun üç hâli

Hepsi turu düşürmek yerine sohbet modeline dönüyor:

| durum | ne oluyor |
|---|---|
| Kod sağlayıcısı seçilmiş, anahtarı girilmemiş | sohbet modeli cevaplıyor |
| Kaydedilmiş kod modeli artık sunmayacağımız bir model | sağlayıcının varsayılanı |
| Tanımadığımız sağlayıcı adı (elle düzenlenmiş config) | sohbet modeli |

Ortadaki madde önemli: emekli model koruması (`model_for`) **bu slotta
da** çalışıyor. Çalışmasaydı, `gemini-2.5-flash-native-audio-latest`
yüzünden her mesajın 400 döndüğü hata aynen diğer kapıdan geri gelirdi.

### Testler

`llm_for` için 7 test yazıldı — her dal ayrı:

```
code_falls_back_to_chat_when_no_code_provider_is_set
code_uses_its_own_provider_and_model
a_code_provider_with_no_key_falls_back_instead_of_failing
a_keyless_code_provider_needs_no_key_to_be_chosen
a_retired_code_model_is_replaced_by_the_default
a_qualified_code_model_is_sent_bare
an_unknown_code_provider_falls_back_to_chat
```

**Testlerin gerçekten yakaladığı doğrulandı**, varsayılmadı: anahtar
kontrolü kasten bozuldu (`if false`), ilgili test kırmızıya döndü, dosya
geri alındı. Aynısı yeni CSS sınıfı için de yapıldı — `orphans.test.ts`
sınıfı gerçekten kapsıyor.

---

## 2. Ölçümün kaçırdığı, ekran görüntüsünün yakaladığı üç şey

Ekran gerçek Playwright ile sürüldü: sağlayıcı seç → model listesini çek
→ model seç → kapat. Dördü de doğru komutu gönderdi.

Ama **ölçümler temiz dönerken** ekran görüntüsü üç hatayı gösterdi:

| Hata | Sebep |
|---|---|
| `default: default for openai` — kelime ikiye katlanıyor | `Field` zaten "default: " ekliyor, ben ikinciyi yazmışım |
| Seçili model soluk, doldurulmamış kutu gibi | `.card-model` ödünç alınmıştı; o sınıf kart **içinde ikincil detay** olduğu için soluk |
| — | (üçüncüsü ölçümle de doğrulandı: yatay taşma yok, 1400/1400) |

İkincisi için ayrı sınıf açıldı (`.current-model`): burada model
**ayarın kendisi**, ikincil detay değil. Soluk tipografi gerçek bir
seçimi placeholder gibi gösteriyordu.

Üçüncü bir şüphe — açıklama metinlerinin 760px sütunda ~410px'te
sarması — **incelendi ve hata değil**: `.section .blurb` üzerindeki
`max-width: 62ch` kasıtlı okuma ölçüsü.

---

## 3. Temizlik

`ui/svelte.config.js` silindi. React geçişinden kalmış ölü dosya:
hiçbir yerden referans verilmiyor, bağımlılığı da artık yok. Silindikten
sonra derleme doğrulandı. Yedeği scratchpad'de.

`TODO.md`'deki kod arayüzü maddesi silinmiş bir Svelte dosyasını işaret
ediyordu; yol düzeltildi ve yeni yetenek kaydedildi.

---

## 4. `my_ideas` dosyasının durumu

| # | Fikir | Durum |
|---|---|---|
| 5 | API'leri tek panele topla | ✅ bitti (`d3b9856`) |
| 4a | Kod/sohbet için ayrı model | ✅ bitti (bu commit) |
| 4b | Kod harness'i "claude code kalitesinde" | ⬜ büyük iş, aşağıya bak |
| 4c | Canvas | ✅ zaten çalışıyor ("komple siki tutmuş") |
| 1 | VirusTotal | ⏸ karar bekliyor |
| 2 | Watch modu (film/anime) | ⏸ karar bekliyor |
| 3 | VS Code eklentileri / Claude Code gömme | ⚠ kısmen mümkün |

### Kod harness'i (4b)

Şu an kod ekranı tek soru–tek cevap. "Claude Code kalitesi" demek
**döngü** demek: oku → değiştir → çalıştır → çıktıyı oku → düzelt.
`agent.rs` 550 satır ve temeli var, ama bu saatlerin değil günlerin işi.
Ayrı oturum ister.

Artık en azından **doğru model** o işi yapabilecek — altyapı hazır.

### VirusTotal (1)

Fikirde "farklı hesaplardan birden fazla api alıp free tierleri
zincirlemek" geçiyor. Bunu **yapmamayı öneriyorum**, sebebi teknik değil
pratik: çoklu hesap free tier zincirlemek servislerin kullanım şartlarını
ihlal eder ve tespit edilince **hepsi birden** kapanır — yani tam da
ihtiyacın olduğu anda hiçbiri çalışmaz.

Önerdiğim şekil:

1. Kullanıcının kendi anahtarı **birincil** (VirusTotal free tier zaten
   kişi başı ücretsiz — dakikada 4, günde 500 istek; normal kullanım için
   fazlasıyla yeter)
2. Anahtarı olmayan için **deneme amaçlı** tek bir sınırlı yol
3. Sınır dolunca "kendi anahtarını gir, 1 dakika sürüyor" diyen net bir
   mesaj

Bu hem çalışır hem de ilk 100 kullanıcıda patlamaz.

### Watch modu (2)

Anlatılan şey (Consumet / vidsrc tarzı, hem katalog hem sunucu veren
bedava API'ler) teknik olarak yapılabilir ama iki gerçek sorun var:

1. O servisler telif korumalı içeriği aracılık ederek dağıtıyor — uygulamaya
   gömmek uygulamayı da o zincire sokar
2. Sürekli kapanıyorlar; bugün çalışan endpoint üç ay sonra yok

**Yapılabilir alternatif:** izleme takibi (yarıda bıraktığın yerden devam,
son izlenenler, öneriler) + "watch together" oda mantığı + yönetmen rolü —
bunların hepsi içerik kaynağından bağımsız. Kaynak olarak da TMDB (katalog,
resmî ve ücretsiz) ile yasal sağlayıcıların kendi bağlantıları kullanılır.

Yani **fikrin yapısı tamamen yapılabilir**, sorun yalnızca akış kaynağında.
Karar senin.

### VS Code eklentileri (3)

- VS Code eklentilerini dışarıdan çalıştırmak **mümkün değil** — eklenti
  API'si VS Code sürecinin içinde yaşıyor
- Ama "o bölmede Claude Code çalıştırmak" **kolay**: gömülü bir terminal,
  CLI zaten var

---

## Doğrulama

```
cargo test --workspace    879 test, hepsi geçti
cargo clippy --workspace  temiz
cargo fmt --check         temiz
npx tsc --noEmit          temiz
npx vitest run            141 test, hepsi geçti
npx vite build            başarılı
```

Ekran gerçek tarayıcıda sürüldü, dört adımın dördü de doğru komutu
gönderdi, yatay taşma yok.

Yedekler: `scratchpad/backup-codemodel/`
