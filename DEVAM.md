# DEVAM — compact sonrası okunacak

> Bu dosya **geçici**. İş bitince sil (en altta yazıyor).
> Tarih: 2026-09-16

---

## Durum: ne çalışıyor, ne kaldı

**Gemini artık çalışıyor.** Üç ayrı hata vardı, üçü de kapandı ve canlı
API'ye karşı doğrulandı. Son derleme `target/release/vavis.exe` içinde,
push'lu (`5f42649`).

**Ayarlar UI'ı yarım.** 14 panelden **4'ü** yeni yapıya taşındı.
Kalan 10 panel eski hâlinde — çalışıyorlar, sadece eski görünüm.

---

## Sıradaki iş: kalan panelleri taşı

Taşınanlar (örnek olarak bunlara bak):

| Panel | Dosya |
|---|---|
| Web arama | `ui/src/lib/settings/panes/SearchPane.svelte` |
| Görsel & video | `ui/src/lib/settings/panes/CanvasPane.svelte` |
| Araçlar | `ui/src/lib/settings/panes/ToolsPane.svelte` |
| Ses | (kısmen — `Settings.svelte` içinde, anahtar eklendi) |

**Taşınmayı bekleyenler:** general, provider, memory, obsidian, spotify,
steam, mcp, shortcuts, data, updates.

### Ortak bileşenler hazır, yeniden yazma

`ui/src/lib/settings/` altında:

- `Field.svelte` — etiket + açıklama + kontrol. `inline` prop'u satır içi yapar.
- `Section.svelte` — başlık + açıklama + içerik
- `KeyInput.svelte` — sağlayıcı başına anahtar satırı
- `ProviderChain.svelte` — sıralı zincir, ↑↓
- `CustomEndpoint.svelte` — zorunlu üstte, gerisi katlanır
- `registry.ts` — 14 kategori, 4 grup, arama süzgeci (12 testi var)

### Taşıma yöntemi (işe yarıyor, aynen tekrarla)

1. Yeni `panes/XPane.svelte` yaz, ortak bileşenleri kullan
2. `Settings.svelte`'te o `{:else if active === "x"}` bloğunu tek satıra indir
3. `npx svelte-check` → "declared but never read" hatalarını takip ederek
   eski fonksiyon/state/CSS'i sil (derleyici yol gösteriyor)
4. Her panelden sonra ayrı commit

---

## Bu oturumda düzeltilen hatalar

| Commit | Ne |
|---|---|
| `155ea6f` | Emekli Gemini modelleri + sağlayıcının kendi sesi |
| `d3fc077` | Ayarlar: görünmez anahtar kutusu, taşan onay kutusu, gruplama |
| `164b82a` | Görsel & video paneli, ikinci zincir kopyası kaldırıldı |
| `f669fed` | Araçlar paneli, süzgeç + risk çipleri |
| `8625413` | Ses: sağlayıcı eşleştirme anahtarı |
| `69b988c` | **Kayıtlı model** + saniyede bir 14 ms kilit |
| `5f42649` | **Gemini thought signature** |

### Öğrenilen üç şey — tekrar etmemek için

**1. Listeyi süzmek yetmiyor, kayıtlı değeri de kontrol et.**
`vavis.toml`'da bozuk model duruyordu. Seçici temizlendi ama kimse listeyi
yeniden okumuyor. Düzeltme `model_for()` içinde — **okuma** anında.

**2. Saniyede bir çalışan kod ucuz olmalı.**
`get_status` tek bir CPU sayısı için `refresh_all()` çağırıyordu: tüm
süreçler, diskler, ağ arayüzleri. **14 ms**, üstelik ayarların beklediği
kilitler elindeyken. `refresh_cpu_usage()` ile **0.1 ms**.

**3. Belge yanlış olabilir, API'ye sor.**
Google'ın belgesi "imzalar standart function call'larda **asla** görünmez"
diyor. Canlı API'de tam tersi. Ölçmeseydik bulunamazdı.

---

## Açık kalanlar

- **Ayarların 10 paneli** (yukarıda)
- `budget.rs` → `PRICES` tablosunda Gemini/Groq fiyatları yok
- `ModelCaps` tablosu elle yazılmış; `/models` gerçeği veriyor
- Compound seçilince Vavis'in araçları çalışmıyor, arayüzde not yok
- **Mark LIII'ten:** mikrofon seçici (`capture.rs:196` hâlâ
  `default_input_device()`), undo, anında karşılık, model ısıtma

---

## Çalışma kuralları

- **Ölçmeden "çalışıyor" deme.** Bu oturumdaki hataların hepsi çalıştırarak
  bulundu, okuyarak değil.
- **Alt ajan çıktısı da ölçülmeden kabul edilmemeli.** Bir ajan Gemini
  modelleri hakkında yanlış bilgi verdi; önerdiği süzgeç 58 modelin hepsini
  silerdi.
- **Teli test et, karar fonksiyonunu değil.** `tests/gemini_wire.rs` gerçek
  soket açıp gövdeyi okuyor — fonksiyon doğru cevap verse bile istemci onu
  çağırmayı unutabilir.
- Test doğrulaması: her suite'in **durumuna** bak, sayı toplama.

---

## Kaynaklar

- `PROVIDER_TEST.md` — 9 başlıklı sağlayıcı kontrol listesi (kökte)
- `example/docs/rapor-groq-yerlesik-araclar.md`
- Obsidian: `Vavis/notes/araclar/Gemini.md`, `Groq yerlesik araclar.md`,
  `bulunan hatalar.md`

---

## ⚠️ BU DOSYAYI SİL

Ayarların kalan panelleri taşınınca `DEVAM.md` silinecek. Depoda kalmamalı —
geçici hatırlatma, proje belgesi değil.

```
rm DEVAM.md
```
