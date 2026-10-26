use super::*;
use crate::{commands::Spec, store::Store};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_rustls::{client::TlsStream, TlsConnector};
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

fn temp() -> PathBuf {
    let base = std::env::var_os("CEP_DESK_TEST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let p = base.join(format!("cepdesk-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&p).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
    p
}
fn store() -> Arc<Store> {
    Arc::new(Store::open(&temp().join("test.sqlite3")).unwrap())
}
fn spec(command: &str, timeout: u64) -> Spec {
    Spec {
        command: command.into(),
        shell: "/bin/bash".into(),
        cwd: "/tmp".into(),
        timeout_seconds: timeout,
        saved_id: None,
        keep_history: false,
    }
}
async fn terminal(jobs: &commands::Jobs, owner: &str, id: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let job = jobs.get(owner, id).unwrap();
            if job["status"] != "running" {
                return job;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap()
}
#[test]
fn clipboard_dedup_promotion_pause_limits_and_restart() {
    let path = temp().join("test.sqlite3");
    let db = Store::open(&path).unwrap();
    assert!(db.record("birinci").unwrap());
    assert!(!db.record("birinci").unwrap());
    let first = db.clips("", 0).unwrap()[0].id.clone();
    std::thread::sleep(Duration::from_millis(2));
    db.record("ikinci").unwrap();
    assert_eq!(db.clips("", 0).unwrap().len(), 2);
    std::thread::sleep(Duration::from_millis(2));
    db.promote(&first).unwrap();
    assert_eq!(db.clips("", 0).unwrap()[0].id, first);
    assert_eq!(db.clips("İLK", 0).unwrap().len(), 0);
    assert_eq!(db.clips("ikin", 0).unwrap().len(), 1);
    let mut s = db.settings().unwrap();
    s.paused = true;
    db.set_settings(s.clone()).unwrap();
    assert!(!db.record("duraklama").unwrap());
    drop(db);
    let db = Store::open(&path).unwrap();
    assert_eq!(db.settings().unwrap(), s);
    assert_eq!(db.clips("", 0).unwrap().len(), 2);
    s.paused = false;
    s.max_entries = 1;
    s.max_item_bytes = 8;
    s.max_total_bytes = 8;
    db.set_settings(s).unwrap();
    assert_eq!(db.clips("", 0).unwrap().len(), 1);
    assert_eq!(
        db.record("0123456789").unwrap_err().code,
        "CONTENT_TOO_LARGE"
    );
    db.delete_clip(Some(&first)).unwrap();
    assert_eq!(db.clips("", 0).unwrap().len(), 0);
    db.record("İZMİR").unwrap();
    assert_eq!(db.clips("izmir", 0).unwrap().len(), 1);
    db.record("yenisi").unwrap();
    db.delete_clip(None).unwrap();
    assert!(db.clips("", 0).unwrap().is_empty());
}
#[test]
fn pairing_requires_local_approval_and_is_single_use() {
    let db = store();
    let p = pairing::Pairing::default();
    let code = p.open()["code"].as_str().unwrap().to_string();
    let pending = p.request(&code, "telefon").unwrap();
    assert!(p.request(&code, "ikinci").is_err());
    let id = pending["pending_id"].as_str().unwrap();
    let secret = pending["poll_secret"].as_str().unwrap();
    assert_eq!(p.poll(id, secret).unwrap()["status"], "pending");
    assert!(db.devices().unwrap().is_empty());
    assert!(p.poll(id, "yanlış").is_err());
    p.approve(id, &db).unwrap();
    let approved = p.poll(id, secret).unwrap();
    assert!(p.poll(id, secret).is_err());
    let token = approved["token"].as_str().unwrap();
    let d = db.authenticate(token).unwrap();
    assert!(!d.free_commands);
    db.permission(&d.id, true).unwrap();
    assert!(db.authenticate(token).unwrap().free_commands);
    db.revoke(&d.id).unwrap();
    assert_eq!(db.authenticate(token).unwrap_err().code, "UNAUTHORIZED");
}
#[test]
fn pair_attempts_are_globally_limited() {
    let p = pairing::Pairing::default();
    p.open();
    for _ in 0..5 {
        assert_eq!(
            p.request("wrong", "x").unwrap_err().code,
            "PAIR_CODE_INVALID"
        );
    }
    assert_eq!(p.request("wrong", "x").unwrap_err().code, "RATE_LIMITED");
}
#[test]
fn public_and_ipv6_peers_cannot_bypass_lan_policy() {
    for ip in ["127.0.0.1", "192.168.1.20", "10.0.0.1", "169.254.1.2"] {
        assert!(network::allowed_peer(ip.parse().unwrap()));
    }
    for ip in ["8.8.8.8", "::1", "2001:4860:4860::8888"] {
        assert!(!network::allowed_peer(ip.parse().unwrap()));
    }
}
#[tokio::test]
async fn commands_stream_output_exit_cancel_timeout_and_limits() {
    let db = store();
    let (events, mut receiver) = broadcast::channel(128);
    let jobs = commands::Jobs::new(db, events);
    let job = jobs
        .start(
            "a",
            spec("printf merhaba; sleep 0.1; printf hata >&2; exit 7", 3),
        )
        .await
        .unwrap();
    let id = job["id"].as_str().unwrap();
    let event = tokio::time::timeout(Duration::from_secs(1), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.name, "job.changed");
    assert_eq!(event.owner.as_deref(), Some("a"));
    let result = terminal(&jobs, "a", id).await;
    assert_eq!(result["exit_code"], 7);
    assert_eq!(result["stdout"], "merhabahata");
    assert_eq!(result["stderr"], "");
    let job = jobs.start("a", spec("sleep 20", 1)).await.unwrap();
    assert_eq!(
        terminal(&jobs, "a", job["id"].as_str().unwrap()).await["status"],
        "timed_out"
    );
    let job = jobs.start("a", spec("sleep 20 & wait", 5)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(jobs.cancel("b", job["id"].as_str().unwrap()).is_err());
    jobs.cancel("a", job["id"].as_str().unwrap()).unwrap();
    assert_eq!(
        terminal(&jobs, "a", job["id"].as_str().unwrap()).await["status"],
        "cancelled"
    );
    let job = jobs
        .start("a", spec("head -c 200000 /dev/zero | tr '\\0' x", 3))
        .await
        .unwrap();
    let result = terminal(&jobs, "a", job["id"].as_str().unwrap()).await;
    assert!(result["output_truncated"].as_bool().unwrap());
    assert_eq!(
        result["stdout"].as_str().unwrap().len(),
        commands::MAX_OUTPUT
    );
    let a = jobs.start("a", spec("sleep 20", 5)).await.unwrap();
    let b = jobs.start("a", spec("sleep 20", 5)).await.unwrap();
    assert_eq!(
        jobs.start("a", spec("sleep 20", 5)).await.unwrap_err().code,
        "BUSY"
    );
    jobs.revoke("a");
    assert_eq!(
        terminal(&jobs, "a", a["id"].as_str().unwrap()).await["status"],
        "cancelled"
    );
    assert_eq!(
        terminal(&jobs, "a", b["id"].as_str().unwrap()).await["status"],
        "cancelled"
    );
}
#[tokio::test]
async fn cancel_kills_ordinary_child_process_group() {
    let (events, _) = broadcast::channel(128);
    let jobs = commands::Jobs::new(store(), events);
    let job = jobs
        .start("a", spec("sleep 25 & echo $!; wait", 5))
        .await
        .unwrap();
    let id = job["id"].as_str().unwrap();
    let child_pid = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let v = jobs.get("a", id).unwrap();
            if let Ok(pid) = v["stdout"].as_str().unwrap().trim().parse::<i32>() {
                break pid;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    jobs.cancel("a", id).unwrap();
    terminal(&jobs, "a", id).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    // A dead child may temporarily remain as a zombie awaiting the system reaper.
    let stat = fs::read_to_string(format!("/proc/{child_pid}/stat")).unwrap_or_default();
    assert!(stat.is_empty() || stat.contains(") Z "));
}
#[test]
fn restart_marks_running_jobs_interrupted() {
    let path = temp().join("test.sqlite3");
    let db = Store::open(&path).unwrap();
    db.put_job(
        "id",
        "a",
        &json!({"id":"id","status":"running","created_at":1}),
    )
    .unwrap();
    drop(db);
    let db = Store::open(&path).unwrap();
    assert_eq!(db.jobs("a").unwrap()[0]["status"], "interrupted");
}
type Socket = WebSocketStream<TlsStream<TcpStream>>;
async fn connect(port: u16, roots: rustls::RootCertStore) -> anyhow::Result<Socket> {
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let stream = TcpStream::connect(("127.0.0.1", port)).await?;
    let tls = TlsConnector::from(Arc::new(config))
        .connect("localhost".try_into()?, stream)
        .await?;
    Ok(
        tokio_tungstenite::client_async(format!("wss://localhost:{port}"), tls)
            .await?
            .0,
    )
}
async fn call(socket: &mut Socket, method: &str, params: Value) -> Value {
    socket
        .send(Message::Text(
            json!({"v":1,"id":"test","method":method,"params":params})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let m = socket
                .next()
                .await
                .unwrap()
                .unwrap_or_else(|error| panic!("RPC {method} failed: {error}"));
            if let Message::Text(v) = m {
                let v: Value = serde_json::from_str(&v).unwrap();
                if v["id"] == "test" {
                    return v;
                }
            }
        }
    })
    .await
    .unwrap()
}
#[tokio::test]
async fn real_tls_pairing_auth_revocation_reconnect_and_unsupported_capabilities() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let paths = Paths::new(Some(temp())).unwrap();
    let (acceptor, fingerprint, server_id) = network::identity(&paths).unwrap();
    let (events, _) = broadcast::channel(128);
    let db = store();
    let jobs = commands::Jobs::new(db.clone(), events.clone());
    let reserve = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = reserve.local_addr().unwrap().port();
    drop(reserve);
    let app = Arc::new(App {
        store: db,
        connections: Arc::new(connections::Connections::default()),
        pairing: pairing::Pairing::default(),
        password: password::Password::create("test-password-long", &temp()).unwrap(),
        desktop: desktop::Desktop::unavailable(),
        media: Arc::new(media::Media::default()),
        jobs,
        events,
        fingerprint,
        server_id,
        port,
        discovery: Mutex::new(false),
    });
    let task = tokio::spawn(network::serve(
        app.clone(),
        acceptor,
        "127.0.0.1".parse().unwrap(),
    ));
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert!(connect(port, rustls::RootCertStore::empty()).await.is_err());
    let pem = fs::read(paths.config.join("server.pem")).unwrap();
    let cert = rustls_pemfile::certs(&mut pem.as_slice())
        .next()
        .unwrap()
        .unwrap();
    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert).unwrap();
    let mut socket = connect(port, roots.clone()).await.unwrap();
    assert_eq!(
        call(&mut socket, "clipboard.list", json!({})).await["error"]["code"],
        "UNAUTHORIZED"
    );
    let mut socket = connect(port, roots.clone()).await.unwrap();
    let hello = call(
        &mut socket,
        "password.hello",
        json!({"nonce":pairing::secret(),"name":"Parolalı telefon"}),
    )
    .await;
    assert_eq!(hello["ok"], true);
    let hello = &hello["data"];
    let mut key = [0u8; 32];
    ring::pbkdf2::derive(
        ring::pbkdf2::PBKDF2_HMAC_SHA256,
        std::num::NonZeroU32::new(password::ITERATIONS).unwrap(),
        &hex::decode(hello["salt"].as_str().unwrap()).unwrap(),
        b"test-password-long",
        &mut key,
    );
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, &key);
    let transcript = hello["transcript"].as_str().unwrap();
    ring::hmac::verify(
        &key,
        format!("server:{transcript}").as_bytes(),
        &hex::decode(hello["proof"].as_str().unwrap()).unwrap(),
    )
    .unwrap();
    let proof =
        hex::encode(ring::hmac::sign(&key, format!("client:{transcript}").as_bytes()).as_ref());
    let paired = call(&mut socket, "password.pair", json!({"proof":proof})).await;
    assert_eq!(paired["ok"], true);
    let token = paired["data"]["token"].clone();
    assert_eq!(
        call(&mut socket, "auth", json!({"token":token})).await["ok"],
        true
    );
    assert_eq!(
        call(&mut socket, "system.info", json!({})).await["data"]["server_id"],
        app.server_id
    );
    assert_eq!(
        call(&mut socket, "system.power", json!({"action":"poweroff"})).await["error"]["code"],
        "CONFIRM_REQUIRED"
    );
    assert_eq!(
        call(&mut socket, "windows.close", json!({"id":"nonexistent"})).await["error"]["code"],
        "UNSUPPORTED"
    );
    assert_eq!(
        call(&mut socket, "system.audio.stop", json!({})).await["data"]["stopped"],
        true
    );
    let applications = call(&mut socket, "applications.list", json!({})).await;
    assert_eq!(applications["ok"], true);
    assert!(applications["data"]["applications"].is_array());
    assert_eq!(
        call(
            &mut socket,
            "applications.icons",
            json!({"ids": ["not-installed.desktop"]})
        )
        .await["data"]["icons"]["not-installed.desktop"],
        Value::Null
    );
    assert_eq!(
        call(
            &mut socket,
            "applications.launch",
            json!({"id": "/tmp/untrusted.desktop"})
        )
        .await["error"]["code"],
        "NOT_FOUND"
    );
    assert_eq!(
        call(
            &mut socket,
            "applications.icons",
            json!({"ids": vec!["invalid"; 25]})
        )
        .await["error"]["code"],
        "INVALID_ARGUMENT"
    );
    drop(socket);
    let mut socket = connect(port, roots.clone()).await.unwrap();
    let code = app.pairing.open()["code"].as_str().unwrap().to_string();
    let pending = call(
        &mut socket,
        "pair.request",
        json!({"code":code,"name":"test phone"}),
    )
    .await["data"]
        .clone();
    assert_eq!(
        call(&mut socket, "pair.poll", pending.clone()).await["data"]["status"],
        "pending"
    );
    app.pairing
        .approve(pending["pending_id"].as_str().unwrap(), &app.store)
        .unwrap();
    let approved = call(&mut socket, "pair.poll", pending).await["data"].clone();
    let token = approved["token"].as_str().unwrap();
    let authenticated = call(&mut socket, "auth", json!({"token":token})).await;
    assert_eq!(
        authenticated["data"]["capabilities"]["desktop"]["window_list"],
        false
    );
    assert_eq!(
        call(&mut socket, "windows.list", json!({})).await["error"]["code"],
        "UNSUPPORTED"
    );
    assert_eq!(
        call(
            &mut socket,
            "commands.run",
            json!({"command":"true","shell":"/bin/bash","cwd":"/tmp","timeout_seconds":2})
        )
        .await["error"]["code"],
        "FORBIDDEN"
    );
    let d = app.store.authenticate(token).unwrap();
    let devices = admin_request(&app, &json!({"method":"devices"}))
        .await
        .unwrap();
    let live_device = devices
        .as_array()
        .unwrap()
        .iter()
        .find(|device| device["id"] == d.id)
        .unwrap();
    assert_eq!(live_device["online"], true);
    assert!(live_device["sessions"][0]["peer"]
        .as_str()
        .unwrap()
        .starts_with("127.0.0.1:"));
    admin_request(
        &app,
        &json!({"method":"permission","id":d.id,"enabled":true}),
    )
    .await
    .unwrap();
    let definition = json!({"id":"admin-test","name":"Test command","command":"printf saved","shell":"/bin/bash","cwd":"/tmp","timeout_seconds":5});
    admin_request(&app, &json!({"method":"saved.add","value":definition}))
        .await
        .unwrap();
    let mut edited = definition.clone();
    edited["command"] = json!("printf edited");
    admin_request(&app, &json!({"method":"saved.add","value":edited}))
        .await
        .unwrap();
    assert_eq!(app.store.saved().unwrap()[0].command, "printf edited");
    admin_request(&app, &json!({"method":"saved.delete","id":"admin-test"}))
        .await
        .unwrap();
    assert!(app.store.saved().unwrap().is_empty());
    assert!(
        admin_request(&app, &json!({"method":"password.set","password":"short"}))
            .await
            .is_err()
    );
    admin_request(
        &app,
        &json!({"method":"password.set","password":"replacement-password-123"}),
    )
    .await
    .unwrap();
    assert!(app.password.configured());
    let mut settings = app.store.settings().unwrap();
    settings.paused = true;
    admin_request(&app, &json!({"method":"settings.set","value":settings}))
        .await
        .unwrap();
    assert!(app.store.settings().unwrap().paused);

    let job=call(&mut socket,"commands.run",json!({"command":"sleep 0.2; printf ok","shell":"/bin/bash","cwd":"/tmp","timeout_seconds":3})).await["data"].clone();
    socket.close(None).await.unwrap();
    let mut socket = connect(port, roots.clone()).await.unwrap();
    assert_eq!(
        call(&mut socket, "auth", json!({"token":token})).await["ok"],
        true
    );
    terminal(&app.jobs, &d.id, job["id"].as_str().unwrap()).await;
    assert_eq!(
        call(&mut socket, "jobs.get", json!({"id":job["id"]})).await["data"]["stdout"],
        "ok"
    );
    let history = admin_request(&app, &json!({"method":"jobs.list"}))
        .await
        .unwrap();
    assert!(history
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value["id"] == job["id"]));
    let local = admin_request(&app, &json!({"method":"rpc","request":{"v":1,"id":"test","method":"commands.run","params":{"command":"printf local","shell":"/bin/bash","cwd":"/tmp","timeout_seconds":5}}})).await.unwrap();
    assert_eq!(
        terminal(&app.jobs, "desktop", local["id"].as_str().unwrap()).await["stdout"],
        "local"
    );
    admin_request(
        &app,
        &json!({"method":"jobs.delete","id":local["id"],"owner":"desktop"}),
    )
    .await
    .unwrap();
    assert!(app
        .jobs
        .get("desktop", local["id"].as_str().unwrap())
        .is_err());
    let cut = admin_request(&app, &json!({"method":"disconnect","id":d.id}))
        .await
        .unwrap();
    assert_eq!(cut["disconnected"], 1);
    tokio::time::timeout(Duration::from_secs(2), async {
        while !app.connections.list(&d.id).is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(app.store.authenticate(token).is_ok());
    let mut socket = connect(port, roots.clone()).await.unwrap();
    assert_eq!(
        call(&mut socket, "auth", json!({"token":token})).await["ok"],
        true
    );
    app.store.revoke(&d.id).unwrap();
    // Revocation closes active connections on the next tick. Sending another
    // RPC races that close and can produce BrokenPipe instead of a JSON reply.
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            match socket.next().await {
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(_)) => continue,
                Some(Err(error)) => panic!("Revoked connection failed to close cleanly: {error}"),
            }
        }
    })
    .await
    .expect("Revoked connection must close");
    let mut socket = connect(port, roots).await.unwrap();
    assert_eq!(
        call(&mut socket, "auth", json!({"token":token})).await["error"]["code"],
        "UNAUTHORIZED"
    );
    task.abort();
}

