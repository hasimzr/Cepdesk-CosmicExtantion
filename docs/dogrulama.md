# Doğrulama ve kullanıcı testi

## Bağımsız masaüstü deposu — 26 Ekim 2026

Servis ve eklenti artık aynı depo kökünden derlenir; mobil/web kaynaklarına ve yerel `.tools` kurulumuna gerek yoktur. `scripts/check.sh` kontrollerinde 31 servis, 6 eklenti ve 4 Python paketleme testi geçti; her iki Rust paketinde format/Clippy ve AppStream/desktop doğrulaması başarılı. Gerçek oturum gerektiren 3 servis testi ignored bırakıldı.

Geçici bağımsız kopyada shell sözdizimi, belge bağlantıları, mağaza gönderim dosyaları ve `.gitignore` doğrulandı. `.deb` paketleme testi derlemeyi sahte Cargo ile ikame eder; gerçek dpkg-deb aşamasını, servis/eklenti/lisans yerleşimini ve farklı dizinden çağrılmayı doğrular. Bu kontrol yeni bir release ikilisi üretildiği veya Flatpak kaynak derlemesinin tamamlandığı anlamına gelmez.

Tam kontrol sırasında çeviri yardımcısındaki gereksiz yaşam süresi kaldırıldı. TLS iptal testindeki kapanmış bağlantıya RPC gönderme yarışı, aktif bağlantının kapanmasını bekleyip yeni bağlantıda iptal edilmiş anahtarın reddini doğrulayacak şekilde düzeltildi; çalışma zamanı iptal davranışı değiştirilmedi.

> Mobil test sonuçları önceki ortak çalışma alanına ait tarihsel kayıtlardır. Bu masaüstü deposunun `scripts/check.sh` betiği yalnızca servis, eklenti ve paketlemeyi kontrol eder; Android veya web kaynaklarına ihtiyaç duymaz.

## Çoklu tuval ve pencere gridi — 10 Ekim 2026

TypeScript ve ESLint geçti; 24 mobil test başarılı. Yeni testler eski düzeni ilk tuvale taşıma, ayrı bölümler/boyutlar ve yeniden açılış, 121 tuval oluşturma (sabit uygulama sınırı yok), boş tuval, son tuvalin korunması, aktif silmede komşu seçimi, kenarda geçiş sınırı, bozuk kayıt reddi, farklı yüksekliklerde çakışmayan grid ve hızlı kayıtları son tam duruma birleştirmeyi kapsar. İki parmağın aynı yönde hareketi doğru yönü üretir; tek/üç parmak, pinch, dikey hareket, kısa hareket ve farklı touch ID'leri geçiş sayılmaz.

Android AtomicFile kayıt API'leri APK'ye derlendi; mevcut 12 native sertifika/parola testi geçti. AtomicFile ve gerçek iki parmak dokunma akışı cihaz üzerinde henüz denenmedi. Gerçek tablet kabul turu: birkaç tuval oluşturun, farklı bölümler ve genişlik/yükseklik seçin, kopyalayın/silin, iki parmakla ve üst oklarla geçin, tekrar açınca tasarımları/aktif tuvali kontrol edin. Medya çubuğundan başlayan iki parmak hareketinin sarma yapmamasını; kapatma × alanını; dikey kaydırmayı ve yatay/dikey yönleri kontrol edin. Telefon hoparlörü açıkken başka tuvale geçiş sesi korumalı; üst simge durdurmalı.

## Sistem, medya sarma ve ilk pano yükleme — 9 Ekim 2026

22 Rust testi geçti; gerçek COSMIC mutasyon testi ve canlı monitor testi standart turda ignored bırakıldı. Canlı monitor testi ayrıca çalıştırıldı ve geçti: sistem ses durumunu salt okunur aldı, varsayılan çıkıştan kısa ve frame-hizalı PCM okudu; ses kaydetmedi. Ayrı test D-Bus oturumunda sahte MPRIS oynatıcısı ile mutlak SetPosition, tarayıcı Seek fallback'i, parça değişikliği ve aralık reddi doğrulandı. PCM okuması frame ortasında iptal edilip sürdürülünce bayt/hiza korunması; monitorların mikrofon listesinden elenmesi; ses aralığı, escaped Wi-Fi alanları ve güç onayı önkoşulu test edildi. Gerçek TLS testi desteklenmeyen windows.close, onaysız güç isteği ve audio.stop yanıtlarını kontrol eder.

