# VAVIS — Manuel Test Kılavuzu

> Otomatik testler (1000+) mantığı doğruluyor; bu liste **gerçek bir Windows
> bilgisayarda gerçekten çalışıyor mu** sorusunu yanıtlıyor. Mikrofon,
> hoparlör, pencereler ve gerçek hesaplar gerektiren her şey burada.
>
> Hazırlık: `cd ui && bun install && bun run build && cd ..` ve
> `cargo run --release`
>
> Kısayollar: `Ctrl+K` komut paleti · `Ctrl+,` ayarlar · `Ctrl+L` yeni sohbet ·
> `Ctrl+B` sohbet paneli · `Ctrl+M` ses modu · `Esc` konuşmayı kes · `F11` pencere

Her bölümün sonunda bir not satırı var; beklenmeyen bir şey olursa oraya yaz.

---

## 0. Açılış (2 dk)

- [ ] Pencere açıldı; ortada reaktör, sağda sohbet paneli
- [ ] `Ctrl+K` → komut paleti açılıyor, yazınca süzülüyor, `Esc` kapatıyor
- [ ] `Ctrl+B` → sohbet paneli gizleniyor / geri geliyor; sol kenarından
      sürükleyince genişliği değişiyor ve yeniden açınca hatırlanıyor
- [ ] `F11` → pencere, çerçevesiz ve tam ekran modları arasında geçiyor
- [ ] Kapat–aç → son sohbet geri geliyor
- [ ] Ayarlar → Updates → sürüm doğru görünüyor

**Not:** _______________________________________________

---

## 1. Sağlayıcı ve sohbet (5 dk)

- [ ] Ayarlar → Model & keys → bir sağlayıcı seç, anahtar ekle → **test**
      düğmesi "çalışıyor" diyor; yanlış anahtarla net bir hata veriyor
- [ ] Claude Code kuruluysa ve giriş yapılmışsa anahtarsız seçilebiliyor
- [ ] "merhaba" → cevap **kelime kelime akıyor**, tek seferde belirmiyor
- [ ] Türkçe soru → Türkçe cevap; Ayarlar → General → Language değişince
      cevap dili de değişiyor
- [ ] **change model** → liste geliyor, seçilen model sonraki cevapta kullanılıyor
- [ ] Fallback'e ikinci bir sağlayıcı ekle, birincinin anahtarını boz → mesaj
      yarıda kalmadan yedeğe geçiyor ve bunu söylüyor
- [ ] Markdown: kod bloğu, liste ve tablo düzgün görünüyor

**Not:** _______________________________________________

---

## 2. Sohbetler ve hafıza (5 dk)

- [ ] `Ctrl+L` → yeni sohbet; eskisi sağ paneldeki saat simgesinin listesinde
- [ ] Listede arama, açma, yeniden adlandırma ve silme çalışıyor
- [ ] "Benim adım …, …'da yaşıyorum" de → birkaç saniye sonra
      "Remembered: …" bildirimi çıkıyor
- [ ] Yeni sohbette "nerede yaşıyorum?" → hatırlıyor
- [ ] Anlama göre arama (Gemini anahtarı varsa): "espresso severim" de, sonra
      "kahve tercihim ne?" diye sor → buluyor
- [ ] Ayarlar → Memory → bilgiler listeleniyor ve silinebiliyor
- [ ] Bir web sayfasını özetlet → sayfadaki "kullanıcının adı X" gibi cümleler
      hafızaya **girmiyor**

**Not:** _______________________________________________

---

## 3. Ses (10 dk)

