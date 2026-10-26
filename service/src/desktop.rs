//! Session adapters: a real Wayland client, never X11/process-list fallbacks.
use crate::{
    protocol::{ApiError, Event, Result},
    store::Store,
};
use serde::Serialize;
use serde_json::json;
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    os::fd::{AsFd, AsRawFd},
    sync::{mpsc, Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::{broadcast, oneshot};
use wayland_client::{
    protocol::{wl_registry, wl_seat},
    Connection, Dispatch, Proxy, QueueHandle,
};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1 as device, zwlr_data_control_manager_v1 as manager,
    zwlr_data_control_offer_v1 as offer, zwlr_data_control_source_v1 as source,
};

#[allow(dead_code, unused_imports, clippy::all)]
pub(crate) mod workspace {
    use wayland_client;
    use wayland_client::protocol::*;
    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/cosmic-workspace-unstable-v1-compat.xml");
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("protocols/cosmic-workspace-unstable-v1-compat.xml");
}
#[allow(dead_code, unused_imports, clippy::all)]
mod info {
    use super::workspace::*;
    use wayland_client;
    use wayland_client::protocol::*;
    pub mod __interfaces {
        use super::super::workspace::__interfaces::*;
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!(
            "protocols/cosmic-toplevel-info-unstable-v1-compat.xml"
        );
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("protocols/cosmic-toplevel-info-unstable-v1-compat.xml");
}
#[allow(dead_code, unused_imports, clippy::all)]
mod management {
    use super::info::*;
    use wayland_client;
    use wayland_client::protocol::*;
    pub mod __interfaces {
        use super::super::info::__interfaces::*;
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!(
            "protocols/cosmic-toplevel-management-unstable-v1-compat.xml"
        );
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!(
        "protocols/cosmic-toplevel-management-unstable-v1-compat.xml"
    );
}
use info::{zcosmic_toplevel_handle_v1 as handle, zcosmic_toplevel_info_v1 as top_info};
use management::zcosmic_toplevel_manager_v1 as top_manager;

#[derive(Clone, Debug, Serialize, Default, PartialEq)]
pub struct Features {
    pub clipboard_watch: bool,
    pub clipboard_write: bool,
    pub window_list: bool,
    pub window_activate: bool,
    pub window_close: bool,
    pub reason: String,
    pub clipboard_notice: String,
}
#[derive(Clone, Debug, Serialize, Default)]
pub struct Window {
    pub id: String,
    pub app_id: String,
    pub app_name: String,
    pub title: String,
    pub active: bool,
    pub icon: Option<String>,
}
#[derive(Default)]
struct Snapshot {
    features: Features,
    windows: Vec<Window>,
}
enum Command {
    Write(String, oneshot::Sender<Result<()>>),
    Activate(String, oneshot::Sender<Result<()>>),
    Close(String, oneshot::Sender<Result<()>>),
}
pub struct Desktop {
    tx: mpsc::Sender<Command>,
    snapshot: Arc<Mutex<Snapshot>>,
}
impl Desktop {
    #[cfg(test)]
    pub fn unavailable() -> Self {
        let (tx, _rx) = mpsc::channel();
        let snapshot = Arc::new(Mutex::new(Snapshot {
            features: Features {
                reason: "Test ortamında Wayland yok".into(),
                ..Default::default()
            },
            windows: vec![],
        }));
        Self { tx, snapshot }
    }
    pub fn start(store: Arc<Store>, events: broadcast::Sender<Event>) -> Self {
        let (tx, rx) = mpsc::channel();
        let snapshot = Arc::new(Mutex::new(Snapshot::default()));
        let shared = snapshot.clone();
        std::thread::spawn(move || loop {
            if run(&rx, shared.clone(), store.clone(), events.clone()).is_err() {
                let mut s = shared.lock().unwrap();
                s.features=Features{reason:"Wayland bağlantısı yok veya protokoller erişilemiyor; oturum değişkenlerini ve COSMIC veri denetimi ayarını kontrol edin.".into(),..Default::default()};
                s.windows.clear();
                let _ = events.send(Event {
                    name: "capabilities.changed",
                    data: json!(s.features),
                    owner: None,
                });
            }
            // Fail requests while disconnected instead of falsely succeeding or queueing forever.
            for _ in 0..60 {
                while let Ok(command) = rx.try_recv() {
                    match command {
                        Command::Write(_, reply)
                        | Command::Activate(_, reply)
                        | Command::Close(_, reply) => {
                            let _ = reply.send(Err(unsupported()));
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        });
        Self { tx, snapshot }
    }
    pub fn features(&self) -> Features {
        self.snapshot.lock().unwrap().features.clone()
    }
    pub fn windows(&self) -> Vec<Window> {
        self.snapshot.lock().unwrap().windows.clone()
    }
    pub async fn write(&self, content: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Command::Write(content, tx))
            .map_err(|_| unsupported())?;
        tokio::time::timeout(Duration::from_secs(4), rx)
            .await
            .map_err(|_| ApiError::new("TIMEOUT", "Pano sunucusu yanıt vermedi."))?
            .map_err(|_| unsupported())?
    }
    pub async fn close(&self, id: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Command::Close(id, tx))
            .map_err(|_| unsupported())?;
        tokio::time::timeout(Duration::from_secs(4), rx)
            .await
            .map_err(|_| ApiError::new("TIMEOUT", "Pencere kapatma isteği yanıt vermedi."))?
            .map_err(|_| unsupported())?
    }
    pub async fn activate(&self, id: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Command::Activate(id, tx))
            .map_err(|_| unsupported())?;
        tokio::time::timeout(Duration::from_secs(4), rx)
            .await
            .map_err(|_| ApiError::new("TIMEOUT", "Pencere etkinleştirmesi doğrulanamadı."))?
            .map_err(|_| unsupported())?
    }
}
fn unsupported() -> ApiError {
    ApiError::new(
        "UNSUPPORTED",
        "Bu oturum ilgili Wayland yeteneğini sunmuyor.",
    )
}
struct Row {
    proxy: handle::ZcosmicToplevelHandleV1,
    window: Window,
}
struct State {
    seat: Option<wl_seat::WlSeat>,
    clipboard_manager: Option<manager::ZwlrDataControlManagerV1>,
    clipboard: Option<device::ZwlrDataControlDeviceV1>,
    source: Option<source::ZwlrDataControlSourceV1>,
    toplevel: Option<top_info::ZcosmicToplevelInfoV1>,
    manager: Option<top_manager::ZcosmicToplevelManagerV1>,
    offers: HashMap<wayland_client::backend::ObjectId, Vec<String>>,
    rows: HashMap<wayland_client::backend::ObjectId, Row>,
    pending: Vec<(String, Instant, oneshot::Sender<Result<()>>)>,
    shared: Arc<Mutex<Snapshot>>,
    store: Arc<Store>,
    events: broadcast::Sender<Event>,
    read_tx: mpsc::Sender<(u64, std::result::Result<String, &'static str>)>,
    read_workers: Arc<tokio::sync::Semaphore>,
    write_workers: Arc<tokio::sync::Semaphore>,
    serial: u64,
    epoch: String,
}
impl State {
    fn publish(&self) {
        let mut shared = self.shared.lock().unwrap();
        shared.windows = self
            .rows
            .values()
            .map(|r| r.window.clone())
            .take(500)
            .collect();
        shared
            .windows
            .sort_by(|a, b| a.app_name.cmp(&b.app_name).then(a.id.cmp(&b.id)));
        let _ = self.events.send(Event {
            name: "windows.changed",
            data: json!({}),
            owner: None,
        });
    }
    fn notice(&self, notice: &str) {
        self.shared.lock().unwrap().features.clipboard_notice = notice.into();
        let _ = self.events.send(Event {
            name: "clipboard.notice",
            data: json!({"message":notice}),
            owner: None,
        });
    }
}
fn run(
    rx: &mpsc::Receiver<Command>,
    shared: Arc<Mutex<Snapshot>>,
    store: Arc<Store>,
    events: broadcast::Sender<Event>,
) -> anyhow::Result<()> {
    let connection = Connection::connect_to_env()?;
    let mut queue = connection.new_event_queue();
    let qh = queue.handle();
    let (read_tx, read_rx) = mpsc::channel();
    let mut state = State {
        seat: None,
        clipboard_manager: None,
        clipboard: None,
        source: None,
        toplevel: None,
        manager: None,
        offers: HashMap::new(),
        rows: HashMap::new(),
        pending: vec![],
        shared,
        store,
        events,
        read_tx,
        read_workers: Arc::new(tokio::sync::Semaphore::new(4)),
        write_workers: Arc::new(tokio::sync::Semaphore::new(4)),
        serial: 0,
        epoch: uuid::Uuid::new_v4().to_string(),
    };
    let _registry = connection.display().get_registry(&qh, ());
    queue.roundtrip(&mut state)?;
    if let (Some(manager), Some(seat)) = (&state.clipboard_manager, &state.seat) {
        state.clipboard = Some(manager.get_data_device(seat, &qh, ()));
    }
    queue.roundtrip(&mut state)?;
    {
        let mut s = state.shared.lock().unwrap();
        s.features.clipboard_watch = state.clipboard.is_some();
        s.features.clipboard_write = state.clipboard.is_some();
        s.features.window_list = state.toplevel.is_some();
        s.features.reason="Eksik yetenekler: COSMIC toplevel bilgi/yönetim veya zwlr_data_control protokolü oturum tarafından sunulmalı.".into();
        let _ = state.events.send(Event {
            name: "capabilities.changed",
            data: json!(s.features),
            owner: None,
        });
    }
    loop {
        queue.dispatch_pending(&mut state)?;
        while let Ok((serial, result)) = read_rx.try_recv() {
            if serial != state.serial {
                continue;
            }
            match result {
                Ok(text) => {
                    state.notice("");
                    match state.store.record(&text) {
                        Ok(true) => {
                            let _ = state.events.send(Event {
                                name: "clipboard.changed",
                                data: json!({}),
                                owner: None,
                            });
                        }
                        Ok(false) => {}
                        Err(e) => state.notice(&e.message),
                    }
                }
                Err(reason) => state.notice(reason),
            }
        }
        while let Ok(command) = rx.try_recv() {
            match command {
                Command::Write(text, reply) => {
                    if let (Some(manager), Some(device)) =
                        (&state.clipboard_manager, &state.clipboard)
                    {
                        let source = manager.create_data_source(&qh, Arc::new(text.into_bytes()));
                        source.offer("text/plain;charset=utf-8".into());
                        source.offer("text/plain".into());
                        device.set_selection(Some(&source));
                        if let Some(old) = state.source.replace(source) {
                            if old.is_alive() {
                                old.destroy();
                            }
                        }
                        let result = queue
                            .roundtrip(&mut state)
                            .map(|_| ())
                            .map_err(|_| unsupported());
                        let _ = reply.send(result);
                    } else {
                        let _ = reply.send(Err(unsupported()));
                    }
                }
                Command::Close(id, reply) => {
                    let result = if !state.shared.lock().unwrap().features.window_close {
                        Err(unsupported())
                    } else if let (Some(row), Some(manager)) = (
                        state.rows.values().find(|r| r.window.id == id),
                        &state.manager,
                    ) {
                        manager.close(&row.proxy);
                        Ok(())
                    } else {
                        Err(ApiError::new("NOT_FOUND", "Pencere artık mevcut değil."))
                    };
                    let _ = reply.send(result);
                }
                Command::Activate(id, reply) => {
                    let row = state.rows.values().find(|r| r.window.id == id);
                    match (row, &state.manager, &state.seat) {
                        (Some(row), Some(manager), Some(seat))
                            if state.shared.lock().unwrap().features.window_activate =>
                        {
                            if row.window.active {
                                let _ = reply.send(Ok(()));
                            } else {
                                manager.activate(&row.proxy, seat);
                                state.pending.push((
                                    id,
                                    Instant::now() + Duration::from_secs(2),
                                    reply,
                                ));
                            }
                        }
                        (None, _, _) => {
                            let _ = reply.send(Err(ApiError::new(
                                "NOT_FOUND",
                                "Pencere kapatılmış olabilir.",
                            )));
                        }
                        _ => {
                            let _ = reply.send(Err(unsupported()));
                        }
                    }
                }
            }
        }
        let mut waiting = vec![];
        for (id, deadline, reply) in state.pending.drain(..) {
            if state
                .rows
                .values()
                .any(|r| r.window.id == id && r.window.active)
            {
                let _ = reply.send(Ok(()));
            } else if Instant::now() > deadline {
                let _ = reply.send(Err(ApiError::new(
                    "ACTIVATION_DENIED",
                    "COSMIC etkinleştirme isteğini kabul etmedi veya pencere kapandı.",
                )));
            } else {
                waiting.push((id, deadline, reply));
            }
        }
        state.pending = waiting;
        connection.flush()?;
        if let Some(guard) = queue.prepare_read() {
            let mut fd = libc::pollfd {
                fd: guard.connection_fd().as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: poll receives a valid, live descriptor and one initialized pollfd.
            let ready = unsafe { libc::poll(&mut fd, 1, 50) };
            if ready > 0 {
                guard.read()?;
            } else {
                drop(guard);
            }
        }
    }
}
impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        s: &mut Self,
        r: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name, interface, ..
        } = event
        {
            match interface.as_str() {
                "wl_seat" if s.seat.is_none() => s.seat = Some(r.bind(name, 1, qh, ())),
                "zwlr_data_control_manager_v1" => {
                    s.clipboard_manager = Some(r.bind(name, 1, qh, ()))
                }
                "zcosmic_toplevel_info_v1" => s.toplevel = Some(r.bind(name, 1, qh, ())),
                "zcosmic_toplevel_manager_v1" => s.manager = Some(r.bind(name, 1, qh, ())),
                _ => {}
            }
        }
    }
}
impl Dispatch<device::ZwlrDataControlDeviceV1, ()> for State {
    fn event(
        s: &mut Self,
        _: &device::ZwlrDataControlDeviceV1,
        event: device::Event,
        _: &(),
        conn: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            device::Event::DataOffer { id } => {
                s.offers.insert(id.id(), vec![]);
            }
            device::Event::Selection { id } => {
                s.serial += 1;
                if let Some(offer) = id {
                    let types = s.offers.remove(&offer.id()).unwrap_or_default();
                    let sensitive = types.iter().any(|m| {
                        m == "x-kde-passwordManagerHint" || m == "application/x-keepassxc-private"
                    });
                    let mime = ["text/plain;charset=utf-8", "text/plain"]
                        .into_iter()
                        .find(|m| types.iter().any(|t| t == m));
                    if sensitive {
                        s.notice("Hassas içerik işareti bulunan pano kaydedilmedi.");
                    } else if s.store.settings().is_ok_and(|v| !v.paused) {
                        if let Some(mime) = mime {
                            if let Ok((reader, writer)) = std::os::unix::net::UnixStream::pair() {
                                let Ok(guard) = s.read_workers.clone().try_acquire_owned() else {
                                    s.notice("Pano aktarım sınırına ulaşıldı; bu seçim atlandı.");
                                    offer.destroy();
                                    return;
                                };
                                offer.receive(mime.into(), writer.as_fd());
                                let _ = conn.flush();
                                drop(writer);
                                let tx = s.read_tx.clone();
                                let serial = s.serial;
                                let limit = s
                                    .store
                                    .settings()
                                    .map(|v| v.max_item_bytes)
                                    .unwrap_or(65536);
                                // At most current transfers can record; stale selections are discarded.
                                std::thread::spawn(move || {
                                    let _guard = guard;
                                    let value = read_text(reader, limit);
                                    let _ = tx.send((serial, value));
                                });
                            }
                        } else {
                            s.notice("Bu pano içeriği metin değil. İlk sürüm yalnızca UTF-8 metni kaydeder.");
                        }
                    }
                    offer.destroy();
                }
                // Unselected offers must be released, including primary-selection offers.
                for (id, _) in s.offers.drain() {
                    if let Ok(o) = offer::ZwlrDataControlOfferV1::from_id(conn, id) {
                        o.destroy();
                    }
                }
            }
            device::Event::Finished => {
                s.clipboard = None;
                let mut v = s.shared.lock().unwrap();
                v.features.clipboard_watch = false;
                v.features.clipboard_write = false;
                let _ = s.events.send(Event {
                    name: "capabilities.changed",
                    data: json!(v.features),
                    owner: None,
                });
            }
            _ => {}
        }
    }
    wayland_client::event_created_child!(State,device::ZwlrDataControlDeviceV1,[0=>(offer::ZwlrDataControlOfferV1,())]);
}
impl Dispatch<offer::ZwlrDataControlOfferV1, ()> for State {
    fn event(
        s: &mut Self,
        o: &offer::ZwlrDataControlOfferV1,
        event: offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let offer::Event::Offer { mime_type } = event {
            s.offers.entry(o.id()).or_default().push(mime_type);
        }
    }
}
impl Dispatch<source::ZwlrDataControlSourceV1, Arc<Vec<u8>>> for State {
    fn event(
        s: &mut Self,
        source: &source::ZwlrDataControlSourceV1,
        event: source::Event,
        bytes: &Arc<Vec<u8>>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            source::Event::Send { fd, .. } => {
                let Ok(guard) = s.write_workers.clone().try_acquire_owned() else {
                    return;
                };
                let bytes = bytes.clone();
                std::thread::spawn(move || {
                    let _guard = guard;
                    // Nonblocking writes with a deadline prevent a stalled client holding a thread forever.
                    let mut file = fs::File::from(fd);
                    let deadline = Instant::now() + Duration::from_secs(2);
                    let mut offset = 0;
                    // SAFETY: fcntl operates on the live file owned by this thread.
                    unsafe {
                        let flags = libc::fcntl(file.as_raw_fd(), libc::F_GETFL);
                        libc::fcntl(file.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK);
                    }
                    while offset < bytes.len() && Instant::now() < deadline {
                        match file.write(&bytes[offset..]) {
                            Ok(0) => break,
                            Ok(n) => offset += n,
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                std::thread::sleep(Duration::from_millis(5))
                            }
                            Err(_) => break,
                        }
                    }
                });
            }
            source::Event::Cancelled => source.destroy(),
            _ => {}
        }
    }
}
fn read_text(
    mut reader: std::os::unix::net::UnixStream,
    limit: usize,
) -> std::result::Result<String, &'static str> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut bytes = vec![];
    let mut buffer = [0u8; 4096];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("Pano kaynağı zamanında yanıt vermedi.");
        }
        reader
            .set_read_timeout(Some(remaining))
            .map_err(|_| "Pano aktarımı başlatılamadı.")?;
        let n = reader
            .read(&mut buffer)
            .map_err(|_| "Pano kaynağı zamanında yanıt vermedi.")?;
        if n == 0 {
            break;
        }
        if bytes.len() + n > limit {
            return Err("İçerik pano boyut sınırını aşıyor.");
        }
        bytes.extend_from_slice(&buffer[..n]);
    }
    String::from_utf8(bytes).map_err(|_| "Pano metni UTF-8 değil; dönüştürülmeden atlandı.")
}
impl Dispatch<top_info::ZcosmicToplevelInfoV1, ()> for State {
    fn event(
        s: &mut Self,
        _: &top_info::ZcosmicToplevelInfoV1,
        event: top_info::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            top_info::Event::Toplevel { toplevel } => {
                let id = format!("{}:{}", s.epoch, toplevel.id().protocol_id());
                s.rows.insert(
                    toplevel.id(),
                    Row {
                        proxy: toplevel,
                        window: Window {
                            id,
                            ..Default::default()
                        },
                    },
                );
            }
            top_info::Event::Finished => {
                s.toplevel = None;
                s.rows.clear();
                s.shared.lock().unwrap().features.window_list = false;
                s.publish();
            }
        }
    }
    wayland_client::event_created_child!(State,top_info::ZcosmicToplevelInfoV1,[0=>(handle::ZcosmicToplevelHandleV1,())]);
}
impl Dispatch<handle::ZcosmicToplevelHandleV1, ()> for State {
    fn event(
        s: &mut Self,
        h: &handle::ZcosmicToplevelHandleV1,
        event: handle::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let Some(row) = s.rows.get_mut(&h.id()) {
            match event {
                handle::Event::Title { title } => {
                    row.window.title = title.chars().take(1024).collect()
                }
                handle::Event::AppId { app_id } => {
                    let (name, icon) = app_info(&app_id);
                    row.window.app_id = app_id;
                    row.window.app_name = name;
                    row.window.icon = icon;
                }
                handle::Event::State { state } => {
                    row.window.active = state
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .any(|v| u32::from_ne_bytes(*v) == 2)
                }
                handle::Event::Closed => {
                    s.rows.remove(&h.id());
                    h.destroy();
                    s.publish();
                }
                handle::Event::Done => s.publish(),
                _ => {}
            }
        }
    }
}
impl Dispatch<top_manager::ZcosmicToplevelManagerV1, ()> for State {
    fn event(
        s: &mut Self,
        _: &top_manager::ZcosmicToplevelManagerV1,
        event: top_manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let top_manager::Event::Capabilities { capabilities } = event;
        {
            let mut v = s.shared.lock().unwrap();
            v.features.window_close = capabilities
                .as_chunks::<4>()
                .0
                .iter()
                .any(|b| u32::from_ne_bytes(*b) == 1);
            v.features.window_activate = s.seat.is_some()
                && capabilities
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|b| u32::from_ne_bytes(*b) == 2);
            let _ = s.events.send(Event {
                name: "capabilities.changed",
                data: json!(v.features),
                owner: None,
            });
        }
    }
}
wayland_client::delegate_noop!(State: ignore manager::ZwlrDataControlManagerV1);
wayland_client::delegate_noop!(State: ignore wl_seat::WlSeat);
fn app_info(app: &str) -> (String, Option<String>) {
    (crate::applications::name(app), None)
}