TypeScript/ESLint ve 14 mobil test geçti; v1/v2 → altı bölümlü v3 grid taşıması önceki sıra/genişlikleri korur. İlk pano sonucunun arama zamanlayıcısı tarafından silinmesi query===search korumasıyla düzeltildi. Native AudioTrack yolu APK'ye derlendi; mevcut 12 Android native sertifika/parola testi geçti. Telefon ses çıkışı başarısız olursa native hata olayı capture/oynatmayı temizler.

Gerçek tablette: ilk açılış pano verisi, × ile kaydetme sorusu olan pencere kapatma, + Komut ekle (cihaz izni açıkken), ikonlu oynatıcı ve sürükleme/tıklama ile konum değiştirme, hoparlör/mikrofon seçimi/seviyeleri, telefon hoparlörü gecikmesi ve background/disconnect temizliği denenmelidir. Wi-Fi bağlantı değiştirme ve güç işlemleri bu test turunda uygulanmadı; OS/Polkit yetkileri ve gerçek etkiler kullanıcı kabul turunda kontrol edilmelidir.

## Tasarım, uygulama başlatıcı ve ikonlar — 9 Ekim 2026

Normal çalışma alanındaki büyük başlıklar/yan menü kaldırıldı; lavanta açık/koyu palet ve daha kompakt pencere/medya/pano kontrolleri eklendi. Beşinci bölümde kurulu uygulama başlatıcı, aramalı bottom sheet ve telefonda sunucuya göre favoriler yer alır. V1 grid ayarları v2'ye sıra/genişlik kaybı olmadan taşınır.

TypeScript/ESLint ve 13 mobil test geçti. Rust format/clippy ve 16 test geçti; gerçek COSMIC oturum testi bu değişiklikte çalıştırılmadı. Uygulama katalog önceliği/Hidden/NoDisplay, main desktop group, izinsiz istemci yolu reddi, geçici test başlatıcısı ile Gio açılışı, SVG oranı korunarak PNG üretimi ve gerçek COSMIC Files ikon çözümlemesi doğrulandı. Yerel TLS integration testi authenticated uygulama liste/ikon API'lerini, batch sınırını ve geçersiz launch ID reddini kapsar.

Tablette yapılacak kabul testi: güncel APK ve güncel servisi çalıştırın; tüm uygulamalar panelinde arayın, yıldızla favori seçip modalı kapatın, favoriden gerçek bir GUI uygulaması başlatın. Uygulamayı kapatıp açın; favoriler korunmalı. Farklı bilgisayarların favorileri karışmamalı. Modalı tutamacından aşağı sürükleme, dışarı dokunma ve Android geri tuşuyla kapatmayı; yatay/dikey, klavye ve büyük yazı kullanımını kontrol edin. Gerçek cihazdaki modal/dokunmatik akışı bu ortamda doğrulanmadı.

## İlk grid sürümünün kontrolleri — 9 Ekim 2026

Pano/Pencereler/Medya/Komutlar tek ekranda birleştirildi. TypeScript ve ESLint geçti; 10 mobil test başarılı (6 yeni grid/kayıt testi). Gridin 256 genişlik kombinasyonu 6 farklı ekran genişliğinde çakışmadan ve yatay taşmadan yerleşmesi, geçersiz kayıtların varsayılana dönmesi, sürükleme sıralaması, genişlik sınırları, geciken kayıtların son değişikliği ezmemesi ve kayıt hatasından sonra tekrar kaydetme kontrol edildi. Fiziksel cihazda dokunmatik kullanım henüz doğrulanmadı.

