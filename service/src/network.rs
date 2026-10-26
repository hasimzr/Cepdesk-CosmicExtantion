use crate::{
    commands::{parse_spec, validate_saved, Jobs, Spec},
    config::Paths,
    desktop::Desktop,
    media::Media,
    pairing::Pairing,
    protocol::{self, text, ApiError, Event, Request, Result, MAX_MESSAGE},
    store::{hash, Device, SavedCommand, Settings, Store},
};
use futures_util::{SinkExt, StreamExt};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use rustls::{pki_types::PrivateKeyDer, ServerConfig};
use serde_json::{json, Value};
use std::{
    fs,
    net::IpAddr,
    os::unix::fs::PermissionsExt,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    net::TcpListener,
    sync::{broadcast, Semaphore},
};
use tokio_rustls::TlsAcceptor;
use tokio_tungstenite::{
    tungstenite::{protocol::WebSocketConfig, Message},
    WebSocketStream,
};

pub struct App {
    pub store: Arc<Store>,
    pub pairing: Pairing,
    pub connections: Arc<crate::connections::Connections>,
    pub password: crate::password::Password,
    pub desktop: Desktop,
    pub media: Arc<Media>,
    pub jobs: Arc<Jobs>,
    pub events: broadcast::Sender<Event>,
    pub fingerprint: String,
    pub server_id: String,
    pub port: u16,
    pub discovery: Mutex<bool>,
}
impl App {
    pub fn capabilities(&self, device: &Device) -> Value {
        json!({"desktop":self.desktop.features(),"media":self.media.available(),"commands":true,"applications":true,"input":crate::input::available(),"monitor":true,"free_commands":device.free_commands,"discovery":*self.discovery.lock().unwrap(),"clipboard_mime_types":["text/plain;charset=utf-8"],"interactive_commands":true})
    }
    pub async fn request(self: &Arc<Self>, d: &Device, req: &Request) -> Result<Value> {
        let p = &req.params;
        match req.method.as_str() {
            "system.info" => Ok(
                json!({"server_id":self.server_id,"capabilities":self.capabilities(d),"device":d,"settings":self.store.settings()?}),
            ),
            "applications.list" => {
                let apps = tokio::task::spawn_blocking(crate::applications::list)
                    .await
                    .map_err(|_| ApiError::new("INTERNAL", "Uygulamalar taranamadı."))?;
                let offset = p
                    .get("offset")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    .min(8192) as usize;
                let total = apps.len();
                Ok(
                    json!({"applications":apps.into_iter().skip(offset).take(256).collect::<Vec<_>>(),"total":total,"has_more":offset.saturating_add(256) < total}),
                )
            }
            "applications.launch" => crate::applications::launch(text(p, "id", 512)?).await,
            "applications.icons" => {
                let ids = p["ids"]
                    .as_array()
                    .filter(|ids| ids.len() <= 24)
                    .ok_or_else(|| {
                        ApiError::new(
                            "INVALID_ARGUMENT",
                            "En fazla 24 uygulama ikonu istenebilir.",
                        )
                    })?;
                let ids = ids
                    .iter()
                    .map(|id| {
                        id.as_str()
                            .filter(|id| id.len() <= 512)
                            .map(str::to_string)
                            .ok_or_else(|| {
                                ApiError::new("INVALID_ARGUMENT", "Geçersiz uygulama kimliği.")
                            })
                    })
                    .collect::<Result<Vec<_>>>()?;
                let icons = tokio::task::spawn_blocking(move || {
                    ids.into_iter()
                        .map(|id| {
                            let icon = crate::applications::icon(&id);
                            (id, icon)
                        })
                        .collect::<std::collections::BTreeMap<_, _>>()
                })
                .await
                .map_err(|_| ApiError::new("INTERNAL", "İkonlar okunamadı."))?;
                Ok(json!({"icons":icons}))
            }
            "system.controls" => Ok(crate::system::status().await),
            "system.control" => crate::system::action(p).await,
            "system.wifi.connect" => crate::system::wifi_connect(p).await,
            "system.power" => crate::system::power(p).await,
            "windows.close" => {
                self.desktop.close(text(p, "id", 128)?.into()).await?;
                Ok(json!({"requested":true}))
            }
            "clipboard.list" => {
                let query = p.get("query").and_then(Value::as_str).unwrap_or("");
                if query.len() > 1024 {
                    return Err(ApiError::new("INVALID_ARGUMENT", "Arama metni çok uzun."));
                }
                let offset = p
                    .get("offset")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    .min(5000) as usize;
                let items = self.store.clips(query, offset)?;
                Ok(
                    json!({"items":items.iter().map(|c|json!({"id":c.id,"mime":c.mime,"preview":c.content.chars().take(400).collect::<String>(),"bytes":c.content.len(),"created_at":c.created_at,"promoted_at":c.promoted_at})).collect::<Vec<_>>(),"offset":offset,"has_more":items.len()==50}),
                )
            }
            "clipboard.get" => Ok(json!(self.store.clip(text(p, "id", 128)?)?)),
            "clipboard.select" | "clipboard.promote" => {
                let id = text(p, "id", 128)?;
                let clip = self.store.clip(id)?;
                self.desktop.write(clip.content).await?;
                if req.method == "clipboard.promote" {
                    self.store.promote(id)?;
                    let _ = self.events.send(Event {
                        name: "clipboard.changed",
                        data: json!({}),
                        owner: None,
                    });
                }
                Ok(json!({"selected":id}))
            }
            "clipboard.delete" => {
                self.store.delete_clip(Some(text(p, "id", 128)?))?;
                self.clip_changed();
                Ok(json!({}))
            }
            "clipboard.clear" => {
                self.store.delete_clip(None)?;
                self.clip_changed();
                Ok(json!({}))
            }
            "settings.set" => {
                let settings: Settings = serde_json::from_value(p.clone())
                    .map_err(|_| ApiError::new("INVALID_ARGUMENT", "Pano ayarları geçersiz."))?;
                self.store.set_settings(settings.clone())?;
                self.clip_changed();
                let _ = self.events.send(Event {
                    name: "settings.changed",
                    data: json!(settings),
                    owner: None,
                });
                Ok(json!(settings))
            }
            "windows.list" => {
                if !self.desktop.features().window_list {
                    return Err(ApiError::new(
                        "UNSUPPORTED",
                        "COSMIC pencere listeleme protokolü mevcut değil.",
                    ));
                }
                let windows = self
                    .desktop
                    .windows()
                    .into_iter()
                    .map(|mut w| {
                        w.icon = None;
                        w
                    })
                    .collect::<Vec<_>>();
                Ok(json!({"windows":windows}))
            }
            "windows.icon" => {
                let id = text(p, "id", 128)?;
                let app_id = self
                    .desktop
                    .windows()
                    .into_iter()
                    .find(|w| w.id == id)
                    .map(|w| w.app_id);
                let icon = tokio::task::spawn_blocking(move || {
                    app_id.and_then(|id| crate::applications::icon(&id))
                })
                .await
                .map_err(|_| ApiError::new("INTERNAL", "İkon okunamadı."))?;
                Ok(json!({"uri":icon}))
            }
            "windows.activate" => {
                self.desktop.activate(text(p, "id", 128)?.into()).await?;
                Ok(json!({"activated":true}))
            }
            "media.list" => Ok(json!({"players":self.media.players()})),
            "media.action" => {
                tokio::time::timeout(
                    Duration::from_secs(4),
                    self.media
                        .action(text(p, "id", 256)?, text(p, "action", 32)?, p),
                )
                .await
                .map_err(|_| ApiError::new("TIMEOUT", "Oynatıcı yanıt vermedi."))??;
                Ok(json!({}))
            }
            "media.artwork" => self.media.artwork(text(p, "id", 256)?).await,
            "commands.list" => Ok(json!({"commands":self.store.saved()?})),
            "commands.save" => {
                require_free(d)?;
                let mut saved: SavedCommand = serde_json::from_value(p.clone()).map_err(|_| {
                    ApiError::new("INVALID_ARGUMENT", "Kayıtlı komut alanları geçersiz.")
                })?;
                if saved.id.is_empty() {
                    saved.id = uuid::Uuid::new_v4().to_string();
                }
                validate_saved(&saved)?;
                if self.store.saved()?.len() >= 100
                    && !self.store.saved()?.iter().any(|s| s.id == saved.id)
                {
                    return Err(ApiError::new("LIMIT", "En fazla 100 kayıtlı komut."));
                }
                self.store.save_command(&saved)?;
                self.commands_changed();
                Ok(json!(saved))
            }
            "commands.delete" => {
                require_free(d)?;
                self.store.delete_command(text(p, "id", 128)?)?;
                self.commands_changed();
                Ok(json!({}))
            }
            "commands.run" => {
                let mut spec = if let Some(id) = p.get("saved_id").and_then(Value::as_str) {
                    Spec::from(
                        self.store
                            .saved()?
                            .into_iter()
                            .find(|s| s.id == id)
                            .ok_or_else(|| {
                                ApiError::new("NOT_FOUND", "Kayıtlı komut bulunamadı.")
                            })?,
                    )
                } else {
                    require_free(d)?;
                    parse_spec(p)?
                };
                spec.keep_history = p
                    .get("keep_history")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                self.jobs.start(&d.id, spec).await
            }
            "jobs.list" => {
                let jobs = self.jobs.list(&d.id)?;
                // Output is fetched separately, bounding a history response to a few KiB.
                Ok(
                    json!({"jobs":jobs.into_iter().map(|mut v|{if let Some(command)=v["spec"]["command"].as_str(){let preview=command.chars().take(256).collect::<String>();v["spec"]["command"]=json!(preview);}v["stdout"]=json!("");v["stderr"]=json!("");v}).collect::<Vec<_>>()}),
                )
            }
            "jobs.get" => self.jobs.get(&d.id, text(p, "id", 128)?),
            "jobs.input" => {
                self.jobs
                    .input(&d.id, text(p, "id", 128)?, text(p, "data", 4096)?)?;
                Ok(json!({}))
            }
            "jobs.delete" => {
                self.jobs.delete(&d.id, text(p, "id", 128)?)?;
                Ok(json!({}))
            }
            "jobs.cancel" => {
                self.jobs.cancel(&d.id, text(p, "id", 128)?)?;
                Ok(json!({}))
            }
            _ => Err(ApiError::new("METHOD_NOT_FOUND", "Bilinmeyen yöntem.")),
        }
    }
    fn clip_changed(&self) {
        let _ = self.events.send(Event {
            name: "clipboard.changed",
            data: json!({}),
            owner: None,
        });
    }
    fn commands_changed(&self) {
        let _ = self.events.send(Event {
            name: "commands.changed",
            data: json!({}),
            owner: None,
        });
    }
}
fn require_free(d: &Device) -> Result<()> {
    if d.free_commands {
        Ok(())
    } else {
        Err(ApiError::new(
            "FORBIDDEN",
            "Serbest komut yetkisi bilgisayardaki CLI üzerinden etkinleştirilmeli.",
        ))
    }
}
pub fn identity(paths: &Paths) -> anyhow::Result<(TlsAcceptor, String, String)> {
    let cert_path = paths.config.join("server.pem");
    let key_path = paths.config.join("server-key.pem");
    if !cert_path.exists() && !key_path.exists() {
        let cert =
            rcgen::generate_simple_self_signed(vec!["cepdesk.local".into(), "localhost".into()])?;
        fs::write(&key_path, cert.key_pair.serialize_pem())?;
        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600))?;
        fs::write(&cert_path, cert.cert.pem())?;
    }
    // Missing one file is an error; never silently change an established trust identity.
    let cert_bytes = fs::read(cert_path)?;
    let key_bytes = fs::read(key_path)?;
    let certs = rustls_pemfile::certs(&mut cert_bytes.as_slice())
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let key: PrivateKeyDer<'static> = rustls_pemfile::private_key(&mut key_bytes.as_slice())?
        .ok_or_else(|| anyhow::anyhow!("TLS anahtarı bulunamadı"))?;
    let fingerprint = hash(
        certs
            .first()
            .ok_or_else(|| anyhow::anyhow!("TLS sertifikası bulunamadı"))?
            .as_ref(),
    );
    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;
    let id_path = paths.config.join("server-id");
    if !id_path.exists() {
        fs::write(&id_path, uuid::Uuid::new_v4().to_string())?;
    }
    Ok((
        TlsAcceptor::from(Arc::new(config)),
        fingerprint,
        fs::read_to_string(id_path)?.trim().into(),
    ))
}
pub fn advertise(app: &App) -> anyhow::Result<ServiceDaemon> {
    let daemon = ServiceDaemon::new()?;
    let instance = format!("CepDesk-{}", &app.server_id[..8]);
    let host = format!("cepdesk-{}.local.", &app.server_id[..8]);
    let properties = [("v", "1"), ("id", app.server_id.as_str())];
    let info = ServiceInfo::new(
        "_cepdesk._tcp.local.",
        &instance,
        &host,
        (),
        app.port,
        &properties[..],
    )?
    .enable_addr_auto();
    daemon.register(info)?;
    *app.discovery.lock().unwrap() = true;
    Ok(daemon)
}
pub async fn serve(app: Arc<App>, acceptor: TlsAcceptor, bind: IpAddr) -> anyhow::Result<()> {
    let listener = TcpListener::bind((bind, app.port)).await?;
    let slots = Arc::new(Semaphore::new(16));
    loop {
        let (stream, peer) = listener.accept().await?;
        if !allowed_peer(peer.ip()) {
            continue;
        }
        let Ok(permit) = slots.clone().try_acquire_owned() else {
            continue;
        };
        let app = app.clone();
        let acceptor = acceptor.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let Ok(Ok(stream)) =
                tokio::time::timeout(Duration::from_secs(5), acceptor.accept(stream)).await
            else {
                return;
            };
            let config = WebSocketConfig::default()
                .max_message_size(Some(MAX_MESSAGE))
                .max_frame_size(Some(MAX_MESSAGE));
            let Ok(Ok(socket)) = tokio::time::timeout(
                Duration::from_secs(5),
                tokio_tungstenite::accept_async_with_config(stream, Some(config)),
            )
            .await
            else {
                return;
            };
            let _ = session(app, socket, peer.to_string()).await;
        });
    }
}
pub struct Rate {
    start: Instant,
    count: u32,
}
impl Default for Rate {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            count: 0,
        }
    }
}
impl Rate {
    pub fn allow(&mut self) -> bool {
        self.allow_limit(30)
    }
    pub fn allow_input(&mut self) -> bool {
        self.allow_limit(180)
    }
    fn allow_limit(&mut self, limit: u32) -> bool {
        if self.start.elapsed() >= Duration::from_secs(1) {
            self.start = Instant::now();
            self.count = 0;
        }
        self.count += 1;
        self.count <= limit
    }
}
async fn session<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    app: Arc<App>,
    mut socket: WebSocketStream<S>,
    peer: String,
) -> anyhow::Result<()> {
    let mut device: Option<Device> = None;
    let mut live: Option<crate::connections::Session> = None;
    let mut audio: Option<crate::system::AudioStream> = None;
    let mut monitor: Option<crate::monitor::Monitor> = None;
    let mut monitor_time = Instant::now();
    let mut input: Option<crate::input::Input> = None;
    let mut input_time = Instant::now();
    let mut input_rate = Rate::default();
    let mut audio_buffer = [0u8; 1920];
    let mut password_challenge = None;
    let mut events = app.events.subscribe();
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    let mut last_message = Instant::now();
    let mut last_ping = Instant::now();
    let created = Instant::now();
    let mut rate = Rate::default();
    let mut seq = 0u64;
    loop {
        tokio::select! {
            _=async { if let Some(session) = &mut live { let _=session.disconnect.changed().await; } else { std::future::pending::<()>().await; } }=>{ let _=socket.close(None).await; break; },
            incoming=socket.next()=>{
                let Some(Ok(message))=incoming else{break;};
                match message {
                    Message::Text(raw)=>{
                        last_message=Instant::now();
                        if let Some(session) = &live { session.touch(); }
                        let req:Request=match serde_json::from_str(&raw){Ok(r)=>r,Err(_)=>{send(&mut socket,protocol::response("",Err(ApiError::new("INVALID_MESSAGE","JSON istek şeması geçersiz.")))).await?;continue;}};
                        let validation=if req.v!=1{Err(ApiError::new("VERSION_UNSUPPORTED","Protokol sürümü 1 olmalı."))}else if req.id.is_empty() || req.id.len()>64 || req.method.len()>64{Err(ApiError::new("INVALID_MESSAGE","İstek kimliği/yöntemi geçersiz."))}else if !(if (req.method.starts_with("input.") || req.method=="monitor.click" || req.method=="monitor.scroll") && device.is_some() { input_rate.allow_input() } else { rate.allow() }){Err(ApiError::new("RATE_LIMITED","İstek sınırı aşıldı (giriş: 180/s, diğer: 30/s)."))}else{Ok(())};
                        let result=if let Err(e)=validation{Err(e)}else if let Some(d)=&device {
                            match app.store.device(&d.id){Ok(current)=>{device=Some(current.clone());
                                match req.method.as_str() {
                                    method if method.starts_with("input.") => {
                                        input_time=Instant::now();
                                        if input.is_none() {
                                            match crate::input::Input::new() {
                                                Ok(created)=>{input=Some(created);tokio::time::sleep(Duration::from_millis(200)).await;},
                                                Err(e)=>{send(&mut socket,protocol::response(&req.id,Err(e))).await?;continue;}
                                            }
                                        }
                                        input.as_mut().unwrap().request(method,&req.params)
                                    },
                                    "monitor.start"=>{
                                        monitor=None;
                                        match crate::monitor::Monitor::start(req.params["mode"].as_str().unwrap_or("").to_string(), req.params["binary"].as_bool().unwrap_or(false), req.params["codec"].as_str()==Some("h264"), app.desktop.windows().into_iter().find(|w|w.active).map(|w|(w.app_id,w.title))).await {
                                            Ok(stream)=>{monitor_time=Instant::now();let result=stream.frame();monitor=Some(stream);result},Err(e)=>Err(e)
                                        }
                                    },
                                    "monitor.keepalive"=>{monitor_time=Instant::now();if monitor.as_ref().is_some_and(|m|req.params["stream_id"].as_str()==Some(m.id.as_str())){Ok(json!({"active":true}))}else{Err(ApiError::new("MONITOR_STOPPED","Yayın durdu."))}},
                                    "monitor.frame"=>{monitor_time=Instant::now();monitor.as_ref().ok_or_else(||ApiError::new("MONITOR_STOPPED","Önce bir yayın kaynağı seçin.")).and_then(|m| if req.params["stream_id"].as_str()==Some(m.id.as_str()){m.frame()}else{Err(ApiError::new("MONITOR_STOPPED","Yayın kimliği değişti."))})},
                                    "monitor.stop"=>{if monitor.as_ref().is_some_and(|m|req.params["stream_id"].as_str()==Some(m.id.as_str())){monitor=None;}Ok(json!({"stopped":true}))},
                                    "monitor.click" | "monitor.scroll"=>{monitor_time=Instant::now();if let Some(m)=&monitor{m.control(req.params.clone(),req.method=="monitor.scroll").await}else{Err(ApiError::new("MONITOR_STOPPED","Yayın durdu."))}},
                                    "system.audio.start"=>match crate::system::AudioStream::start().await {Ok(stream)=>{audio=Some(stream);Ok(json!({"rate":24000,"channels":2,"format":"s16le"}))},Err(e)=>Err(e)},
                                    "system.audio.stop"=>{audio=None;Ok(json!({"stopped":true}))},
                                    _=>app.request(&current,&req).await
                                }
                            },Err(e)=>Err(e)}
                        }else{
                            match req.method.as_str() {
                                "auth"=>match text(&req.params,"token",128).and_then(|t|app.store.authenticate(t)) {Ok(d)=>{let result=json!({"server_id":app.server_id,"device":d,"capabilities":app.capabilities(&d),"settings":app.store.settings()?});live=Some(app.connections.add(&d.id, &peer));device=Some(d);Ok(result)},Err(e)=>Err(e)},
                                "password.hello"=>app.password.hello(&req.params, &app.server_id, &app.fingerprint, &mut password_challenge),
                                "password.pair"=>app.password.pair(&req.params, &app.store, &mut password_challenge),
                                "pair.request"=>preauth_pair(&app,&req),
                                "pair.poll"=>preauth_pair(&app,&req),
                                _=>Err(ApiError::new("UNAUTHORIZED","Önce cihazı eşleştirin ve kimlik doğrulayın.")),
                            }
                        };
                        let close=result.as_ref().is_err_and(|e|e.code=="UNAUTHORIZED");
                        send(&mut socket,protocol::response(&req.id,result)).await?;
                        if close{break;}
                    },
                    Message::Close(_)=>break,
                    Message::Ping(data)=>{last_message=Instant::now();if let Some(session)=&live {session.touch();}socket.send(Message::Pong(data)).await?;},
                    Message::Pong(_)=>{last_message=Instant::now();if let Some(session)=&live {session.touch();}},
                    Message::Binary(_)=>{send(&mut socket,protocol::response("",Err(ApiError::new("INVALID_MESSAGE","İkili mesajlar desteklenmiyor.")))).await?;},
                    _=>{},
                }
            },
            event=events.recv()=>{
                if let Some(d)=&device {
                    if app.store.device(&d.id).is_err(){break;}
                    match event {
                        Ok(e) if e.owner.as_ref().is_none_or(|owner|owner==&d.id)=>{
                            seq+=1;
                            // Window icons and command output are fetched on demand to bound events.
                            let data=if e.name=="capabilities.changed"{app.capabilities(&app.store.device(&d.id)?)}else if e.name=="windows.changed"{json!({})}else if e.name=="job.changed"{json!({"id":e.data["id"],"status":e.data["status"]})}else{e.data};
                            send(&mut socket,json!({"v":1,"event":e.name,"seq":seq,"data":data})).await?;
                        },
                        Err(broadcast::error::RecvError::Lagged(_))=>{seq+=1;send(&mut socket,json!({"v":1,"event":"resync.required","seq":seq,"data":{}})).await?;},
                        _=>{},
                    }
                }
            },
            video=async {
                match &mut monitor {
                    Some(stream) if stream.video => stream.video_frames.recv().await,
                    Some(stream) if stream.binary => {
                        if stream.frames.changed().await.is_ok() { stream.frames.borrow_and_update().clone() } else { None }
                    },
                    _ => std::future::pending().await,
                }
            }=>{
                if device.as_ref().is_none_or(|d|app.store.device(&d.id).is_err()){break;}
                if let Some(frame) = video {
                    tokio::time::timeout(Duration::from_secs(2),socket.send(Message::Binary(frame.packet().into()))).await??;
                } else {
                    let reason=monitor.as_ref().and_then(|stream|stream.frame().err()).map(|e|e.message).unwrap_or_else(||"Yayın durdu.".into());
                    monitor=None;seq+=1;send(&mut socket,json!({"v":1,"event":"monitor.stopped","seq":seq,"data":{"message":reason}})).await?;
                }
            },
            chunk=async { match &mut audio {Some(stream)=>stream.read(&mut audio_buffer).await,None=>std::future::pending().await} }=>{
                if device.as_ref().is_none_or(|d|app.store.device(&d.id).is_err()){break;}
                match chunk {
                    Ok(n) if n>0=>{tokio::time::timeout(Duration::from_secs(2),socket.send(Message::Binary(audio_buffer[..n].to_vec().into()))).await??;},
                    _=>{audio=None;seq+=1;send(&mut socket,json!({"v":1,"event":"system.audio.stopped","seq":seq,"data":{}})).await?;}
                }
            },
            _=tick.tick()=>{
                if monitor_time.elapsed()>Duration::from_secs(15){monitor=None;}
                if input_time.elapsed()>Duration::from_secs(10) { if let Some(i)=&mut input { let _=i.release(); } }
                if device.is_some() && last_message.elapsed()>Duration::from_secs(90) { let _=socket.close(None).await;break; }
                if device.is_some() && last_ping.elapsed()>Duration::from_secs(15) { socket.send(Message::Ping(Vec::new().into())).await?;last_ping=Instant::now(); }
                if let Some(d)=&device{if app.store.device(&d.id).is_err(){let _=socket.close(None).await;break;}}
                if device.is_none() && (last_message.elapsed()>Duration::from_secs(30) || created.elapsed()>Duration::from_secs(210)){break;}
            }
        }
    }
    Ok(())
}
pub(crate) fn allowed_peer(ip: IpAddr) -> bool {
    matches!(ip,IpAddr::V4(ip) if ip.is_private() || ip.is_loopback() || ip.is_link_local())
}
async fn send<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    socket: &mut WebSocketStream<S>,
    value: Value,
) -> anyhow::Result<()> {
    let mut raw = value.to_string();
    if raw.len() > 1024 * 1024 {
        raw = protocol::response(
            value["id"].as_str().unwrap_or(""),
            Err(ApiError::new(
                "RESPONSE_TOO_LARGE",
                "Yanıt 1 MiB sınırını aşıyor; daha küçük bir liste veya içerik isteyin.",
            )),
        )
        .to_string();
    }
    tokio::time::timeout(
        Duration::from_secs(5),
        socket.send(Message::Text(raw.into())),
    )
    .await??;
    Ok(())
}
fn preauth_pair(app: &App, req: &Request) -> Result<Value> {
    match req.method.as_str() {
        "pair.request" => app.pairing.request(
            text(&req.params, "code", 8)?,
            text(&req.params, "name", 120)?,
        ),
        "pair.poll" => app.pairing.poll(
            text(&req.params, "pending_id", 128)?,
            text(&req.params, "poll_secret", 128)?,
        ),
        _ => Err(ApiError::new(
            "METHOD_NOT_FOUND",
            "Bilinmeyen eşleştirme yöntemi.",
        )),
    }
}
