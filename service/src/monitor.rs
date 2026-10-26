//! Explicit, session-owned COSMIC capture. Latest-frame storage never builds a video backlog.
use crate::protocol::{ApiError, Result};
use anyhow::Context;
use base64::Engine;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    os::fd::{AsFd, AsRawFd, FromRawFd},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};
use wayland_client::{
    protocol::{wl_buffer, wl_output, wl_registry, wl_shm, wl_shm_pool},
    Connection, Dispatch, Proxy, QueueHandle, WEnum,
};
use wayland_protocols::ext::{
    foreign_toplevel_list::v1::client::{
        ext_foreign_toplevel_handle_v1 as foreign, ext_foreign_toplevel_list_v1 as list,
    },
    image_capture_source::v1::client::{
        ext_foreign_toplevel_image_capture_source_manager_v1 as tops,
        ext_image_capture_source_v1 as source,
        ext_output_image_capture_source_manager_v1 as outputs,
    },
    image_copy_capture::v1::client::{
        ext_image_copy_capture_frame_v1 as frame, ext_image_copy_capture_manager_v1 as copy,
        ext_image_copy_capture_session_v1 as session,
    },
};
use wayland_protocols::xdg::xdg_output::zv1::client::{
    zxdg_output_manager_v1 as output_manager, zxdg_output_v1 as logical_output,
};
#[allow(dead_code, unused_imports, clippy::all)]
mod info {
    use crate::desktop::workspace::*;
    use wayland_client;
    use wayland_client::protocol::*;
    use wayland_protocols::ext::foreign_toplevel_list::v1::client::*;
    pub mod __interfaces {
        use crate::desktop::workspace::__interfaces::*;
        use wayland_client::protocol::__interfaces::*;
        use wayland_protocols::ext::foreign_toplevel_list::v1::client::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/cepdesk-toplevel-info-v2.xml");
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("protocols/cepdesk-toplevel-info-v2.xml");
}
#[allow(dead_code, unused_imports, clippy::all)]
mod management {
    use super::info::*;
    use crate::desktop::workspace::*;
    use wayland_client;
    use wayland_client::protocol::*;
    pub mod __interfaces {
        use super::super::info::__interfaces::*;
        use crate::desktop::workspace::__interfaces::*;
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
use info::{zcosmic_toplevel_handle_v1 as handle, zcosmic_toplevel_info_v1 as cosmic};
use management::zcosmic_toplevel_manager_v1 as management_api;
fn failure(message: &str) -> ApiError {
    ApiError::new("MONITOR_UNAVAILABLE", message)
}
#[derive(Clone, Default)]
struct Geometry {
    output: u32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}
#[derive(Default)]
struct Output {
    proxy: Option<wl_output::WlOutput>,
    name: String,
    width: i32,
    height: i32,
}
struct Window {
    proxy: foreign::ExtForeignToplevelHandleV1,
    cosmic: Option<handle::ZcosmicToplevelHandleV1>,
    title: String,
    app_id: String,
    initialized: bool,
    active: bool,
    geometry: Option<Geometry>,
}
struct State {
    focused: Option<u32>,
    capture_epoch: u64,
    seat: Option<wayland_client::protocol::wl_seat::WlSeat>,
    manager: Option<management_api::ZcosmicToplevelManagerV1>,
    activation: bool,
    shm: Option<wl_shm::WlShm>,
    copy: Option<copy::ExtImageCopyCaptureManagerV1>,
    outputs_manager: Option<outputs::ExtOutputImageCaptureSourceManagerV1>,
    tops: Option<tops::ExtForeignToplevelImageCaptureSourceManagerV1>,
    cosmic: Option<cosmic::ZcosmicToplevelInfoV1>,
    logical_manager: Option<output_manager::ZxdgOutputManagerV1>,
    outputs: HashMap<u32, Output>,
    windows: HashMap<u32, Window>,
    width: u32,
    height: u32,
    formats: Vec<wl_shm::Format>,
    constraints: bool,
    ready: bool,
    stopped: bool,
    failed: bool,
    transform: u32,
}
impl Default for State {
    fn default() -> Self {
        Self {
            focused: None,
            capture_epoch: 1,
            seat: None,
            manager: None,
            activation: false,
            shm: None,
            copy: None,
            outputs_manager: None,
            tops: None,
            cosmic: None,
            logical_manager: None,
            outputs: HashMap::new(),
            windows: HashMap::new(),
            width: 0,
            height: 0,
            formats: vec![],
            constraints: false,
            ready: false,
            stopped: false,
            failed: false,
            transform: 0,
        }
    }
}
pub struct CapturedFrame {
    pub meta: Value,
    pub data: Vec<u8>,
}
impl CapturedFrame {
    pub fn packet(&self) -> Vec<u8> {
        let header = self.meta.to_string().into_bytes();
        let mut packet = Vec::with_capacity(8 + header.len() + self.data.len());
        packet.extend_from_slice(if self.meta["codec"] == "h264" {
            b"CDV1"
        } else {
            b"CDM2"
        });
        packet.extend_from_slice(&(header.len() as u32).to_be_bytes());
        packet.extend_from_slice(&header);
        packet.extend_from_slice(&self.data);
        packet
    }
}
type Latest = Arc<Mutex<Result<Arc<CapturedFrame>>>>;
type Reply = tokio::sync::oneshot::Sender<Result<Value>>;
struct Control {
    params: Value,
    scroll: bool,
    reply: Reply,
}
pub struct Monitor {
    pub id: String,
    latest: Latest,
    pub frames: tokio::sync::watch::Receiver<Option<Arc<CapturedFrame>>>,
    pub binary: bool,
    pub video: bool,
    pub video_frames: tokio::sync::mpsc::Receiver<Arc<CapturedFrame>>,
    tx: mpsc::Sender<Control>,
    stop: Arc<AtomicBool>,
}
impl Monitor {
    pub async fn start(
        mode: String,
        binary: bool,
        video: bool,
        hint: Option<(String, String)>,
    ) -> Result<Self> {
        if video && !binary {
            return Err(ApiError::new(
                "INVALID_ARGUMENT",
                "H.264 ikili aktarım gerektirir.",
            ));
        }
        if mode != "screen" && mode != "window" {
            return Err(ApiError::new(
                "INVALID_ARGUMENT",
                "Kaynak screen veya window olmalı.",
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let latest = Arc::new(Mutex::new(Err(failure("İlk görüntü bekleniyor."))));
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let (frames_tx, frames) = tokio::sync::watch::channel(None);
        let (video_tx, video_frames) = tokio::sync::mpsc::channel(3);
        let (ready, wait) = tokio::sync::oneshot::channel();
        let monitor = Self {
            id: id.clone(),
            latest: latest.clone(),
            tx,
            stop: stop.clone(),
            frames,
            binary,
            video,
            video_frames,
        };
        std::thread::spawn(move || {
            let mut ready = Some(ready);
            let result = run(
                &id,
                &mode,
                binary,
                video.then_some(&video_tx),
                hint,
                &frames_tx,
                &latest,
                &stop,
                &rx,
                &mut ready,
            );
            if let Err(error) = result {
                let error = failure(&error.to_string());
                *latest.lock().unwrap() = Err(error.clone());
                if let Some(reply) = ready.take() {
                    let _ = reply.send(Err(error));
                }
            }
        });
        match tokio::time::timeout(Duration::from_secs(8), wait).await {
            Ok(Ok(Ok(()))) => Ok(monitor),
            Ok(Ok(Err(e))) => Err(e),
            _ => Err(failure(
                "Ekran yakalama zaman aşımı. Wayland oturumunu kontrol edin.",
            )),
        }
    }
    pub fn frame(&self) -> Result<Value> {
        let frame = self.latest.lock().unwrap().clone()?;
        let mut meta = frame.meta.clone();
        if !self.binary {
            meta["image"] = Value::String(format!(
                "data:image/jpeg;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&frame.data)
            ));
        }
        Ok(meta)
    }
    pub async fn control(&self, p: Value, scroll: bool) -> Result<Value> {
        if p["stream_id"].as_str() != Some(self.id.as_str()) {
            return Err(ApiError::new("INVALID_ARGUMENT", "Yayın kimliği değişti."));
        }
        let (reply, wait) = tokio::sync::oneshot::channel();
        self.tx
            .send(Control {
                params: p,
                scroll,
                reply,
            })
            .map_err(|_| failure("Yayın durdu."))?;
        tokio::time::timeout(Duration::from_secs(3), wait)
            .await
            .map_err(|_| failure("Dokunma isteği zaman aşımı."))?
            .map_err(|_| failure("Yayın durdu."))?
    }
}
impl Drop for Monitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
fn pump(
    queue: &mut wayland_client::EventQueue<State>,
    state: &mut State,
    connection: &Connection,
    timeout: i32,
) -> anyhow::Result<()> {
    queue
        .dispatch_pending(state)
        .context("Wayland olayları okunamadı")?;
    if let Err(error) = connection.flush() {
        if !matches!(&error, wayland_client::backend::WaylandError::Io(e) if e.kind() == std::io::ErrorKind::WouldBlock)
        {
            return Err(error.into());
        }
    }
    if let Some(guard) = queue.prepare_read() {
        let mut fd = libc::pollfd {
            fd: guard.connection_fd().as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: pollfd is initialized and lives for the syscall.
        if unsafe { libc::poll(&mut fd, 1, timeout) } > 0 {
            if let Err(error) = guard.read() {
                if !matches!(&error, wayland_client::backend::WaylandError::Io(e) if e.kind() == std::io::ErrorKind::WouldBlock)
                {
                    return Err(error.into());
                }
            }
        }
    }
    queue
        .dispatch_pending(state)
        .context("Wayland olayları okunamadı")?;
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn run(
    id: &str,
    mode: &str,
    binary: bool,
    video: Option<&tokio::sync::mpsc::Sender<Arc<CapturedFrame>>>,
    hint: Option<(String, String)>,
    frames: &tokio::sync::watch::Sender<Option<Arc<CapturedFrame>>>,
    latest: &Latest,
    stop: &Arc<AtomicBool>,
    controls: &mpsc::Receiver<Control>,
    ready_reply: &mut Option<tokio::sync::oneshot::Sender<Result<()>>>,
) -> anyhow::Result<()> {
    let connection = Connection::connect_to_env()?;
    let mut queue = connection.new_event_queue::<State>();
    let qh = queue.handle();
    let mut state = State::default();
    let _registry = connection.display().get_registry(&qh, ());
    // Use bounded dispatch rather than a roundtrip that can leave an abandoned capture thread stuck.
    let initialized = Arc::new(AtomicBool::new(false));
    connection
        .display()
        .sync::<Arc<AtomicBool>, State>(&qh, initialized.clone());
    let start = Instant::now();
    while !initialized.load(Ordering::Relaxed) {
        if stop.load(Ordering::Relaxed) || start.elapsed() > Duration::from_secs(3) {
            anyhow::bail!("Wayland kayıtları okunamadı.");
        }
        pump(&mut queue, &mut state, &connection, 30)?;
    }
    if let Some(manager) = &state.logical_manager {
        for (key, output) in &state.outputs {
            manager.get_xdg_output(output.proxy.as_ref().unwrap(), &qh, *key);
        }
    }
    // The registry creates managers before initial toplevel events. A second sync includes those events and geometry.
    let initialized = Arc::new(AtomicBool::new(false));
    connection
        .display()
        .sync::<Arc<AtomicBool>, State>(&qh, initialized.clone());
    while !initialized.load(Ordering::Relaxed) {
        if stop.load(Ordering::Relaxed) || start.elapsed() > Duration::from_secs(4) {
            anyhow::bail!("Pencere bilgileri okunamadı.");
        }
        pump(&mut queue, &mut state, &connection, 30)?;
    }
    // Cosmic extensions created by foreign toplevel events need one further dispatch fence.
    let initialized = Arc::new(AtomicBool::new(false));
    connection
        .display()
        .sync::<Arc<AtomicBool>, State>(&qh, initialized.clone());
    while !initialized.load(Ordering::Relaxed) {
        if stop.load(Ordering::Relaxed) || start.elapsed() > Duration::from_secs(5) {
            anyhow::bail!("Pencere konumu okunamadı.");
        }
        pump(&mut queue, &mut state, &connection, 30)?;
    }
    // Foreign windows may arrive before their COSMIC extension; attach any missed extensions.
    if let Some(info) = &state.cosmic {
        for (key, window) in &mut state.windows {
            if window.cosmic.is_none() {
                window.cosmic =
                    Some(info.get_cosmic_toplevel::<u32, State>(&window.proxy, &qh, *key));
            }
        }
    }
    // wl_display.sync is not a commit fence for asynchronous toplevel state.
    // Wait for the actual properties rather than treating their initial defaults as "inactive".
    let deadline = Instant::now() + Duration::from_millis(1500);
    while mode == "window"
        && !state
            .windows
            .values()
            .any(|w| w.active && w.initialized && w.geometry.is_some())
        && Instant::now() < deadline
    {
        if stop.load(Ordering::Relaxed) {
            anyhow::bail!("Yayın iptal edildi.");
        }
        pump(&mut queue, &mut state, &connection, 20)?;
    }
    let mut window = if mode == "window" {
        let selected = state
            .windows
            .iter()
            .find(|(_, w)| w.active && w.initialized)
            .map(|(key, _)| *key)
            .or_else(|| {
                hint.as_ref().and_then(|(app, title)| {
                    let matches: Vec<_> = state
                        .windows
                        .iter()
                        .filter(|(_, w)| &w.app_id == app && &w.title == title)
                        .map(|(key, _)| *key)
                        .collect();
                    if matches.len() == 1 {
                        Some(matches[0])
                    } else {
                        None
                    }
                })
            });
        Some(selected.ok_or_else(||anyhow::anyhow!("Aktif uygulama bilgisi alınamadı. Bilgisayarda paylaşmak istediğiniz pencereye tıklayıp tekrar deneyin."))?)
    } else {
        None
    };
    let mut output_id = window
        .and_then(|key| state.windows[&key].geometry.as_ref().map(|g| g.output))
        .or_else(|| {
            state
                .windows
                .values()
                .find(|w| w.active)
                .and_then(|w| w.geometry.as_ref().map(|g| g.output))
        })
        .or_else(|| state.outputs.keys().min().copied())
        .ok_or_else(|| anyhow::anyhow!("Monitör bulunamadı."))?;
    let mut session = create_capture(&state, window, output_id, &qh)?;
    let mut buffer: Option<(File, wl_buffer::WlBuffer, u32, u32, wl_shm::Format)> = None;
    let mut capture: Option<frame::ExtImageCopyCaptureFrameV1> = None;
    let mut sequence = 0u64;
    let mut failures = 0u32;
    let mut next = Instant::now();
    let mut requested = Instant::now();
    let mut first = true;
    let mut absolute: Option<crate::input::Input> = None;
    let mut pixels = Vec::new();
    let mut rgb = Vec::new();
    let mut rate_time = Instant::now();
    let mut rate_count = 0u32;
    let mut source_fps = 0u32;
    let mut encoder: Option<crate::video::Encoder> = None;
    let mut encoder_id = 0u64;
    let interval = Duration::from_micros(if video.is_some() {
        1_000_000 / crate::video::FPS as u64
    } else {
        16_667
    });
    while !stop.load(Ordering::Relaxed) {
        let wait = if capture.is_none() && state.constraints {
            next.saturating_duration_since(Instant::now())
                .as_millis()
                .min(4) as i32
        } else {
            4
        };
        pump(&mut queue, &mut state, &connection, wait)?;
        if mode == "window" {
            let mut active = active_window(&state);
            if window.is_some_and(|key| !state.windows.contains_key(&key)) && active.is_none() {
                let deadline = Instant::now() + Duration::from_millis(750);
                while active.is_none() && Instant::now() < deadline && !stop.load(Ordering::Relaxed)
                {
                    pump(&mut queue, &mut state, &connection, 10)?;
                    active = active_window(&state);
                }
            }
            if let Some(key) = active.filter(|key| Some(*key) != window) {
                if let Some(frame) = capture.take() {
                    frame.destroy();
                }
                session.destroy();
                if let Some((_, old, _, _, _)) = buffer.take() {
                    old.destroy();
                }
                state.capture_epoch += 1;
                state.ready = false;
                state.failed = false;
                state.stopped = false;
                state.constraints = false;
                state.width = 0;
                state.height = 0;
                state.formats.clear();
                window = Some(key);
                output_id = state.windows[&key]
                    .geometry
                    .as_ref()
                    .map(|g| g.output)
                    .unwrap_or(output_id);
                session = create_capture(&state, window, output_id, &qh)?;
                failures = 0;
                next = Instant::now();
            }
        }
        if !state.outputs.contains_key(&output_id)
            || state.stopped
            || window.is_some_and(|key| !state.windows.contains_key(&key))
        {
            anyhow::bail!("Paylaşılan ekran/pencere kapandı. Yeniden kaynak seçin.");
        }
        if first && start.elapsed() > Duration::from_secs(7) {
            anyhow::bail!("İlk ekran görüntüsü alınamadı.");
        }
        while let Ok(control) = controls.try_recv() {
            let result = (|| -> Result<Value> {
                if binary && control.params["source_id"].as_u64() != Some(state.capture_epoch) {
                    return Err(ApiError::new(
                        "MONITOR_SOURCE_CHANGED",
                        "Aktif uygulama değişti; yeni görüntüde tekrar dokunun.",
                    ));
                }
                if window.is_some_and(|key| !state.windows.get(&key).is_some_and(|w| w.active)) {
                    return Err(ApiError::new(
                        "MONITOR_SOURCE_CHANGED",
                        "Uygulama odağı değişiyor; yeni görüntüyü bekleyin.",
                    ));
                }
                let x = normalized(&control.params, "x")?;
                let y = normalized(&control.params, "y")?;
                let geometry = if let Some(key) = window {
                    state
                        .windows
                        .get(&key)
                        .and_then(|w| w.geometry.clone())
                        .ok_or_else(|| failure("Pencerenin konumu alınamadı."))?
                } else {
                    Geometry {
                        output: output_id,
                        x: 0,
                        y: 0,
                        width: state.outputs[&output_id].width,
                        height: state.outputs[&output_id].height,
                    }
                };
                let output = state
                    .outputs
                    .get(&geometry.output)
                    .ok_or_else(|| failure("Monitör çıkarıldı."))?;
                if output.width <= 0 || output.height <= 0 {
                    return Err(failure("Monitör boyutu bilinmiyor."));
                }
                // libinput maps the absolute mouse onto the active output. Refuse ambiguous multi-output input.
                if state.outputs.len() != 1 {
                    return Err(failure(
                        "Dokunmatik kontrol şu an tek fiziksel monitörle destekleniyor.",
                    ));
                }
                let ax = (geometry.x as f64 + x * geometry.width as f64) / output.width as f64;
                let ay = (geometry.y as f64 + y * geometry.height as f64) / output.height as f64;
                if !(0.0..=1.0).contains(&ax) || !(0.0..=1.0).contains(&ay) {
                    return Err(failure(
                        "Dokunulan pencere noktası görünür ekranın dışında.",
                    ));
                }
                if absolute.is_none() {
                    absolute = Some(crate::input::Input::new_absolute()?);
                    std::thread::sleep(Duration::from_millis(200));
                }
                let pointer = absolute.as_mut().unwrap();
                pointer.request(
                    "input.absolute",
                    &json!({"x":(ax*65535.0).round() as i32,"y":(ay*65535.0).round() as i32}),
                )?;
                if control.scroll {
                    pointer.request(
                        "input.scroll",
                        &json!({"x":control.params["dx"],"y":control.params["dy"]}),
                    )?;
                } else {
                    pointer.request("input.key", &json!({"code":272,"down":true}))?;
                    pointer.request("input.key", &json!({"code":272,"down":false}))?;
                }
                Ok(json!({"applied":true}))
            })();
            let _ = control.reply.send(result);
        }
        if state.ready {
            state.ready = false;
            failures = 0;
            let (file, wl_buffer, width, height, format) = buffer.as_mut().unwrap();
            file.seek(SeekFrom::Start(0))?;
            pixels.resize((*width as usize) * (*height as usize) * 4, 0);
            file.read_exact(&mut pixels)?;
            let completed_request = requested;
            if let Some(frame) = capture.take() {
                frame.destroy();
            }
            // The private pixel copy is complete: let the compositor capture into
            // the shared buffer again while this frame is encoded on the CPU.
            if state.width == *width
                && state.height == *height
                && Instant::now() >= completed_request + interval
            {
                requested = Instant::now();
                let pending = session.create_frame(&qh, state.capture_epoch);
                pending.attach_buffer(wl_buffer);
                pending.damage_buffer(0, 0, *width as i32, *height as i32);
                pending.capture();
                capture = Some(pending);
                if let Err(error) = connection.flush() {
                    if !matches!(&error, wayland_client::backend::WaylandError::Io(e) if e.kind() == std::io::ErrorKind::WouldBlock)
                    {
                        return Err(error.into());
                    }
                }
            }
            // Predictive video cannot drop arbitrary encoded frames. When the bounded
            // transport queue is full, skip new work and restart with an IDR on recovery.
            if video.is_some_and(|tx| tx.capacity() == 0) {
                encoder = None;
                next = completed_request + interval;
                continue;
            }
            let encode_start = Instant::now();
            let (data, w, h, codec, video_id, keyframe) = if video.is_some() {
                let (w, h) =
                    convert_rgb(&pixels, *width, *height, *format, state.transform, &mut rgb);
                let source = (state.capture_epoch, w, h);
                if encoder.as_ref().is_none_or(|e| e.source != source) {
                    encoder_id += 1;
                    encoder = Some(crate::video::Encoder::new(source, encoder_id)?);
                }
                let encoder = encoder.as_mut().unwrap();
                let data = encoder.encode(&rgb, stop)?;
                let keyframe = crate::video::keyframe(&data);
                (
                    data,
                    encoder.size.0 as i32,
                    encoder.size.1 as i32,
                    "h264",
                    encoder.id,
                    keyframe,
                )
            } else {
                let (data, w, h) = encode(
                    &pixels,
                    *width,
                    *height,
                    *format,
                    state.transform,
                    binary,
                    &mut rgb,
                )?;
                (data, w, h, "jpeg", 0, true)
            };
            let encode_ms = encode_start.elapsed().as_millis();
            rate_count += 1;
            if rate_time.elapsed() >= Duration::from_secs(1) {
                source_fps = (rate_count as f64 / rate_time.elapsed().as_secs_f64()).round() as u32;
                rate_count = 0;
                rate_time = Instant::now();
            }
            sequence += 1;
            let title = window
                .and_then(|key| state.windows.get(&key).map(|w| w.title.clone()))
                .unwrap_or_else(|| state.outputs[&output_id].name.clone());
            let frame = Arc::new(CapturedFrame {
                meta: json!({"stream_id":id,"frame_id":sequence,"width":w,"height":h,"title":title.chars().take(300).collect::<String>(),"mode":mode,"touch":crate::input::available() && state.outputs.len()==1,"target_fps":if video.is_some() { crate::video::FPS } else { 60 },"codec":codec,"video_id":video_id,"keyframe":keyframe,"source_id":state.capture_epoch,"capture_fps":source_fps,"encode_ms":encode_ms}),
                data,
            });
            *latest.lock().unwrap() = Ok(frame.clone());
            frames.send_replace(Some(frame.clone()));
            if let Some(tx) = video {
                if tx.try_send(frame).is_err() {
                    encoder = None;
                }
            }
            if let Some(reply) = ready_reply.take() {
                let _ = reply.send(Ok(()));
            }
            first = false;
            next = completed_request + interval;
        }
        if state.failed {
            state.failed = false;
            failures += 1;
            if let Some(frame) = capture.take() {
                frame.destroy();
            }
            next = Instant::now();
            if failures >= 3 {
                anyhow::bail!("Ekran görüntüsü alınamadı; yayın kaynağını yeniden seçin.");
            }
        }
        if capture.is_none() && state.constraints && Instant::now() >= next {
            let format = state
                .formats
                .iter()
                .copied()
                .find(|f| {
                    matches!(
                        f,
                        wl_shm::Format::Xrgb8888
                            | wl_shm::Format::Argb8888
                            | wl_shm::Format::Xbgr8888
                            | wl_shm::Format::Abgr8888
                    )
                })
                .ok_or_else(|| {
                    anyhow::anyhow!("Paylaşımlı ekran tamponu biçimi desteklenmiyor.")
                })?;
            if state.width == 0
                || state.height == 0
                || state.width > 8192
                || state.height > 8192
                || state.width as u64 * state.height as u64 > 32_000_000
            {
                anyhow::bail!("Ekran çözünürlüğü desteklenen sınırın dışında.");
            }
            if buffer.as_ref().is_none_or(|(_, _, w, h, f)| {
                *w != state.width || *h != state.height || *f != format
            }) {
                if let Some((_, old, _, _, _)) = buffer.take() {
                    old.destroy();
                }
                // SAFETY: memfd_create returns an owned fd, transferred exactly once to File.
                let fd =
                    unsafe { libc::memfd_create(c"cepdesk-monitor".as_ptr(), libc::MFD_CLOEXEC) };
                if fd < 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
                let file = unsafe { File::from_raw_fd(fd) };
                let size = state.width as usize * state.height as usize * 4;
                file.set_len(size as u64)?;
                let pool = state
                    .shm
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("wl_shm bulunamadı."))?
                    .create_pool(file.as_fd(), size as i32, &qh, ());
                let wl_buffer = pool.create_buffer(
                    0,
                    state.width as i32,
                    state.height as i32,
                    (state.width * 4) as i32,
                    format,
                    &qh,
                    (),
                );
                pool.destroy();
                buffer = Some((file, wl_buffer, state.width, state.height, format));
            }
            requested = Instant::now();
            let frame = session.create_frame(&qh, state.capture_epoch);
            frame.attach_buffer(&buffer.as_ref().unwrap().1);
            frame.damage_buffer(0, 0, state.width as i32, state.height as i32);
            frame.capture();
            capture = Some(frame);
        }
    }
    if let Some(frame) = capture {
        frame.destroy();
    }
    session.destroy();
    Ok(())
}
fn active_window(state: &State) -> Option<u32> {
    state
        .focused
        .filter(|key| {
            state
                .windows
                .get(key)
                .is_some_and(|w| w.active && w.initialized)
        })
        .or_else(|| {
            state
                .windows
                .iter()
                .find(|(_, w)| w.active && w.initialized)
                .map(|(key, _)| *key)
        })
}
fn create_capture(
    state: &State,
    window: Option<u32>,
    output: u32,
    qh: &QueueHandle<State>,
) -> anyhow::Result<session::ExtImageCopyCaptureSessionV1> {
    let source = if let Some(key) = window {
        state
            .tops
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Uygulama yakalama desteklenmiyor."))?
            .create_source(&state.windows[&key].proxy, qh, ())
    } else {
        state
            .outputs_manager
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Ekran yakalama desteklenmiyor."))?
            .create_source(state.outputs[&output].proxy.as_ref().unwrap(), qh, ())
    };
    let session = state
        .copy
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Ekran yakalama protokolü yok."))?
        .create_session(
            &source,
            copy::Options::PaintCursors,
            qh,
            state.capture_epoch,
        );
    source.destroy();
    Ok(session)
}
fn normalized(p: &Value, key: &str) -> Result<f64> {
    p[key]
        .as_f64()
        .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
        .ok_or_else(|| ApiError::new("INVALID_ARGUMENT", "Dokunma koordinatı 0..1 olmalı."))
}
impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        s: &mut Self,
        r: &wl_registry::WlRegistry,
        e: wl_registry::Event,
        _: &(),
        _: &Connection,
        q: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = e
        {
            match interface.as_str() {
                "wl_seat" if s.seat.is_none() => s.seat = Some(r.bind(name, 1, q, ())),
                "zcosmic_toplevel_manager_v1" => s.manager = Some(r.bind(name, 1, q, ())),
                "wl_shm" => s.shm = Some(r.bind(name, 1, q, ())),
                "wl_output" => {
                    let proxy: wl_output::WlOutput = r.bind(name, version.min(4), q, name);
                    s.outputs.insert(
                        name,
                        Output {
                            proxy: Some(proxy),
                            ..Default::default()
                        },
                    );
                }
                "ext_image_copy_capture_manager_v1" => s.copy = Some(r.bind(name, 1, q, ())),
                "ext_output_image_capture_source_manager_v1" => {
                    s.outputs_manager = Some(r.bind(name, 1, q, ()))
                }
                "ext_foreign_toplevel_image_capture_source_manager_v1" => {
                    s.tops = Some(r.bind(name, 1, q, ()))
                }
                "zcosmic_toplevel_info_v1" if version >= 2 => {
                    s.cosmic = Some(r.bind(name, 2, q, ()))
                }
                "ext_foreign_toplevel_list_v1" => {
                    let _: list::ExtForeignToplevelListV1 = r.bind(name, 1, q, ());
                }
                "zxdg_output_manager_v1" => {
                    s.logical_manager = Some(r.bind(name, version.min(3), q, ()))
                }
                _ => {}
            }
        } else if let wl_registry::Event::GlobalRemove { name } = e {
            s.outputs.remove(&name);
        }
    }
}
impl Dispatch<wl_output::WlOutput, u32> for State {
    fn event(
        s: &mut Self,
        _: &wl_output::WlOutput,
        e: wl_output::Event,
        key: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let Some(o) = s.outputs.get_mut(key) {
            match e {
                wl_output::Event::Name { name } => o.name = name,
                wl_output::Event::Mode { width, height, .. } if o.width == 0 => {
                    o.width = width;
                    o.height = height;
                }
                _ => {}
            }
        }
    }
}
impl Dispatch<logical_output::ZxdgOutputV1, u32> for State {
    fn event(
        s: &mut Self,
        _: &logical_output::ZxdgOutputV1,
        e: logical_output::Event,
        key: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let Some(o) = s.outputs.get_mut(key) {
            match e {
                logical_output::Event::LogicalSize { width, height } => {
                    o.width = width;
                    o.height = height;
                }
                logical_output::Event::Name { name } => o.name = name,
                _ => {}
            }
        }
    }
}
impl Dispatch<list::ExtForeignToplevelListV1, ()> for State {
    fn event(
        s: &mut Self,
        _: &list::ExtForeignToplevelListV1,
        e: list::Event,
        _: &(),
        _: &Connection,
        q: &QueueHandle<Self>,
    ) {
        if let list::Event::Toplevel { toplevel } = e {
            let key = toplevel.id().protocol_id();
            let cosmic = s
                .cosmic
                .as_ref()
                .map(|info| info.get_cosmic_toplevel::<u32, State>(&toplevel, q, key));
            s.windows.insert(
                key,
                Window {
                    proxy: toplevel,
                    cosmic,
                    title: String::new(),
                    app_id: String::new(),
                    initialized: false,
                    active: false,
                    geometry: None,
                },
            );
        }
    }
    wayland_client::event_created_child!(State,list::ExtForeignToplevelListV1,[0=>(foreign::ExtForeignToplevelHandleV1,())]);
}
impl Dispatch<foreign::ExtForeignToplevelHandleV1, ()> for State {
    fn event(
        s: &mut Self,
        p: &foreign::ExtForeignToplevelHandleV1,
        e: foreign::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let key = p.id().protocol_id();
        match e {
            foreign::Event::Closed => {
                s.windows.remove(&key);
                p.destroy();
            }
            foreign::Event::AppId { app_id } => {
                if let Some(w) = s.windows.get_mut(&key) {
                    w.app_id = app_id;
                }
            }
            foreign::Event::Title { title } => {
                if let Some(w) = s.windows.get_mut(&key) {
                    w.title = title;
                }
            }
            _ => {}
        }
    }
}
impl Dispatch<handle::ZcosmicToplevelHandleV1, u32> for State {
    fn event(
        s: &mut Self,
        _: &handle::ZcosmicToplevelHandleV1,
        e: handle::Event,
        key: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let Some(w) = s.windows.get_mut(key) {
            match e {
                handle::Event::State { state } => {
                    w.initialized = true;
                    w.active = state
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .any(|v| u32::from_ne_bytes(*v) == 2);
                    if w.active {
                        s.focused = Some(*key);
                    } else if s.focused == Some(*key) {
                        s.focused = None;
                    }
                }
                handle::Event::Geometry {
                    output,
                    x,
                    y,
                    width,
                    height,
                } => {
                    let output = output.data::<u32>().copied().unwrap_or(0);
                    w.geometry = Some(Geometry {
                        output,
                        x,
                        y,
                        width,
                        height,
                    });
                }
                _ => {}
            }
        }
    }
}
impl Dispatch<session::ExtImageCopyCaptureSessionV1, u64> for State {
    fn event(
        s: &mut Self,
        _: &session::ExtImageCopyCaptureSessionV1,
        e: session::Event,
        epoch: &u64,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if *epoch != s.capture_epoch {
            return;
        }
        match e {
            session::Event::BufferSize { width, height } => {
                s.width = width;
                s.height = height;
            }
            session::Event::ShmFormat {
                format: WEnum::Value(format),
            } => s.formats.push(format),
            session::Event::Done => s.constraints = true,
            session::Event::Stopped => s.stopped = true,
            _ => {}
        }
    }
}
impl Dispatch<frame::ExtImageCopyCaptureFrameV1, u64> for State {
    fn event(
        s: &mut Self,
        _: &frame::ExtImageCopyCaptureFrameV1,
        e: frame::Event,
        epoch: &u64,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if *epoch != s.capture_epoch {
            return;
        }
        match e {
            frame::Event::Ready => s.ready = true,
            frame::Event::Failed { .. } => s.failed = true,
            frame::Event::Transform {
                transform: WEnum::Value(t),
            } => s.transform = t as u32,
            _ => {}
        }
    }
}
impl Dispatch<wayland_client::protocol::wl_callback::WlCallback, Arc<AtomicBool>> for State {
    fn event(
        _: &mut Self,
        _: &wayland_client::protocol::wl_callback::WlCallback,
        _: wayland_client::protocol::wl_callback::Event,
        done: &Arc<AtomicBool>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        done.store(true, Ordering::Relaxed);
    }
}
wayland_client::delegate_noop!(State: ignore wl_shm::WlShm);
wayland_client::delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
wayland_client::delegate_noop!(State: ignore wl_buffer::WlBuffer);
wayland_client::delegate_noop!(State: ignore copy::ExtImageCopyCaptureManagerV1);
wayland_client::delegate_noop!(State: ignore outputs::ExtOutputImageCaptureSourceManagerV1);
wayland_client::delegate_noop!(State: ignore tops::ExtForeignToplevelImageCaptureSourceManagerV1);
wayland_client::delegate_noop!(State: ignore source::ExtImageCaptureSourceV1);
wayland_client::delegate_noop!(State: ignore cosmic::ZcosmicToplevelInfoV1);
wayland_client::delegate_noop!(State: ignore output_manager::ZxdgOutputManagerV1);
#[link(name = "libgdk_pixbuf-2.0.so.0", modifiers = "+verbatim")]
unsafe extern "C" {
    fn gdk_pixbuf_new_from_data(
        data: *const u8,
        colorspace: i32,
        alpha: i32,
        bits: i32,
        width: i32,
        height: i32,
        stride: i32,
        destroy: Option<unsafe extern "C" fn(*mut u8, *mut libc::c_void)>,
        user: *mut libc::c_void,
    ) -> *mut libc::c_void;
    fn gdk_pixbuf_scale_simple(
        pixbuf: *mut libc::c_void,
        width: i32,
        height: i32,
        interpolation: i32,
    ) -> *mut libc::c_void;
    fn gdk_pixbuf_save_to_bufferv(
        pixbuf: *mut libc::c_void,
        buffer: *mut *mut libc::c_char,
        size: *mut usize,
        format: *const libc::c_char,
        keys: *mut *mut libc::c_char,
        values: *mut *mut libc::c_char,
        error: *mut *mut libc::c_void,
    ) -> i32;
}
#[link(name = "gobject-2.0")]
unsafe extern "C" {
    fn g_object_unref(object: *mut libc::c_void);
}
#[link(name = "glib-2.0")]
unsafe extern "C" {
    fn g_free(mem: *mut libc::c_void);
    fn g_error_free(error: *mut libc::c_void);
}
fn convert_rgb(
    pixels: &[u8],
    width: u32,
    height: u32,
    format: wl_shm::Format,
    transform: u32,
    rgb: &mut Vec<u8>,
) -> (u32, u32) {
    let rotated = transform % 2 == 1;
    let (w, h) = if rotated {
        (height, width)
    } else {
        (width, height)
    };
    rgb.resize(w as usize * h as usize * 3, 0);
    if transform == 0 {
        let bgr = matches!(format, wl_shm::Format::Xrgb8888 | wl_shm::Format::Argb8888);
        if bgr {
            for (src, dst) in pixels
                .as_chunks::<4>()
                .0
                .iter()
                .zip(rgb.as_chunks_mut::<3>().0.iter_mut())
            {
                dst[0] = src[2];
                dst[1] = src[1];
                dst[2] = src[0];
            }
        } else {
            for (src, dst) in pixels
                .as_chunks::<4>()
                .0
                .iter()
                .zip(rgb.as_chunks_mut::<3>().0.iter_mut())
            {
                dst.copy_from_slice(&src[..3]);
            }
        }
    } else {
        for y in 0..height {
            for x in 0..width {
                let mut px = x;
                if transform >= 4 {
                    px = width - 1 - px;
                }
                let (dx, dy) = match transform % 4 {
                    0 => (px, y),
                    1 => (y, width - 1 - px),
                    2 => (width - 1 - px, height - 1 - y),
                    _ => (height - 1 - y, px),
                };
                let src = ((y * width + x) * 4) as usize;
                let dst = ((dy * w + dx) * 3) as usize;
                let bgr = matches!(format, wl_shm::Format::Xrgb8888 | wl_shm::Format::Argb8888);
                rgb[dst] = pixels[src + if bgr { 2 } else { 0 }];
                rgb[dst + 1] = pixels[src + 1];
                rgb[dst + 2] = pixels[src + if bgr { 0 } else { 2 }];
            }
        }
    }
    (w, h)
}

fn encode(
    pixels: &[u8],
    width: u32,
    height: u32,
    format: wl_shm::Format,
    transform: u32,
    binary: bool,
    rgb: &mut Vec<u8>,
) -> anyhow::Result<(Vec<u8>, i32, i32)> {
    let (w, h) = convert_rgb(pixels, width, height, format, transform, rgb);
    let ratio = ((if binary { 2560.0 } else { 1280.0 }) / w.max(h) as f64).min(1.0);
    let sw = (w as f64 * ratio).round().max(1.0) as i32;
    let sh = (h as f64 * ratio).round().max(1.0) as i32;
    // SAFETY: RGB storage stays alive until pixbufs are unref'd. GDK/GLib own and release returned allocations.
    unsafe {
        let original = gdk_pixbuf_new_from_data(
            rgb.as_ptr(),
            0,
            0,
            8,
            w as i32,
            h as i32,
            (w * 3) as i32,
            None,
            std::ptr::null_mut(),
        );
        if original.is_null() {
            anyhow::bail!("Görüntü tamponu oluşturulamadı.");
        }
        let scaled = if sw == w as i32 && sh == h as i32 {
            original
        } else {
            let scaled = gdk_pixbuf_scale_simple(original, sw, sh, 2);
            g_object_unref(original);
            scaled
        };
        if scaled.is_null() {
            anyhow::bail!("Görüntü ölçeklenemedi.");
        }
        let mut data = std::ptr::null_mut();
        let mut len = 0;
        let mut error = std::ptr::null_mut();
        let mut keys = [
            c"quality".as_ptr() as *mut libc::c_char,
            std::ptr::null_mut(),
        ];
        let mut values = [
            (if binary { c"90" } else { c"68" }).as_ptr() as *mut libc::c_char,
            std::ptr::null_mut(),
        ];
        let success = gdk_pixbuf_save_to_bufferv(
            scaled,
            &mut data,
            &mut len,
            c"jpeg".as_ptr(),
            keys.as_mut_ptr(),
            values.as_mut_ptr(),
            &mut error,
        );
        g_object_unref(scaled);
        let result = if success != 0
            && !data.is_null()
            && len <= if binary { 2 * 1024 * 1024 } else { 700 * 1024 }
        {
            Ok((
                std::slice::from_raw_parts(data as *const u8, len).to_vec(),
                sw,
                sh,
            ))
        } else {
            Err(anyhow::anyhow!(
                "JPEG görüntüsü kodlanamadı veya kare çok büyük."
            ))
        };
        if !data.is_null() {
            g_free(data.cast());
        }
        if !error.is_null() {
            g_error_free(error);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packet_distinguishes_video_from_legacy_images() {
        for (codec, magic) in [("h264", b"CDV1"), ("jpeg", b"CDM2")] {
            let frame = CapturedFrame {
                meta: json!({"codec":codec,"stream_id":"test","frame_id":1}),
                data: vec![0, 0, 0, 1, 0x65],
            };
            let packet = frame.packet();
            assert_eq!(&packet[..4], magic);
            let length = u32::from_be_bytes(packet[4..8].try_into().unwrap()) as usize;
            assert_eq!(
                serde_json::from_slice::<Value>(&packet[8..8 + length]).unwrap(),
                frame.meta
            );
            assert_eq!(&packet[8 + length..], frame.data);
        }
    }

    #[tokio::test]
    async fn video_requires_binary_transport() {
        let error = Monitor::start("screen".into(), false, true, None)
            .await
            .err()
            .unwrap();
        assert_eq!(error.code, "INVALID_ARGUMENT");
    }

    #[tokio::test]
    #[ignore = "Real COSMIC capture; no input injection"]
    async fn live_monitor_capture() {
        for mode in ["screen", "window"] {
            let mut monitor = match Monitor::start(mode.into(), true, false, None).await {
                Ok(monitor) => monitor,
                Err(error)
                    if mode == "window"
                        && error.message.contains("Aktif uygulama bilgisi alınamadı")
                        && std::env::var("CEP_DESK_REQUIRE_WINDOW").ok().as_deref()
                            != Some("1") =>
                {
                    println!("window: empty desktop correctly refuses capture");
                    continue;
                }
                Err(error) => panic!("{mode}: {error:?}"),
            };
            let frame = monitor.frame().unwrap();
            let captured = monitor.latest.lock().unwrap().clone().unwrap();
            assert!(captured.data.starts_with(&[0xff, 0xd8]));
            assert!(captured.packet().starts_with(b"CDM2"));
            assert!(frame["width"].as_u64().unwrap() > 0);
            assert!(frame["height"].as_u64().unwrap() > 0);
            assert_eq!(frame["mode"], mode);
            assert_eq!(frame["target_fps"], 60);
            println!("{mode}: {} × {}", frame["width"], frame["height"]);
            if mode == "screen"
                && std::env::var("CEP_DESK_MEASURE_FPS").ok().as_deref() == Some("1")
            {
                let start = Instant::now();
                let first = frame["frame_id"].as_u64().unwrap();
                while start.elapsed() < Duration::from_secs(2) {
                    let _ =
                        tokio::time::timeout(Duration::from_millis(500), monitor.frames.changed())
                            .await;
                }
                let last = monitor.frame().unwrap();
                println!(
                    "measured PC: {:.1} FPS, JPEG encoding: {} ms",
                    (last["frame_id"].as_u64().unwrap() - first) as f64
                        / start.elapsed().as_secs_f64(),
                    last["encode_ms"]
                );
            }
            if mode == "window"
                && std::env::var("CEP_DESK_FOLLOW_TEST").ok().as_deref() == Some("1")
            {
                let old_source = frame["source_id"].clone();
                let old_image = captured.data.clone();
                println!("follow_ready");
                let switched = tokio::time::timeout(Duration::from_secs(10), async {
                    loop {
                        monitor.frames.changed().await.unwrap();
                        let next = monitor.frame().unwrap();
                        if next["source_id"] != old_source {
                            assert!(next["title"].as_str().unwrap().contains("second"));
                            assert_ne!(
                                monitor.latest.lock().unwrap().clone().unwrap().data,
                                old_image
                            );
                            println!("active window switched: {}", next["title"]);
                            break;
                        }
                    }
                })
                .await;
                assert!(switched.is_ok(), "Focus changed but capture did not follow");
            }
            drop(monitor);
        }
    }
}

impl Dispatch<management_api::ZcosmicToplevelManagerV1, ()> for State {
    fn event(
        s: &mut Self,
        _: &management_api::ZcosmicToplevelManagerV1,
        e: management_api::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let management_api::Event::Capabilities { capabilities } = e;
        s.activation = capabilities
            .as_chunks::<4>()
            .0
            .iter()
            .any(|v| u32::from_ne_bytes(*v) == 2);
    }
}
wayland_client::delegate_noop!(State: ignore wayland_client::protocol::wl_seat::WlSeat);
