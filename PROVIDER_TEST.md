# Sağlayıcı test listesi

Vavis'te **8 sağlayıcı** var. Çoğu uygulama tek modele basıp geçiyor; bizim
sekiz ayrı tuhaflıkla uğraşmamız gerekiyor. Bu dosya, bir sağlayıcıyı sağlama
alırken **sırayla neye bakılacağını** tutuyor.

Her madde Groq'ta gerçekten bulunmuş bir hatadan geliyor. Uydurma senaryo yok —
hepsi canlı API'ye sorularak ortaya çıktı.

> **Kural:** belgeye değil **API'ye** sor. Groq'un belgesi bu bulguların
> yarısını söylemiyordu; bazılarında belgenin tersi çıktı.

---

## Sağlayıcı durumu

| Sağlayıcı | Denendi mi | Notlar |
|---|---|---|
| **Groq** | ✅ tam tarandı | 6 hata bulundu, hepsi düzeltildi — aşağıda |
| OpenAI | ⬜ | |
| **Gemini** | 🟡 kod hazır, anahtar bekliyor | Yerel API'ye taşındı — OpenAI-uyumlu kapı bırakıldı |
| Mistral | ⬜ | |
| DeepSeek | ⬜ | |
| XAI (Grok) | ⬜ | |
| Nvidia NIM | ⬜ | |
| Anthropic | ⬜ | Ayrı gövde şeması — kendi yolu var (`anthropic.rs`) |
| Local (Ollama/LM Studio) | ⬜ | Anahtar istemez; ayrı kontroller |

---

## Nasıl test edilir

Canlı API'ye soran geçici bir test yazılır, çalıştırılır, **silinir**.

```rust
// crates/vavis-brain/tests/zz_probe.rs  — işin sonunda sil
use vavis_brain::keys::KeyStore;

fn key(provider: &str) -> String {
    let root = std::path::PathBuf::from(std::env::var("APPDATA").unwrap())
        .join("vavis").join("data");
    KeyStore::load(&root).get(provider).unwrap().to_string()
}

#[tokio::test]
async fn probe() {
    if std::env::var("VAVIS_PROBE").is_err() { return; }   // kazara çalışmasın
    // ... sorular ...
}
```

```bash
VAVIS_PROBE=1 cargo test -p vavis-brain --test zz_probe -- --nocapture
```

Sondaj dosyası depoya **girmez**. Bulgular teste ve bu dosyaya yazılır.

---

## Kontrol listesi

### 1. Model listesi — `/models` ne veriyor?

Sağlayıcının kendi listesi çoğu zaman bizim tahmin ettiğimizden fazlasını
söylüyor. Groq her model için şunları veriyor:

```json
{ "id": "...", "context_window": 131042, "max_completion_tokens": 16384,
  "supported_features": ["tools","json_mode","reasoning"],
  "pricing": { "prompt": "0.0000008", "completion": "0.000004" },
  "active": true }
```

- [ ] `context_window` bizim `budget.rs` tablosuyla uyuşuyor mu?
- [ ] `max_completion_tokens` tablodaki `max_output`'tan küçük mü? (küçükse 400 yeriz)
- [ ] `supported_features` araç desteğini söylüyor mu?
- [ ] Listede **sohbet olmayan** modeller var mı? (TTS, whisper, guard, embed)
- [ ] Fiyat bilgisi var mı? (`PRICES` tablosu elle yazılmış, eskiyor)

> **Groq'ta çıkan:** listedeki 13 modelin 4'ü sohbet modeli bile değildi ve
> ikisi kullanıcıya gösteriliyordu. Tablomuz 7 modele gerçekte olmayan pencere
> boyları veriyordu.

### 2. Bağlam penceresi — tablomuz gerçeği söylüyor mu?

İki yönlü hata var, ikisi de farklı zarar veriyor:

| Hata | Sonuç |
|---|---|
| Gerçekten **büyük** sanmak | İstek sağlayıcıda reddedilir, tur kaybolur |
| Gerçekten **küçük** sanmak | Her turda boşuna geçmiş budanır |

- [ ] Her model için `ModelCaps::for_model(id)` ile `/models` karşılaştırıldı mı?
- [ ] Genel bir satır (örn. `"llama"`) dar pencereli bir modeli yakalıyor mu?

> **Groq'ta çıkan:** `"llama"` satırı `llama-prompt-guard` modellerini
> yakalıyordu — gerçek penceresi **512**, biz **32000** diyorduk. O modele
> giden her istek taşardı.

