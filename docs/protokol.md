# CepDesk protokolü v1

TCP `43821` üzerinde TLS WebSocket (`wss://IP:43821`). HTTP API ve bulut servisi yok. mDNS `_cepdesk._tcp.local.`: TXT `v=1`, `id=SERVER_UUID`. Parmak izi keşiften alınmaz.

## Mesajlar

```json
{"v":1,"id":"42","method":"clipboard.list","params":{"query":"not","offset":0}}
{"v":1,"id":"42","ok":true,"data":{"items":[],"has_more":false,"offset":0}}
{"v":1,"id":"42","ok":false,"error":{"code":"UNAUTHORIZED","message":"Önce eşleştirin.","retryable":false}}
{"v":1,"event":"clipboard.changed","seq":3,"data":{}}
```

İstek kimliği 1–64 UTF-8 bayt; yöntem en fazla 64 bayt. Bilinmeyen kök alanları reddedilir. V1 dışındaki sürümler `VERSION_UNSUPPORTED`. İkili mesaj kabul edilmez. Olay sırası her bağlantıda 1’den başlar; kalıcı replay günlüğü değildir. Yeni bağlantıda istemci tüm ilgili listeleri tekrar alır. Olay tamponu taşarsa `resync.required` gönderilir.

## Eşleştirme ve kimlik doğrulama

1. Servis ilk terminal açılışında ortak parola belirletir. Rastgele 32 bayt salt ile PBKDF2-HMAC-SHA256 (600.000 tur, UTF-8 parola, 32 bayt çıktı) türetilir; salt ve türetilmiş anahtar `password.json` içinde 0600 izinle saklanır.
2. Yeni telefon geçici TLS bağlantısında sertifikanın geçerliliğini kontrol eder ve ilk görülen sertifikaya kilitlenir. Bu aşamada native transport yalnızca `password.hello` göndermeye izin verir.
3. `password.hello {nonce,name}`: nonce 32 bayt rastgele verinin küçük harf hex kodlamasıdır. Yanıt `{salt,iterations,server_id,fingerprint,nonce,proof,transcript}` içerir. Transcript JSON dizisi `["cepdesk-password-v1",server_id,fingerprint,client_nonce,server_nonce,name]` olarak sunucuda oluşturulur. Sunucu kanıtı HMAC-SHA256(key, UTF8(`server:` + transcript)) hex kodlamasıdır.
4. Telefon transcript alanlarını kendi nonce'u, cihaz adı ve TLS üzerinde gözlemlediği sertifika özetiyle karşılaştırır; aynı paroladan anahtar türetip sunucu kanıtını sabit zamanlı karşılaştırmayla doğrular. Kanıt yanlışsa bağlantıyı keser ve istemci kanıtı göndermez.
5. `password.pair {proof}`: istemci kanıtı HMAC-SHA256(key, UTF8(`client:` + transcript)) hex kodlamasıdır. İstek yalnızca aynı WebSocket'teki 90 saniyelik challenge için geçerlidir ve her denemede tüketilir. Başarı → `{device,token}`; kayıtlı erişim anahtarı rastgele 256 bittir. Ortak parola veya türetilmiş anahtar ağda taşınmaz.
6. `auth {token}` → `{server_id,device,capabilities,settings}`. Telefon anahtarı ve doğrulanmış sertifikayı SecureStore'da saklar. Sonraki bağlantılar doğrudan kaydedilen sertifikaya sabitlenir.

Kimliği doğrulanmamış bağlantıya yalnızca `password.hello/password.pair`, eski `pair.request/pair.poll` ve `auth` açıktır. Parolalı ilk bağlantı denemeleri servis genelinde dakikada beşle sınırlıdır. Birden fazla cihaz aynı parolayla kaydolabilir; hepsi ayrı iptal edilebilir anahtar alır. Cihaz yetkileri varsayılan olarak pano/pencere/medya ve kayıtlı komutları kapsar; serbest komut kapalıdır.

Eski CLI kodlu akış geriye uyumluluk için korunur: `cepdesk pair` → `pair.request {code,name}` → yerel `approve` → `pair.poll {pending_id,poll_secret}` → `auth`. Güncel mobil ekran bu akışı kullanmaz.

## Yüklü uygulama API'si

Kimlik doğrulaması sonrasında:

