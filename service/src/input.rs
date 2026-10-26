//! Session-owned Linux uinput device; closing it releases every held key.
use crate::protocol::{ApiError, Result};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::Write,
    os::fd::AsRawFd,
};
#[repr(C)]
struct Setup {
    id: [u16; 4],
    name: [u8; 80],
    effects: u32,
}
pub struct Input {
    file: File,
    held: BTreeSet<u16>,
    absolute: bool,
}
fn unavailable() -> ApiError {
    ApiError::new("INPUT_UNAVAILABLE", "Sanal giriş aygıtı açılamadı. /dev/uinput iznini kurulum belgesine göre ayarlayıp servisi yeniden başlatın.")
}
fn ioctl(file: &File, request: libc::c_ulong, value: libc::c_ulong) -> Result<()> {
    // SAFETY: these ioctls accept an integer, not a userspace pointer.
    if unsafe { libc::ioctl(file.as_raw_fd(), request, value) } < 0 {
        Err(unavailable())
    } else {
        Ok(())
    }
}
pub fn available() -> bool {
    OpenOptions::new().write(true).open("/dev/uinput").is_ok()
}
impl Input {
    pub fn new() -> Result<Self> {
        Self::create(false)
    }
    pub fn new_absolute() -> Result<Self> {
        Self::create(true)
    }
    fn create(absolute: bool) -> Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .open("/dev/uinput")
            .map_err(|_| unavailable())?;
        for kind in if absolute {
            vec![1, 2, 3]
        } else {
            vec![1, 2, 20]
        } {
            ioctl(&file, 0x40045564, kind)?;
        }
        for code in (if absolute { 0..=0 } else { 1..=248 })
            .chain(272..=274)
            .filter(|c| *c != 0)
        {
            ioctl(&file, 0x40045565, code)?;
        }
        for axis in if absolute {
            vec![6, 8]
        } else {
            vec![0, 1, 6, 8]
        } {
            ioctl(&file, 0x40045566, axis)?;
        }
        if absolute {
            #[repr(C)]
            struct Abs {
                code: u16,
                pad: u16,
                info: [i32; 6],
            }
            for code in [0u16, 1] {
                ioctl(&file, 0x40045567, code as libc::c_ulong)?;
                let info = Abs {
                    code,
                    pad: 0,
                    info: [0, 0, 65535, 0, 0, 0],
                };
                // SAFETY: Abs matches Linux uinput_abs_setup; pointer remains valid during ioctl.
                if unsafe { libc::ioctl(file.as_raw_fd(), 0x401c5504, &info) } < 0 {
                    return Err(unavailable());
                }
            }
        }
        let mut setup = Setup {
            id: [3, 0x1209, 1, 1],
            name: [0; 80],
            effects: 0,
        };
        let name = b"CepDesk remote keyboard and mouse";
        setup.name[..name.len()].copy_from_slice(name);
        // SAFETY: Setup matches Linux uinput_setup and remains alive for ioctl.
        if unsafe { libc::ioctl(file.as_raw_fd(), 0x405c5503, &setup) } < 0 {
            return Err(unavailable());
        }
        ioctl(&file, 0x5501, 0)?;
        Ok(Self {
            file,
            held: BTreeSet::new(),
            absolute,
        })
    }
    fn event(&mut self, kind: u16, code: u16, value: i32) -> Result<()> {
        let mut bytes = vec![0; std::mem::size_of::<libc::timeval>()];
        bytes.extend(kind.to_ne_bytes());
        bytes.extend(code.to_ne_bytes());
        bytes.extend(value.to_ne_bytes());
        self.file.write_all(&bytes).map_err(|_| unavailable())
    }
    pub fn release(&mut self) -> Result<()> {
        for code in self.held.clone() {
            self.event(1, code, 0)?;
            self.held.remove(&code);
        }
        self.event(0, 0, 0)
    }
    pub fn request(&mut self, method: &str, p: &Value) -> Result<Value> {
        match method {
            "input.start" | "input.keepalive" => {}
            "input.release" => self.release()?,
            "input.key" => {
                let code = p["code"]
                    .as_u64()
                    .filter(|c| (1..=248).contains(c) || (272..=274).contains(c))
                    .ok_or_else(|| ApiError::new("INVALID_ARGUMENT", "Geçersiz tuş kodu."))?
                    as u16;
                let down = p["down"]
                    .as_bool()
                    .ok_or_else(|| ApiError::new("INVALID_ARGUMENT", "down boolean olmalı."))?;
                let value = if down {
                    if self.held.contains(&code) {
                        2
                    } else {
                        1
                    }
                } else {
                    0
                };
                self.event(1, code, value)?;
                if down {
                    self.held.insert(code);
                } else {
                    self.held.remove(&code);
                }
                self.event(0, 0, 0)?;
            }
            "input.absolute" => {
                if !self.absolute {
                    return Err(ApiError::new(
                        "INVALID_ARGUMENT",
                        "Bu aygıt göreli hareket kullanıyor.",
                    ));
                }
                let value = |key: &str| {
                    p[key]
                        .as_i64()
                        .filter(|v| (0..=65535).contains(v))
                        .map(|v| v as i32)
                        .ok_or_else(|| {
                            ApiError::new("INVALID_ARGUMENT", "Mutlak koordinat geçersiz.")
                        })
                };
                let x = value("x")?;
                let y = value("y")?;
                self.event(3, 0, x)?;
                self.event(3, 1, y)?;
                self.event(0, 0, 0)?;
            }
            "input.move" | "input.scroll" => {
                let x = delta(p, "x")?;
                let y = delta(p, "y")?;
                let (ax, ay) = if method == "input.move" {
                    (0, 1)
                } else {
                    (6, 8)
                };
                self.event(2, ax, x)?;
                self.event(2, ay, y)?;
                self.event(0, 0, 0)?;
            }
            _ => {
                return Err(ApiError::new(
                    "METHOD_NOT_FOUND",
                    "Giriş yöntemi bulunamadı.",
                ))
            }
        }
        Ok(json!({"applied":true}))
    }
}
fn delta(p: &Value, key: &str) -> Result<i32> {
    p[key]
        .as_i64()
        .filter(|n| (-2048..=2048).contains(n))
        .map(|n| n as i32)
        .ok_or_else(|| {
            ApiError::new(
                "INVALID_ARGUMENT",
                "Hareket -2048..2048 arası tamsayı olmalı.",
            )
        })
}
impl Drop for Input {
    fn drop(&mut self) {
        let _ = self.release();
        let _ = ioctl(&self.file, 0x5502, 0);
    }
}
