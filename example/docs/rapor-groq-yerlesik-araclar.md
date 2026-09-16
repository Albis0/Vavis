# Groq yerleşik araçları — araştırma ve düzeltme

**Tarih:** 2026-09-16
**İstek:** *"groq ta built in tools varmış onları araştırcan ve groq kullanılırken o toollar katiyen gitmicek mantığı"*

---

## Kısa cevap

Groq'un yerleşik araçları var ve bunlardan bir grubu **bizim araçlarımızla bir arada çalışmıyor** — belgeler böyle diyordu, canlı API de doğruladı. `groq/compound` ailesine tek bir araç şeması göndermek isteğin tamamını düşürüyor:

```
POST /openai/v1/chat/completions   model=groq/compound   tools=[tek bir fonksiyon]
→ 400 {"message": "`tool calling` is not supported with this model"}
```

Aynı istek `tools` alanı olmadan **200** dönüyor. Yani bu model bizde bugüne kadar hiç çalışamazdı: kullanıcı ayarlardan seçseydi her mesaj hatayla dönerdi.

Artık seçilebiliyor — şemalar gönderilmiyor, model kendi web aramasıyla cevaplıyor.

---

## Groq'ta yerleşik araçlar gerçekte ne

İki ayrı sistem var ve karıştırmak kolay.

### 1. Compound sistemleri — `groq/compound`, `groq/compound-mini`

Araçları **sunucu tarafında kendisi** çalıştırıyor: web araması, sayfa okuma, kod çalıştırma, Wolfram Alpha. Tek bir API çağrısında tüm döngüyü tamamlayıp sonucu döndürüyor.

| Araç | Tanımlayıcı |
|---|---|
| Web araması | `web_search` |
| Sayfa okuma | `visit_website` |
| Kod çalıştırma | `code_interpreter` |
| Wolfram Alpha | `wolfram_alpha` |

Açma/kapama yeri `compound_custom.tools.enabled_tools`; varsayılan olarak zaten açıklar.

> Belgelerin kendi cümlesi: *"Groq's Compound systems only support built-in tools and cannot be used with local tool calling or remote MCP tools."*

**`browser_automation` emekli edildi.** İstekte hâlâ kabul ediliyor ama hiçbir etkisi yok — koda eklemedik.

### 2. GPT-OSS ailesi — `openai/gpt-oss-120b`, `openai/gpt-oss-20b`

Bunlar farklı: yerleşik araçlarını normal `tools` dizisinde tip nesnesi olarak bildiriyorlar ve **bizim fonksiyon şemalarımızla aynı dizide yaşayabiliyorlar.**

```json
"tools": [ {"type": "browser_search"}, {"type": "function", "function": {...}} ]
```

---

## Ölçüm — belgeye değil API'ye soruldu

Canlı Groq'a tek bir fonksiyon şemasıyla dört model denendi:

| Model | Sonuç |
|---|---|
| `groq/compound` | **400** — `tool calling is not supported with this model` |
| `groq/compound-mini` | **400** — aynı mesaj |
| `openai/gpt-oss-120b` | 200 |
| `qwen/qwen3.8-27b` | 200, `tool_calls: [saat]` |

İkinci tur — yerleşik tipler ve karışım:

| Deneme | Sonuç |
|---|---|
| gpt-oss: `browser_search` + bizim şema | **200** — ikisi bir arada gidiyor |
| gpt-oss: `code_interpreter` + bizim şema | **200** |
| **qwen: `browser_search`** | **400** — `tools[0].type must be one of [function, mcp]` |
| compound: `tools` alanı yok | **200** |
| compound: `tools: []` (boş dizi) | **200** |

Son iki satır önemliydi: boş dizi de kabul ediliyor, ama yine de alanı hiç koymamayı seçtik — Groq yarın boş diziyi de reddederse fark etmesin diye.

**Üçüncü satır sürprizdi.** "Yerleşik araçları Groq kullanırken hep ekleyelim" yaklaşımı `qwen`'i — yani şu anki varsayılan modelimizi — tamamen kırardı. Bu yüzden kural **sağlayıcıya göre değil modele göre** yazıldı: Groq kullanmak tek başına bir şey söylemiyor, hangi Groq modeli olduğu söylüyor.

---

## Ne yapıldı

### Yeni modül: `crates/vavis-brain/src/builtin.rs`

Üç soruyu cevaplıyor:

- `tool_support(provider, model)` → bu model bizim şemalarımızı kabul eder mi?
- `extra_tools(provider, model)` → isteğe eklenecek yerleşik araçlar
- `covers_web_search(provider, model)` → yerleşik arama bizimkinin yerini alıyor mu?

Compound eşleşmesi **tam ad değil önek**: `groq/compound` ile başlayan her şey. Sebep, hatanın iki yönünün eşit olmaması — yanlışlıkla "araç yok" demek asistanı sadece zayıflatır, yanlışlıkla "araç var" demek her isteği 400'e düşürür. İleride `groq/compound-3` çıkarsa güvenli tarafa düşer.