Telefonda: dört bölümü kullanın, **Düzeni düzenle** ile başlıklardan sürükleyin ve genişlikleri değiştirin. Daha aşağıdaki bölüme ulaşmak için parmağı üst/alt kenarda tutun. Uygulamayı kapatıp açın; sıra/genişlik aynı olmalı. Yatay/dikey yönlerde, açık/koyu temada, büyük yazı ve TalkBack ile kullanım kontrol edilmelidir. Bağlantı ekranına geçip dönün; seçili pano/medya ve komut taslağı korunmalı. **Varsayılan düzen** ve kayıt hatasında **Tekrar kaydet** akışlarını kontrol edin.

## Ortak parola değişikliği — 9 Ekim 2026

Rust: 11 test başarılı, 1 COSMIC testi bu değişiklikte çalıştırılmadı. Yeni parola testleri kalıcı kayıt/0600 dosya izni, aynı parolayla çoklu cihaz, ayrı token/iptal, yanlış-süresi dolmuş-tekrar kullanılan kanıt ve ortak deneme sınırını kapsar. Gerçek yerel TLS testi artık parolalı eşleştirme → auth akışını da doğrular. Format ve clippy geçti.

Android native: 12 test başarılı (5 sertifika, 7 parola testi). Bağımsız PBKDF2-HMAC-SHA256 vektörleri ve Türkçe UTF-8 parola ile platformlar arası uyum kontrol edildi. Yanlış parola, gözlenen sertifika uyuşmazlığı, nonce/cihaz adı değişikliği ve anormal iterasyon sayısı reddedildi. Gizli terminal parola kurulumu izole dizinde denendi. TypeScript/ESLint ve 4 mobil yardımcı test geçti. Fiziksel cihaz akışı henüz denenmedi.

## Önceki kontroller

9 Ekim 2026, bu Pop!_OS COSMIC/Wayland oturumunda:

- Rust format ve `clippy --all-targets -- -D warnings` kontrolü geçti.
- Rust testleri: onaysız eşleştirme erişim vermez; kod tek kullanımlık; hatalı kod denemeleri sınırlı; iptal edilen anahtar reddedilir; gerçek TLS istemcisi güvenilmeyen sertifikayı reddeder; yeniden bağlantı çalışan işin durumunu alır.
- Pano: dedup, üste taşıma, duraklatma, boyut/kayıt budama, Türkçe arama, silme/temizleme ve yeniden başlatmada ayar/geçmiş korunması.
- Komut: stdout/stderr, çıkış kodu, canlı durum olayları, zaman aşımı, iptal, alt süreç grubu sonlandırma, 128 KiB çıktı kesme, cihaz başına iş sınırı ve revoke sonrası iptal.
- Gerçek COSMIC testi **ayrıca çalıştırıldı ve geçti**: arka plan pano takibi, aktif panoya yazma, içeriği birkaç kez sunma, kendi yazımından kopya kayıt üretmeme, sekiz gerçek pencereyi listeleme ve farklı pencereyi etkinleştirme. Önceki aktif pencereye dönüldü. MPRIS bus erişimi ve test anlarında 1–2 oynatıcı gözlendi.
- TypeScript, ESLint, dört mobil yardımcı test ve Android Metro/Hermes paketlemesi geçti.
- Android release APK yerel Gradle ile oluşturuldu; Kotlin native modülü ve Expo autolinking derlendi. JavaScript paketi APK içindedir. Son native test/build sonuçları `release/BUILD.md` içinde kaydedilir.
- ADB kontrolünde fiziksel Android cihazı bağlı değildi. APK’nin gerçek telefonda/tablette açılması, NSD keşfi, SecureStore yaşam döngüsü ve dokunmatik kullanım **bu ortamda doğrulanmadı**.

## Tekrar çalıştırma

```bash
bash scripts/check.sh
# Gerçek oturum; panoyu ve aktif pencereyi geçici değiştirir:
bash scripts/verify-session.sh
```

Oturum testi standart testlerde `ignored` durumundadır; bu makinede ayrıca çalıştırılmıştır. Metin veya boş pano bekler; görüntü panosunu değiştirmemek için o durumda durur. Metin geri yükleme transient kullanıcı systemd birimiyle test sürecinden bağımsız sunulur; başka uygulama panoyu değiştirdiğinde geçici birim kapanır. Geçici veritabanı temizlenir, içerikler loglanmaz. Boş pano önkoşulu testte düzeltildi ve test tekrar geçti.

