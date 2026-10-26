# CepDesk'i COSMIC Store'da yayımlama hazırlığı

10 Ekim 2026'da resmi belgeler ve örnekler kontrol edildi. COSMIC Store'da APK yüklenmez; mağaza yapılandırılmış Flatpak ve sistem paket depolarından içerik gösterir. Resmi COSMIC Flatpak deposu, Flathub'a uygun olmayan panel eklentilerini kabul etmek üzere hazırlanmıştır.

Resmi kaynaklar:

- COSMIC eklenti deposu: https://github.com/pop-os/cosmic-flatpak
- Mağazanın depo davranışı: https://support.system76.com/support/managing-apps/
- COSMIC metainfo işareti: https://github.com/pop-os/libcosmic#made-for-cosmic-flatpak-ids
- Geleneksel uygulamalar için Flathub kuralları: https://docs.flathub.org/docs/for-app-authors/submission/

## Bu çalışma alanında hazır olanlar

Bu depo: yerel libcosmic eklentisi ve yönetim penceresi. `resources`: panel ve normal pencere desktop kayıtları, ikon, AppStream metainfo; `com.system76.CosmicApplet` sağlar. `packaging/debian`: yerel servis unit'i; `scripts/build-cosmic-package.sh`: servis ve eklentiyi birlikte paketler. `packaging/flatpak/dev.cepdesk.Applet.json.in`: COSMIC BaseApp ve 26.08 runtime için şablon. `scripts/prepare-cosmic-store.py`: gerçek depo/commit/lock kaynak listesi verilince resmi depo düzeninde gönderim dosyalarını üretir. Kimlik `dev.cepdesk.Applet` şu anda proje içi kimliktir; yayın öncesinde sahip olunan alan adı veya GitHub hesabına göre kalıcı kimliği kesinleştirin.

MIT, mevcut masaüstü Cargo.toml lisansıyla uyumlu hazırlık seçimidir. Depo sahibi yayın öncesi lisans ve telif adını doğrulamalıdır. Ürün homepage adresi https://cepdesk.hasimozer.com olarak tanımlıdır. Kamuya açık GitHub depo adresi henüz kesinleşmediği için kaynak adresi uydurulmadı. Hazırlık betiği homepage adresini koruyarak vcs-browser bağlantısını ekler; üretilen metainfo upstream'e eklenip yeni commit alınmalıdır.

## Tam kurulum hangi pakette?

Native `.deb` ve COSMIC Flatpak şablonu servis + eklentiyi birlikte paketler. Flatpak derlemesi iki ayrı Cargo.lock için kaynak listesi kullanır; eklenti `/app/bin/cepdesk-applet`, servis `/app/libexec/cepdesk-host/bin/cepdesk` altında yer alır. Servisin ELF yükleyicisi, bağımlı kütüphaneleri ve GdkPixbuf resim eklentileri aynı mimari için pakete dahil edilir. Host programlara `LD_LIBRARY_PATH` taşınmaz; servisin resim yükleyici ayarı da başlatılan host komutlarından temizlenir.

Flatpak kurulumunda servis dosyaları da indirilir. Flatpak kurulum sonrası host script çalıştırmadığı için kullanıcı ilk kez eklentiyi panele eklediğinde veya yönetim penceresini açtığında otomatik hazırlık yapılır. Kullanıcıya özel `cepdesk.service`, Flatpak içindeki `cepdesk-host-launch` komutunu başlatır; o da paket içindeki servisi `flatpak-spawn --host --watch-bus` ile host üzerinde çalıştırır. Sudo, ayrı `.deb`, internetten ikinci bir indirme veya root servisi gerekmez. Güncellemelerde unit sabit Flatpak kimliğini çağırdığı için yeni paket sürümü bir sonraki servis başlangıcında kullanılır.