- `applications.list {offset?:number}` → `{applications:[{id,name,description}],total,has_more}`. 256 kayıtlık sayfalar; offset aralığı 0–8192. XDG/Flatpak katalogları 30 saniyede bir yenilenir.
- `applications.icons {ids:string[]}` → `{icons:{ID:string|null}}`. En fazla 24 ID. Yerel ikonlar oranları korunarak PNG data URI olarak aktarılır (ikon başına en fazla 24 KiB ham PNG); SVG/XPM de desteklenir. Kaynak bulunamazsa null.
- `applications.launch {id:string}` → `{launched:ID}`. Yalnızca sunucunun görünür kurulu uygulama kataloğundaki tam ID kabul edilir. İstemciden yol, Exec veya shell metni alınmaz. Gio başlatıcı hatasında LAUNCH_FAILED/UNSUPPORTED/TIMEOUT döner.

`capabilities.applications=true` bu API desteğini bildirir. Favoriler sunucuya gönderilmez; telefonda sunucu kimliğine göre saklanır. İkonlar isim/StartupWMClass üzerinden açık pencerelerle de eşleştirilir; `windows.icon` aynı ikon çözümleyiciyi kullanır.

## Sistem ve medya uzantıları

Tüm yöntemler kayıtlı cihaz kimlik doğrulaması gerektirir:

- `windows.close {id}` → `{requested:true}`. Compositor'ın `desktop.window_close` capability'si gereklidir. Normal kapatma talebidir; kaydetme iletişim kutusu nedeniyle pencere açık kalabilir.
- `media.action {id,action:"position",position_us,track_id}` mutlak konuma sarar. Güncel track ID eşleşmesi ve 0–length sınırı doğrulanır. SetPosition olmayan tarayıcı adaptörlerinde güncel Position ile Seek farkı kullanılır. TRACK_CHANGED/INVALID_ARGUMENT hataları yanlış parçaya sarılmasını sınırlar.
- `system.controls {}` → `{audio,wifi,power,phone_audio}`. Ses cihazları/varsayılanlar, seviyeler/sessiz durumları, ağlar/kayıtlı profiller ve logind izinleri.
- `system.control {action,volume?,muted?,id?,enabled?}`: action yalnızca volume/mic_volume/mute/mic_mute/sink/source/wifi_radio olabilir. Ses aralığı 0–1; seçilecek cihaz sunucunun listesinde bulunmalıdır.
- `system.wifi.connect {profile}` kayıtlı Wi-Fi UUID'sini etkinleştirir; alternatif `{ssid,password?,security?}` yeni kişisel bağlantı oluşturur. Ağ/parola uzunlukları sınırlıdır. SSID/parola D-Bus sözlüğünde taşınır; istemciden shell komutu alınmaz.
- `system.power {action,confirmed:true}` action poweroff/reboot/suspend. Açık onay bayrağı yoksa CONFIRM_REQUIRED. logind işlemleri etkileşimli yetki artırma olmadan çağrılır; oturum politikası reddederse FORBIDDEN.
- `system.audio.start {}` → `{rate:24000,channels:2,format:"s16le"}`. Sonrasında aynı WSS üzerinde stereo PCM16 ikili frame'ler (en fazla 1920 bayt, 4 bayt hizalı) gelir. Kontrol yanıtları/olayları JSON kalır. Android native oynatıcı ses paketlerini doğrudan tüketir.
- `system.audio.stop {}` → `{stopped:true}`. Capture EOF/hata durumunda system.audio.stopped olayı gönderilir. Bağlantı kopma/revoke halinde capture process kill edilir. Her ses paketi gönderiminden önce cihaz yetkisi yeniden kontrol edilir; yavaş ses alıcısında yazma sınırı 2 saniyedir.

Komut oluşturma/düzenleme mevcut `commands.save` ve free_commands izniyle çalışır. Bu uzantılar serbest shell API'si eklemez; sabit araç/argümanlar, doğrulanan cihaz/profil kimlikleri ve D-Bus yöntemleri kullanılır.

## Yöntemler