Sandbox’ın localhost/Unix socket ve bus erişimini engellediği denemeler başarı sayılmadı; izinli gerçek çalıştırmalar yapıldı.

## Telefonda kabul testi

1. APK’yi kurun. Aynı Wi-Fi’de servis başlatın. Keşfi ve elle IP/port bağlantısını deneyin.
2. Servisin ilk açılışında ortak parola belirleyin. Telefonda yanlış parolanın reddedildiğini, doğru parolayla bağlandığını ve ikinci cihazın aynı parolayla kaydolabildiğini kontrol edin. Uygulamayı ve servisi yeniden açın; parola tekrar sorulmamalı.
3. İki metin kopyalayın. Arama, seçim, üste taşıma, tekrar seçimde dedup, silme, temizleme ve duraklatmayı deneyin. Servisi yeniden başlatıp geçmiş/ayarları kontrol edin.
4. Aynı uygulamanın iki penceresini ayrı seçin; doğru pencere öne gelmeli.
5. MPRIS oynatıcı seçimi, oynat/duraklat, önceki/sonraki, ±10 sn ve oynatıcı sesini deneyin. Yerel kapaklı oynatıcıyla kapak aktarımını deneyin.
6. CLI’den kayıtlı komut ekleyin. Serbest yetki kapalıyken engellendiğini görün; yetki verin. `printf merhaba; sleep 2; printf hata >&2; exit 7`, iptal ve kısa timeout örneklerini deneyin.
7. İş çalışırken Wi-Fi’yi kapatıp açın; iş bilgisayarda sürmeli ve yeniden bağlanınca durum/çıktı alınmalı. Arka plan/öne gelme akışını deneyin.
8. `cepdesk revoke CİHAZ_ID` açık bağlantıyı kapatmalı; eski anahtar yeniden bağlanamamalı.
9. Telefon/tablet, yatay/dikey, açık/koyu/sistem teması, boş listeler ve hata durumlarını inceleyin.

## Kalan işler ve sınırlar

- Fiziksel Android/LAN kabul turu; oynatıcı bazında canlı medya kontrolleri. Gerçek oturumda bus/keşif doğrulandı; kullanıcının mevcut müziği otomatik değiştirilmedi.
- Görsel/zengin MIME pano, HTTP(S) kapak, IPv6 ve interaktif terminal ilk sürüm kapsamı dışındadır; belgeler ve arayüz bunu belirtir.
- Mağaza/kalıcı dağıtım için kişisel keystore. Yerel APK geliştirme/test anahtarıyla imzalanır.
- Uyumlu `npm audit fix` uygulandı; **38 uyarı (15 orta, 23 yüksek)** kaldı. Expo/Metro/Jest/config transitive bağımlılıkları arasında braces/micromatch, node-forge, postcss, image-size, sprintf-js ve uuid bulunuyor. `--force` uyumsuz Expo/RN sürümleri önerebildiğinden uygulanmadı. TLS doğrulaması Kotlin/OkHttp üzerinden yapılır. Uyarılar sıfırlanmış veya kapsamlı güvenlik denetimi yapılmış gibi sunulmaz.

## 10 Ekim 2026 — Touchpad ve klavye

İki yeni tuval bölümü ve boyuta göre ölçeklenme eklendi. WSS giriş yöntemleri kimliği doğrulanmış oturuma ait uinput aygıtına yazılır; tuval/görünürlük değişimi, bağlantı kapanması ve 10 saniyelik giriş zaman aşımı tuş bırakma davranışına bağlandı. TypeScript, ESLint, 29 mobil test ve Rust release derlemesi geçti. Yeni kapsam için yalnızca eski tuval kaydının korunması ve giriş bölümü boyutlarının saklanması temel testi eklendi. Telefon touchpad/multitouch/titreşim ve COSMIC üzerinde gerçek uinput davranışı bu ortamda denenmedi (/dev/uinput yok); kullanıcı kabul testi bekleniyor.

