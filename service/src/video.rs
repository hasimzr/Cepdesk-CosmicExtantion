//! Persistent low-latency H.264 encoder. AVI is only the local pipe envelope;
//! the phone receives one Annex-B access unit per packet, never an AVI file.
use anyhow::{bail, Context, Result};
use std::{
    io::{Read, Write},
    os::fd::AsRawFd,
    process::{Child, ChildStdin, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub const FPS: u32 = 30;
pub const MAX_PACKET: usize = 2 * 1024 * 1024;

pub fn dimensions(width: u32, height: u32) -> (u32, u32) {
    let ratio = (1920.0 / width.max(height) as f64).min(1.0);
    let even = |v: u32| ((v as f64 * ratio).round() as u32 / 2 * 2).max(2);
    (even(width), even(height))
}

pub fn keyframe(bytes: &[u8]) -> bool {
    bytes
        .windows(4)
        .any(|v| v[..3] == [0, 0, 1] && v[3] & 31 == 5)
}

pub struct Encoder {
    child: Child,
    input: ChildStdin,
    output: mpsc::Receiver<Result<Vec<u8>>>,
    reader: Option<JoinHandle<()>>,
    pub source: (u64, u32, u32),
    pub id: u64,
    pub size: (u32, u32),
}

impl Encoder {
    pub fn new(source: (u64, u32, u32), id: u64) -> Result<Self> {
        let size = dimensions(source.1, source.2);
        let mut child = crate::host_process::std_command("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-f",
                "rawvideo",
                "-pixel_format",
                "rgb24",
                "-video_size",
            ])
            .arg(format!("{}x{}", source.1, source.2))
            .args(["-framerate", "30", "-i", "pipe:0", "-an", "-vf"])
            .arg(format!(
                "scale={}:{}:flags=fast_bilinear:out_color_matrix=bt709:out_range=tv",
                size.0, size.1
            ))
            .args([
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-tune",
                "zerolatency",
                "-profile:v",
                "baseline",
                "-pix_fmt",
                "yuv420p",
                "-colorspace",
                "bt709",
                "-color_primaries",
                "bt709",
                "-color_trc",
                "bt709",
                "-color_range",
                "tv",
                "-threads",
                "2",
                "-filter_threads",
                "1",
                "-crf",
                "20",
                "-maxrate",
                "10M",
                "-bufsize",
                "1M",
                "-g",
                "30",
                "-keyint_min",
                "30",
                "-bf",
                "0",
                "-x264-params",
                "scenecut=0:repeat-headers=1:aud=1",
                "-flush_packets",
                "1",
                "-f",
                "avi",
                "pipe:1",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("H.264 başlatılamadı. FFmpeg ve libx264 kurulu olmalı.")?;
        let input = child.stdin.take().unwrap();
        let mut stdout = child.stdout.take().unwrap();
        let (tx, output) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            let result = read_avi(&mut stdout, |data| {
                tx.send(Ok(data))
                    .map_err(|_| anyhow::anyhow!("Video alıcısı kapandı"))
            });
            if let Err(error) = result {
                let _ = tx.send(Err(error));
            }
        });
        let encoder = Self {
            child,
            input,
            output,
            reader: Some(reader),
            source,
            id,
            size,
        };
        // Never let a stalled encoder keep screen capture alive after disconnect.
        let fd = encoder.input.as_raw_fd();
        // SAFETY: fd belongs to our live ChildStdin; fcntl changes only its pipe flags.
        unsafe {
            let flags = libc::fcntl(fd, libc::F_GETFL);
            if flags < 0 || libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        Ok(encoder)
    }

    pub fn encode(&mut self, rgb: &[u8], stop: &Arc<AtomicBool>) -> Result<Vec<u8>> {
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut remaining = rgb;
        while !remaining.is_empty() {
            self.check(stop, deadline)?;
            match self.input.write(remaining) {
                Ok(0) => bail!("H.264 kodlayıcısı kapandı."),
                Ok(n) => remaining = &remaining[n..],
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    let mut fd = libc::pollfd {
                        fd: self.input.as_raw_fd(),
                        events: libc::POLLOUT,
                        revents: 0,
                    };
                    // SAFETY: initialized pollfd is live for the call.
                    unsafe {
                        libc::poll(&mut fd, 1, 10);
                    }
                }
                Err(e) => return Err(e).context("H.264 kodlayıcısına görüntü yazılamadı."),
            }
        }
        loop {
            self.check(stop, deadline)?;
            match self.output.recv_timeout(Duration::from_millis(10)) {
                Ok(data) => return data.context("FFmpeg/libx264 video kodlama hatası."),
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => bail!("H.264 kodlayıcısı kapandı."),
            }
        }
    }

    fn check(&mut self, stop: &AtomicBool, deadline: Instant) -> Result<()> {
        if stop.load(Ordering::Relaxed) {
            bail!("Video yayını durduruldu.");
        }
        if Instant::now() >= deadline {
            bail!("H.264 kodlama zaman aşımı.");
        }
        if self.child.try_wait()?.is_some() {
            bail!("FFmpeg/libx264 kodlayıcısı kapandı.");
        }
        Ok(())
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn read_avi(reader: &mut impl Read, mut emit: impl FnMut(Vec<u8>) -> Result<()>) -> Result<()> {
    let mut riff = [0u8; 12];
    reader.read_exact(&mut riff)?;
    if &riff[..4] != b"RIFF" || &riff[8..] != b"AVI " {
        bail!("Geçersiz video başlığı.");
    }
    let mut in_movi = false;
    loop {
        let mut header = [0u8; 8];
        reader.read_exact(&mut header)?;
        let len = u32::from_le_bytes(header[4..].try_into().unwrap()) as usize;
        if &header[..4] == b"LIST" {
            let mut kind = [0u8; 4];
            reader.read_exact(&mut kind)?;
            if &kind == b"movi" {
                in_movi = true;
                continue;
            }
            if !(4..=MAX_PACKET).contains(&len) {
                bail!("Video başlığı sınırı aşıldı.");
            }
            let mut skip = reader.take((len - 4 + len % 2) as u64);
            let copied = std::io::copy(&mut skip, &mut std::io::sink())?;
            if copied != (len - 4 + len % 2) as u64 {
                bail!("Eksik video başlığı.");
            }
            continue;
        }
        if len > MAX_PACKET {
            bail!("Video paketi sınırı aşıldı.");
        }
        let mut data = vec![0; len + len % 2];
        reader.read_exact(&mut data)?;
        data.truncate(len);
        if in_movi && &header[..4] == b"00dc" && len > 0 {
            emit(data)?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_dimensions_preserve_aspect_and_are_even() {
        assert_eq!(dimensions(3840, 2160), (1920, 1080));
        assert_eq!(dimensions(1080, 1920), (1080, 1920));
        assert_eq!(dimensions(641, 401), (640, 400));
    }

    #[test]
    fn avi_stream_handles_padding_and_rejects_oversized_chunks() {
        let mut avi = b"RIFF\xff\xff\xff\xffAVI LIST\xff\xff\xff\xffmovi00dc".to_vec();
        avi.extend_from_slice(&3u32.to_le_bytes());
        avi.extend_from_slice(&[1, 2, 3, 0]);
        avi.extend_from_slice(b"00dc");
        avi.extend_from_slice(&((MAX_PACKET + 1) as u32).to_le_bytes());
        let mut frames = Vec::new();
        assert!(read_avi(&mut avi.as_slice(), |v| {
            frames.push(v);
            Ok(())
        })
        .is_err());
        assert_eq!(frames, vec![vec![1, 2, 3]]);
    }

    #[test]
    fn ffmpeg_encodes_inter_frames_and_restarts_with_headers() {
        // This integration test deliberately requires the runtime FFmpeg dependency.
        let stop = Arc::new(AtomicBool::new(false));
        for id in 1..=2 {
            let mut encoder = Encoder::new((id, 64, 64), id).unwrap();
            let mut bytes = Vec::new();
            for n in 0..4 {
                let frame = encoder.encode(&vec![n * 20; 64 * 64 * 3], &stop).unwrap();
                assert_eq!(keyframe(&frame), n == 0);
                bytes.extend_from_slice(&frame);
            }
            let path = std::env::temp_dir()
                .join(format!("cepdesk-video-{}-{id}.h264", std::process::id()));
            std::fs::write(&path, bytes).unwrap();
            let result = crate::host_process::std_command("ffmpeg")
                .args(["-v", "error", "-i"])
                .arg(&path)
                .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"])
                .output()
                .unwrap();
            std::fs::remove_file(path).unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(result.stdout.len(), 4 * 64 * 64 * 3);
        }
    }
}