| Yöntem | Params | Data / davranış |
|---|---|---|
| `system.info` | `{}` | auth ile aynı güncel bilgi |
| `clipboard.list` | `{query?,offset?}` | 50 önizleme: id, mime, preview, bytes, created_at, promoted_at; has_more |
| `clipboard.get` | `{id}` | tam metin kaydı |
| `clipboard.select` | `{id}` | aktif panoya yaz; sıralama korunur |
| `clipboard.promote` | `{id}` | aktif panoya yaz ve üste taşı |
| `clipboard.delete` | `{id}` | tek kayıt sil |
| `clipboard.clear` | `{}` | geçmişi temizle; mevcut selection’ı bozmaz |
| `settings.set` | tüm Settings alanları | paused, max_entries, max_total_bytes, max_item_bytes |
| `windows.list` | `{}` | windows: id, app_id, app_name, title, active; icon ayrı alınır |
| `windows.icon` | `{id}` | `{uri: data-image-or-null}` |
| `windows.activate` | `{id}` | aktif state doğrulanır; `{activated:true}` |
| `media.list` | `{}` | oynatıcı metadata ve kontrol yetenekleri |
| `media.action` | `{id,action,offset_us?,volume?}` | action: play, pause, next, previous, seek, volume |
| `media.artwork` | `{id}` | yerel PNG/JPEG/WebP için `{uri}` |
| `commands.list` | `{}` | kayıtlı komut tanımları |
| `commands.save` | SavedCommand | ekle/güncelle; serbest komut izni gerekli |
| `commands.delete` | `{id}` | serbest komut izni gerekli |
| `commands.run` | `{saved_id,keep_history?}` veya Spec | Job; işi başlatır, bitmesini beklemez |
| `jobs.list` | `{}` | cihazın işleri; stdout/stderr ayrıntıda |
| `jobs.get` | `{id}` | cihazın tam işi ve çıktısı |
| `jobs.input` | `{id,data}` | kendi çalışan terminaline 1–4096 bayt; Enter `\n`, Ctrl+C `\u0003`, Ctrl+D `\u0004` |
| `jobs.delete` | `{id}` | kendi bitmiş işini bellekten ve kalıcı geçmişten siler |
| `jobs.cancel` | `{id}` | kendi çalışan işini iptal isteği |

Spec: `{command,shell,cwd,timeout_seconds,saved_id?,keep_history?}`. Serbest çağrıda command en fazla 8192 bayt, shell/cwd mutlak mevcut yol, timeout 1–3600 sn. SavedCommand: `{id,name,command,shell,cwd,timeout_seconds}`; `id:""` mobil kaydetmede yeni UUID üretir. Mobil kayıtlı iş tekrarında eski shell metni yerine `saved_id` gönderir; güncel tanım ve izinler kullanılır.

Job: id, owner, spec, status, created_at, finished_at, exit_code, stdout, stderr, output_truncated. Status: running/finished/cancelled/timed_out/failed/interrupted/output_limit. finished içinde sıfır dışı exit_code komutun başarısızlığını ifade eder. Signal ile biten süreçte exit_code null olabilir. PTY çıktısında stdout/stderr birleşir; stderr yalnızca başlangıç hatasında kullanılır. `interactive_commands:true` giriş desteğini bildirir. `keep_history` varsayılan false; geçici işler sınırlı bellekte tutulur, servis yeniden başlatıldığında kaybolur.

## Olaylar

`clipboard.changed`, `windows.changed`, `commands.changed`: boş bildirim; ilgili liste yeniden alınır. `job.changed`: `{id,status}`, çıktı `jobs.get` ile alınır. `media.changed`: `{players}`. `settings.changed`: Settings. `clipboard.notice`: `{message}`. `capabilities.changed`: auth içindeki tam capabilities; her cihazın güncel izinlerini içerir. `resync.required`: tüm ekranları yeniden sorgula.

## Sınırlar ve hata kodları

Gelen mesaj/frame en fazla 256 KiB, giden yanıt en fazla 1 MiB; bağlantı başına saniyede 30 istek, aynı anda en fazla 16 ağ bağlantısı; TLS/WS handshake beş saniye, istek göndermeyen eşleşmemiş bağlantı 30 saniye, toplam eşleşmemiş bağlantı ömrü 210 saniye. Yavaş istemciye yazma beş saniyede kesilir. İlk sürüm IPv4 özel/loopback/link-local adreslerini kabul eder; internete port açmayın. İş sınırları ve pano sınırları mimari belgesinde açıklanır.