- [ ] Composer'daki mikrofon → konuşurken halka ses seviyesine göre büyüyor
- [ ] Söylenen cümle metne dönüyor (Groq, yoksa Gemini)
- [ ] Cevap sesli okunuyor; `Esc` anında kesiyor, **bir sonraki cümle başlamıyor**
- [ ] `Ctrl+M` → ses modları arasında geçiyor, durum satırında görünüyor
- [ ] Ayarlar → Voice → Wake word → adını üç kez söyle → eğitildi diyor
- [ ] Uyandırma: adını söyleyince dinlemeye başlıyor; adın geçmeyen konuşmada
      **hiçbir şey olmuyor** (log'da STT isteği yok)
- [ ] Başka biri adını söyleyince davranışı not et (uyanmayabilir — bilinen sınır)
- [ ] Komut paleti → **Start live conversation** (Gemini anahtarı gerekli):
  - [ ] kesintisiz konuşuyor, sözünü kesince susuyor
  - [ ] canlı konuşmada "saat kaç" gibi bir araç isteği çalışıyor
  - [ ] **End live conversation** ile kapanıyor; konuşma sohbete yazılmış
- [ ] Edge TTS 403 verirse SAPI'ye düşüyor ve bunu söylüyor

**Not:** _______________________________________________

---

## 4. Onaylar ve güvenlik (10 dk)

- [ ] "Masaüstüne deneme.txt yaz" → onay mesajı sohbet akışında çıkıyor;
      **Deny** → dosya oluşmuyor; **Allow** → oluşuyor
- [ ] **Always allow** dedikten sonra aynı araç sormuyor; bir turda üçten fazla
      yıkıcı işlemde yine soruyor
- [ ] Ayarlar → Tools → **Full authority** açılınca onay sorusu gelmiyor
- [ ] İçinde "ignore previous instructions" geçen bir sayfayı okut (ya da böyle
      adlı bir dosyayı izlenen klasöre koy), ardından dosya yazdır → full
      authority açıkken bile **soruyor**
- [ ] "Uygulama aç: powershell" → reddediyor, `run_command`'ı öneriyor
- [ ] Panoya `it’s a test` (kıvrık tırnaklı) yazdır → aynen kopyalanıyor
- [ ] Çok satırlı bir metni Not Defteri'ne yazdır → satırlar ayrı, mesaj
      uygulamalarında yarıda gönderilmiyor

**Not:** _______________________________________________

---

## 5. Bilgisayar kontrolü (10 dk)

- [ ] "Açık pencereleri listele" → doğru liste
- [ ] Not Defteri açıkken "Not Defteri'ndeki düğmeleri listele" → kontroller
      adlarıyla geliyor (`list_ui_elements`)
- [ ] "Dosya menüsüne tıkla" → ekran görüntüsü almadan adıyla tıklıyor
- [ ] "Metin alanına merhaba yaz" → `set_element_text` ile yazıyor
- [ ] Adında `[1]` gibi köşeli parantez olan bir düğme adıyla bulunuyor
- [ ] Tarayıcıda "aşağı kaydır" ve bir öğeyi "sürükle" çalışıyor
- [ ] "Ekran görüntüsü al, ne görüyorsun?" → ekranı doğru tarif ediyor
- [ ] Ses düzeyi, parlaklık (dizüstünde), medya tuşları, pano okuma çalışıyor
- [ ] "Chrome'u aç", bir PDF'i aç, `ms-settings:` aç → hepsi açılıyor;
      olmayan bir uygulama "bulunamadı" diyor

**Not:** _______________________________________________

---

## 6. Otomasyonlar (10 dk)

- [ ] "2 dakika sonra bana su içmemi hatırlat" → zamanında tetikleniyor
- [ ] İndirilenler klasörüne bağlı otomasyon ("VirusTotal ile kontrol et:
      {file}") → dosya inip büyümesi bitince **bir kez** tetikleniyor; yarım
      inen `.crdownload` dosyası tetiklemiyor
- [ ] Bir program açılınca / kapanınca tetiklenen otomasyon çalışıyor
- [ ] Vavis açılınca çalışan otomasyon (sabah özeti) açılıştan kısa süre sonra geliyor
- [ ] Komut paleti → Automations → liste, kapatma ve silme çalışıyor
- [ ] Meşgulken tetiklenen otomasyon kuyruğa alınıp cevap bitince çalışıyor

**Not:** _______________________________________________

---

## 7. Telefon — Telegram (10 dk)

- [ ] Ayarlar → Phone → kendi bot token'ını gir, eşleştirme adımlarını izle
- [ ] Eşleşen hesaptan yazılan mesaja cevap geliyor
- [ ] Başka bir hesaptan yazınca cevap **gelmiyor**
- [ ] Botu bir gruba ekle → grupta cevap **vermiyor**
- [ ] Yıkıcı bir istek → telefonda düğmeli onay geliyor, full authority açıkken bile
- [ ] "Bana telefondan söyle: …" → mesaj telefona düşüyor
- [ ] `/yeni` → yeni sohbet başlıyor

**Not:** _______________________________________________

---

## 8. Kod ekranı (10 dk)

- [ ] `Ctrl+K` → Code → bir proje klasörü aç, dosya ağacı geliyor
- [ ] Bir dosyayı aç, düzenle, `Ctrl+S` → kaydediliyor
- [ ] "Testleri çalıştır, hata varsa düzelt" → okuyor, düzenliyor, çalıştırıyor;
      her düzenleme onaydan geçiyor ve açık dosya ekranda güncelleniyor
- [ ] Claude Code ile: projenin dışındaki bir dosyayı okutmaya çalış → okuyamıyor
- [ ] Projede `.claude/settings.json` içinde bir hook varsa **çalışmıyor**

**Not:** _______________________________________________

---

## 9. Entegrasyonlar (15 dk, hesabın olanlar)

- [ ] **Web araması** — güncel bir soru → kaynaklı cevap; Ayarlar → Web search
      sırası değişince ona uyuyor
- [ ] **Görsel üretimi** — Canvas ekranında üret; galeriye düşüyor
- [ ] **Konsey** — Council ekranında bir soru → birkaç modelin cevabı yan yana
- [ ] **Spotify** — bağlan, "şunu çal", "sıraya ekle", şu an çalan paneli
- [ ] **Steam** — kütüphane, "şu an ne oynuyorum", bir oyunu başlat (soruyor)
- [ ] **Obsidian** — not ara, oku, oluştur; silinen not `.trash`'e gidiyor
- [ ] **VirusTotal** — bir dosyayı sor; dosya yüklenmiyor, yalnızca hash soruluyor
- [ ] **MCP** — Ayarlar → MCP servers → `npx` ile bir sunucu ekle → başlıyor,
      araçları listeleniyor ve kullanılabiliyor

**Not:** _______________________________________________

---

## 10. Kapanış (2 dk)

- [ ] `%APPDATA%\vavis\data\` içinde `vavis.toml`, `vavis.db`, `keys.dat`,
      `media\`, `logs\` var
- [ ] `keys.dat` bir metin düzenleyicide açılınca anahtarlar okunamıyor
- [ ] `logs\crash.log` **yok** — varsa içindeki hatayı bildir
- [ ] Günlük `vavis.log`'da anahtar görünmüyor

**Not:** _______________________________________________

---

*VAVIS · manuel test ~90 dk*