### Süzgecin yeri: `client.rs`, gövdenin kurulduğu tek nokta

Çağıran taraf unutamasın diye seçim yerine **gönderim** anına konuldu. Bütçe sığdırmasından da önce çalışıyor, böylece gönderilmeyecek şemalar bütçeyi boşuna yemiyor.

```rust
let tools = if builtin::tool_support(cfg.provider, &cfg.model).accepts_functions() {
    tools
} else {
    &[]   // Compound: tools alanı gövdede hiç oluşmaz
};
```

Yerleşik araçlar da burada ekleniyor. Bütçeye sayılmıyorlar — şema değil tek satırlık tip nesneleri, toplamı birkaç token.

### `web_search` çakışması: `commands.rs`

GPT-OSS'te yerleşik arama açıkken kendi `web_search` aracımız **artık gönderilmiyor**. İkisini birden vermek modele aynı iş için iki kapı açıyor ve hangisinin sonucunu kullanıcının görebildiği belirsiz kalıyor. Yerleşik olan bizden anahtar istemiyor, o yüzden o kazanıyor.

Aynı eleme tur ortasındaki `request_tools` yoluna da kondu — model sonradan "bana arama aracı ver" derse geri sızmasın diye.

### Anthropic yolu

Elle kontrol edildi, dokunulmadı: ayrı gövde şeması kullanıyor ve hiçbir zaman Groq olmuyor.

---

## Testler

**Birim testleri** (`builtin.rs`, 6 adet) kararları sabitliyor: compound reddedilir, bilinmeyen compound sürümü de reddedilir, sıradan Groq modelleri şema alır, kural sadece Groq'a uygulanır, yerleşik arama sadece gpt-oss'e gider.

**Asıl testler telde ne olduğunu ölçüyor** (`tests/groq_builtin_tools.rs`, 4 adet). Sebep: `tool_support` doğru cevabı verse bile istemci onu çağırmayı unutabilir — birim testi bunu göremez. Bu testler gerçek bir TCP sunucusu açıp isteğin **gövdesini** okuyor:

| Test | Ne garanti ediyor |
|---|---|
| `compound_gets_no_tools_field_at_all` | `tools` alanı gövdede yok — boş dizi bile değil |
| `compound_mini_is_treated_the_same` | Aile geneli |
| `an_ordinary_groq_model_still_receives_our_schemas` | Süzgeç fazla geniş değil; qwen'e yerleşik tip karışmıyor |
| `gpt_oss_gets_the_builtin_search_next_to_our_schemas` | Yerleşik eklenirken bizimkiler düşmüyor |

Üçüncüsü bilerek var: bir süzgecin en sinsi hatası fazla eleyip asistanı sessizce araçsız bırakmak.

### Doğrulama

- `cargo test --workspace` → çıkış kodu **0**, 16 suite'in **hepsi** `ok`, `FAILED` satırı yok
- `cargo clippy --workspace --all-targets -- -D warnings` → çıkış kodu **0**
- Canlı uçtan uca, düzeltme devredeyken:

| Model | Önce | Sonra |
|---|---|---|
| `groq/compound` | 400, her mesaj hata | **200**, kendi aramasıyla cevaplıyor |
| `openai/gpt-oss-120b` | çalışıyordu | çalışıyor, `tool_calls: [saat]` |
| `qwen/qwen3.8-27b` | çalışıyordu | çalışıyor, `tool_calls: [saat]` |

`groq/compound` gerçek bir cevap döndü: *"saat kaç? 13:04:39 (Turkey – TRT, UTC+3)"* — sunucu tarafı araçlarını kendisi kullanarak.

---

## Açık kalanlar

- **`code_interpreter` bilerek eklenmedi.** Vavis'in kendi dosya ve kabuk araçları var; ikisini birden vermek modeli aynı işi iki ayrı yerde yapmaya davet ediyor ve kullanıcı hangisinin çalıştığını göremiyor. İstenirse `extra_tools` tek satırla açar.
- **Compound'da araç yok demek, Vavis'in araçları yok demek.** O modeli seçen kullanıcı dosya, hafıza, Spotify, Obsidian hiçbirine erişemez — sadece sohbet ve web. Artık çöküyor değil ama kısıtlı. Arayüzde bunu söyleyen bir not iyi olur; bu rapor kapsamında yapılmadı.
- **Fiyat tablosunda Groq modelleri yok** (`budget.rs` içindeki `PRICES`). Konuyla ilgisiz ama bu iş sırasında görüldü.

---

## Kaynaklar

- [Groq Built-In Tools](https://console.groq.com/docs/tool-use/built-in-tools)
- [Compound Built-in Tools](https://console.groq.com/docs/compound/built-in-tools)
- [Compound Systems](https://console.groq.com/docs/compound/systems/compound)
- [Browser Automation (emekli)](https://console.groq.com/docs/tool-use/built-in-tools/browser-automation)