Hatalar: INVALID_ARGUMENT, INVALID_MESSAGE, VERSION_UNSUPPORTED, METHOD_NOT_FOUND, UNAUTHORIZED, FORBIDDEN, UNSUPPORTED, NOT_FOUND, RATE_LIMITED, PAIR_CODE_INVALID, PAIR_EXPIRED, ALREADY_APPROVED, CONTENT_TOO_LARGE, BUSY, TIMEOUT, ACTIVATION_DENIED, INTERNAL, LIMIT, RESPONSE_TOO_LARGE. Hata mesajları hassas payload içermez. retryable v1’de false; istemci yalnızca bağlantı hatalarını kontrollü yeniden dener, mutasyon isteklerini otomatik tekrar etmez.

Bağlantı tekrarları: 1, 2, 4, 8, 16, 30 saniye + en fazla 499 ms jitter; auth/certificate/identity hatasında otomatik tekrar durur. Arka planda mobil bağlantı kapatılır; uygulama öne geldiğinde güvenli anahtarla tekrar bağlanır. Arka planda sürekli Android servisi veya pil muafiyeti gerekmez.

### Giriş kontrolü

Capability `input` `/dev/uinput` yazma erişimini bildirir. Kimliği doğrulanmış her bağlantı kendi sanal aygıtına sahiptir. `input.start` (isteğe bağlı ilk hazırlık), `input.keepalive`, `input.release` boş params alır. `input.key` `{code:number, down:boolean}` alır: Linux evdev 1–248 ve fare 272/273/274; tekrar down EV_KEY tekrar olayıdır. `input.move` ve `input.scroll` `{x:number,y:number}` alır; -2048..2048 tamsayı göreli imleç veya yatay/dikey tekerlek adımları. Yanıt `{applied:true}`. Giriş için ayrı 180 istek/s sınırı uygulanır; diğer yöntemlerde 30 sınırı korunur. Bağlantı kopması aygıtı kapatır; 10 saniye giriş gelmezse basılı tuşlar bırakılır. `INPUT_UNAVAILABLE` aygıt erişimi/kurulum hatasını bildirir.

### Monitör

Capability `monitor:true` yöntem desteğini bildirir. Kimliği doğrulanmış bağlantı başına tek kaynak bulunur. `monitor.start {mode:"screen"|"window",binary?:boolean,codec?:"h264"}` yalnızca açık seçimle çağrılır. Başlangıç ilk kodlanmış kare hazır olunca metadata ile yanıtlar. `monitor.frame {stream_id}` son karenin metadata bilgisini alır. `monitor.keepalive {stream_id}` 3 saniyede bir akışı canlı tutar. `monitor.stopped` olayı yakalama/kodlama hatasını bildirir.

Güncel Android istemci `binary:true,codec:"h264"` ister. WSS paket biçimi: ASCII `CDV1`, 4 bayt big-endian metadata JSON uzunluğu, UTF-8 metadata JSON, tek H.264 Annex-B erişim birimi. Metadata `{stream_id,source_id,frame_id,width,height,title,mode,touch,target_fps,capture_fps,encode_ms,codec:"h264",video_id,keyframe}`. Hedef 30 FPS; oranı korunmuş, çift sayılı boyutlar ve maksimum 1920 uzun kenar (16:9 ekranda 1080p). FFmpeg/libx264 kalıcı kodlayıcısı `ultrafast`, `zerolatency`, Baseline / YUV420, CRF 20, en fazla 10 Mbps / 1 Mbit VBV, B-kare yok ve 30 karede bir IDR kullanır. Her IDR SPS/PPS içerir. BT.709 renk matrisi ve sınırlı video aralığı açıkça belirtilir. FFmpeg'in yerel AVI borusu yalnızca erişim birimlerini ayırmak içindir; ağa AVI/MP4 dosyası gönderilmez. Kodlama ilk sürümde PC CPU'sunda yapılır.

Tahminli karelerin bağımlılıkları korunur: masaüstü WSS kuyruğu üç kareyle sınırlıdır. Dolunca yeni kodlama atlanır ve sonraki kodlayıcı yeni `video_id` ve IDR ile başlar. Kaynak veya çözünürlük değişimi de kodlayıcıyı yeniden başlatır. Android kuyruğu en fazla dört erişim birimi tutar; taşma veya sıra atlamasında bağımlı kareleri silip yeni IDR bekler. MediaCodec donanım çözücüsü önceliklidir; uygun donanım yoksa yazılım çözücüsü denenir. SPS/PPS `csd-0/csd-1` ile verilir; görüntü TextureView yüzeyine doğrudan çözülür. Telefon FPS göstergesi `OnFrameRenderedListener` bildirimlerini sayar. Tam ekran yüzey değişiminde en geç sonraki IDR geldiğinde görüntü tekrar başlar; sabit 30 FPS akışta yaklaşık bir saniye, yavaş akışta daha uzun olabilir. Decode hatası mobilde gösterilir. Görüntü baytları JS köprüsünden geçmez.

