use crate::protocol::{internal, ApiError, Event, Result};
use base64::Engine;
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::broadcast;
use zbus::{zvariant::OwnedValue, Connection, Proxy};

const PATH: &str = "/org/mpris/MediaPlayer2";
const IFACE: &str = "org.mpris.MediaPlayer2.Player";
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Player {
    pub id: String,
    pub name: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: String,
    pub status: String,
    pub length_us: i64,
    pub position_us: i64,
    pub track_id: String,
    pub volume: Option<f64>,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_next: bool,
    pub can_previous: bool,
    pub can_seek: bool,
    pub can_control: bool,
    pub artwork_available: bool,
    #[serde(skip)]
    pub artwork_url: String,
}
#[derive(Default)]
pub struct Media {
    players: Mutex<Vec<Player>>,
    available: Mutex<bool>,
}
impl Media {
    pub fn start(events: broadcast::Sender<Event>) -> Arc<Self> {
        let media = Arc::new(Self::default());
        let shared = media.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                // Bounded polling also obtains Position, which MPRIS intentionally does not signal.
                let result = tokio::time::timeout(Duration::from_secs(3), enumerate()).await;
                let (ok, players) = match result {
                    Ok(Ok(p)) => (true, p),
                    _ => (false, vec![]),
                };
                let mut available = shared.available.lock().unwrap();
                if *available != ok {
                    *available = ok;
                    let _ = events.send(Event {
                        name: "capabilities.changed",
                        data: json!({"media":ok}),
                        owner: None,
                    });
                }
                let mut old = shared.players.lock().unwrap();
                if *old != players {
                    *old = players;
                    let _ = events.send(Event {
                        name: "media.changed",
                        data: json!({"players":*old}),
                        owner: None,
                    });
                }
            }
        });
        media
    }
    pub fn available(&self) -> bool {
        *self.available.lock().unwrap()
    }
    pub fn players(&self) -> Vec<Player> {
        self.players.lock().unwrap().clone()
    }
    pub async fn action(&self, id: &str, action: &str, params: &Value) -> Result<()> {
        let player = self
            .players()
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| ApiError::new("NOT_FOUND", "Oynatıcı artık mevcut değil."))?;
        let permitted = match action {
            "play" => player.can_play,
            "pause" => player.can_pause,
            "next" => player.can_next,
            "previous" => player.can_previous,
            "seek" | "position" => player.can_seek,
            "volume" => player.can_control && player.volume.is_some(),
            _ => false,
        };
        if !player.can_control || !permitted {
            return Err(ApiError::new(
                "UNSUPPORTED",
                "Oynatıcı bu kontrolü desteklemiyor.",
            ));
        }
        let connection = Connection::session().await.map_err(internal)?;
        let proxy = Proxy::new(&connection, id, PATH, IFACE)
            .await
            .map_err(internal)?;
        match action {
            "position" => {
                let position = params
                    .get("position_us")
                    .and_then(Value::as_i64)
                    .filter(|v| *v >= 0 && *v <= player.length_us)
                    .ok_or_else(|| ApiError::new("INVALID_ARGUMENT", "Geçersiz medya konumu."))?;
                if text_track(params) != Some(player.track_id.as_str()) {
                    return Err(ApiError::new(
                        "TRACK_CHANGED",
                        "Medya değişti; güncel çubukla tekrar deneyin.",
                    ));
                }
                if !player.track_id.is_empty() && !player.track_id.ends_with("/NoTrack") {
                    if let Ok(track) =
                        zbus::zvariant::ObjectPath::try_from(player.track_id.as_str())
                    {
                        if proxy
                            .call::<_, _, ()>("SetPosition", &(track, position))
                            .await
                            .is_ok()
                        {
                            return Ok(());
                        }
                    }
                }
                // Some browser MPRIS adapters expose Seek but omit a usable
                // track object or SetPosition. Use a fresh position for the delta.
                let current: i64 = proxy.get_property("Position").await.map_err(internal)?;
                proxy
                    .call::<_, _, ()>("Seek", &(position.saturating_sub(current),))
                    .await
                    .map_err(internal)?;
            }
            "seek" => {
                let offset = params
                    .get("offset_us")
                    .and_then(Value::as_i64)
                    .filter(|v| v.unsigned_abs() <= 3_600_000_000)
                    .ok_or_else(|| {
                        ApiError::new(
                            "INVALID_ARGUMENT",
                            "Sarma miktarı en fazla bir saat olabilir.",
                        )
                    })?;
                proxy
                    .call::<_, _, ()>("Seek", &(offset,))
                    .await
                    .map_err(internal)?;
            }
            "volume" => {
                let volume = params
                    .get("volume")
                    .and_then(Value::as_f64)
                    .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
                    .ok_or_else(|| {
                        ApiError::new("INVALID_ARGUMENT", "Oynatıcı sesi 0 ile 1 arasında olmalı.")
                    })?;
                proxy
                    .set_property("Volume", volume)
                    .await
                    .map_err(internal)?;
            }
            method => {
                let method = match method {
                    "play" => "Play",
                    "pause" => "Pause",
                    "next" => "Next",
                    "previous" => "Previous",
                    _ => unreachable!(),
                };
                proxy
                    .call::<_, _, ()>(method, &())
                    .await
                    .map_err(internal)?;
            }
        }
        Ok(())
    }
    pub async fn artwork(&self, id: &str) -> Result<Value> {
        let url = self
            .players()
            .into_iter()
            .find(|p| p.id == id)
            .map(|p| p.artwork_url)
            .ok_or_else(|| ApiError::new("NOT_FOUND", "Oynatıcı bulunamadı."))?;
        // Only MPRIS-provided local file URLs, no arbitrary path or URL supplied by a remote device.
        // Remote covers are explicitly unavailable: playback itself never requires Internet access.
        let url = reqwest::Url::parse(&url)
            .map_err(|_| ApiError::new("UNSUPPORTED", "Yerel kapak bulunamadı."))?;
        if url.scheme() != "file" || url.host_str().is_some_and(|v| v != "localhost") {
            return Err(ApiError::new(
                "UNSUPPORTED",
                "Bu kapak yerel dosya değil; ağ üzerinden kapak indirme kapalı.",
            ));
        }
        let path = url.to_file_path().map_err(internal)?;
        let meta = tokio::fs::metadata(&path).await.map_err(internal)?;
        if !meta.is_file() || meta.len() > 128 * 1024 {
            return Err(ApiError::new(
                "CONTENT_TOO_LARGE",
                "Kapak en fazla 128 KiB olabilir.",
            ));
        }
        use tokio::io::AsyncReadExt;
        let mut bytes = vec![];
        tokio::fs::File::open(&path)
            .await
            .map_err(internal)?
            .take(128 * 1024 + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(internal)?;
        if bytes.len() > 128 * 1024 {
            return Err(ApiError::new(
                "CONTENT_TOO_LARGE",
                "Kapak boyut sınırını aşıyor.",
            ));
        }
        let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            "image/png"
        } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            "image/jpeg"
        } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
            "image/webp"
        } else {
            return Err(ApiError::new(
                "UNSUPPORTED",
                "Yalnızca PNG, JPEG ve WebP kapakları desteklenir.",
            ));
        };
        Ok(
            json!({"uri":format!("data:{mime};base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes))}),
        )
    }
}
async fn enumerate() -> anyhow::Result<Vec<Player>> {
    let connection = Connection::session().await?;
    let dbus = zbus::fdo::DBusProxy::new(&connection).await?;
    let mut players = vec![];
    for name in dbus
        .list_names()
        .await?
        .into_iter()
        .filter(|n| n.as_str().starts_with("org.mpris.MediaPlayer2."))
        .take(16)
    {
        if let Ok(player) = read_player(&connection, name.as_str()).await {
            players.push(player);
        }
    }
    players.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(players)
}
async fn read_player(connection: &Connection, name: &str) -> anyhow::Result<Player> {
    let proxy = Proxy::new(connection, name, PATH, IFACE).await?;
    let base = Proxy::new(connection, name, PATH, "org.mpris.MediaPlayer2").await?;
    let properties = zbus::fdo::PropertiesProxy::builder(connection)
        .destination(name)?
        .path(PATH)?
        .build()
        .await?;
    let values = properties.get_all(IFACE.try_into()?).await?;
    let bool_prop = |key: &str| {
        values
            .get(key)
            .and_then(|v| bool::try_from(v).ok())
            .unwrap_or(false)
    };
    let string_prop = |key: &str| {
        values
            .get(key)
            .and_then(|v| <&str>::try_from(v).ok())
            .unwrap_or("")
            .to_string()
    };
    let metadata: HashMap<String, OwnedValue> =
        proxy.get_property("Metadata").await.unwrap_or_default();
    let string = |key: &str| {
        metadata
            .get(key)
            .and_then(|v| <&str>::try_from(v).ok())
            .unwrap_or("")
            .chars()
            .take(2048)
            .collect::<String>()
    };
    let artwork_url = string("mpris:artUrl");
    Ok(Player {
        id: name.into(),
        name: base
            .get_property::<String>("Identity")
            .await
            .unwrap_or_else(|_| name.into()),
        title: string("xesam:title"),
        artists: metadata
            .get("xesam:artist")
            .and_then(|v| v.try_clone().ok())
            .and_then(|v| Vec::<String>::try_from(v).ok())
            .unwrap_or_default(),
        album: string("xesam:album"),
        status: string_prop("PlaybackStatus"),
        length_us: metadata
            .get("mpris:length")
            .and_then(|v| i64::try_from(v).ok())
            .unwrap_or(0),
        position_us: proxy.get_property("Position").await.unwrap_or(0),
        track_id: metadata
            .get("mpris:trackid")
            .and_then(|v| <&zbus::zvariant::ObjectPath>::try_from(v).ok())
            .map(|v| v.to_string())
            .unwrap_or_default(),
        volume: values
            .get("Volume")
            .and_then(|v| f64::try_from(v).ok())
            .filter(|v| v.is_finite()),
        can_play: bool_prop("CanPlay"),
        can_pause: bool_prop("CanPause"),
        can_next: bool_prop("CanGoNext"),
        can_previous: bool_prop("CanGoPrevious"),
        can_seek: bool_prop("CanSeek"),
        can_control: bool_prop("CanControl"),
        artwork_available: artwork_url.starts_with("file:"),
        artwork_url,
    })
}

fn text_track(params: &Value) -> Option<&str> {
    params.get("track_id").and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct MockPlayer {
        calls: Arc<Mutex<Vec<(String, i64)>>>,
        fallback: bool,
    }
    #[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
    impl MockPlayer {
        fn set_position(
            &self,
            track: zbus::zvariant::OwnedObjectPath,
            position: i64,
        ) -> zbus::fdo::Result<()> {
            if self.fallback {
                return Err(zbus::fdo::Error::NotSupported(
                    "SetPosition unavailable".into(),
                ));
            }
            self.calls
                .lock()
                .unwrap()
                .push((track.to_string(), position));
            Ok(())
        }
        fn seek(&self, offset: i64) {
            self.calls.lock().unwrap().push(("Seek".into(), offset));
        }
        #[zbus(property)]
        fn position(&self) -> i64 {
            1_000_000
        }
    }
    #[tokio::test]
    async fn absolute_seek_and_browser_fallback_use_the_expected_position() {
        for fallback in [false, true] {
            let name = format!(
                "org.mpris.MediaPlayer2.CepDeskTest_{}",
                uuid::Uuid::new_v4().simple()
            );
            let calls = Arc::new(Mutex::new(vec![]));
            let _service = zbus::connection::Builder::session()
                .unwrap()
                .name(name.clone())
                .unwrap()
                .serve_at(
                    PATH,
                    MockPlayer {
                        calls: calls.clone(),
                        fallback,
                    },
                )
                .unwrap()
                .build()
                .await
                .unwrap();
            let player = Player {
                id: name,
                name: "Test".into(),
                title: "Test".into(),
                artists: vec![],
                album: String::new(),
                status: "Paused".into(),
                length_us: 10_000_000,
                position_us: 0,
                track_id: "/test/track".into(),
                volume: Some(1.0),
                can_play: true,
                can_pause: true,
                can_next: true,
                can_previous: true,
                can_seek: true,
                can_control: true,
                artwork_available: false,
                artwork_url: String::new(),
            };
            let media = Media {
                players: Mutex::new(vec![player]),
                available: Mutex::new(true),
            };
            media
                .action(
                    &media.players()[0].id,
                    "position",
                    &json!({"position_us":5_000_000,"track_id":"/test/track"}),
                )
                .await
                .unwrap();
            assert_eq!(
                calls.lock().unwrap()[0],
                if fallback {
                    ("Seek".into(), 4_000_000)
                } else {
                    ("/test/track".into(), 5_000_000)
                }
            );
            assert_eq!(
                media
                    .action(
                        &media.players()[0].id,
                        "position",
                        &json!({"position_us":5_000_000,"track_id":"/wrong/track"})
                    )
                    .await
                    .unwrap_err()
                    .code,
                "TRACK_CHANGED"
            );
            assert_eq!(
                media
                    .action(
                        &media.players()[0].id,
                        "position",
                        &json!({"position_us":11_000_000,"track_id":"/test/track"})
                    )
                    .await
                    .unwrap_err()
                    .code,
                "INVALID_ARGUMENT"
            );
            assert_eq!(calls.lock().unwrap().len(), 1);
        }
    }
}
