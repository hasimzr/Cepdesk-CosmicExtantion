# CepDesk — COSMIC masaüstü eklentisi

Android telefon veya tabletinden COSMIC masaüstünü yerel ağ üzerinden yönet. Bu depo panel eklentisini, yönetim penceresini ve gerekli masaüstü servisini birlikte içerir.

[Web sitesi ve Android APK](https://cepdesk.hasimozer.com) · [Kurulum](docs/kurulum.md) · [Mağaza başvurusu](docs/cosmic-magaza.md) · [MIT lisansı](LICENSE)

## Kullanıcı kurulumu

Siteden veya bu deponun GitHub Releases bölümünden masaüstü `.deb` paketini indirin. Pop!_OS 24.04 / COSMIC üzerinde indirdiğiniz dizinde:

```sh
sudo apt install ./cepdesk_0.1.0_amd64.deb
cepdesk-applet --settings
```

Yönetim penceresinden ilk bağlantı parolasını belirleyin. Android APK'yi telefona kurun; aynı yerel ağda bilgisayarı seçerek eşleştirin. Panel eklentisini COSMIC Ayarları → Masaüstü → Panel üzerinden ekleyin. COSMIC Store yayını henüz tamamlanmamıştır.

## Depo düzeni

- `src/`, `resources/`: COSMIC eklentisi, yönetim penceresi ve mağaza metadatası.
- `service/`: telefon bağlantılarını ve masaüstü işlemlerini yürüten Rust servisi; gerekli bir bileşendir.
- `packaging/`, `scripts/`, `tests/`, `docs/`: paketleme, derleme, test ve belgeler.
- `release/`: yerel derleme çıktıları; Git'e eklenmez, dağıtım için GitHub Releases kullanılır.

Android ve web kaynakları bu depoya dahil değildir ve masaüstü derlemesi için gerekmez. `Cargo.lock` ve `service/Cargo.lock` kaynak depoda tutulmalıdır. Aşağıdaki yollar, klasörü hangi isimle klonladığınızdan bağımsızdır.

## Masaüstü bileşenleri

Yerel bilgisayar servisi için libcosmic tabanlı panel eklentisi ve ayrı yönetim penceresi. `cepdesk-applet` panelde, `cepdesk-applet --settings` normal pencerede çalışır. Panel eklentisi COSMIC'in eklenti listesine `X-CosmicApplet=true` ile katılır. Kaynaklar sabit libcosmic commit'i ve Cargo.lock ile kilitlenir; Rust 1.99 üzerinde derlenmiştir.

Servis bu projenin `service/` alt bileşenidir; ayrı bir ana proje değildir. `src/` eklenti ve yönetim arayüzünü, `service/src/` telefon bağlantılarını ve masaüstü işlemlerini içerir. `scripts/`, `packaging/`, `tests/`, `docs/` ve `release/` her iki parçaya birlikte aittir. Bu belgedeki komutlar bu masaüstü deposunun kökünden çalıştırılır; Cargo.toml ve service/ bu dizinde bulunur.

## Dil ve arayüz

Panel açılır menüsü durum, hızlı servis kontrolleri ve ayarları gösterir. Ayrıntılı cihaz/komut/iş/API yönetimi **Yönetim merkezi** ile açılan ayrı pencerededir; seçili sayfa yan menüde vurgulanır ve başlık gezinme sırasında görünür kalır.

Arayüz Türkçe ve İngilizce destekler. İlk açılışta `LC_ALL`, `LC_MESSAGES`, ardından `LANG` önceliğiyle cihaz dili algılanır; Türkçe dışındaki diller için İngilizce kullanılır. **Ayarlar → Dil** bölümünden **Cihaz dilini kullan**, **Türkçe** veya **English** seçilebilir. Tercih `$XDG_CONFIG_HOME/cepdesk/applet-language` (varsayılan `~/.config/cepdesk/applet-language`) dosyasında saklanır; açık panel ve yönetim pencereleri bir sonraki yenilemede aynı tercihi kullanır. Flatpak sürümünde yapılandırma sandbox içindeki kullanıcı dizininde saklanır.

Durum ve Ayarlar sayfalarından [mobil uygulama ve proje sitesine](https://cepdesk.hasimozer.com) ve [geliştirici Haşim Özer’in GitHub profiline](https://github.com/hasimzr) erişilebilir. Bağlantılar masaüstü portalı üzerinden varsayılan tarayıcıda açılır.

## Yönetim

- Durum: servis, kayıtlı/çevrimiçi cihaz sayısı, port, keşif, sertifika; başlat/durdur/yeniden başlat ve otomatik başlatma.
- Cihazlar: gerçek kimliği doğrulanmış bağlantılar/IP, serbest komut izni, mevcut bağlantıyı kesme, kalıcı anahtar iptali.
- Komutlar: tanım ekleme/güncelleme/silme ve kaydederek veya kaydetmeden çalıştırma.
- İşler: tüm cihazların ve yerel yönetimin işleri, seçilen çıktı, terminal yanıtı, iptal ve geçmiş silme.
- Ayarlar: ilk bağlantı parolası/değiştirme, pano takibi, geçici kodla eşleştirme/onay/red.
- Servis API: pano/pencere/medya/sistem/komut çağrıları için yöntem + JSON. Örnekler ana protokol belgesinde. Monitör, uinput ve ses yayınının ağ oturumu yaşam döngüsü telefonda kalır.

Çevrimiçi gösterimi kayıtlı cihaz varlığına dayanmaz: auth sonrası oturum eklenir, bağlantı/kesme/hata sonrasında RAII ile kaldırılır. 15 saniyelik ping ve 90 saniyelik yanıtsızlık sınırı kopan ağları temizler. Bağlantıyı kesmek kalıcı engelleme değildir; telefon yeniden bağlanabilir. Yetki iptali cihazın anahtarını ve çalışan işlerini iptal eder.

## Derleme ve kurulum

Sistem bağımlılıkları (Pop!_OS/Ubuntu): Rust, pkg-config, C/C++ derleyici, cmake, libxkbcommon-dev, libfontconfig-dev, libfreetype-dev, libexpat1-dev, libwayland-dev, libegl-dev, libgles-dev, libgbm-dev. Sistemdeki Rust araç zinciri kullanılır. İsteğe bağlı `.tools` araçları varsa betikler bunları da kullanabilir; temiz klonda bu dizin gerekmez. Paketlemek için ayrıca `dpkg-deb`, metadata kontrolü için `appstreamcli` ve `desktop-file-validate` kullanılabilir.

Bu deponun kökünden:

```sh
./scripts/build-cosmic-package.sh
```

Paket: `release/cepdesk_0.1.0_amd64.deb` (bu makinede amd64). Servis ve eklenti aynı pakettedir. Pop!_OS 24.04 veya eşdeğer glibc 2.39+ hedeflenir. Dağıtım kurulumu:

```sh
sudo apt install ./release/cepdesk_0.1.0_amd64.deb
cepdesk-applet --settings
```

Paket kurulunca açık grafik kullanıcı oturumlarında servis otomatik başlar; sonraki grafik oturumlarında da başlatılır. Eklenti veya yönetim penceresi açıldığında otomatik başlatması açık olan servis bir kez başlatılır. Düzenli durum yenilemesi servisi yeniden başlatmaz; Durdur seçimi korunur. Otomatik başlatmayı kapat düğmesi kullanıcıya özel systemd maskesiyle paket varsayılanını geçersiz kılar; yeniden açmak maskeyi kaldırıp servisi başlatır. Paket kaldırılırken açık oturumlardaki servis durdurulur. Kullanıcı yapılandırması korunur.

İlk servisin sistemd altında terminal olmadan başlaması desteklenir. Parola henüz yokken telefonların parola eşleştirmesi reddedilir; yönetim penceresinden ilk parola belirlenir. Telefon/sunucu anahtarları ve mevcut veriler korunur.

Daha önce `scripts/install-desktop.sh` ile kullanıcı kurulumu yapıldıysa kullanıcı unit dosyası sistem paketinden önce gelir. Mevcut kullanıcı kurulumunu güncellemek için, kaynak kökünden `./scripts/install-cosmic-local.sh` çalıştırın. Betik servis/eklenti ikililerini kullanıcı dizinine kurar ve servisi yeniden başlatır.

COSMIC Ayarları → Masaüstü → Panel → Eklentileri düzenle bölümünden CepDesk'i ekleyin. Etiketler COSMIC sürümüne göre değişebilir. Aynı eklenti birden fazla panele eklenebilir; yönetim penceresi ayrıca uygulama başlatıcısından açılır.

## Güven sınırı

Yönetim Unix soketi `$XDG_RUNTIME_DIR/cepdesk/admin.sock`, dizin 0700/soket 0600 ve peer UID kontrolüyle aynı kullanıcıya özeldir. Telefon anahtarları/parola hiçbir liste yanıtında verilmez. Parola yalnızca kullanıcı soketinden aktarılır, shell argümanı olarak taşınmaz. Servis işlemleri sabit `cepdesk.service` için kullanıcı systemd D-Bus çağrılarıdır; shell birleştirmesi kullanılmaz. Sudo komutları kullanıcının terminal oturumunda açıkça parola ister; root servis kurulmaz.

## Mağaza

[Mağaza hazırlığı ve yayımlama adımları](docs/cosmic-magaza.md). Flatpak şablonu servis ve eklentiyi birlikte içerir. İlk eklenti/yönetim penceresi açılışında paket içindeki host servisi kullanıcı systemd servisi olarak hazırlanıp başlatılır; ayrı `.deb` kurulumu gerekmez. Bu akış `org.freedesktop.Flatpak` host çalıştırma izni kullanır. `.deb` sürümü de her ikisini birlikte içerir. Kaynak deposu, ekran görüntüleri ve gerçek Flatpak build doğrulaması tamamlanmadan mağazaya hazır olarak işaretlenmez.

## Geliştirme kontrolleri

```sh
cargo build --locked --release --manifest-path service/Cargo.toml
cargo build --locked --release
bash scripts/check.sh
```

`check.sh` her iki Rust paketinin format, clippy ve testlerini; Python paketleme testlerini ve kuruluysa AppStream/desktop dosyası doğrulamasını çalıştırır. D-Bus testleri için `dbus-run-session` gerekir. Gerçek COSMIC oturumu gerektiren test ayrıca `bash scripts/verify-session.sh` ile çalıştırılır; pano ve pencere işlemleri yapar.

Touchpad/klavye için kullanıcı tarafından bir kez `sudo ./scripts/setup-input.sh` çalıştırılması gerekir. H.264 görüntü için `ffmpeg`/libx264, ses için PulseAudio araçları ve Wi-Fi için NetworkManager gerekebilir. Bu host gereksinimleri Flatpak kurulumuyla kendiliğinden sağlanmaz.

## GitHub'a aktarım

Bu klasörün **içeriğini** depo köküne yerleştirin: `Cargo.toml`, `service/`, `scripts/`, `resources/`, `README.md`, `LICENSE` ve `.gitignore` kökte görünmelidir. `.gitignore` derleme çıktıları, yerel araçlar ve yaygın anahtar/test dosyalarını dışarıda tutar; `git add` öncesinde `git status --short` ve staged değişikliklerini inceleyin. Parola, sertifika özel anahtarı veya cihaz durumlarını kaynak klasörüne kopyalamayın. Paketleri GitHub Releases'a yükleyin. Mağaza manifesti bu bağımsız depo düzeninden derlenir.
# Depdesk-CosmicExtantion
