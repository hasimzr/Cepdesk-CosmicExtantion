# Mimari

## Bileşenler

`service/src/` modülleri:

- `system`: sabit pactl/nmcli adaptörleri, NetworkManager/logind D-Bus işlemleri ve bağlantı ömürlü PCM capture. Android AudioTrack bounded kuyrukla yerel ses çalar.
- `applications`: XDG/Flatpak kataloğu, görünür kurulu başlatıcıların Gio ile açılması, ikon teması tarama/önbellek ve GDK Pixbuf üzerinden PNG dönüştürme.
- `network`: TLS WebSocket, keşif, RPC yönlendirmesi, kimlik doğrulama, hız/bağlantı sınırları.
- `password`: kalıcı ortak parola, sertifikaya ve iki nonce’a bağlı HMAC kanıtları, cihaz bazında erişim anahtarı.
- `pairing` (eski CLI akışı): iki dakikalık tek kullanımlık kod, üç dakikalık yerel onay, tek teslimlik erişim anahtarı.
- `desktop`: ayrı thread’de gerçek Wayland istemcisi; pano ve COSMIC pencere adaptörü. RPC tarafına yalnızca komut kanalı/snapshot sunar.
- `media`: oturum D-Bus’ı üzerinden MPRIS; bir saniyelik kontrollü sorgulama.
- `commands`: süreç grupları, zaman aşımı, kapalı stdin, sınırlı stdout/stderr ve cihaz bazlı işler.
- `store`: SQLite işlemleri, geçmiş, cihazlar, ayarlar, kayıtlı komutlar ve işler.
- `config`: XDG dizinleri ve test için izole `--state-dir`.
- `protocol`: v1 şema ve yapılandırılmış hata modeli.
- `main`: servis yaşam döngüsü ve kullanıcıya özel Unix socket üzerinden CLI.

Ayrı mobil projedeki `src/client.ts` bağlantı/eşleştirme, SecureStore ve yeniden bağlanmayı yönetir. `App.tsx` gerçek RPC’lere bağlanan ekranları içerir. `modules/cepdesk-native` yerel Expo Kotlin modülüdür. Config plugin yalnızca gerekli manifest ayarlarını uygular; Expo autolinking modülü bulur. `android/` prebuild çıktısıdır ve yeniden üretilebilir.

## Güven modeli

İlk güven ortak bağlantı parolasıyla kurulur. NSD/mDNS kayıtları, IP adresleri ve cihaz adları yetki sağlamaz. İlk TLS oturumunda Kotlin geçerlilik kontrolünden geçen ilk sertifikaya kilitlenir; parola kanıtı doğrulanana kadar yalnızca `password.hello` gönderilebilir. Sunucu kanıtı, TLS üzerinde gözlenen sertifikaya, sunucu kimliğine, iki rastgele nonce'a ve cihaz adına bağlıdır. Sunucu doğrulanmadan istemci parola kanıtı gönderilmez. Sonraki bağlantılarda Kotlin TrustManager kayıtlı SHA-256 sertifika özetini kontrol eder.

Ortak parola açık metin olarak saklanmaz veya ağda gönderilmez. Rastgele salt ve 600.000 turluk PBKDF2-HMAC-SHA256 çıktısı bilgisayarda 0600 izinli dosyada saklanır. Bu türetilmiş anahtar da gizlidir; dosyayı ele geçiren biri parolalı eşleştirmeyi taklit edebilir. Bu protokol PAKE değildir: ilk eşleştirme kanıtını elde eden bir saldırgan çevrimdışı parola tahminleri yapabilir. Bu nedenle en az 12 karakterlik güçlü parola gerekir; düşük entropili ortak parola bu tehdidi ortadan kaldırmaz. Ağdaki tahminler ayrıca servis genelinde dakikada beşle sınırlıdır. Kanıtlar aynı oturumda bir kez ve 90 saniye içinde kullanılabilir.

Başarılı doğrulamada her cihaza rastgele 256 bit erişim anahtarı verilir; bilgisayarda SHA-256 özeti, Android’de SecureStore/Keystore kaydı saklanır. Telefon ortak parolayı saklamaz. Parola değişikliği mevcut cihaz anahtarlarını iptal etmez; iptal CLI üzerinden cihaz bazında yapılır. DHCP adres değişikliklerinde keşif yalnızca adres bulur, kayıtlı sertifikaya duyulan güveni değiştirmez.

Yerel CLI socket dizini `0700`, socket `0600`; ayrıca Linux peer credentials ile aynı UID kontrol edilir. Veriler ve özel anahtar `umask 0077` altında oluşturulur. Bu uygulama aynı UID ile çalışan kötü amaçlı yerel süreçten korunmayı amaçlamaz. SQLite içerikleri disk üzerinde şifrelenmez; disk şifreleme işletim sistemine aittir.

`revoke` yeni bağlantıları hemen reddeder, açık bağlantıları en geç bir saniyelik denetimde kapatır; her istek ve olayda da yetki yeniden doğrulanır. İlgili cihazın çalışan işleri iptal edilir. Serbest komut ayrı bir izin bayrağıdır. Kayıtlı komut metnini kaydetme/değiştirme/silme de bu izni gerektirir; böylece kayıtlı komut mekanizması serbest komut yasağını aşamaz. `permission off` yeni serbest işlemleri engeller; daha önce başlamış işler devam eder. Bunları sonlandırmak için iptal veya `revoke` kullanılır.