Bu işlev için `org.freedesktop.Flatpak` D-Bus izni gerekir; izin host üzerinde kullanıcı yetkisiyle program çalıştırır. Mağaza açıklaması ve PR bu geniş yetkiyi açıkça belirtmelidir. Mağaza kabulü garanti değildir. Mevcut native/custom unit veya kullanıcı maskesi üzerine yazılmaz. Kullanıcının otomatik başlatmayı kapatma tercihi korunur; durum yenilemesi durdurulmuş servisi yeniden açmaz.

İlk bağlantı parolasını yönetim penceresinden belirlemek, Android uygulamasını kurmak ve eşleştirmek hâlâ gereklidir. `/dev/uinput` erişimi, isteğe bağlı ffmpeg/pactl/parec ve oturum ilkeleri host sistemine bağlıdır; pakete servis eklemek bu sistem izinlerini otomatik vermez.

Flatpak kaldırıldığında ilk açılışta oluşturulan kullanıcı unit'i kalabilir. Unit'in ExecCondition kontrolü uygulama artık kurulu değilse servisin yeniden başlamasını engeller. Paket kaldırılmadan önce yönetim penceresinden servisi durdurup otomatik başlatmayı kapatın. Flatpak tarafından oluşturulan unit'i tamamen temizlemek için yalnızca `# CepDesk Flatpak managed unit v1` işaretli kullanıcı dosyasını kaldırıp kullanıcı systemd yöneticisini yeniden yükleyin; native/custom unit'e dokunmayın.

## COSMIC eklenti deposuna gönderim

1. Projeyi size ait kamuya açık GitHub deposuna koyun. `.tools`, `target`, telefon parolaları, cihaz anahtarları ve test durumları kaynak deposuna girmemeli. Lisans, build talimatları, gerçek homepage/vcs URL'leri ve eklenti ekran görüntülerini upstream'e ekleyin. AppStream screenshot URL'leri kalıcı HTTPS adresleri olmalı; sahte ekran görüntüsü kullanmayın.
2. Gerçek COSMIC oturumunda panel ekleme/popup, yönetim penceresi, cihaz çevrimiçi/çevrimdışı, izin/kesme/iptal, ilk parola ve servis yeniden başlatma akışlarını doğrulayın. Yerel `.deb` kurulumu ile eski kullanıcı unit önceliğini de kontrol edin. Bu hazırlık tek başına mağaza kabulü değildir.
3. `flatpak-cargo-generator.py` aracını resmi `https://github.com/flatpak/flatpak-builder-tools` deposundan alın. **Son** `Cargo.lock` ve `service/Cargo.lock` için ayrı kaynak listeleri üretin. Aracın `--help` çıktısına göre, örnek kullanım:

   ```sh
   python3 flatpak-cargo-generator.py Cargo.lock -o cargo-sources.json
   python3 flatpak-cargo-generator.py service/Cargo.lock -o service-cargo-sources.json
   python3 scripts/prepare-cosmic-store.py \
     --repository https://github.com/HESABIN/CepDesk.git \
     --commit TAM_40_KARAKTER_COMMIT \
     --cargo-sources cargo-sources.json \
     --service-cargo-sources service-cargo-sources.json
   ```

   Kaynak kodda homepage/screenshot güncellemesi yaptıktan sonra yeni **son commit** ile betiği tekrar çalıştırın. Şablon bağımsız masaüstü deposunun kökünü esas alır; Cargo.toml, service/, resources/ ve scripts/ kökte bulunmalıdır. Bu alandaki commit metni örnektir; gönderimde gerçek SHA zorunludur.
