# Kurulum ve kullanım

Bu belgedeki masaüstü komutları bu deponun kökünden çalıştırılır. Android uygulaması ayrı bir projedir; masaüstünü derlemek için kaynakları gerekmez.

## Masaüstü

Pop!_OS COSMIC/Wayland kullanıcı oturumunda çalıştırın. Servis normal kullanımda root istemez. Derleme için Rust, C derleyicisi ve pkg-config; oturumda D-Bus ve Wayland gerekir. SQLite Rust derlemesine dahil edilir. `wl-copy/wl-paste` yalnızca gerçek oturum doğrulama testi için gerekir; çalışan servis bunlara bağımlı değildir.

Sistemdeki Rust araç zincirini kullanın. `cargo --version` ve `rustc --version` çalışmalıdır; derleme bağımlılıkları README'de listelenir. `.tools/` temiz klonda gerekli değildir. Yerel araçlarınızı bu dizinde tutuyorsanız betikler bunları otomatik algılar.

```bash
cargo build --locked --release --manifest-path service/Cargo.toml
./service/target/release/cepdesk serve
```

Özel dinleme adresi/port: `cepdesk serve --bind 192.168.1.20 --port 43821`. Varsayılan IPv4 `0.0.0.0:43821`; özel/loopback/link-local kaynaklar kabul edilir. WAN port yönlendirmesi gerekmiyor. Aynı kullanıcı için ikinci servis reddedilir. İzole geliştirme: `cepdesk --state-dir /mutlak/test/yolu serve --bind 127.0.0.1 --port 43822`; CLI’de de aynı `--state-dir` kullanılmalıdır.

## Otomatik başlatma

Elle çalıştırdığınız servisi durdurduktan sonra:

```bash
bash scripts/install-desktop.sh
systemctl --user status cepdesk.service
journalctl --user -u cepdesk.service
```

Betik `~/.local/bin/cepdesk` ve `~/.config/systemd/user/cepdesk.service` kurar; gerekli oturum değişkenlerini kullanıcı systemd yöneticisine aktarır. `graphical-session.target` ile başlatır/durdurur. Bu bilgisayarda hem `cosmic-session.target` hem `graphical-session.target` aktif olarak doğrulandı. Kullanıcı lingering’i gerekmez. Servisi sistem/root systemd servisine taşımayın.

Pano/pencere capability’si kapalıysa `cepdesk status` ve `scripts/inspect-session.sh` ile durumu inceleyin. Terminal oturumunda `WAYLAND_DISPLAY`, `XDG_RUNTIME_DIR`, `DBUS_SESSION_BUS_ADDRESS` doğru olmalıdır. Oturum değişkenleri değiştiyse:

```bash
systemctl --user import-environment WAYLAND_DISPLAY XDG_CURRENT_DESKTOP XDG_SESSION_TYPE
systemctl --user restart cepdesk.service
```

`zwlr_data_control_manager_v1` bulunmayan eski COSMIC sürümlerinde ilgili sürümün veri denetimi ayarını/resmî belgelerini kontrol edin; compositor başlatılırken verilmesi gereken ayarı sonradan yalnızca CepDesk’e vermek yeterli değildir. Bu makinede protokol zaten mevcut; sistem ayarları değiştirilmedi.

## İlk bağlantı ve ortak parola

Terminalde `./release/cepdesk serve` çalıştırın. İlk kurulumda parola iki kez sorulur ve giriş gizlenir. En az 12 karakterlik bir parola belirleyin. Sonraki açılışlarda kayıtlı parola kullanılır. systemd ile ilk kurulumda terminalde `./release/cepdesk password` çalıştırabilir veya COSMIC CepDesk yönetim penceresinden ilk parolayı belirleyebilirsiniz. Arka plandaki servis parola bulunmadan yerel yönetim için açılır; telefonların parola eşleştirmesini yapılandırma tamamlanana kadar reddeder.

Android uygulamasında Bağlantı ekranı bilgisayarları otomatik arar. Bilgisayarı seçip aynı parolayı girin ve **Bağlan ve hatırla** düğmesine basın. Her yeni telefon/tablet aynı parolayla bir kez bağlanabilir; ayrıca bilgisayar onayı gerekmez. Keşif başarısız olursa IP/port elle girilebilir.