### 3. Araç desteği — model bizim şemalarımızı alıyor mu?

Bu, sağlayıcı bazında değil **model** bazında sorulmalı.

- [ ] Tek bir fonksiyon şemasıyla istek at — kabul ediliyor mu?
- [ ] Sağlayıcının kendi yerleşik araçları var mı? Bizimkilerle aynı dizide
      yaşayabiliyorlar mı?
- [ ] Yerleşik araç tipi gönderilirse sıradan model ne diyor?

> **Groq'ta çıkan:** `groq/compound` tek şema görünce **400** veriyor —
> o model bizde hiç çalışamazdı. Ve `qwen` yerleşik tipleri reddediyor, yani
> "sağlayıcı Groq ise yerleşikleri ekle" demek varsayılan modelimizi kırardı.
> Ayrıntı: [Groq yerleşik araçlar raporu](example/docs/rapor-groq-yerlesik-araclar.md)

### 4. Hata biçimleri — sözlerimiz onların sözlerine uyuyor mu?

Bizde üç sınıflandırıcı var (`commands.rs`): `is_quota_refusal`,
`is_too_long`, `quota_too_small_for_request`. Her sağlayıcı aynı şeyi **başka
kelimelerle** söylüyor ve tutmazsa kurtarma yolu hiç çalışmıyor.

Sorulacaklar:

- [ ] **Bağlam taşması** — çok büyük bir mesaj at. Ne diyor, hangi status?
- [ ] **Geçersiz anahtar** — status ve gövde?
- [ ] **Olmayan model** — status ve gövde?
- [ ] **Hız sınırı** — art arda istek at. 429 mu, başka bir şey mi?
- [ ] **`max_tokens` çok büyük** — reddediyor mu, kırpıyor mu?
- [ ] Hata gövdesi **anahtarı yansıtıyor mu?** (yansıtıyorsa kullanıcıya
      gösterilmeden önce temizlenmeli)

> **Groq'ta çıkan:** bağlam taşmasını `"Please reduce the length of the
> messages or completion."` diye ve **400** ile söylüyor — bizim aradığımız
> kelimelerin **hiçbiri** yok. Kurtarma yolu vardı ve hiç çalışmıyordu.

### 5. Hız sınırı başlıkları

- [ ] `x-ratelimit-*` başlıkları geliyor mu?
- [ ] `retry-after` var mı? Saniye mi, tarih mi?
- [ ] Limit **anahtar** başına mı, **kuruluş** başına mı?
- [ ] Sınır dakikalık mı günlük mü? (günlükse beklemek anlamsız)

> **Groq'ta çıkan:** her cevapta `x-ratelimit-limit-tokens`,
> `-remaining-tokens`, `-reset-tokens` geliyor; 413'te `retry-after: 455`
> geliyor. Sıfırlama süresi `"2m59.56s"` biçiminde — saniye değil.
> **Limit kuruluş başına**, anahtar başına değil: ikinci anahtar açmak
> kapasiteyi artırmıyor.

### 6. Dakikalık token sınırı ≠ bağlam penceresi

Bu ikisi karıştırılıyor ve karıştırılınca bütçe modülü yalan söylüyor.

- [ ] Sağlayıcının dakikalık girdi token sınırı (ITPM/TPM) kaç?
- [ ] Bizim `input_budget()` bu sayının üstünde istek kuruyor mu?

> **Groq'ta çıkan:** `qwen` penceresi 131042, bizim bütçemiz ~101k tokenlık
> istek kurmaya izin veriyor. Ama ücretsiz katmanda **ITPM 7000**. Yani
> bütçeye rahatça sığan bir istek sağlayıcıda reddediliyor. Pencere doğru,
> sınır başka yerde.
>
> Beklemek de kurtarmıyor: 60012 tokenlık istek, dakikada 7000 veren bir
> limitle **asla** geçmez. Artık kullanıcıya "biraz bekle" değil "mesajı
> kısalt" diyoruz.

### 7. İstek gövdesi uyumu

- [ ] `max_tokens` mi kabul ediyor, `max_completion_tokens` mi?
- [ ] Bilinmeyen alanlar (`seed`, `frequency_penalty`) hata veriyor mu?
- [ ] `temperature` üst sınırı kaç?
- [ ] Sistem mesajı ilk sırada olmak zorunda mı?
- [ ] Ard arda aynı rol kabul ediliyor mu?
- [ ] Boş içerikli mesaj kabul ediliyor mu?
- [ ] Araç turu (`assistant.tool_calls` → `tool`) sorunsuz mu?

