# Obsidian kasası kontrolü

**Tarih:** 2026-09-16 · **Sürüm:** 0.7.4

---

## Kısa özet

Kasa sağlıklı: 35 not, tek kasa, bağlantılar tutarlı, ekler yerinde.
Ama iki şey çıktı: **kodda gerçek bir hata** (tablo içindeki bağlantılar
çözülmüyordu) ve **notların eskimiş olması** (0.6.3'te kalmışlar).

---

## 1. Kasanın durumu

| | |
|---|---|
| Kasa | `C:\Users\Albis\Documents\Obsidian Vault` (tek, açık) |
| Not sayısı | 35 |
| Ek (görsel) | 12 |
| Projeler | `Vavis/` ve `yt2mp/` |
| Kırık bağlantı | **yok** |
| Eksik ek | **yok** |

İki proje de aynı düzeni kullanıyor: `context/` kodun hâlini, `notes/`
senin isteklerini anlatıyor. Bu ayrım iyi çalışmış — çelişki çıktığında
hangisinin doğru olduğu belli.

### Ayar boş ama çalışıyor

`vavis.toml` içinde `[obsidian] vault = ""`. Bu bir eksiklik değil: yol
boşken Vavis, Obsidian'ın kendi `obsidian.json` dosyasından en son açılan
kasayı buluyor. Canlı doğrulandı — kasa otomatik bulunup etkinleştiriliyor.

### Dokuz not aracı da çalışıyor

Gerçek kasaya karşı denendi: listeleme (35 not, iki klasör), arama
("ses" → üç ilgili not), okuma, bağlantı çıkarma. Hepsi doğru sonuç
verdi.

---

## 2. Bulunan hata: kaçırılmış boru işareti

Tablo içinde Obsidian, takma adlı bağlantının borusunu kaçırmayı
zorunlu tutuyor — `[[Not\|görünen]]` — yoksa tablo ayrıştırıcısı onu
sütun ayracı sanıyor. **Obsidian bu biçimi kendisi yazıyor**, yani nadir
bir durum değil.

Ayrıştırıcı `|` işaretinden bölüyordu ama ters bölüyü hedefte
bırakıyordu:

```
[[00 - Start Here\|context]]  →  hedef: "00 - Start Here\"
```

Böyle bir not yok. Yani tablodaki her bağlantı, araçlara **kırık**
görünüyordu — Obsidian aynı bağlantıyı sorunsuz çözerken.

Nasıl bulundu: araçları gerçek kasaya karşı çalıştırdım. `Vavis/00 -
Vavis.md` iki kardeş notuna bir tablodan bağlanıyor ve ikisi de ters
bölüyle geri geldi.

Düzeltildi, test eklendi. Aynı çağrı şimdi `00 - Start Here` dönüyor.

---

## 3. Notlar eskimişti

`Vavis/context/` klasörü **2 Eylül**de yazılmış; o tarihten sonra iki
hafta boyunca kod değişti, notlar değişmedi. Bu klasörün işi "kodun şu
anki hâli" olduğu için, eskimesi diğer klasörden daha zararlı: yeni bir
oturum buradaki sayıya güvenip yanlış iş yapar.

Yanlış olanlar:

| Notta yazan | Gerçek |
|---|---|
| Sürüm 0.6.3 | 0.7.4 |
| 57 tool, 13 alan | 58 tool, 14 alan |
| 61 komut | 68 komut |
| `MAX_TOOLS = 12` sabit | Bütçe modelden geliyor |
| Tool adları Türkçe | İngilizce |

Düzelttim, ve "0.6.3'ten sonra değişen dört şey" diye bir bölüm ekledim:
tool adlarının İngilizceleşmesi, sabit sınırın kalkması, sesin beş
motorlu olması, güncelleme kontrolü. Ayrıca notun kolay eskidiğini
söyleyen bir uyarı koydum — çünkü yine eskiyecek.

`notes/tools/olmazsa olmaz tools.md` bir **istek** notu, gerçek notu
değil. Oradaki fikirleri değiştirmedim; yalnızca adların eskidiğini
söyleyen bir not düştüm. Senin yazdığın şeyi benim düzeltmem yanlış
olurdu.

---

## 4. Kodda kalan eski adlar

Dokuz yerde eski Türkçe tool adı geçiyor (`komut_calistir`,
`simdiki_zaman`, `dosya_oku`). Hepsi **yorum ya da test mesajı** —
modele giden hiçbir ad etkilenmiyor, çalışmaya zararı yok. Ama kodu
okuyan biri için yanıltıcı.

Bunları değiştirmedim: yorumlar tasarımın gerekçesini anlatıyor ve
bir kısmı bilerek eski adı anıyor ("eskiden şöyleydi"). Toplu bir
yeniden adlandırma, gerekçeyi anlatan cümleleri bozardı. Ayrı ve
bilinçli bir geçiş olarak yapılmalı.

---

## Doğrulama

| | Sonuç |
|---|---|
| `cargo test --workspace` | 15 suite'in **hepsi ok**, 0 FAILED, çıkış kodu 0 |
| `cargo clippy --all-targets` | 0 uyarı |
| `cargo fmt --check` | temiz |
| Canlı: kasa keşfi | 1 kasa bulundu ve etkinleşti |
| Canlı: dokuz not aracı | listeleme/arama/okuma/bağlantı — hepsi doğru |
| Canlı: düzeltilen bağlantı | `00 - Start Here\` → `00 - Start Here` |
| Kasa bağlantı taraması | 35 not, kırık bağlantı yok, eksik ek yok |

Suite sayılarını toplamak yerine her suite'in durumuna bakılıyor.

---

## Yedekler

- `scratchpad/obsidian-vault-before/` — **kasanın tamamı**, notlara
  dokunmadan önceki hâli (35 not)
- `scratchpad/vavis-data-before-update-work/` — önceki oturumdan
- `scratchpad/before-rename/crates/` — önceki oturumdan
- `scratchpad/branch-backup/all-refs.bundle` — deponun tüm ref'leri

Hepsi "sil" denene kadar duruyor.

---

## Açık kalan

- Kodda dokuz yerde eski Türkçe tool adı geçiyor (yalnızca yorumlarda).
- `context/` notları yine eskiyecek. Sürüm çıkarken bu klasörü gözden
  geçirmek, sürüm betiğinin yanına konabilecek bir alışkanlık.