### Çoklu dokunma düzeltmesi

Klavye ve touchpad dokunmaları Android pager'da pointer ID ile ayrı izlenir. Ctrl basılıyken C'nin inip kalkması Ctrl'ü bırakmaz; fare düğmesine basan parmak imleç/kaydırma parmaklarına dahil edilmez. Son imleç hareketi mouse-up'tan önce gönderilir. Klavye/touchpad üzerinde başlayan giriş akışı tuval kaydırmasına dönüşmez; boş alanda iki parmak geçişi tekrar etkindir. Temel native testler Ctrl+C sırası, sol düğme + touchpad sürüklemesi ve iptalde bırakma/iki parmak kaydırmasını kapsar. Fiziksel telefonda tekrar deneme beklenir.

### Touchpad hareket kuyruğu ve klavye tasarımı

İki parmak kaydırmasından kalan 1'den küçük tekerlek değerlerinin sonraki imleç hareketini sonsuza kadar bloke ettiği doğrulandı. Gönderilebilir tam tekerlek adımı yoksa imleç kuyruğu boşaltılır; kesirler sonraki kaydırma için korunur. Tek regresyon testi kaydırma sonrası hareket ve mouse-up öncesi hareket sırasını kapsar. TypeScript/ESLint ve 30 mobil test geçti. Klavye ana alan/gezinme blokları, ABC/123 sayısal düzen, Shift sembolleri, kilit ışıkları, penguen Super ikonu ve fare bölgesi ikonlarıyla yenilendi. Fiziksel cihaz testi kullanıcıda.

## Monitör ekranı

Monitör bölümünde açık kaynak seçimi, WSS üzerinden son kare aktarımı, tek pencere kaynağı, oranı korunmuş dokunma koordinatları, iki parmak kaydırma, tam ekran modalı ve yaşam döngüsünde durdurma eklendi. Gerçek COSMIC oturumunda ekran kaynağından 1280 × 720 JPEG alındı. Bu test oturumunda açık/etkin uygulama olmadığından pencere yakalaması gerçek bir uygulamayla doğrulanamadı; boş masaüstünde yanlışlıkla tüm ekranı paylaşmadan hata vermesi kontrol edildi. Ortamda /dev/uinput bulunmadığından gerçek tıklama enjekte edilmedi. Fiziksel telefon kabul testi kullanıcıda. TypeScript/ESLint, 30 mobil test ve APK derlemesi geçti; temel native test görüntü koordinatında dokunmayı ve iki parmakla kaydırmanın tıklama üretmemesini doğruladı.

## Monitör kalite, FPS ve aktif pencere düzeltmesi

Yeni istemcide Base64/React Image yenileme yerine ikili WSS JPEG ve Android Canvas bitmap çizimi kullanılır. Çözücü ve UI kuyruğu son kareyle sınırlandırılır, decode sırasında mevcut görüntü korunur. Uzun kenar 1280 → 2560, kalite 68 → 90, hedef 10 → 30 FPS; telefonun 120 ms kare sorgulaması kaldırıldı. Gerçek FPS ve çözünürlük ekranda gösterilir; hedef hız ağ/CPU ve görüntü değişimlerine bağlıdır. Aktif pencere seçiminde ilk COSMIC özellikleri beklenir, kaçırılmış extension nesneleri yeniden bağlanır ve mevcut masaüstü pencerelerinin odak bilgisi doğrulama için kullanılır.

Geçici, kendi renkli Wayland penceremizle gerçek test: tüm ekran 1920 × 1080, pencere 640 × 400 JPEG; iki kaynak da başarıyla yakalandı ve ikili paket başlığı doğrulandı. Test penceresi bağlantı kapanınca kaldırıldı. Fiziksel telefonda sürekli kararma ve gerçek FPS kabul turu bekleniyor. TypeScript/ESLint, 30 mobil test, temel Android testleri ve APK derlemesi geçti.

## Aktif uygulama takibi ve 60 FPS hedefi