4. `pop-os/cosmic-flatpak` deposunu kendi hesabınıza fork edin. Üretilen `release/store-submission/app/dev.cepdesk.Applet/` klasörünü fork içindeki `app/dev.cepdesk.Applet/` yoluna koyun. Yalnızca manifest ve bağımlılık kaynak tanımları bu gönderimdedir; kaynak kodunuz kendi upstream deponuzdan sabit commit ile alınır.
5. Resmi deponun README'sindeki bağımlılıkları kurun (`flatpak`, `flatpak-builder`, `just`). Runtime/BaseApp kaynaklarını erişilebilir hale getirip repo kökünde `just build dev.cepdesk.Applet` çalıştırın. Build, socket izni, systemd D-Bus, desktop kaydı ve AppStream doğrulamasını bitirin. Bu çalışma ortamında Flatpak builder kurulmadığından bu aşama henüz çalıştırılmadı; native derleme Flatpak sandbox doğrulaması yerine geçmez.
6. Fork'tan `pop-os/cosmic-flatpak` reposunun varsayılan dalına Pull Request açın. Eklentinin ne yaptığı, Türkçe arayüz, paketlenmiş host servisi, otomatik hazırlık ve org.freedesktop.Flatpak dahil kullanılan socket/D-Bus izinleri ve test sonuçlarını yazın. Bakımcıların inceleme/düzeltme ve paketleme süreci sonrasında mağazaya dağıtım mümkün olur; kabul ve görünme süresi garanti değildir.

Kullanıcı tarafında COSMIC deposu yoksa resmi komut:

```sh
flatpak remote-add --if-not-exists --user cosmic https://apt.pop-os.org/cosmic/cosmic.flatpakrepo
```

Kabul edilip depoya yayımlandıktan sonra COSMIC Store'da CepDesk aranıp kurulabilir. Panel eklentisi ayrıca COSMIC Ayarları'ndan panele eklenir.

## Tek pakette servis + eklentiyle mağaza dağıtımı

Tam host entegrasyonu için `.deb` daha doğrudan yoldur. Paketi GitHub Releases'a koymak otomatik olarak tüm kullanıcıların COSMIC Store aramasına eklemez. İmzalı, AppStream metadatalı bir APT deposuna koyup kullanıcıların bu depoyu eklemesini sağlamak veya Pop!_OS paket bakımcılarıyla depoya kabul sürecini yürütmek gerekir. Resmi Pop deposuna otomatik kabul edilmiş gibi bir durum yoktur. Özel APT deposu yolu seçilecekse repository anahtarı, `signed-by`, Packages/Release/InRelease ve AppStream dağıtımı ayrıca hazırlanmalıdır.

Şimdi hiçbir uzak depo, mağaza PR'ı veya yayın oluşturulmadı. Yerel kurulum da bu hazırlık sırasında kullanıcı hesabına uygulanmadı.

## Yerel birleşik Flatpak test paketi

`release/CepDesk-cosmic-0.1.0.flatpak`, bu makinedeki x86_64 release ikililerinden oluşturulan yerel entegrasyon paketidir; servis, eklenti ve host servisinin kütüphaneleri aynı dosyadadır. Üretmek için önce native release derlemesini yapın, ardından `bash scripts/build-flatpak-local.sh` çalıştırın. Freedesktop Platform 26.08 gereklidir. `CEPDESK_LOW_DISK=1` yalnızca oluşturulan geçici export deposunun varsayılan yüzde rezervini kaldırır; kurulu Flatpak depolarını değiştirmez.

Mağaza PR'ında yerel prebuilti kullanmayın: yukarıdaki iki lockfile kaynak listesi ve kaynak derleyen manifest kullanılır. Gerçek Flatpak sandbox → host başlatıcı → paketlenmiş servis akışında durum, ilk parola ve cihaz API'si test edildi. Systemd'nin KillMode davranışı gibi test süreçleri birlikte sonlandırıldığında host servisi de kapandı. İlk hazırlık, tekrar açılış, custom/native unit koruma ve maskeye saygı izole testlerde doğrulandı. Temiz kullanıcı hesabında mağazadan kurulum, panele ekleme ve kaynak manifestinin SDK/BaseApp ile tam derlenmesi hâlâ kabul testidir.