Geriye uyumlu JPEG yolu: `codec` verilmezse eski davranış korunur. `binary:false` JSON `image` JPEG data URI döndürür (1280 / kalite 68 / 700 KiB). `binary:true` JPEG paketleri ASCII `CDM2`, 4 bayt big-endian metadata uzunluğu, metadata JSON ve JPEG baytlarıdır (60 FPS hedefi / 2560 uzun kenar / kalite 90 / 2 MiB). Ses paketleri mevcut PCM biçimini korur; hem `CDV1` hem `CDM2` native tarafta ses kuyruğundan ayrılır.


`monitor.click {stream_id,source_id,x,y}` normalize 0..1 görüntü koordinatında sol tık, `monitor.scroll {stream_id,source_id,x,y,dx,dy}` o konumda tamsayı tekerlek adımları gönderir. Dokunma yöntemleri girişe ait 180 istek/sn bütçesini kullanır. Window modu mevcut odaklı pencereyi takip eder; her kaynak değişiminde `source_id` artırılır. Eski kareyle gönderilen dokunma `MONITOR_SOURCE_CHANGED` ile reddedilir; önceki pencereyi tekrar odaklama yapılmaz. Eski capture session/frame olayları kaynak neslinin dışındaysa yok sayılır. Mutlak uinput aygıtı oturuma/yayına aittir; çoklu fiziksel monitör girişine bu sürümde izin verilmez. `monitor.stop {stream_id}` yalnızca eşleşen yayını kapatır. 15 saniye yayın etkinliği olmaması ve bağlantı kapanması yakalamayı sonlandırır. `MONITOR_UNAVAILABLE`, `MONITOR_STOPPED`, `INPUT_UNAVAILABLE` hataları açıklamalarıyla gösterilir.

Protokol kaynakları: [Wayland image capture](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/staging/ext-image-copy-capture/ext-image-copy-capture-v1.xml), [COSMIC toplevel bilgisi ve geometri](https://github.com/pop-os/cosmic-protocols/blob/main/unstable/cosmic-toplevel-info-unstable-v1.xml).

## COSMIC yerel yönetim soketi

Ağ RPC'sinden ayrı, aynı UID'ye özel `$XDG_RUNTIME_DIR/cepdesk/admin.sock`. Satır başına JSON ve `{ok,data,error}` yanıtı; 32768 bayt istek ve 5 saniyelik okuma sınırı. Parola veya cihaz anahtarı liste yanıtlarında verilmez.

- `status`, `devices`: kayıtlara `online`, `sessions[{id,peer,connected_at,last_seen}]` eklenir; status ayrıca `password_configured` taşır.
- `disconnect {id}`: cihazın mevcut oturumlarını kapatır; anahtarı iptal etmez. `revoke {id}` anahtarı/işleri/oturumları iptal eder.
- `password.set {password}`: ilk kurulum veya atomik parola değişikliği; anahtarlar korunur.
- `settings.set {value:Settings}`: pano ayarları ve bildirim.
- `saved.list`, `saved.add {value:SavedCommand}`, `saved.delete {id}`: 100 tanım sınırı ve değişim bildirimi.
- `jobs.list`: tüm cihazların ve desktop sahibinin son 100 işi, çıktı ayrıntıda. `jobs.get/cancel/delete/input {id,owner,data?}` yerel yönetimden hedef sahibin işini kontrol eder.
- `rpc {request:{v:1,id,method,params}}`: servis işlemlerini yerel `desktop` sahibi olarak çağırır. Ağ oturumuna bağlı input/monitor/audio yayınları bu köprüde kurulmaz.

Auth sonrası canlı oturum eklenir, hata/kapanışta RAII ile silinir. 15 saniyede ping ve 90 saniye yanıtsızlık sınırı vardır. Yerel yönetim soketi telefonların WSS API'sine açılmaz.