Sabit pencere paylaşımı otomatik odak takibine çevrildi. Yeni kaynak açılırken eski frame/session/tamponlar kapanır; kaynak nesli eski Wayland olaylarını ve eski görüntüye ait dokunmaları ayırır. İki geçici gerçek Wayland penceresinde odak değişimi, `source_id` artışı, pencere başlığının ve JPEG içeriğinin değişmesi kontrol edildi.

Hedef aralığı 16.667 µs (60 FPS), uyarlanır kısa olay beklemeleri, yeniden kullanılan piksel/RGB tamponları ve ölçekleme gerekmeyen karelerde fazladan GDK kopyasını kaldırma eklendi. Piksel kopyası tamamlanınca sonraki yakalama JPEG kodlamadan önce gönderilir; GPU yakalama ile CPU kodlama üst üste çalışır. Aynı release testi / 1920×1080 örnekte ölçülen PC üretimi yaklaşık 17,1 → 29,9 FPS; kodlama 16–17 ms. Bu test ağ/telefon ölçümü değildir ve 60 FPS garantisi vermez. PC ve telefon FPS değerleri uygulamada ayrı görünür; JPEG maliyeti, ağ ve telefon gerçek hızı sınırlar. TypeScript/ESLint, 30 mobil test, APK ve native testler geçti.

## H.264 video yansıtma — 10 Ekim 2026

Güncel APK `monitor.start` ile H.264 ister. Masaüstü FFmpeg/libx264 kalıcı CPU kodlayıcısı, Android MediaCodec + TextureView doğrudan video gösterimi eklendi. Hedef 30 FPS, oranı korunmuş en fazla 1920 uzun kenar (16:9 ekranda 1080p), CRF 20 / 10 Mbps üst sınır. Donanım video çözücüsü önceliklidir. Telefon FPS'i artık gerçekten yüzeye çizilen video karelerinden ölçülür. Masaüstünde üç, Android'de dört kareyle sınırlı kuyruklar; taşma/sıra atlama/kaynak veya boyut değişiminde yeni IDR ile toparlanma vardır. Her IDR SPS/PPS içerir, Android'e csd-0/csd-1 olarak da verilir. Eski JPEG istemcileri desteklenir.

Doğrulama: Rust format/clippy, 25 standart Rust testi ve iki yeni protokol/argüman testi geçti (toplam 27; üç canlı oturum testi ignored). Gerçek FFmpeg ile anahtar ve tahminli kareler üretildi, kodlayıcı yeniden başlatıldı ve iki akış da tekrar çözülerek kare sayısı doğrulandı; BT.709 ayarından sonra codec testi yeniden geçti. TypeScript/ESLint, 30 mobil test, 26 Android native test (altı yeni video testi dahil) ve ARM64/x86_64 release APK derlemesi başarılı. APK imzası doğrulandı; paket içindeki H.264 isteği/etiketi ve native decoder sınıfları kontrol edildi. Release dosyaları derleme çıktılarıyla birebir eşleşir.

**APK ve masaüstü ikilisi birlikte güncellenmeli; çalışan eski servis güncel ikiliyle yeniden başlatılmalı.** FFmpeg/libx264 runtime gereklidir; bu PC'de kurulu. Kaynak/ekran değişimleri, tam ekran/küçültme, küçük yazı netliği, telefon FPS'i, dokunma ve eşzamanlı ses fiziksel cihazda henüz doğrulanmadı. Tam ekran video yüzeyi değişince sonraki anahtar kare beklenebilir (30 FPS akışta yaklaşık bir saniye). Gerçek ağ/telefon akıcılığı kullanıcı kabul testinde ölçülecek.

## 10 Ekim 2026 — Uygulama sırası ve etkileşimli komutlar

Favori/pencere sırası için uzun basma, kartın büyümesi, sürükleme hedefi ve bırakmada kalıcı kayıt eklendi. Aktif pencereye öncelik veren sunucu sıralaması kaldırıldı; mobil sıra odak değişiminde korunur, yeni pencereler sona eklenir. İkinci parmak/gesture iptali sürüklemeyi kaydetmeden sonlandırır; native iki parmakla tuval geçişi korunur.

