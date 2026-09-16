# Neden onlarınki akıcı, bizimki değil

**Tarih:** 2026-09-16 · **Sürüm:** 0.7.4

---

## Kısa cevap

**Qwen değil.** İki somut hata vardı, ikisi de kodda, ikisi de düzeltildi.

Biri modelin doğru aracı **hiç görmemesine** yol açıyordu. Diğeri sesin
cevabın tamamını beklemesine. İkisi birleşince uygulama hem yanlış
cevap veriyor hem geç konuşuyor gibi hissettiriyordu.

---

## 1. "bilgisayar" kelimesi hafızaya gidiyordu

En ciddi olanı ve en sinsisi.

Alan eşleştirme, anahtar kelimeyi **kelimenin herhangi bir yerinde**
arıyordu. Sebebi meşru: Türkçe sondan eklemeli, `"dosya"` araması
`"dosyayı"` kelimesini de yakalamalı.

Ama `"bilgisayar"` kelimesinin içinde `"bilgi"` var — ve `"bilgi"`
hafıza alanının anahtar kelimesiydi.

Düzeltmeden önce ölçülen:

| Cümle | Giden alan |
|---|---|
| bilgisayarı kapat | `[Memory]` |
| bilgisayarımı yeniden başlat | `[Control, Memory]` |
| bilgisayar yavaş | `[Memory]` |
| bilgisayarda ne var | `[Memory]` |

Yani kullanıcı "bilgisayarı kapat" dediğinde modele **hafıza araçları**
gidiyordu; kapatma, komut çalıştırma, uygulama yönetimi araçları hiç
ulaşmıyordu. Model de elindeki araçlarla bir şey uydurmaya çalışıyordu.

Bir asistanda "bilgisayar" en sık geçen kelimelerden biri. Tek bir
anahtar kelime, isteklerin önemli bir kısmını yanlış yere gönderiyordu.

### Düzeltme

`"bilgi"` kaldırıldı — hafıza alanı zaten `"hatırla"`, `"unut"`,
`"hafıza"`, `"söylemiştim"` ile tanımlı.

Cihazın kendisi kontrol alanına eklendi (`bilgisayar`, `makine`,
`computer`, `pc`, `tarayıcı`, `browser`). Gerekliydi çünkü `"kapat"` ve
`"aç"` bilerek **zayıf fiil**: tek başına alan tetiklemiyorlar
("gözlerini aç" dosya açmak değil). Yanlarında güçlü bir kelime lazım,
ve kullanıcının söylediği kelime tam olarak buydu.

Sonrası:

| Cümle | Giden alan |
|---|---|
| bilgisayarı kapat | `[Control]` |
| bilgisayarımı yeniden başlat | `[Control]` |
| tarayıcı aç | `[Control]` |

### Neden hiçbir test yakalamadı

Yakalayamazdı: test edilen şey "zayıf fiil tek başına tetiklemesin"di,
"doğru alan gelsin" değil. Hata sessizdi — panik yok, hata mesajı yok,
sadece model yanlış araçlarla baş başa kalıyordu.

İki regresyon testi eklendi.

---

## 2. Ses, cevabın tamamını bekliyordu

Metin zaten akıyordu (`chat:delta`), ama ses **cevap tamamen bitene
kadar** hiç başlamıyordu:

```rust
Ok(reply) => {
    ...
    AppState::lock(&voice).speak(&reply);   // ← cevabın sonunda
}
```

Yani ekran kelimelerle dolarken hoparlör susuyordu. Uzun bir cevapta
beklemenin büyük kısmı buydu.

### Düzeltme

Cümleler **tamamlandıkça** ses katmanına veriliyor. İlk cümle çalarken
model geri kalanını hâlâ yazıyor.

Sadece **bitmiş** cümleler gidiyor — yarım cümle okunursa düşüncenin
ortasında kesiliyor ve sonraki parça baştan başlıyor, ki beklemekten
kötü. Kalan kısım tamponda duruyor.

İki incelik:

- **Barge-in tamponu da siliyor.** Kullanıcı sözü kestiğinde tamponda
  kalan cümle sonradan okunmamalı.
- **Yeni tur tamponu temizliyor.** Terk edilmiş bir turun yarım cümlesi
  sonraki cevaba sızmamalı.

Dördü de testle korunuyor.

---

## 3. Yorumlarda kalan eski adlar

Dokuz yorum hâlâ `komut_calistir`, `simdiki_zaman`, `arac_iste` diyordu.
Modele giden bir şey değil — ama kodu okuyan biri için yanıltıcı.
Düzeltildi.

---

## Peki neden onlarınki hatasız çalışıyor

Dürüst cevap: **onlarınki de hatasız değil**, sen onun hatalarını
görmüyorsun çünkü senin makinende senin kullandığın yollardan
geçmiyorsun.

Somut örnek: onun ses motoru hata alınca sadece `print` yapıp susuyor.
Kokoro modeli inmemişse ya da ElevenLabs kotası bittiyse kullanıcı
**sessizlik** duyuyor ve sebebini hiç öğrenmiyor. Bizde aynı durumda
yedek motor devreye giriyor ve sesli haber veriyor.

Ama haklı olduğun taraf şu: onda bu iki hata yok, çünkü

- **Tool seçimi yok.** 16 aracın hepsi her istekte gidiyor, yani
  "yanlış araç seçildi" diye bir kategori bulunmuyor. Bizde 58 araç var
  ve seçmek zorundayız — bu bizim ölçek problemimiz, onun değil.
- **Ses akışla geliyor.** Gemini Live ses üretiyor, metin değil. Bizim
  elle kurduğumuz akış onda mimarinin kendisi.

Yani fark modelden değil, **iki farklı mimari tercihten** ve bizim o
tercihlerin bedelini iki yerde ödememiş olmamızdan geliyordu. Şimdi
ödendi.

---

## Doğrulama

| | Sonuç |
|---|---|
| `cargo test --workspace` | 15 suite'in **hepsi ok**, 0 FAILED, çıkış kodu 0 |
| `cargo clippy --all-targets` | 0 uyarı |
| `cargo fmt --check` | temiz |
| `svelte-check` | 146 dosya, 0 hata |
| Ölçüm: alan eşleştirme | 14 cümle önce/sonra karşılaştırıldı |
| Canlı: uygulama | açıldı, günlük temiz |

## Açık kalan

Mark LIII'ten alınacaklar listesi duruyor, sırayla:

1. **Mikrofon seçici** — hâlâ `default_input_device()` kullanıyoruz,
   Windows varsayılanı kulaklık takınca kendiliğinden değişiyor
2. **Undo** — dosya işlemleri ve ayar değişiklikleri için
3. **Anında karşılık** — uzun iş başlarken "tamam, bakıyorum"
4. **Model ısıtma** — yerel modelde ilk cevap 17 saniye fark ediyor

Detay: Obsidian'da `Mark LIII/00 - Mark LIII`.