> **Groq'ta çıkan:** hepsi temiz. `max_tokens` çalışıyor, bilinmeyen alanlar
> sessizce yutuluyor, sistem mesajı herhangi bir yerde olabiliyor, ard arda
> aynı rol sorun değil. `temperature` üst sınırı **2** (2.5 → 400).

### 8. Akış (streaming)

- [ ] `data: [DONE]` gönderiyor mu? (bazıları göndermiyor)
- [ ] Araç çağrıları parça parça mı geliyor? İndeksle birleştirme gerekiyor mu?
- [ ] Akış **başladıktan sonra** hata olursa ne oluyor?
- [ ] Akış içinde boş/bozuk parça geliyor mu?

### 9. Anahtar güvenliği

- [ ] Anahtar loglara düşüyor mu? (`tracing` çağrılarını tara)
- [ ] Hata gövdesi kullanıcıya gösteriliyor — anahtarı içerebilir mi?
- [ ] Anahtar şifreli saklanıyor mu? (bizde Windows DPAPI, `keys.dat`)
- [ ] Anahtar depoya girmiş mi? `git log --all -S "<önek>"`

> **Groq'ta çıkan:** temiz. Anahtar hiçbir log çağrısında geçmiyor, hata
> gövdesi anahtarı yansıtmıyor (`Invalid API Key`, anahtarın kendisi yok),
> `keys.dat` DPAPI ile kullanıcı hesabına bağlı.
>
> Groq anahtarları `gsk_` önekli, **tek tip ve tam yetkili** — kapsamlı
> (scoped) anahtar yok. Sızarsa yapılacak tek şey konsoldan iptal etmek.

---

## Groq — kapanan maddeler (2026-09-16)

| # | Sorun | Nasıl bulundu | Durum |
|---|---|---|---|
| 1 | `groq/compound` tek araç şeması görünce 400; model kullanılamaz | canlı sondaj | ✅ araç gönderilmiyor |
| 2 | Yerleşik `browser_search` gpt-oss'te vardı, kullanılmıyordu | belge + sondaj | ✅ ekleniyor |
| 3 | `llama-prompt-guard` penceresi 512, biz 32000 diyorduk | `/models` karşılaştırması | ✅ tablo düzeltildi |
| 4 | gpt-oss / compound penceresi 131072, biz 32768 diyorduk | `/models` karşılaştırması | ✅ tablo düzeltildi |
| 5 | TTS ve guard modelleri sohbet modeli diye gösteriliyordu | süzgeç çıktısı | ✅ elendi |
| 6 | `gpt-oss-safeguard-20b` "guard" diye eleniyordu ama tam sohbet modeli | süzgeç çıktısı | ✅ gösteriliyor |
| 7 | Bağlam taşması mesajı hiçbir kalıba uymuyordu → kurtarma çalışmıyordu | canlı sondaj | ✅ kalıp eklendi |
| 8 | Kotadan büyük istek "biraz bekle" diyordu, beklemek çözmüyordu | canlı sondaj | ✅ "mesajı kısalt" |

**Açık kalan (Groq):**

- Fiyat tablosu (`budget.rs` → `PRICES`) elle yazılmış ve Groq modelleri yok.
  `/models` fiyatı veriyor; oradan okunabilir.
- `ModelCaps` tablosu elle yazılmış. `/models` gerçeği veriyor — açılışta bir
  kez çekip tabloyu doğrulamak bu sınıf hatanın tamamını kapatır.
- Compound seçildiğinde Vavis'in araçları çalışmıyor; arayüzde bunu söyleyen
  bir not yok.

---

## Gemini — yerel API'ye geçiş (2026-09-16)

**Neden OpenAI-uyumlu kapı bırakıldı:** o kapı bir **alt küme**. Düşünme
(thinking) ayarları, güvenlik eşikleri, `systemInstruction`, çok parçalı
içerik, `usageMetadata` — hiçbiri oradan geçmiyor. Google'ın kendi
belgelediği yol `generateContent`.

### Şekil farkları — OpenAI'a benzeyen hiçbir yanı yok

| | OpenAI | Gemini |
|---|---|---|
| Mesaj dizisi | `messages` | `contents` |
| Rol adları | `assistant` | `model` (`system` rolü **yok**) |
| Sistem istemi | dizide bir mesaj | ayrı `systemInstruction` alanı |
| Metin | `content` düz metin | `parts[].text` |
| Araç şeması | `tools[].function` | `tools[].functionDeclarations[]` |
| Araç çağrısı | `tool_calls[]` | `parts[].functionCall` |
| Araç sonucu | `role: "tool"` | `role: "user"` + `functionResponse` |
| Cevap sınırı | `max_tokens` | `generationConfig.maxOutputTokens` |
| **Model adı** | gövdede | **URL'de** |
| Model listesi | `data[].id` | `models[].name` (`models/` önekli) |

