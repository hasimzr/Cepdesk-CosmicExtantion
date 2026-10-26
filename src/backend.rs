use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

pub const UNIT: &str = "cepdesk.service";
pub fn socket_path() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("CEPDESK_ADMIN_SOCKET") {
        return Ok(path.into());
    }
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(|path| PathBuf::from(path).join("cepdesk/admin.sock"))
        .ok_or_else(|| "Kullanıcı oturumu bulunamadı (XDG_RUNTIME_DIR).".into())
}
pub async fn request(value: Value) -> Result<Value, String> {
    let run = async {
        let socket = UnixStream::connect(socket_path()?).await.map_err(|_| "CepDesk servisi bağlı değil. Servisi başlatın; kurulu değilse yerel CepDesk paketini kurun.".to_string())?;
        let mut socket = BufReader::new(socket);
        socket
            .get_mut()
            .write_all(format!("{value}\n").as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        // A local response may contain one bounded job output, never unbounded input.
        let mut response = String::new();
        use tokio::io::AsyncReadExt;
        socket
            .take(1024 * 1024 + 1)
            .read_line(&mut response)
            .await
            .map_err(|e| e.to_string())?;
        if response.len() > 1024 * 1024 {
            return Err("Servis yanıtı çok büyük.".into());
        }
        let response: Value = serde_json::from_str(&response).map_err(|e| e.to_string())?;
        if response["ok"].as_bool() != Some(true) {
            return Err(response["error"]["message"]
                .as_str()
                .unwrap_or("İşlem tamamlanamadı.")
                .into());
        }
        Ok(response["data"].clone())
    };
    tokio::time::timeout(Duration::from_secs(8), run)
        .await
        .map_err(|_| "Servis yanıt vermedi.".to_string())?
}
pub async fn snapshot() -> Result<Value, String> {
    let status = request(json!({"method":"status"})).await?;
    let saved = request(json!({"method":"saved.list"})).await?;
    let pending = request(json!({"method":"pending"})).await?;
    let jobs = request(json!({"method":"jobs.list"}))
        .await
        .map_err(|error| {
            if error.contains("Bilinmeyen CLI") {
                "Kurulu CepDesk servisi eski. Güncel paketi kurup servisi yeniden başlatın."
                    .to_string()
            } else {
                error
            }
        })?;
    Ok(json!({"status":status,"saved":saved,"pending":pending,"jobs":jobs}))
}
pub async fn startup_snapshot() -> Result<Value, String> {
    // Run once at launch, never during polling: Stop must stay stopped.
    if std::env::var("FLATPAK_ID").as_deref() == Ok("io.github.hasimzr.CepDesk") {
        let output = tokio::time::timeout(
            Duration::from_secs(30),
            tokio::process::Command::new("/app/libexec/cepdesk-flatpak-setup").output(),
        )
        .await
        .map_err(|_| "Paket içindeki servis kurulumu zaman aşımına uğradı.".to_string())?
        .map_err(|e| format!("Paket içindeki servis kurulumu başlatılamadı: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "Paket içindeki servis hazırlanamadı: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    }
    let connection = zbus::Connection::session()
        .await
        .map_err(|e| e.to_string())?;
    let proxy = zbus::Proxy::new(
        &connection,
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
    )
    .await
    .map_err(|e| e.to_string())?;
    let state: Result<String, _> = proxy.call("GetUnitFileState", &(UNIT,)).await;
    if matches!(state.as_deref(), Ok("enabled" | "enabled-runtime")) {
        service("start").await?;
        // StartUnit queues a job; the admin socket can appear a little later.
        let ready = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(value) = snapshot().await {
                    return value;
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        })
        .await;
        if let Ok(value) = ready {
            return Ok(value);
        }
    }
    snapshot().await
}
pub async fn service(action: &str) -> Result<Value, String> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|e| e.to_string())?;
    let proxy = zbus::Proxy::new(
        &connection,
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
    )
    .await
    .map_err(|e| e.to_string())?;
    match action {
        "start" | "stop" | "restart" => {
            let method = match action {
                "start" => "StartUnit",
                "stop" => "StopUnit",
                _ => "RestartUnit",
            };
            let _: zbus::zvariant::OwnedObjectPath = proxy
                .call(method, &(UNIT, "replace"))
                .await
                .map_err(|e| e.to_string())?;
        }
        "enable" => {
            let _: Vec<(String, String, String)> = proxy
                .call("UnmaskUnitFiles", &(vec![UNIT], false))
                .await
                .map_err(|e| e.to_string())?;
            let _: (bool, Vec<(String, String, String)>) = proxy
                .call("EnableUnitFiles", &(vec![UNIT], false, false))
                .await
                .map_err(|e| e.to_string())?;
            let _: () = proxy.call("Reload", &()).await.map_err(|e| e.to_string())?;
            let _: zbus::zvariant::OwnedObjectPath = proxy
                .call("StartUnit", &(UNIT, "replace"))
                .await
                .map_err(|e| e.to_string())?;
        }
        "disable" => {
            let _: Vec<(String, String, String)> = proxy
                .call("DisableUnitFiles", &(vec![UNIT], false))
                .await
                .map_err(|e| e.to_string())?;
            // A user mask overrides the package's default wants symlink.
            let _: Vec<(String, String, String)> = proxy
                .call("MaskUnitFiles", &(vec![UNIT], false, false))
                .await
                .map_err(|e| e.to_string())?;
            let _: () = proxy.call("Reload", &()).await.map_err(|e| e.to_string())?;
        }
        _ => return Err("Geçersiz servis işlemi.".into()),
    }
    Ok(json!({"service_action":action}))
}
pub fn rpc(method: &str, params: Value) -> Value {
    json!({"method":"rpc","request":{"v":1,"id":"applet","method":method,"params":params}})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct FakePortal {
        urls: Arc<Mutex<Vec<String>>>,
    }
    #[zbus::interface(name = "org.freedesktop.portal.OpenURI")]
    impl FakePortal {
        #[zbus(name = "OpenURI")]
        fn open_uri(
            &self,
            parent: &str,
            uri: &str,
            _options: std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
        ) -> zbus::zvariant::OwnedObjectPath {
            assert!(parent.is_empty());
            self.urls.lock().unwrap().push(uri.into());
            zbus::zvariant::OwnedObjectPath::try_from(
                "/org/freedesktop/portal/desktop/request/test",
            )
            .unwrap()
        }
    }
    #[tokio::test]
    async fn project_and_developer_links_use_the_desktop_portal() {
        let urls = Arc::new(Mutex::new(Vec::new()));
        let _bus = zbus::connection::Builder::session()
            .unwrap()
            .name("org.freedesktop.portal.Desktop")
            .unwrap()
            .serve_at(
                "/org/freedesktop/portal/desktop",
                FakePortal { urls: urls.clone() },
            )
            .unwrap()
            .build()
            .await
            .unwrap();
        open_link(Link::Project).await.unwrap();
        open_link(Link::Developer).await.unwrap();
        assert_eq!(
            *urls.lock().unwrap(),
            [
                "https://cepdesk.hasimozer.com",
                "https://github.com/hasimzr"
            ]
        );
    }

    struct FakeManager {
        state: Arc<Mutex<String>>,
        calls: Arc<Mutex<Vec<String>>>,
    }
    type Changes = Vec<(String, String, String)>;
    #[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
    impl FakeManager {
        fn get_unit_file_state(&self, unit: &str) -> String {
            assert_eq!(unit, UNIT);
            self.state.lock().unwrap().clone()
        }
        fn start_unit(&self, unit: &str, mode: &str) -> zbus::zvariant::OwnedObjectPath {
            assert_eq!((unit, mode), (UNIT, "replace"));
            self.calls.lock().unwrap().push("start".into());
            zbus::zvariant::OwnedObjectPath::try_from("/org/freedesktop/systemd1/job/1").unwrap()
        }
        fn enable_unit_files(
            &self,
            units: Vec<String>,
            runtime: bool,
            force: bool,
        ) -> (bool, Changes) {
            assert_eq!(units, vec![UNIT]);
            assert!(!runtime && !force);
            self.calls.lock().unwrap().push("enable".into());
            *self.state.lock().unwrap() = "enabled".into();
            (true, vec![])
        }
        fn disable_unit_files(&self, units: Vec<String>, runtime: bool) -> Changes {
            assert_eq!(units, vec![UNIT]);
            assert!(!runtime);
            self.calls.lock().unwrap().push("disable".into());
            vec![]
        }
        fn mask_unit_files(&self, units: Vec<String>, runtime: bool, force: bool) -> Changes {
            assert_eq!(units, vec![UNIT]);
            assert!(!runtime && !force);
            self.calls.lock().unwrap().push("mask".into());
            *self.state.lock().unwrap() = "masked".into();
            vec![]
        }
        fn unmask_unit_files(&self, units: Vec<String>, runtime: bool) -> Changes {
            assert_eq!(units, vec![UNIT]);
            assert!(!runtime);
            self.calls.lock().unwrap().push("unmask".into());
            vec![]
        }
        fn reload(&self) {
            self.calls.lock().unwrap().push("reload".into());
        }
    }

    #[tokio::test]
    async fn startup_respects_opt_out_and_enable_starts_service() {
        let state = Arc::new(Mutex::new("disabled".to_string()));
        let calls = Arc::new(Mutex::new(Vec::new()));
        // Run under dbus-run-session: never contact the real session manager.
        let _bus = zbus::connection::Builder::session()
            .unwrap()
            .name("org.freedesktop.systemd1")
            .unwrap()
            .serve_at(
                "/org/freedesktop/systemd1",
                FakeManager {
                    state: state.clone(),
                    calls: calls.clone(),
                },
            )
            .unwrap()
            .build()
            .await
            .unwrap();
        let socket =
            std::env::temp_dir().join(format!("cepdesk-applet-test-{}.sock", std::process::id()));
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        unsafe {
            std::env::set_var("CEPDESK_ADMIN_SOCKET", &socket);
        }
        let server = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let mut stream = BufReader::new(stream);
                let mut line = String::new();
                stream.read_line(&mut line).await.unwrap();
                stream
                    .get_mut()
                    .write_all(b"{\"ok\":true,\"data\":{}}\n")
                    .await
                    .unwrap();
            }
        });
        startup_snapshot().await.unwrap();
        assert!(calls.lock().unwrap().is_empty());
        service("enable").await.unwrap();
        assert_eq!(
            *calls.lock().unwrap(),
            ["unmask", "enable", "reload", "start"]
        );
        calls.lock().unwrap().clear();
        startup_snapshot().await.unwrap();
        assert_eq!(*calls.lock().unwrap(), ["start"]);
        calls.lock().unwrap().clear();
        snapshot().await.unwrap();
        assert!(calls.lock().unwrap().is_empty());
        service("disable").await.unwrap();
        assert_eq!(*calls.lock().unwrap(), ["disable", "mask", "reload"]);
        calls.lock().unwrap().clear();
        startup_snapshot().await.unwrap();
        assert!(calls.lock().unwrap().is_empty());
        server.abort();
        std::fs::remove_file(socket).unwrap();
        unsafe {
            std::env::remove_var("CEPDESK_ADMIN_SOCKET");
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Link {
    Project,
    Developer,
}
impl Link {
    pub fn url(self) -> &'static str {
        match self {
            Self::Project => "https://cepdesk.hasimozer.com",
            Self::Developer => "https://github.com/hasimzr",
        }
    }
}
/// The desktop portal opens the user's browser, including inside Flatpak.
pub async fn open_link(link: Link) -> Result<(), String> {
    let operation = async {
        let connection = zbus::Connection::session()
            .await
            .map_err(|e| e.to_string())?;
        let proxy = zbus::Proxy::new(
            &connection,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.OpenURI",
        )
        .await
        .map_err(|e| e.to_string())?;
        let options: std::collections::HashMap<&str, zbus::zvariant::Value<'_>> =
            std::collections::HashMap::new();
        let _: zbus::zvariant::OwnedObjectPath = proxy
            .call("OpenURI", &("", link.url(), options))
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    };
    tokio::time::timeout(Duration::from_secs(8), operation)
        .await
        .map_err(|e| e.to_string())?
}