Cihaz başına farklı erişim anahtarı oluşturulur. Telefon ortak parolayı saklamaz; anahtarı ve doğrulanan sertifikayı SecureStore'a kaydeder. Servis ve uygulama yeniden açıldığında parola tekrar istenmez. Kayıtlı cihazların erişimi `cepdesk revoke CİHAZ_ID` ile ayrı ayrı iptal edilebilir.

Parola değişikliği: servisi durdurun, `./release/cepdesk password` çalıştırın ve yeniden başlatın. Bu işlem mevcut cihaz anahtarlarını iptal etmez; gerekirse ayrıca iptal edin. Eski CLI kodlu eşleştirme komutları geriye uyumluluk için korunur; güncel mobil ekran ortak parola kullanır.

## Android uygulaması

Güncel Android APK ve ürün açıklaması: [cepdesk.hasimozer.com](https://cepdesk.hasimozer.com). APK'yi telefona kurun; Metro veya Expo hesabı gerekmez. Android kaynak kodu bu masaüstü deposuna dahil değildir. Mobil geliştirme/Gradle komutları ayrı mobil projenin kökünde çalıştırılmalıdır. Masaüstü ve mobil sürümleri birlikte güncel tutun.

## Çalışma alanı ve uygulama başlatıcı

Güncel APK'yi kurup masaüstü servisini güncel `release/cepdesk` ile yeniden başlatın. Yeni uygulama alanındaki **Tüm uygulamalar** alttan açılan aramalı paneli gösterir. İkona dokunmak uygulamayı bilgisayarda başlatır, yıldız favoriye ekler/çıkarır. Favoriler bu cihazın SecureStore alanında bilgisayar kimliğine göre `cepdesk.favorites.SUNUCU_KİMLİĞİ` anahtarıyla saklanır. Kaldırılan uygulamaların favori kimlikleri korunur; tekrar kurulunca yeniden görünür. Bağlantıyı unutmak favorileri veya grid düzenini silmez.

Servis sistem/kullanıcı XDG applications dizinlerini ve Flatpak export dizinlerini tarar. Gizli/NoDisplay ve mevcut masaüstü için görünmez yardımcı başlatıcılar elenir. Kayıtlar 30 saniyede bir yenilenir. İstemci yalnızca kayıtlı uygulama ID'sini gönderir; sunucu tarafından bulunan `.desktop` dosyası `gio launch` ile başlatılır. GUI uygulaması başlatmak serbest shell komutu yetkisini gerektirmez. Başlatıcı bağımlılıkları: `gio`, GLib/GObject ve `libgdk-pixbuf-2.0.so.0`; SVG desteği için sistemin SVG loader'ı gerekir. Bu Pop!_OS kurulumunda mevcuttur.

Başlıklar normal çalışma alanında gizlidir. Üstte **Düzenle** ile tutamaçları açıp bölümleri sürükleyin ve **↔ Genişlik** tutamacıyla/yüzde düğmeleriyle genişlik değiştirin. Ekran kenarında tutmak otomatik kaydırır. Her bölüm kendi içinde kaydırılır. **Sıfırla** başlangıç düzenine, **Tamam** kullanıma döner. Bağlantı ve renk modu üstteki durum/dişli düğmesinden açılır.

Önceki tek grid sürümü `cepdesk.dashboard.v1` SecureStore anahtarında v1/v2/v3 olarak saklanıyordu; güncel APK bu kaydı ilk tuvale taşır. Yeni tuval kayıtları aşağıda belirtilen özel uygulama dosyasına yazılır. Dar ekranda minimum %50, geniş ekranda %25 kullanılır; dönünce kayıtlı tercihler korunur. Kayıt hatasında tekrar kaydetme desteği bulunur. Yeni lavanta/mor palet açık/koyu/sistem modlarıyla çalışır.

İkonlar PNG/SVG/XPM gibi kurulu yerel biçimlerden 128 px PNG'ye çevrilir; büyük görüntüler yanıt sınırı için 64 px'e düşürülür. Mobil ikonlar `contain` ile tam görünür. Kaynak ikon yoksa ilk harf gösterilir.

## Tuval tasarlama ve geçiş

Güncel APK ile **Ayarlar → Tuvaller** bölümüne girin. **+ Tuval oluştur** ile boş ekran açın, adını yazın ve bölüm anahtarlarından gösterilecek içerikleri seçin. Bölümün sırasını ↑/↓ düğmeleriyle; genişliğini yüzde seçimleriyle; yüksekliğini sayısal alan veya ± düğmeleriyle değiştirin. Yükseklik 160–1600 mantıksal ekran birimidir. Yeni tuval, kopyalama ve silme desteği vardır; son tuval silinmez. Uygulama tuval adedi için sabit sınır koymaz; cihaz depolama/bellek kapasitesi geçerlidir.

**Bu tuvali aç** ile tasarlanan ekrana gidin. İki parmakla birlikte sola kaydırmak sonraki, sağa kaydırmak önceki tuvale geçer. Yakınlaştırma, dikey hareket, kısa hareket ve tek parmak kaydırma geçiş sayılmaz. İki parmak hareketi oynatıcı çubuklarından başlarsa çubuk hareketi iptal edilir. Düzenleme modunda tuval geçişi devre dışıdır. Üstteki önceki/sonraki okları alternatif erişim sağlar. İlk/son tuvalde geçiş durur.

Tuvaller ve son aktif ID `filesDir/cepdesk-workspace.json` dosyasında saklanır. Android AtomicFile başarısız/yarım dosya yazımlarında önceki kayıtla güvenli değişim sağlar. Kayıt kuyruğu çok hızlı değişiklikleri birleştirir; sadece gereken son tam kayıt yazılır. Bozuk veya bilinmeyen şemadaki kayıt sessizce sıfırlanmaz; düzenleme kapatılır ve tekrar okuma sunulur. Eski `cepdesk.dashboard.v1` SecureStore v1/v2/v3 kayıtları ilk Ana tuvale taşınır; bağlantı/favori anahtarlarına dokunulmaz.

Sadece aktif tuvalin bölümleri çalışır; kapalı tuvallere gereksiz RPC/poll yapılmaz. Telefon hoparlörü durumu bağlantı istemcisinde tutulur ve tuval geçişlerinde devam eder. Üstteki hoparlör simgesinden durdurulabilir; background/disconnect davranışı korunur. Pencere kartları konteyner genişliğine göre grid sütunlarına yerleşir; ikonlar tam görünür ve kapatma düğmesi ikon alanıyla çakışmaz.

## Sistem bölümü ve telefon hoparlörü

Güncel servis ve APK birlikte gereklidir. Linux tarafında `pactl`/`parec` (PulseAudio araçları; PipeWire-Pulse ile de çalışır), `nmcli`/NetworkManager ve logind kullanılır. Bu bilgisayarda ses cihazları ve varsayılan çıkıştan kısa PCM örneği salt okunur olarak doğrulandı.

Hoparlör ve mikrofon sesini çubuktan ayarlayın; sessiz düğmesini ve cihaz seçicileri kullanın. Varsayılan çıkış/giriş değişirken mevcut uygulama ses akışları da taşınır. Telefon sesi kaydedicisi mikrofon cihazı değişikliğinde korunur; ses çıkışı değişirse aktarım yeni çıkış için yeniden başlatılır.

**Telefonu hoparlör olarak kullan** bilgisayarın varsayılan çıkış monitörünü stereo 24 kHz PCM16 olarak mevcut sertifikası doğrulanan WSS üzerinden Android AudioTrack'e aktarır. JSON/JS katmanına ses paketleri taşınmaz. Bounded oynatma kuyruğu eski paketlerin gecikme biriktirmesini sınırlar. Ağ/telefon tamponlarına bağlı gecikme vardır; gerçek tablette henüz ölçülmedi. Uygulama arka plana geçince veya bağlantı/yetki kaybolunca kaydedici ve oynatma kapanır. Ses çıkışı hatasında aktarım temizlenir ve yeniden başlatılabilir.

Wi-Fi seçicinin üstünde kayıtlı profiller, altında görünen ağlar bulunur. Yeni kişisel WPA/WPA2/WPA3 ağında parola girilebilir; parola telefonda kalıcı saklanmaz ve komut satırı argümanına yazılmaz. NetworkManager'a D-Bus ayarlarıyla iletilir. Kurumsal ağ için bilgisayarda kayıtlı profili kullanın. NetworkManager/Polkit oturum izni gerekebilir. Ağ değişikliği bağlantıyı kesebileceği için telefon onayı gösterilir; iki cihazı aynı erişilebilir ağa bağlayın.

Kapatma, yeniden başlatma ve uyku logind üzerinden ve açık telefon onayıyla istenir. OS politikası izin vermeyen düğmeler kapalıdır; uygulama sudo/root ya da yetki politikasını otomatik değiştirmez. Bu işlemler test sırasında uygulanmadı.

**+ Komut ekle** düğmesi kayıt formunu açar. Komut kaydetme mevcut `free_commands` iznini gerektirir; bilgisayarda bir kez `cepdesk permission CİHAZ_ID on` verin. Cihaz kimliği ekranda yazılıdır. Kayıt sonrası liste hemen yenilenir. Pencere × düğmesi zorla süreç sonlandırmak yerine COSMIC kapatma isteği gönderir; kaydetme sorusu bilgisayarda açılabilir.

## Kayıtlı cihazları yönetme

`cepdesk devices`, `cepdesk permission CİHAZ_ID on/off`, `cepdesk revoke CİHAZ_ID` bilgisayardan yönetilir. Her telefon/tablet ortak parolayı doğruladıktan sonra ayrı anahtar alır. Pano/pencere/medya ortak, komut işleri cihaz bazlıdır. Serbest komut varsayılan kapalıdır. Telefonda “eşleşmeyi unut” yalnızca telefon anahtarını siler; bilgisayar kaydını ayrıca iptal edin.

DHCP adresi değişirse uygulama kayıtlı bilgisayarı yerel ağda yeniden bulmayı dener; sertifika kimliği korunur. Keşif kapalıysa **Adresi güncelle ve bağlan** kullanın. Sertifika uyuşmazlığında otomatik yeniden bağlantı durur. Bilgisayarın XDG anahtarları kaybolduysa eski cihaz kayıtlarını kaldırıp yeniden eşleştirin.

## LAN ve firewall

Telefon ve bilgisayar aynı erişilebilir ağda olmalı; misafir ağı/AP client isolation bağlantıyı engelleyebilir. Manuel IP, multicast keşif kısıtını aşar; cihaz izolasyonunu aşamaz. VPN rotası ve yanlış ağ arayüzü de kontrol edilmelidir.

UFW kullanıyorsanız kendi LAN subnet’inizi kullanarak yöneticinizle gereken kuralları açın:

```bash
# Örnek subnet; kendi ağınıza uyarlayın.
sudo ufw allow from 192.168.1.0/24 to any port 43821 proto tcp
sudo ufw allow from 192.168.1.0/24 to any port 5353 proto udp
```

5353 yalnızca mDNS keşfi içindir. Port/subnet kuralları otomatik değiştirilmez. Android v1 target SDK 36’dır; INTERNET izni LAN erişimini sağlar. SDK 37’ye geçildiğinde yeni yerel ağ izni ve runtime akışı eklenmelidir. [Android resmî yerel ağ izin rehberi](https://developer.android.com/privacy-and-security/local-network-permission).

## Veri konumları ve kaldırma

- Config/sertifika: `$XDG_CONFIG_HOME/cepdesk` veya `~/.config/cepdesk`.
- SQLite/pano/iş geçmişi: `$XDG_DATA_HOME/cepdesk` veya `~/.local/share/cepdesk`.
- State dizini: `$XDG_STATE_HOME/cepdesk` veya `~/.local/state/cepdesk`.
- CLI socket: `$XDG_RUNTIME_DIR/cepdesk/admin.sock`.

```bash
bash scripts/uninstall-desktop.sh
```

Varsayılan kaldırma geçmişi/anahtarları korur. Tüm eşleşmeleri ve geçmişi de **kalıcı olarak** silmek isterseniz, servis kapalıyken kendi XDG yollarınızı doğrulayarak:

```bash
rm -rf -- "${XDG_CONFIG_HOME:-$HOME/.config}/cepdesk" \
  "${XDG_DATA_HOME:-$HOME/.local/share}/cepdesk" \
  "${XDG_STATE_HOME:-$HOME/.local/state}/cepdesk"
```

Android uygulamasını sistemden kaldırın; bilgisayarda cihazı `revoke` edin. Yeniden yüklemede yeni eşleştirme yapın.

## Touchpad ve özel klavye

Güncel APK ve masaüstü servisi birlikte gereklidir. Wayland/COSMIC girişleri Linux `uinput` sanal aygıtıyla iletilir. Masaüstü hesabınıza bir kez izin verin:

```bash
sudo ./scripts/setup-input.sh
```

Betik uinput modülünü yükler ve yalnızca betiği sudo ile çalıştıran hesabın `/dev/uinput` aygıtını açabilmesi için kalıcı udev kuralı yazar. CepDesk root olarak çalıştırılmaz. Sonra servisi yeniden başlatın ve telefondan yeniden bağlanın. İzin yoksa ekran açıklama gösterir.

**Düzenle → + Ekran ekle → Touchpad / Klavye** ile istediğiniz tuvale ekleyin. Köşeden yüksekliği/genişliği ayarlamak touchpad alanını ve klavye tuşlarını ölçekler; küçük bilgisayar klavyesi için tam genişlik ve yatay telefon konumu daha rahat olur.

Touchpad: bir parmak imleci hareket ettirir; kısa dokunma sol, iki parmakla kısa dokunma sağ tık gönderir. İki parmak dikey/yatay kaydırır. Sol/orta/sağ düğmeleri basılı tutulabilir; sol düğmeye bir parmakla basarken diğer parmağı touchpad üzerinde hareket ettirmek sürükler. Tıklamalarda kısa titreşim vardır.

Klavye Android klavyesini açmaz. Esc, F1–F12, Caps Lock, sol/sağ Shift/Ctrl/Super, Alt/AltGr, Menu, gezinme tuşları, sayısal tuşlar, Print Screen/Scroll Lock/Pause ve ses tuşları bulunur. Tuşları birlikte basılı tutarak kombinasyon yapın; tekrar davranışı bilgisayarın ayarlarını kullanır. Etiketler US QWERTY'dir; çıkan karakterleri bilgisayarın etkin klavye düzeni belirler (US düzeninde Shift+4 → `$`). Caps/Num göstergeleri yerel dokunuşları takip eder; bilgisayarda değiştirilen kilit durumu telefona senkronize edilmez. Geniş panellerde ana klavyenin yanında gezinme bloğu görünür; **123** gerçek sayısal bloğa geçer, **ABC** harfler/gezinme düzenini geri getirir. Dar panellerde sayısal mod daha büyük tuşlarla ayrı görünür. Super tuşları penguen ikonuyla, fare düğmeleri tıklanan bölümü vurgulayan fare ikonlarıyla gösterilir.

Klavye/touchpad alanı üzerinde başlayan dokunmalar giriş kontrolüne ayrılır. Bu alanların dışındaki boş tuvalde iki parmakla sayfa geçişi çalışır; üst oklar da kullanılabilir. Tuval değişiminde/arka plana geçişte basılı tuşlar bırakılır. Bağlantı sonlanınca sanal aygıt kaldırılır; 10 saniye giriş/heartbeat gelmezse basılı tuşlar ayrıca bırakılır. Fiziksel telefon davranışını kullanıcı deneyecektir.

## Monitör ekranı

Güncel `release/cepdesk` servisini yeniden başlatın ve güncel siteden indirilen Android APK kurun. **Düzenle → + Ekran ekle → Monitör** ile bir tuvale yerleştirin; boyutları diğer ekranlar gibi ayarlanır. İlk görünümde iki kaynak düğmesi vardır, otomatik yayın başlamaz.

- **Tüm ekranı yansıt:** odaktaki uygulamanın bulunduğu fiziksel monitörü, aktif uygulama yoksa ilk monitörü yakalar. Masaüstü, paneller, diğer uygulamalar ve işaretçi görüntüde bulunur.
- **Aktif uygulamayı yansıt:** seçim anında etkin olan toplevel pencereyi yakalar. Aktif uygulama modu odak değişimlerini izler ve aynı yayın içinde yeni etkin pencereyi yakalamaya geçer. Önceki kaynak ve tamponlar kapatılır, yeni kaynak hazırlanırken son görüntü korunur. Kaynak değiştirilirken eski görüntüye ait dokunmalar yeni uygulamaya gönderilmez. Etkin pencere kapanırsa başka etkin pencereye geçiş için kısa süre beklenir; etkin uygulama kalmazsa kaynak seçimi yeniden açılır. İlk pencere özelliklerinin gelmesi beklenir; gerekirse mevcut pencere listesinin odak bilgisiyle doğru kaynak doğrulanır. Etkin pencere yoksa açıklayıcı hata gösterilir.

Görüntü oranı korunur. Siyah kenarlar tıklanabilir görüntüye dahil değildir; koordinatlar gerçek görüntü sınırları üzerinden hesaplanır. Bir parmakla kısa dokunma doğrudan sol tıktır; ayrı sol/sağ/orta düğmeleri bulunmaz. İki parmakla dikey/yatay kaydırma tekerlek olayına dönüştürülür. **Tam ekran / Küçült** ve Android geri tuşu desteklenir. Dokunmatik giriş uinput izni gerektirir; izin yoksa görüntü izlenebilir. İlk sürümde mutlak dokunma tek fiziksel monitörle sınırlandırılmıştır; birden çok monitörde yanlış ekrana tıklamayı önlemek için kontrol kapalıdır. Masaüstünde görünür sınırların dışına taşan pencere noktalarına tıklama gönderilmez.

Görüntü ext-image-copy-capture + ext-image-capture-source ile paylaşımlı tamponlardan okunur. Güncel APK `codec:"h264"` video akışı ister; 30 kare/sn hedefi ve en fazla 1920 piksel uzun kenar (16:9 ekranda 1080p) kullanılır. FFmpeg/libx264 CPU kodlayıcısı kalite odaklı CRF 20 ve 10 Mbps üst bitrate sınırıyla çalışır. Android MediaCodec donanım çözücüsünü öncelikli seçip TextureView yüzeyine doğrudan görüntü verir. PC üretim FPS'i ve telefonda ekrana çizilen kare FPS'i ayrı gösterilir; gerçek hız CPU, telefon ve ağ hızına bağlıdır. Kuyruklar sınırlıdır; kare atlama olduğunda H.264 bağımlılıklarını bozmamak için yeni anahtar kareyle toparlanır. Tam ekran yüzey değişiminde sonraki anahtar kare beklenebilir (30 FPS'te yaklaşık bir saniye). Görüntü mevcut sertifikası doğrulanan WSS içinde taşınır, ayrı port veya harici sunucu açılmaz; telefon hoparlörü aktarımıyla aynı bağlantıda kullanılabilir. Eski istemcilerin JPEG yolu korunur. Ayrıntılı paket biçimi ve codec ayarları [protokol belgesinde](protokol.md#monitör) bulunur.


**Durdur**, tuval değişimi, arka plana geçiş ve bağlantı kesilmesi yayını kapatır. İstemci 15 saniye yayın isteği göndermiyorsa servis de yakalamayı bırakır. Aynı bağlantıda yeni Monitör yayını önceki kaynağı kapatır; eski ekranın gecikmiş durdurma isteği yeni yayını kapatmaz. Pencere yeniden boyutlanırken tamponlar yenilenir; art arda başarısız yakalama olursa kaynak tekrar seçilir.

## H.264 yansıtma bağımlılığı

Yeni video aktarımı için masaüstünde `ffmpeg` (libx264 desteğiyle) kurulu olmalıdır. Debian/Ubuntu/Pop!_OS üzerinde `sudo apt install ffmpeg`; kontrol: `ffmpeg -hide_banner -encoders` çıktısında `libx264` bulunmalıdır. Yeni `release/cepdesk` ve siteden indirilen Android APK birlikte kullanılır. Sistem servisi kurulmuşsa mevcut servis ikilisini güncel derlemeyle değiştirip servisi yeniden başlatın. Monitörde H.264 etiketi görünmelidir. Güncel sürüm 30 FPS / 1920 uzun kenar hedefler; donanım ve bağlantı hızına göre gerçek FPS düşebilir.