PTY stdin/stdout/stderr ve controlling terminal üzerinden soru/parola yanıtları, telefondan satır gönderme, parola gizleme ve Ctrl+C/Ctrl+D eklendi. Çalıştırma geçmişi varsayılan kalıcı yazılmaz; tercihle SQLite kaydı açılır. Kayıtlı komut tanımları düzenlenip silinir; bitmiş işler geçmişten silinebilir veya düzenleme formuna aktarılabilir. Çıktı sınırı aşımında iş sonlandırılır, zaman aşımı/iptal korunur ve çıktı güncellemeleri yaklaşık 120 ms ile sınırlanır.

Doğrulama: TypeScript/ESLint, 31 mobil test, 29 Rust test (3 gerçek oturum testi ignored), Rust fmt/clippy, Android bundle, release APK ve 26 mevcut Android native test başarılı. Yeni testler ileri/geri sıralama, odak değişiminde sıra koruma, kapanan/yeni pencere uzlaştırma, gerçek `/dev/tty` istemleri, art arda iki yanıt, echo kapalı giriş, başka cihazın terminale erişimini reddetme, isteğe bağlı geçmiş/silme, sonsuz çıktı durdurma ve Ctrl+C/Ctrl+D'yi kapsar. APK imzası ve paketlenmiş kaynakların güncelliği doğrulandı.

Fiziksel kabul: Favoride/pencerede kısa dokunuş uygulamayı açmalı; yaklaşık 450 ms basınca kart büyümeli, başka kartın üzerine bırakınca sıra değişmeli ve yeniden açılışta korunmalı. Basılı tutma öncesinde/sonrasında ikinci parmak eklendiğinde sıralama kaydedilmemeli; iki parmakla tuval geçişi çalışmalı. Tek parmak normal liste kaydırması korunmalı. Sudo parolasını ve bir onay sorusunu telefon klavyesiyle yanıtlayın. Bu fiziksel testler ve gerçek sudo yetki yükseltme bu çalışma sırasında uygulanmadı.

## 10 Ekim 2026 — COSMIC yönetim eklentisi

Panel eklentisi ve ayrı yönetim penceresi, libcosmic sabit commit ve Cargo.lock ile derlendi. Gerçek auth oturumları için RAII bağlantı kaydı, peer/son etkinlik ve ping/yanıtsızlık temizliği eklendi. Yerel admin soketi cihaz kesme, parola belirleme/değiştirme, ayar/komut/iş yönetimi ve mevcut servis RPC işlemlerini destekler. GUI kullanıcı systemd D-Bus arayüzünden sabit servis unit'ini yönetir; host shell kaçışı yoktur.

Servis: 31 test başarılı, 3 gerçek ses/COSMIC test ignored. Canlı TLS'de çevrimiçi cihaz seçimi, disconnect sonrası oturumun silinmesi ve anahtarın korunması, tekrar auth, izin değişikliği, kayıtlı komut güncelleme/silme, parola atomik değiştirme, pano ayarı, yerel job çalıştırma/silme doğrulandı. Eklenti typecheck/clippy ve release derlemeleri geçti; son GUI düzenlemeleri release derlemesinde doğrulandı. Desktop dosyaları doğrulandı. `.deb` içeriği, dinamik kütüphaneler, kaynak arşivi ve Flatpak JSON şablonu kontrol edildi. AppStream yalnızca henüz verilmeyen gerçek homepage URL'si için uyarı verir.

Flatpak builder/sandbox turu, gerçek panele ekleyerek popup ve kullanıcı systemd düğmelerinin kabul turu henüz yapılmadı. Kamuya açık GitHub adresi ve gerçek ekran görüntüleri yayın öncesi tamamlanmalıdır. Kaynaklar veya paketler uzak depoya gönderilmedi; kullanıcı kurulumuna uygulama/servis kuran betikler çalıştırılmadı.

Ayrı yönetim penceresi gerçek COSMIC oturumunda 8 saniyelik açılış smoke testiyle başlatıldı; panic/başlangıç hatası olmadan çalıştı ve süre sınırıyla kapatıldı. Bu açılış testi panele yerleştirme, düğmelerin elle kabul turu veya Flatpak testi değildir.
