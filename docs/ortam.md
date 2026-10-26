# Gerçek ortam ve protokol kaynakları

9 Ekim 2026’da kodlamadan önce incelenen bilgisayar:

- Pop!_OS 24.04 LTS, COSMIC, `XDG_SESSION_TYPE=wayland`, `WAYLAND_DISPLAY=wayland-1`.
- Kullanıcı D-Bus socket’i `/run/user/1000/bus` erişilebilir. Sandbox ilk erişimi engelledi; açık izinli salt okunur inceleme ile doğrulandı.
- `cosmic-session.target` ve `graphical-session.target` aktif; kullanıcı systemd ortamında Wayland/COSMIC değişkenleri mevcut.
- Registry gerçekten `ext_data_control_manager_v1` v1, `zwlr_data_control_manager_v1` v2, `zcosmic_toplevel_info_v1` v3, `ext_foreign_toplevel_list_v1` v1 ve `zcosmic_toplevel_manager_v1` v4 sunuyor.
- Uygulama bunların **v1 uyumlu** `zwlr_data_control`, COSMIC info/manager arayüzlerini bind eder. Yüksek global sürümün v1 client’ı desteklemesi protokolün geriye uyumlu sürüm anlaşmasının parçasıdır; v2+ olarak bind edip kaldırılmış v1 title/toplevel olaylarını beklemez.
- Gerçek pencere bilgisi/etkinleştirme Wayland’dan alınır. D-Bus’ta görülen CosmicComp/Session isimlerine hayalî pencere metotları gönderilmez. İşlem listesi pencere listesi olarak kullanılmaz.
- Portal introspection’ında RemoteDesktop ve ScreenCast bulundu; **Clipboard portalı bulunmadı**. RemoteDesktop oturumuna bağlı Clipboard portalını genel arka plan pano geçmişi API’si olarak varsaymadık.
- MPRIS servisleri oturumda gözlendi; gerçek test anında bir oynatıcı, sonraki salt okunur incelemede Spotify ve KDE Connect MPRIS servis isimleri görüldü. Sayı oturum etkinliğine göre değişir.
- Rust başlangıçta yoktu; proje içine Rust 1.99.0 araç zinciri indirildi. Node 25.6.1/npm 11.9.0 ve JDK 21 sistemde mevcuttu. SDK/Gradle proje içi `.tools/` altında hazırlandı. ADB incelemesinde fiziksel Android cihazı bağlı değildi.

## Resmî kaynaklar

- [COSMIC protokolleri kaynak deposu](https://github.com/pop-os/cosmic-protocols).
- [COSMIC toplevel info XML](https://raw.githubusercontent.com/pop-os/cosmic-protocols/main/unstable/cosmic-toplevel-info-unstable-v1.xml): v1 toplevel/title/app_id/done/closed; v2’de ext-foreign-toplevel entegrasyonu.
- [COSMIC toplevel management XML](https://raw.githubusercontent.com/pop-os/cosmic-protocols/main/unstable/cosmic-toplevel-management-unstable-v1.xml): gerçek capabilities olayı ve activate(handle,seat); compositor isteği reddedebilir.
- [wl-clipboard resmî proje](https://github.com/bugaevc/wl-clipboard): gerçek ortam incelemesinde registry/MIME bilgisi ve testlerde karşılaştırma için kullanıldı.
- [MPRIS Player arayüzü](https://specifications.freedesktop.org/mpris/latest/Player_Interface.html): metadata, mikrosaniye konum/sarma, Can* ve Volume.
- [XDG Clipboard portalı](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Clipboard.html): RemoteDesktop session bağlamı; bu makinede ayrıca introspection ile mevcut olmadığı doğrulandı.
- [Expo yerel modül rehberi](https://docs.expo.dev/modules/get-started/), [SecureStore](https://docs.expo.dev/versions/latest/sdk/securestore/), [SDK 54](https://docs.expo.dev/versions/v54.0.0/).
- [Android SDK araçları](https://developer.android.com/studio), [sdkmanager](https://developer.android.com/tools/sdkmanager), [LAN izinleri](https://developer.android.com/privacy-and-security/local-network-permission).

Vendored XML’ler kaynak copyright/izin metinlerini korur. `*-compat.xml` yalnızca resmî dosyanın v1 arayüzlerini tutar; v2+ istek/olay/enum üyeleri kaldırılmıştır. Yeni veya uydurulmuş protokol metodu eklenmez. XML scanner’ın açıklamalardaki entity sorununu önlemek için açıklama metninin kaçışı sadeleştirilmiştir. İstek/event wire imzaları v1 ile aynıdır.

## Sınırlamalar

COSMIC unstable protokolleri ileride değişebilir. Desteklenmeyen compositor oturumunda capability bayrakları kapanır, mobil anlaşılır hata gösterir. Bu sürüm alternatif compositor için ext-only pencere listeleme/etkinleştirme adaptörü içermez. X11/xdo/wmctrl kullanılmaz.

İlk sürüm yalnızca UTF-8 metin panosudur; görsel/HTML/zengin MIME saklama yoktur. Parola tanıma garanti edilmez. Uygulama simgeleri sistem/kullanıcı/Flatpak desktop-entry kayıtlarından ve kurulu ikon temalarından bulunur. SVG/XPM dahil yerel biçimler GDK Pixbuf ile oranı korunarak PNG’ye dönüştürülür; mobilde toplu ve önbellekli isteklerle yüklenip kırpılmadan gösterilir. İlk 12 pencere sınırı kaldırılmıştır. Kaynak ikon bulunamazsa uygulama baş harfi gösterilir.

MPRIS cover HTTP(S) URL’leri indirilmez; yalnızca sınırlı yerel PNG/JPEG/WebP aktarımı yapılır. Sistem sesi veya uygulamanın MPRIS dışında kalan işlevleri yönetilmez. Metadatasını/Can* özelliklerini eksik veren oynatıcılar eksik bilgi veya kapalı kontrollerle görünür.

Satır tabanlı etkileşimli terminal, stdin ve sudo parola girişi desteklenir. Tam ekran TUI ve process group’tan özellikle kaçan alt süreçleri eksiksiz izleme yoktur. Dört iş/iki iş-cihaz, 128 KiB çıktı ve bir saat maksimum süre bilinçli sınırlardır. IPv6 yoktur. Mobil arka planda sürekli bağlantı tutmaz; öne gelince yeniden bağlanır.