#[tokio::test]
#[ignore = "Gerçek COSMIC oturumu gerekir; panoyu ve aktif pencereyi geçici değiştirir."]
async fn cosmic_session_clipboard_and_window_roundtrip() {
    use std::io::Write;
    let original = std::process::Command::new("wl-paste")
        .args(["--no-newline", "--type", "text/plain"])
        .output()
        .unwrap();
    let original = if original.status.success() {
        Some(original.stdout)
    } else {
        let types = std::process::Command::new("wl-paste")
            .arg("--list-types")
            .output()
            .unwrap();
        assert!(!types.status.success(),"Metin olmayan pano korunmalı; bu testi mevcut selection'ı temizledikten sonra çalıştırın.");
        None
    };
    struct Restore(Option<Vec<u8>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            if let Some(bytes) = &self.0 {
                let file = temp().join("original-clipboard.txt");
                fs::write(&file, bytes).unwrap();
                // A transient user unit owns the restored selection beyond the test runner lifetime.
                // It exits automatically when another application replaces the clipboard.
                let status = std::process::Command::new("systemd-run")
                    .args(["--user", "--quiet", "--collect", "--property=Type=exec"])
                    .arg(format!(
                        "--unit=cepdesk-test-restore-{}",
                        uuid::Uuid::new_v4()
                    ))
                    .arg(format!("--property=StandardInput=file:{}", file.display()))
                    .arg(format!(
                        "--setenv=WAYLAND_DISPLAY={}",
                        std::env::var("WAYLAND_DISPLAY").unwrap_or_default()
                    ))
                    .args([
                        "/usr/bin/wl-copy",
                        "--foreground",
                        "--type",
                        "text/plain;charset=utf-8",
                    ])
                    .status()
                    .unwrap();
                assert!(status.success(), "Pano geri yükleme birimi başlatılamadı.");
            } else {
                let _ = std::process::Command::new("wl-copy")
                    .arg("--clear")
                    .status();
            }
        }
    }
    let _restore = Restore(original);
    let db = store();
    let (events, _) = broadcast::channel(128);
    let desktop = desktop::Desktop::start(db.clone(), events);
    tokio::time::timeout(Duration::from_secs(5), async {
        while !desktop.features().clipboard_watch || !desktop.features().window_list {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    let marker = format!("CepDesk test {}", uuid::Uuid::new_v4());
    let mut external = std::process::Command::new("wl-copy")
        .args(["--type", "text/plain;charset=utf-8"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    external
        .stdin
        .take()
        .unwrap()
        .write_all(marker.as_bytes())
        .unwrap();
    assert!(external.wait().unwrap().success());
    let clip = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let clips = db.clips(&marker, 0).unwrap();
            if let Some(clip) = clips.into_iter().next() {
                return clip;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    desktop.write(clip.content.clone()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    for _ in 0..3 {
        let pasted = std::process::Command::new("wl-paste")
            .args(["--no-newline", "--type", "text/plain"])
            .output()
            .unwrap();
        assert!(pasted.status.success());
        assert!(
            pasted.stdout == marker.as_bytes(),
            "Servis içeriği tekrar sunmalı."
        );
    }
    assert_eq!(
        db.clips(&marker, 0).unwrap().len(),
        1,
        "Kendi yazımı bir pano döngüsü oluşturmamalı."
    );
    desktop.write(clip.content).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(db.clips(&marker, 0).unwrap().len(), 1);
    let windows = desktop.windows();
    assert!(!windows.is_empty());
    let active = windows.iter().find(|w| w.active).map(|w| w.id.clone());
    if desktop.features().window_activate {
        if let Some(other) = windows.iter().find(|w| !w.active) {
            let result = desktop.activate(other.id.clone()).await;
            if let Some(active) = active {
                let _ = desktop.activate(active).await;
            }
            assert!(
                result.is_ok(),
                "COSMIC pencere etkinleştirmesi doğrulanmalı."
            );
        }
    }
    let connection = zbus::Connection::session().await.unwrap();
    let dbus = zbus::fdo::DBusProxy::new(&connection).await.unwrap();
    let names = dbus.list_names().await.unwrap();
    let players = names
        .iter()
        .filter(|n| n.as_str().starts_with("org.mpris.MediaPlayer2."))
        .count();
    println!("Gerçek oturum: pano takip/yazma/dedup doğrulandı; {} pencere; etkinleştirme capability={}; MPRIS bus erişildi, {} oynatıcı.",windows.len(),desktop.features().window_activate,players);
}

#[tokio::test]
async fn interactive_terminal_answers_tty_prompts_without_echo_and_history_is_optional() {
    let db = store();
    let (events, _) = broadcast::channel(128);
    let jobs = commands::Jobs::new(db.clone(), events);
    let job = jobs.start("phone", spec("test -t 0 && test -t 1 && test -t 2 || exit 9; printf 'Parola: ' >/dev/tty; read -r answer </dev/tty; [ \"$answer\" = 'secret' ] || exit 8; printf 'Soru: '; read -r answer; printf 'Yanıt:%s' \"$answer\"", 5)).await.unwrap();
    let id = job["id"].as_str().unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if jobs.get("phone", id).unwrap()["stdout"]
                .as_str()
                .unwrap()
                .contains("Parola:")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert!(jobs.input("other", id, "secret\n").is_err());
    assert!(jobs.input("phone", id, &"x".repeat(4097)).is_err());
    jobs.input("phone", id, "secret\n").unwrap();
    jobs.input("phone", id, "evet\n").unwrap();
    let result = terminal(&jobs, "phone", id).await;
    assert_eq!(result["exit_code"], 0);
    assert_eq!(result["stdout"], "Parola: Soru: Yanıt:evet");
    assert!(db.jobs("phone").unwrap().is_empty());
    assert!(jobs.input("phone", id, "late\n").is_err());
    assert!(jobs.get("other", id).is_err());
    assert!(jobs.delete("other", id).is_err());
    jobs.delete("phone", id).unwrap();
    assert!(jobs.get("phone", id).is_err());
    let mut saved = spec("printf kept", 5);
    saved.keep_history = true;
    let job = jobs.start("phone", saved).await.unwrap();
    let id = job["id"].as_str().unwrap();
    let result = terminal(&jobs, "phone", id).await;
    assert_eq!(result["stdout"], "kept");
    assert_eq!(db.jobs("phone").unwrap().len(), 1);
    jobs.delete("phone", id).unwrap();
    assert!(db.jobs("phone").unwrap().is_empty());
}

#[tokio::test]
async fn infinite_output_stops_at_limit_and_terminal_controls_work() {
    let (events, _) = broadcast::channel(128);
    let jobs = commands::Jobs::new(store(), events);
    let job = jobs.start("phone", spec("yes", 5)).await.unwrap();
    let result = terminal(&jobs, "phone", job["id"].as_str().unwrap()).await;
    assert_eq!(result["status"], "output_limit");
    assert_eq!(
        result["stdout"].as_str().unwrap().len(),
        commands::MAX_OUTPUT
    );
    let job = jobs
        .start("phone", spec("read -r value; printf eof", 5))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    jobs.input("phone", job["id"].as_str().unwrap(), "\u{4}")
        .unwrap();
    assert_eq!(
        terminal(&jobs, "phone", job["id"].as_str().unwrap()).await["stdout"],
        "eof"
    );
    let job = jobs.start("phone", spec("sleep 20", 5)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    jobs.input("phone", job["id"].as_str().unwrap(), "\u{3}")
        .unwrap();
    assert_eq!(
        terminal(&jobs, "phone", job["id"].as_str().unwrap()).await["status"],
        "finished"
    );
}
