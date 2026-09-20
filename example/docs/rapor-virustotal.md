# VirusTotal entegrasyonu

Tarih: 2026-09-20

---

## 0. Önce: anahtar yenilenmeli

Anahtar sohbete yapıştırıldı, yani konuşma geçmişinde duruyor. **Yakılmış
kabul edilmeli.** VirusTotal → profil → API key → yeniden üret. Bir dakika
sürüyor.

Yeni anahtar arayüzden girilecek; kodun hiçbir yerinde anahtar yok, olmayacak.

---

## 1. En önemli karar: dosya yüklenmiyor

VirusTotal'a **dosya göndermek** o dosyayı kalıcı olarak yükler ve premium
hesaplar indirebilir. Kullanıcının makinesindeki bir dosyayı habersiz oraya
göndermek, bu projede en baştan çizilen "hiçbir şey dışarı sızmaz" kuralının
doğrudan ihlali olurdu.

Bu yüzden akış:

1. Dosyanın SHA-256'sı **yerel olarak** hesaplanır (parça parça — 4 GB'lık
   bir ISO belleğe alınmaz)
2. Yalnızca bu özet sorulur — **dosyanın kendisi makineden çıkmaz**
3. Özet bilinmiyorsa "bilinmiyor" denir; **yükleme yapılmaz**

Özet bir parmak izi: VirusTotal onu tanıyorsa dosya zaten orada demektir.
Tanımıyorsa cevap "daha önce görülmemiş" olur — bu tek başına faydalı bir
bilgi, ve "temiz" ile **karıştırılmaması** gereken bir bilgi. Hata mesajı
bunu açıkça söylüyor, ve bir test bunu koruyor.

---

## 2. Canlı API bir tasarım hatamı yakaladı

Önce şunu yazmıştım:

```rust
if self.malicious == 1 { "tek motor çoğu zaman yanlış alarmdır" }
// 2 ve üstü:
format!("{} motor zararlı dedi. Bu ciddi.", self.malicious)
```

Sonra gerçek servise sordum:

```
google.com → 2 motor zararlı dedi (89 motordan)
```

Yani kodum **google.com'a "bu ciddi" diyordu.**

Bu sadece yanlış değil, zararlı: kullanıcı Google'a malware dendiğini
görürse uyarıyı tamamen yok saymayı öğrenir — ve gerçek uyarı geldiğinde de
okumaz.

**Düzeltme:** eşik sabit sayı değil, **oran**. Yüzde onun altı "birkaç motor,
çoğu zaman yanlış alarm"; üstü "ciddi". Ölçülen gerçek şu: büyük ve tanınmış
şeyler neredeyse her zaman birkaç tanınmayan motordan işaret alır, gerçek
zararlı yazılım onlarca motor tarafından yakalanır. Aradaki fark oran.

Aynı 2 motor, 15 motorluk küçük bir havuzda hâlâ "ciddi" diyor — ikisi de
test edildi.

> Bu, ölçmeden yazsaydım fark edilmeyecek bir hataydı. Birim testlerim
> hepsi yeşildi; yanlış olan testlerin doğruladığı varsayımdı.

---

## 3. Ne eklendi

**Üç tool** (hepsi okuma, hiçbiri onay kapısına takılmıyor):

| Tool | Ne yapıyor |
|---|---|
| `virus_tara_dosya` | Dosyayı yerel hash'leyip sorar — dosya gitmez |
| `virus_tara_hash` | Bilinen bir MD5/SHA-1/SHA-256'yı sorar |
| `virus_tara_adres` | Adres veya alan adı sorar |

Yeni bir tool alanı (`Domain::Security`) açıldı: alan başına tool bütçesi
olduğu için bunları mevcut bir alana tıkıştırmak o alandaki tool'ları
sınırın dışına iterdi.

**Sınır istemci tarafında da tutuluyor.** Ücretsiz katman dakikada 4 istek
veriyor. Sunucu zaten 429 döndürürdü ama kullanıcı "bir şeyler ters gitti"
görürdü; burada sayınca ne olduğunu söyleyebiliyoruz.

**Anahtar kaydedilirken doğrulanıyor.** Bir harfi eksik yapıştırılmış anahtar
doğru olanla birebir aynı görünür — ta ki ilk tarama başarısız olana kadar,
ve o noktada kullanıcı başka bir yerde olduğu için ikisini bağlayamaz. Kayıt
anında gerçek bir sorgu atılıp sonuç gösteriliyor.

---

## 4. Zincirleme neden yapılmadı

Fikir dosyasında "farklı hesaplardan birden fazla api alıp free tierleri
zincirlemek" yazıyordu. Yapılmadı, sebebi teknik değil pratik:

- Çoklu hesap free tier zincirlemek servislerin kullanım şartlarını ihlal
  eder
- Tespit edilince **hepsi birden** kapanır — yani tam ihtiyacın olduğu anda
  hiçbiri çalışmaz
- VirusTotal ücretsiz katmanı zaten **kişi başı** cömert: dakikada 4, günde
  500 istek

Normal kullanımda günde 500 tarama yapmak çok zor. Kullanıcı kendi anahtarını
giriyor, o anahtar kendi kotasını kullanıyor, kimse kimsenin kotasını
yemiyor. İlk 100 kullanıcıda da patlamaz, çünkü ortak bir havuz yok.

---

## 5. Doğrulama

**Canlı API'ye karşı** (4 test, `--ignored` ile çalışır, CI'da anahtar yok):

```
a_known_hash_comes_back_with_an_analysis   ok  (60 motor, temiz)
a_well_known_domain_is_reported_clean      ok  (89 motordan 2 — oran düşük)
a_url_id_is_accepted_by_the_service        ok  (90 motor)
a_bad_key_is_reported_as_a_bad_key         ok  (reddedildi, "bilinmiyor" değil)
```

Son test önemli: bozuk anahtar "bilinmiyor" dese kullanıcı yanlış sorunu
kovalardı.

**Arayüz gerçek tarayıcıda sürüldü:** satır görünüyor, anahtar ekleniyor,
satır "saved"e dönüyor, bildirim servisin kendi cevabını gösteriyor.

**Testler gerçekten yakalıyor mu — denendi:**

| Bozulan | Kırmızıya dönen |
|---|---|
| Kayıt anahtarları (`registry.ts`) | 4 arama testi |

```
cargo test --workspace     900 test, hepsi geçti
cargo clippy --workspace   temiz
cargo fmt --check          temiz
npx tsc --noEmit           temiz
npx vitest run             149 test, hepsi geçti
```

---

## 6. Fikir dosyası

Tamamlananlar silindi: API'leri tek panele toplama ve kod/sohbet için ayrı
model. Kalanlar: watch modu, VS Code eklentileri, kod harness'i.

VirusTotal maddesi de artık kapandı — zincirleme kısmı hariç, ve o kısım
bilerek yapılmadı (bkz. bölüm 4).