### Dikkat edilenler

- **Anahtar başlıkta gidiyor** (`x-goog-api-key`), sorgu parametresinde değil.
  İki sebep: (1) URL'e giren anahtar istek kaydına, vekil sunucuya ve
  kullanıcıya gösterdiğimiz hata gövdesine sızar; (2) Google'ın yeni biçimli
  anahtarları sorgu parametresini zaten kabul etmiyor — o yolla **404**.
- **`alt=sse` şart.** Olmadan Google akışı bir JSON **dizisi** olarak
  gönderiyor, SSE olarak değil; satır satır çözümleyen kodumuz okuyamaz.
- **Model adı yolda.** `models/` öneki iki kez eklenirse 404. Kullanıcı çıplak
  ad yazıyor, Google önekli yayınlıyor; ikisi de kabul ediliyor.
- **Şema temizliği.** Google OpenAPI'nin bir alt kümesini kabul ediyor;
  tanımadığı bir anahtar görünce aracı yok saymak yerine **isteğin tamamını**
  reddediyor. `additionalProperties` ve `$schema` bizim şemalarımızda var —
  iç içe olanlar dahil ayıklanıyor.
- **Araç sonucu ada göre bağlanıyor**, kimliğe göre değil: Gemini'de
  `tool_call_id` diye bir şey yok.
- **Argümanlar nesne**, metin değil. Bozuk JSON gelirse boş nesneye
  düşürülüyor — olduğu gibi göndermek isteğin tamamını düşürür.
- **Bitiş olayı yok.** Anthropic'teki `message_stop` gibi bir şey gelmiyor;
  akış kapanınca bitiyor. Araç çağrısının o kapanışta kaybolmadığı teste
  bağlandı.
- **Model listesi süzgeci `supportedGenerationMethods` okuyor** — adına bakıp
  tahmin etmek yerine modelin kendi beyanı. Alan yoksa model eleniyor değil,
  ad süzgecine bırakılıyor.

### Test durumu

18 birim testi + 7 tel testi (`tests/gemini_wire.rs`). Tel testleri gerçek bir
soket açıp gövdeyi ve **başlıkları** okuyor; anahtarın URL'e sızmadığı da
orada ölçülüyor.

> **Anahtar yok, canlı doğrulama yapılamadı.** Groq'ta yaptığımız gibi API'ye
> sorulmadı — kod belgeye göre yazıldı. Anahtar girildiğinde şu 9 başlık
> sırayla sorulmalı; özellikle **4 (hata biçimleri)** ve **6 (dakikalık
> sınır)** Groq'ta sürpriz çıkmıştı.

### Anahtar girilince ilk yapılacaklar

1. Model listesi geliyor mu, `models/` öneki temizlenmiş mi?
2. Basit bir tur — metin akıyor mu?
3. Araçlı bir tur — `functionCall` geliyor mu, sonuç geri bağlanıyor mu?
4. Şema reddi var mı? (`clean_schema` yeterli mi, başka yasak anahtar var mı)
5. Hata biçimleri: geçersiz anahtar, olmayan model, bağlam taşması, hız sınırı
6. Güvenlik eşikleri bir şeyi engelliyor mu? (`safetySettings` şu an
   gönderilmiyor — Google varsayılanları uygulanıyor)

---

## Bir sonraki sağlayıcıya geçerken

1. Yukarıdaki 9 başlığı sırayla sor, cevapları not et.
2. Bulunan her hatayı **önce teste yaz**, sonra düzelt.
3. Testi telin üzerinden ölç: karar fonksiyonunu test etmek yetmiyor,
   istemci onu çağırmayı unutabilir. Gerçek bir soket açıp gövdeyi oku
   (örnek: `crates/vavis-brain/tests/groq_builtin_tools.rs`).
4. Bu dosyadaki tabloyu güncelle.
5. Sondaj dosyasını sil.

---

## Kaynaklar

- [Groq Rate Limits](https://console.groq.com/docs/rate-limits)
- [Groq Built-In Tools](https://console.groq.com/docs/tool-use/built-in-tools)
- [Groq Security Onboarding](https://console.groq.com/docs/production-readiness/security-onboarding)