## Pano

`zwlr_data_control` arka planda selection değişikliklerini alır. Metin, zaman ve boyut sınırıyla ayrı bir aktarımda okunur; eski selection’dan geç dönen sonuçlar atılır. Metin olmayan MIME veya geçersiz UTF-8 dönüştürülmez. `x-kde-passwordManagerHint` / `application/x-keepassxc-private` işaretli teklifler kaydedilmez. Her parolanın işaretleneceği/tanınacağı iddia edilmez.

İçerik özeti UNIQUE kısıtıyla dedup edilir. Servisin kendi yazımı aynı kaydı yeniden üretmez. “Seç” sıralamayı değiştirmez; “üste taşı” başarılı pano yazımından sonra `promoted_at` zamanını değiştirir. Selection kaynağı Wayland bağlantısı açık kaldıkça içerik sunar; `paste-once` kullanımı yoktur. Başka uygulama panoyu değiştirdiğinde sahiplik normal Wayland davranışına göre devredilir.

Varsayılan: 500 kayıt, toplam **içerik** 8 MiB, tek içerik 64 KiB. Toplam sınır SQLite dosyasının fiziksel boyutuna değil kayıt payload’larının toplamına uygulanır; dosya, indeks ve WAL ek alan kullanır. Ayarlar değişince eski/büyük kayıtlar aynı işlemde budanır. Silme SQLite `secure_delete` kullanır; yedeklerin ve SSD üzerinde fiziksel izlerin silinmesi garanti edilmez.

Telefon geçmişi diske yazmaz. Liste yalnızca kısa önizleme içerir; tam metin seçilen kayıtta istenir. Bağlantı kaybında ekran snapshot’ları kaldırılır. Sunucunun olayları birden fazla cihaz için aynı veritabanını yansıtır; son başarılı pano seçimi aktif olur. İşler ve çıktılar cihaz sahibiyle sınırlandırılmıştır.

## Pencere ve medya

COSMIC v1 uyumlu protokol bind’i gerçek toplevel handle’larını kullanır; pencere kimliklerinde oturum UUID’si vardır. Açılma/kapanma/başlık/aktif durum olayları listeyi günceller. Etkinleştirme capability’si manager’ın gerçek capabilities olayından okunur. Başarı ancak hedef pencerenin aktif state’i görülünce döner; compositor reddederse iki saniye sonunda `ACTIVATION_DENIED`.

MPRIS konumu PropertiesChanged sinyali yayımlamadığından bir saniyelik sorgu kullanılır. En fazla 16 oynatıcı sorgulanır, yanıt vermeyen D-Bus sorguları zamanla sınırlandırılır. Her kontrol CanControl/CanPlay/CanPause/CanSeek/CanGo* bilgisine göre açılır. Volume oynatıcıya aittir. Kapak yalnızca oynatıcının metadata’sındaki yerel dosyadan, görüntü imzası ve 128 KiB sınırıyla base64 aktarılır; telefon Linux yolu açmaz. HTTP(S) kapakları ilk sürümde açıkça desteklenmez.

## Komut yaşam döngüsü

`shell -c command`, mutlak shell ve cwd, controlling PTY ile stdin/stdout/stderr. stdout/stderr terminalde birleşir. Terminal giriş echo özelliği kapalıdır; yanıtlar ayrıca saklanmaz. `jobs.input` yalnızca işi başlatan cihazın çalışan işine, 4096 bayta kadar girdi gönderir. En fazla dört iş, cihaz başına iki iş; toplam 128 KiB çıktı. Sınırda süreç grubu sonlandırılır ve durum `output_limit` olur. Çıktı bildirimleri yaklaşık 120 ms ile sınırlanır. İşler varsayılan olarak sınırlı bellekte tutulur; `keep_history:true` ile SQLite kaydı açılır. Son 100 kalıcı bitmiş iş korunur. Mobil listeler kısa bilgiler, ayrıntı seçilen işin gerçek çıktısını içerir. `jobs.delete` bitmiş işleri cihaz sahipliği kontrolüyle siler.

İptal/zaman aşımı `setsid` ile oluşturulan tüm normal süreç grubuna SIGKILL gönderir. Kendi session/process group’una kaçan alt süreçler bu yöntemden kaçabilir. Normal systemd kurulumunda `KillMode=control-group` servis kapanırken kalan alt süreçleri de toplar. Servis yeniden başladığında kalmış running kayıtları `interrupted` olur; iş yeniden otomatik çalıştırılmaz. Telefonun bağlantısı kopunca çalışan işler kendi zaman aşımına kadar sürer. Yeniden bağlantıda `jobs.list/get` durumu sorgular.

Pano içeriği, komut ve çıktı loglanmaz; komut geçmişinde hassas bilgiler bulunabileceği için XDG verisi özel tutulur. Kaydettiğiniz komutların/çıktıların veritabanında bulunması günlük loglamadan ayrıdır.
