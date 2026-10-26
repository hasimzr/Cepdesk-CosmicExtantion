//! Fixed desktop control adapters. No remote shell command or arbitrary program.
use crate::protocol::{text, ApiError, Result};
use serde_json::{json, Value};
use std::{collections::HashMap, process::Stdio, time::Duration};
use tokio::{
    io::AsyncReadExt,
    process::{Child, ChildStdout},
};
use zbus::{
    zvariant::{OwnedObjectPath, OwnedValue},
    Connection, Proxy,
};
async fn output(program: &str, args: &[&str]) -> Result<String> {
    let mut child = crate::host_process::command(program)
        .args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| ApiError::new("UNSUPPORTED", "Gerekli masaüstü aracı bulunamadı."))?;
    let mut stdout = child.stdout.take().unwrap();
    let mut bytes = Vec::new();
    tokio::time::timeout(Duration::from_secs(4), async {
        (&mut stdout)
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .await?;
        child.wait().await
    })
    .await
    .map_err(|_| ApiError::new("TIMEOUT", "Masaüstü kontrolü yanıt vermedi."))?
    .map_err(|_| ApiError::new("SYSTEM_FAILED", "Masaüstü kontrolü başarısız."))?
    .success()
    .then_some(())
    .ok_or_else(|| {
        ApiError::new(
            "SYSTEM_FAILED",
            "İşlem reddedildi; oturum yetkilerini ve cihazı kontrol edin.",
        )
    })?;
    if bytes.len() > 1024 * 1024 {
        return Err(ApiError::new("LIMIT", "Sistem yanıtı çok büyük."));
    }
    String::from_utf8(bytes).map_err(|_| ApiError::new("SYSTEM_FAILED", "Sistem yanıtı okunamadı."))
}
fn devices(raw: &str, source: bool) -> Result<Vec<Value>> {
    let rows: Vec<Value> = serde_json::from_str(raw)
        .map_err(|_| ApiError::new("SYSTEM_FAILED", "Ses cihazları okunamadı."))?;
    Ok(rows.into_iter().filter(|row| !source || !(row["name"].as_str().is_some_and(|n| n.ends_with(".monitor")) || row["properties"]["device.class"] == "monitor" || row["monitor_of_sink"].as_u64().is_some_and(|id| id != u32::MAX as u64))).map(|row| {
        let volume = row["volume"].as_object().map(|channels| channels.values().filter_map(|v| v["value"].as_u64()).max().unwrap_or(0) as f64 / 65536.0).unwrap_or(0.0);
        json!({"id":row["name"],"name":row["description"],"volume":volume.clamp(0.0,1.0),"muted":row["mute"]})
    }).collect())
}
pub(crate) fn fields(line: &str) -> Vec<String> {
    let mut result = vec![String::new()];
    let mut escaped = false;
    for ch in line.chars() {
        if escaped {
            result.last_mut().unwrap().push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == ':' {
            result.push(String::new());
        } else {
            result.last_mut().unwrap().push(ch);
        }
    }
    if escaped {
        result.last_mut().unwrap().push('\\');
    }
    result
}
async fn wifi_status() -> Result<Value> {
    let (radio, networks, profiles) = tokio::join!(
        output("nmcli", &["-t", "radio", "wifi"]),
        output(
            "nmcli",
            &[
                "-t",
                "-e",
                "yes",
                "-f",
                "IN-USE,SSID,BSSID,SIGNAL,SECURITY",
                "device",
                "wifi",
                "list",
                "--rescan",
                "no"
            ]
        ),
        output(
            "nmcli",
            &[
                "-t",
                "-e",
                "yes",
                "-f",
                "UUID,NAME,TYPE",
                "connection",
                "show"
            ]
        )
    );
    let networks = networks?.lines().filter_map(|line| { let f = fields(line); (f.len() == 5 && !f[1].is_empty()).then(|| json!({"active":f[0]=="*","ssid":f[1],"bssid":f[2],"signal":f[3].parse::<u32>().unwrap_or(0),"security":f[4]})) }).take(128).collect::<Vec<_>>();
    let profiles = profiles?
        .lines()
        .filter_map(|line| {
            let f = fields(line);
            (f.len() == 3 && ["802-11-wireless", "wifi"].contains(&f[2].as_str()))
                .then(|| json!({"id":f[0],"name":f[1]}))
        })
        .take(128)
        .collect::<Vec<_>>();
    Ok(json!({"enabled":radio?.trim()=="enabled","networks":networks,"profiles":profiles}))
}
pub async fn status() -> Value {
    let (sinks, sources, sink, source, wifi) = tokio::join!(
        output("pactl", &["--format=json", "list", "sinks"]),
        output("pactl", &["--format=json", "list", "sources"]),
        output("pactl", &["get-default-sink"]),
        output("pactl", &["get-default-source"]),
        wifi_status()
    );
    let audio = match (
        sinks.and_then(|s| devices(&s, false)),
        sources.and_then(|s| devices(&s, true)),
        sink,
        source,
    ) {
        (Ok(sinks), Ok(sources), Ok(sink), Ok(source)) => {
            json!({"available":true,"sinks":sinks,"sources":sources,"sink":sink.trim(),"source":source.trim()})
        }
        _ => json!({"available":false,"reason":"PulseAudio/PipeWire ses oturumuna erişilemiyor."}),
    };
    let wifi = wifi.unwrap_or_else(
        |_| json!({"available":false,"reason":"NetworkManager Wi-Fi bilgileri alınamadı."}),
    );
    let power = power_capabilities()
        .await
        .unwrap_or_else(|_| json!({"poweroff":false,"reboot":false,"suspend":false}));
    json!({"audio":audio,"wifi":wifi,"power":power,"phone_audio": std::path::Path::new("/usr/bin/parec").exists()})
}
async fn power_capabilities() -> Result<Value> {
    let bus = Connection::system()
        .await
        .map_err(|_| ApiError::new("UNSUPPORTED", "Sistem D-Bus erişimi yok."))?;
    let proxy = Proxy::new(
        &bus,
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )
    .await
    .map_err(|_| ApiError::new("UNSUPPORTED", "logind yok."))?;
    let (off, reboot, sleep) = tokio::join!(
        proxy.call::<_, _, String>("CanPowerOff", &()),
        proxy.call::<_, _, String>("CanReboot", &()),
        proxy.call::<_, _, String>("CanSuspend", &())
    );
    Ok(
        json!({"poweroff":off.is_ok_and(|v|v=="yes"),"reboot":reboot.is_ok_and(|v|v=="yes"),"suspend":sleep.is_ok_and(|v|v=="yes")}),
    )
}
fn fraction(p: &Value) -> Result<f64> {
    p["volume"]
        .as_f64()
        .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
        .ok_or_else(|| ApiError::new("INVALID_ARGUMENT", "Ses 0–100% arasında olmalı."))
}
pub async fn action(p: &Value) -> Result<Value> {
    let action = text(p, "action", 32)?;
    match action {
        "volume" | "mic_volume" | "mute" | "mic_mute" | "sink" | "source" => {
            let source = ["mic_volume", "mic_mute", "source"].contains(&action);
            let default = if source {
                "@DEFAULT_SOURCE@"
            } else {
                "@DEFAULT_SINK@"
            };
            let command = match action {
                "volume" => "set-sink-volume",
                "mic_volume" => "set-source-volume",
                "mute" => "set-sink-mute",
                "mic_mute" => "set-source-mute",
                "sink" => "set-default-sink",
                _ => "set-default-source",
            };
            if action == "sink" || action == "source" {
                let id = text(p, "id", 512)?;
                let raw = output(
                    "pactl",
                    &[
                        "--format=json",
                        "list",
                        if source { "sources" } else { "sinks" },
                    ],
                )
                .await?;
                if !devices(&raw, source)?.iter().any(|d| d["id"] == id) {
                    return Err(ApiError::new("NOT_FOUND", "Ses cihazı artık mevcut değil."));
                }
                output("pactl", &[command, id]).await?;
                // Move existing application streams as well as changing the default.
                let raw = output(
                    "pactl",
                    &[
                        "--format=json",
                        "list",
                        if source {
                            "source-outputs"
                        } else {
                            "sink-inputs"
                        },
                    ],
                )
                .await?;
                let streams: Vec<Value> = serde_json::from_str(&raw).unwrap_or_default();
                for stream in streams.into_iter().take(64) {
                    if source && stream["properties"]["application.name"] == "CepDesk-phone-speaker"
                    {
                        continue;
                    }
                    if let Some(index) = stream["index"].as_u64() {
                        let _ = output(
                            "pactl",
                            &[
                                if source {
                                    "move-source-output"
                                } else {
                                    "move-sink-input"
                                },
                                &index.to_string(),
                                id,
                            ],
                        )
                        .await;
                    }
                }
            } else if action == "volume" || action == "mic_volume" {
                output(
                    "pactl",
                    &[
                        command,
                        default,
                        &format!("{}%", (fraction(p)? * 100.0).round()),
                    ],
                )
                .await?;
            } else {
                let mute = p["muted"]
                    .as_bool()
                    .ok_or_else(|| ApiError::new("INVALID_ARGUMENT", "Sessiz durumu geçersiz."))?;
                output("pactl", &[command, default, if mute { "1" } else { "0" }]).await?;
            }
        }
        "wifi_radio" => {
            let on = p["enabled"]
                .as_bool()
                .ok_or_else(|| ApiError::new("INVALID_ARGUMENT", "Wi-Fi durumu geçersiz."))?;
            output("nmcli", &["radio", "wifi", if on { "on" } else { "off" }]).await?;
        }
        _ => {
            return Err(ApiError::new(
                "METHOD_NOT_FOUND",
                "Bilinmeyen sistem kontrolü.",
            ))
        }
    }
    Ok(json!({"changed":true}))
}
pub async fn wifi_connect(p: &Value) -> Result<Value> {
    if let Some(id) = p["profile"].as_str() {
        let profiles = wifi_status().await?;
        if !profiles["profiles"]
            .as_array()
            .unwrap()
            .iter()
            .any(|profile| profile["id"] == id)
        {
            return Err(ApiError::new("NOT_FOUND", "Kayıtlı Wi-Fi ağı bulunamadı."));
        }
        let bus = Connection::system()
            .await
            .map_err(|_| ApiError::new("UNSUPPORTED", "NetworkManager D-Bus erişimi yok."))?;
        let settings = Proxy::new(
            &bus,
            "org.freedesktop.NetworkManager",
            "/org/freedesktop/NetworkManager/Settings",
            "org.freedesktop.NetworkManager.Settings",
        )
        .await
        .map_err(|_| ApiError::new("UNSUPPORTED", "NetworkManager ayarları bulunamadı."))?;
        let profile: OwnedObjectPath = settings
            .call("GetConnectionByUuid", &(id,))
            .await
            .map_err(|_| ApiError::new("NOT_FOUND", "Wi-Fi profili artık mevcut değil."))?;
        let manager = Proxy::new(
            &bus,
            "org.freedesktop.NetworkManager",
            "/org/freedesktop/NetworkManager",
            "org.freedesktop.NetworkManager",
        )
        .await
        .map_err(|_| ApiError::new("UNSUPPORTED", "NetworkManager bulunamadı."))?;
        let _: OwnedObjectPath = tokio::time::timeout(
            Duration::from_secs(5),
            manager.call(
                "ActivateConnection",
                &(
                    profile,
                    OwnedObjectPath::try_from("/").unwrap(),
                    OwnedObjectPath::try_from("/").unwrap(),
                ),
            ),
        )
        .await
        .map_err(|_| ApiError::new("TIMEOUT", "Wi-Fi işlemi yanıt vermedi."))?
        .map_err(|_| ApiError::new("SYSTEM_FAILED", "Wi-Fi bağlantısı reddedildi."))?;
        return Ok(json!({"connecting":true}));
    }
    let ssid = text(p, "ssid", 128)?;
    if ssid.len() > 32 {
        return Err(ApiError::new(
            "INVALID_ARGUMENT",
            "Wi-Fi adı en fazla 32 bayt olabilir.",
        ));
    }
    let password = p["password"].as_str().unwrap_or("");
    if !password.is_empty() && !(8..=63).contains(&password.len()) {
        return Err(ApiError::new(
            "INVALID_ARGUMENT",
            "Wi-Fi parolası 8–63 karakter olmalı.",
        ));
    }
    let bus = Connection::system()
        .await
        .map_err(|_| ApiError::new("UNSUPPORTED", "NetworkManager D-Bus erişimi yok."))?;
    let proxy = Proxy::new(
        &bus,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager",
        "org.freedesktop.NetworkManager",
    )
    .await
    .map_err(|_| ApiError::new("UNSUPPORTED", "NetworkManager bulunamadı."))?;
    let device_lines = output("nmcli", &["-t", "-f", "DEVICE,TYPE", "device"]).await?;
    let device = device_lines
        .lines()
        .map(fields)
        .find(|f| f.len() == 2 && f[1] == "wifi")
        .ok_or_else(|| ApiError::new("UNSUPPORTED", "Wi-Fi adaptörü bulunamadı."))?;
    let device_path: OwnedObjectPath = proxy
        .call("GetDeviceByIpIface", &(device[0].as_str(),))
        .await
        .map_err(|_| ApiError::new("SYSTEM_FAILED", "Wi-Fi cihazı bulunamadı."))?;
    let value = |s: &str| OwnedValue::try_from(zbus::zvariant::Value::from(s)).unwrap();
    let mut settings: HashMap<&str, HashMap<&str, OwnedValue>> = HashMap::new();
    settings.insert(
        "connection",
        HashMap::from([
            ("id", value(ssid)),
            ("type", value("802-11-wireless")),
            ("uuid", value(&uuid::Uuid::new_v4().to_string())),
        ]),
    );
    settings.insert(
        "802-11-wireless",
        HashMap::from([
            (
                "ssid",
                OwnedValue::try_from(zbus::zvariant::Value::from(ssid.as_bytes().to_vec()))
                    .unwrap(),
            ),
            ("mode", value("infrastructure")),
        ]),
    );
    settings.insert("ipv4", HashMap::from([("method", value("auto"))]));
    settings.insert("ipv6", HashMap::from([("method", value("auto"))]));
    if !password.is_empty() {
        settings.insert(
            "802-11-wireless-security",
            HashMap::from([
                (
                    "key-mgmt",
                    value(
                        if p["security"]
                            .as_str()
                            .is_some_and(|s| s.contains("WPA3") && !s.contains("WPA2"))
                        {
                            "sae"
                        } else {
                            "wpa-psk"
                        },
                    ),
                ),
                ("psk", value(password)),
            ]),
        );
    }
    let _: (OwnedObjectPath, OwnedObjectPath) = tokio::time::timeout(
        Duration::from_secs(5),
        proxy.call(
            "AddAndActivateConnection",
            &(
                settings,
                device_path,
                OwnedObjectPath::try_from("/").unwrap(),
            ),
        ),
    )
    .await
    .map_err(|_| ApiError::new("TIMEOUT", "Wi-Fi bağlantısı başlatılamadı."))?
    .map_err(|_| {
        ApiError::new(
            "SYSTEM_FAILED",
            "Wi-Fi bağlantısı reddedildi. Parolayı ve oturum yetkisini kontrol edin.",
        )
    })?;
    Ok(json!({"connecting":true}))
}
pub async fn power(p: &Value) -> Result<Value> {
    if p["confirmed"] != true {
        return Err(ApiError::new(
            "CONFIRM_REQUIRED",
            "Güç işlemi için açık onay gerekiyor.",
        ));
    }
    let method = match text(p, "action", 32)? {
        "poweroff" => "PowerOff",
        "reboot" => "Reboot",
        "suspend" => "Suspend",
        _ => return Err(ApiError::new("INVALID_ARGUMENT", "Geçersiz güç işlemi.")),
    };
    let bus = Connection::system()
        .await
        .map_err(|_| ApiError::new("UNSUPPORTED", "Sistem D-Bus erişimi yok."))?;
    let proxy = Proxy::new(
        &bus,
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )
    .await
    .map_err(|_| ApiError::new("UNSUPPORTED", "logind bulunamadı."))?;
    proxy
        .call::<_, _, ()>(method, &(false,))
        .await
        .map_err(|_| {
            ApiError::new(
                "FORBIDDEN",
                "Güç işlemi oturum ilkesi tarafından reddedildi.",
            )
        })?;
    Ok(json!({"requested":true}))
}
pub struct AudioStream {
    child: Child,
    stdout: ChildStdout,
    carry: Vec<u8>,
}
impl AudioStream {
    pub async fn start() -> Result<Self> {
        let sink = output("pactl", &["get-default-sink"]).await?;
        let mut child = crate::host_process::command("parec")
            .args([
                "--format=s16le",
                "--rate=24000",
                "--channels=2",
                "--latency-msec=40",
                "--client-name=CepDesk-phone-speaker",
                "--stream-name=CepDesk-system-audio",
            ])
            .arg(format!("--device={}.monitor", sink.trim()))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| {
                ApiError::new("UNSUPPORTED", "Telefon hoparlörü için parec gereklidir.")
            })?;
        let stdout = child.stdout.take().unwrap();
        Ok(Self {
            child,
            stdout,
            carry: Vec::new(),
        })
    }
    pub async fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let mut scratch = [0u8; 1920];
        loop {
            let n = self.stdout.read(&mut scratch).await?;
            if n == 0 {
                return Ok(0);
            }
            self.carry.extend_from_slice(&scratch[..n]);
            let aligned = (self.carry.len().min(buffer.len()) / 4) * 4;
            if aligned > 0 {
                buffer[..aligned].copy_from_slice(&self.carry[..aligned]);
                self.carry.drain(..aligned);
                return Ok(aligned);
            }
        }
    }
}
impl Drop for AudioStream {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn network_fields_preserve_escaped_ssids_and_bssids() {
        assert_eq!(
            fields("*:A\\:B:AA\\:BB\\:CC:80:WPA2"),
            vec!["*", "A:B", "AA:BB:CC", "80", "WPA2"]
        );
    }
    #[test]
    fn volume_validation_blocks_out_of_range_and_non_numeric_values() {
        assert_eq!(fraction(&json!({"volume":0.5})).unwrap(), 0.5);
        for value in [json!(-1), json!(1.1), json!("0.5"), Value::Null] {
            assert!(fraction(&json!({"volume":value})).is_err());
        }
    }
    #[test]
    fn microphones_exclude_monitor_sources_and_keep_real_channel_levels() {
        let raw = json!([
            {"name":"output.monitor","description":"Monitor","properties":{"device.class":"monitor"},"volume":{"left":{"value":65536}}},
            {"name":"real-mic","description":"Mic","monitor_of_sink":4294967295u64,"mute":true,"volume":{"left":{"value":32768}}}
        ]).to_string();
        let sources = devices(&raw, true).unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0]["id"], "real-mic");
        assert_eq!(sources[0]["volume"], 0.5);
    }
    #[tokio::test]
    async fn pcm_chunks_remain_aligned_when_reads_are_cancelled_mid_frame() {
        use tokio::io::AsyncWriteExt;
        let mut child = crate::host_process::command("/bin/cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let writer = tokio::spawn(async move {
            stdin.write_all(&[0, 1]).await.unwrap();
            tokio::time::sleep(Duration::from_millis(60)).await;
            stdin
                .write_all(&(2u8..19).collect::<Vec<_>>())
                .await
                .unwrap();
        });
        let mut stream = AudioStream {
            child,
            stdout,
            carry: Vec::new(),
        };
        let mut buffer = [0u8; 1920];
        assert!(
            tokio::time::timeout(Duration::from_millis(20), stream.read(&mut buffer))
                .await
                .is_err()
        );
        let mut bytes = vec![];
        loop {
            let n = stream.read(&mut buffer).await.unwrap();
            if n == 0 {
                break;
            }
            assert_eq!(n % 4, 0);
            bytes.extend_from_slice(&buffer[..n]);
        }
        assert_eq!(bytes, (0u8..16).collect::<Vec<_>>());
        writer.await.unwrap();
    }
    #[tokio::test]
    #[ignore = "Gerçek kullanıcı ses oturumundan yalnızca kısa bir PCM örneği okur; ses içeriğini kaydetmez"]
    async fn live_monitor_produces_pcm_and_status_is_read_only() {
        let info = status().await;
        assert_eq!(info["audio"]["available"], true);
        assert!(info["audio"]["sinks"]
            .as_array()
            .is_some_and(|s| !s.is_empty()));
        let mut stream = AudioStream::start().await.unwrap();
        let mut buffer = [0u8; 1920];
        let n = tokio::time::timeout(Duration::from_secs(3), stream.read(&mut buffer))
            .await
            .unwrap()
            .unwrap();
        assert!(n > 0);
        assert_eq!(n % 4, 0);
        drop(stream);
    }
    #[tokio::test]
    async fn power_requires_confirmation_before_touching_the_system() {
        assert_eq!(
            power(&json!({"action":"poweroff"})).await.unwrap_err().code,
            "CONFIRM_REQUIRED"
        );
    }
}
