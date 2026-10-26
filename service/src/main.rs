mod applications;
mod commands;
mod config;
mod connections;
mod desktop;
mod host_process;
mod input;
mod media;
mod monitor;
mod network;
mod pairing;
mod password;
mod protocol;
mod store;
mod system;
mod terminal;
#[cfg(test)]
mod tests;
mod video;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use config::Paths;
use network::App;
use serde_json::{json, Value};
use std::{
    fs,
    net::IpAddr,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    sync::broadcast,
};

#[derive(Parser)]
#[command(version, about = "COSMIC için yerel ağ çalışma yardımcısı")]
struct Cli {
    #[arg(long, global = true)]
    state_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Cmd,
}
#[derive(Subcommand)]
enum Cmd {
    Serve {
        #[arg(long, default_value = "0.0.0.0")]
        bind: IpAddr,
        #[arg(long, default_value_t = 43821)]
        port: u16,
    },
    /// Ortak bağlantı parolasını belirle/değiştir (servis kapalıyken).
    Password,
    Status,
    Pair,
    Pending,
    Approve {
        id: String,
    },
    Deny {
        id: String,
    },
    Devices,
    Revoke {
        id: String,
    },
    Permission {
        id: String,
        #[arg(value_parser=["on","off"])]
        free_commands: String,
    },
    Saved {
        #[command(subcommand)]
        action: Saved,
    },
}
#[derive(Subcommand)]
enum Saved {
    List,
    Add {
        name: String,
        command: String,
        #[arg(long, default_value = "/bin/bash")]
        shell: String,
        #[arg(long)]
        cwd: String,
        #[arg(long, default_value_t = 60)]
        timeout: u64,
    },
    Delete {
        id: String,
    },
}
#[tokio::main]
async fn main() -> Result<()> {
    // Must precede creation of secret files, database, and Unix socket.
    // SAFETY: performed before the application creates any secret files.
    unsafe {
        libc::umask(0o077);
    }
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cli = Cli::parse();
    let paths = Paths::new(cli.state_dir)?;
    if let Cmd::Serve { bind, port } = cli.command {
        return run(paths, bind, port).await;
    }
    if let Cmd::Password = cli.command {
        if UnixStream::connect(&paths.socket).await.is_ok() {
            anyhow::bail!("Parolayı değiştirmeden önce servisi durdurun");
        }
        password::Password::configure(&paths.config)?;
        println!("Bağlantı parolası kaydedildi. Servisi başlatabilirsiniz.");
        return Ok(());
    }
    let request = match cli.command {
        Cmd::Status => json!({"method":"status"}),
        Cmd::Pair => json!({"method":"pair"}),
        Cmd::Pending => json!({"method":"pending"}),
        Cmd::Approve { id } => json!({"method":"approve","id":id}),
        Cmd::Deny { id } => json!({"method":"deny","id":id}),
        Cmd::Devices => json!({"method":"devices"}),
        Cmd::Revoke { id } => json!({"method":"revoke","id":id}),
        Cmd::Permission { id, free_commands } => {
            json!({"method":"permission","id":id,"enabled":free_commands=="on"})
        }
        Cmd::Saved { action } => match action {
            Saved::List => json!({"method":"saved.list"}),
            Saved::Delete { id } => json!({"method":"saved.delete","id":id}),
            Saved::Add {
                name,
                command,
                shell,
                cwd,
                timeout,
            } => {
                json!({"method":"saved.add","value":{"id":uuid::Uuid::new_v4().to_string(),"name":name,"command":command,"shell":shell,"cwd":cwd,"timeout_seconds":timeout}})
            }
        },
        Cmd::Serve { .. } | Cmd::Password => unreachable!(),
    };
    let mut socket = UnixStream::connect(&paths.socket).await.context(
        "Servis çalışmıyor; önce cepdesk serve veya systemd kullanıcı servisini başlatın",
    )?;
    socket.write_all(format!("{request}\n").as_bytes()).await?;
    let mut response = String::new();
    BufReader::new(socket).read_line(&mut response).await?;
    let result: Value = serde_json::from_str(&response)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    if result["ok"] == false {
        std::process::exit(1);
    }
    Ok(())
}
async fn run(paths: Paths, bind: IpAddr, port: u16) -> Result<()> {
    if !bind.is_ipv4() {
        anyhow::bail!("İlk sürüm yalnızca IPv4 dinleme adreslerini destekler");
    }
    if port == 0 {
        anyhow::bail!("Sabit bir port seçin (1–65535)");
    }
    // A second daemon must not replace the first daemon's control socket.
    if paths.socket.exists() {
        if UnixStream::connect(&paths.socket).await.is_ok() {
            anyhow::bail!("Servis zaten çalışıyor");
        }
        fs::remove_file(&paths.socket)?;
    }
    let password = password::Password::load_or_configure(&paths.config)?;
    let admin = UnixListener::bind(&paths.socket)?;
    fs::set_permissions(&paths.socket, fs::Permissions::from_mode(0o600))?;
    let (acceptor, fingerprint, server_id) = network::identity(&paths)?;
    let store = Arc::new(store::Store::open(&paths.data.join("cepdesk.sqlite3"))?);
    let (events, _) = broadcast::channel(128);
    let desktop = desktop::Desktop::start(store.clone(), events.clone());
    let media = media::Media::start(events.clone());
    let jobs = commands::Jobs::new(store.clone(), events.clone());
    let app = Arc::new(App {
        store,
        connections: Arc::new(connections::Connections::default()),
        pairing: pairing::Pairing::default(),
        password,
        desktop,
        media,
        jobs,
        events,
        fingerprint,
        server_id,
        port,
        discovery: Mutex::new(false),
    });
    let discovery = network::advertise(&app).ok();
    let admin_app = app.clone();
    let admin_task = tokio::spawn(async move { admin_server(admin, admin_app).await });
    println!("CepDesk hazır; port {port}. Telefonda bilgisayarı seçip ortak bağlantı parolanızı girin. Pano ve komut içerikleri loglanmaz.");
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {result=network::serve(app.clone(),acceptor,bind)=>result?,_=tokio::signal::ctrl_c()=>{},_=terminate.recv()=>{}}
    app.jobs.shutdown();
    tokio::time::sleep(Duration::from_millis(300)).await;
    admin_task.abort();
    if let Some(d) = discovery {
        let _ = d.shutdown();
    }
    let _ = fs::remove_file(paths.socket);
    Ok(())
}
async fn admin_server(listener: UnixListener, app: Arc<App>) -> Result<()> {
    loop {
        let (socket, _) = listener.accept().await?;
        let app = app.clone();
        tokio::spawn(async move {
            if !socket
                .peer_cred()
                .is_ok_and(|c| c.uid() == unsafe { libc::getuid() })
            {
                return;
            }
            let mut socket = BufReader::new(socket);
            let mut raw = String::new();
            // Unix stream is user-only, but enforce size and time bounds anyway.
            let result = tokio::time::timeout(
                Duration::from_secs(5),
                read_admin_line(&mut socket, &mut raw),
            )
            .await;
            if !matches!(result, Ok(Ok(_))) || raw.len() > 32768 {
                return;
            }
            let result = match serde_json::from_str::<Value>(&raw) {
                Ok(p) => admin_request(&app, &p).await,
                Err(_) => Err(protocol::ApiError::new(
                    "INVALID_MESSAGE",
                    "Geçersiz CLI isteği.",
                )),
            };
            let result = protocol::response("cli", result);
            let _ = socket
                .get_mut()
                .write_all(format!("{result}\n").as_bytes())
                .await;
        });
    }
}
async fn admin_request(app: &Arc<App>, p: &Value) -> protocol::Result<Value> {
    let id = || protocol::text(p, "id", 128);
    match p["method"].as_str().unwrap_or("") {
        "status" => Ok(
            json!({"server_id":app.server_id,"port":app.port,"fingerprint_sha256":app.fingerprint,"desktop":app.desktop.features(),"media":app.media.available(),"discovery":*app.discovery.lock().unwrap(),"settings":app.store.settings()?,"password_configured":app.password.configured(),"devices":admin_devices(app)?}),
        ),
        "pair" => Ok(
            json!({"pairing":app.pairing.open(),"fingerprint_sha256":app.fingerprint,"port":app.port}),
        ),
        "pending" => Ok(app.pairing.pending()),
        "approve" => app.pairing.approve(id()?, &app.store),
        "deny" => Ok(app.pairing.deny(id()?)),
        "devices" => Ok(json!(admin_devices(app)?)),
        "disconnect" => Ok(json!({"disconnected":app.connections.disconnect(id()?)})),
        "password.set" => {
            let value = protocol::text(p, "password", 256)?.to_string();
            let app = app.clone();
            tokio::task::spawn_blocking(move || app.password.change(&value))
                .await
                .map_err(protocol::internal)?
                .map_err(protocol::internal)?;
            Ok(json!({"configured":true}))
        }
        "settings.set" => {
            let settings: store::Settings =
                serde_json::from_value(p["value"].clone()).map_err(protocol::internal)?;
            app.store.set_settings(settings.clone())?;
            let _ = app.events.send(protocol::Event {
                name: "settings.changed",
                data: json!(settings),
                owner: None,
            });
            Ok(json!(settings))
        }
        "jobs.list" => {
            let mut jobs = app.jobs.list("desktop")?;
            for device in app.store.devices()? {
                jobs.extend(app.jobs.list(&device.id)?);
            }
            jobs.sort_by_key(|job| std::cmp::Reverse(job["created_at"].as_i64().unwrap_or(0)));
            jobs.truncate(100);
            for job in &mut jobs {
                job["stdout"] = json!("");
                job["stderr"] = json!("");
                if let Some(command) = job["spec"]["command"].as_str() {
                    job["spec"]["command"] = json!(command.chars().take(256).collect::<String>());
                }
            }
            Ok(json!(jobs))
        }
        "jobs.get" | "jobs.cancel" | "jobs.delete" | "jobs.input" => {
            let owner = protocol::text(p, "owner", 128)?;
            let job_id = id()?;
            match p["method"].as_str().unwrap() {
                "jobs.get" => app.jobs.get(owner, job_id),
                "jobs.cancel" => {
                    app.jobs.cancel(owner, job_id)?;
                    Ok(json!({}))
                }
                "jobs.delete" => {
                    app.jobs.delete(owner, job_id)?;
                    Ok(json!({}))
                }
                _ => {
                    app.jobs
                        .input(owner, job_id, protocol::text(p, "data", 4096)?)?;
                    Ok(json!({}))
                }
            }
        }
        "rpc" => {
            let request: protocol::Request =
                serde_json::from_value(p["request"].clone()).map_err(protocol::internal)?;
            if request.v != 1 || request.method.len() > 64 {
                return Err(protocol::ApiError::new(
                    "INVALID_ARGUMENT",
                    "Geçersiz servis isteği.",
                ));
            }
            let local = store::Device {
                id: "desktop".into(),
                name: "Yerel yönetim".into(),
                free_commands: true,
                created_at: 0,
            };
            app.request(&local, &request).await
        }
        "revoke" => {
            let id = id()?;
            app.store.revoke(id)?;
            app.jobs.revoke(id);
            app.connections.disconnect(id);
            Ok(json!({"revoked":id}))
        }
        "permission" => {
            app.store
                .permission(id()?, p["enabled"].as_bool().unwrap_or(false))?;
            let _ = app.events.send(protocol::Event {
                name: "capabilities.changed",
                data: json!({}),
                owner: None,
            });
            Ok(json!({}))
        }
        "saved.list" => Ok(json!(app.store.saved()?)),
        "saved.add" => {
            let saved: store::SavedCommand =
                serde_json::from_value(p["value"].clone()).map_err(protocol::internal)?;
            commands::validate_saved(&saved)?;
            if app.store.saved()?.len() >= 100
                && !app.store.saved()?.iter().any(|value| value.id == saved.id)
            {
                return Err(protocol::ApiError::new(
                    "LIMIT",
                    "En fazla 100 kayıtlı komut.",
                ));
            }
            app.store.save_command(&saved)?;
            let _ = app.events.send(protocol::Event {
                name: "commands.changed",
                data: json!({}),
                owner: None,
            });
            Ok(json!(saved))
        }
        "saved.delete" => {
            app.store.delete_command(id()?)?;
            let _ = app.events.send(protocol::Event {
                name: "commands.changed",
                data: json!({}),
                owner: None,
            });
            Ok(json!({}))
        }
        _ => Err(protocol::ApiError::new(
            "METHOD_NOT_FOUND",
            "Bilinmeyen CLI işlemi.",
        )),
    }
}

async fn read_admin_line(
    socket: &mut BufReader<UnixStream>,
    raw: &mut String,
) -> std::io::Result<usize> {
    use tokio::io::AsyncReadExt;
    let mut bytes = vec![];
    let mut byte = [0u8; 1];
    while bytes.len() <= 32768 {
        if socket.read(&mut byte).await? == 0 {
            break;
        }
        bytes.push(byte[0]);
        if byte[0] == b'\n' {
            break;
        }
    }
    *raw = String::from_utf8(bytes)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "UTF-8"))?;
    Ok(raw.len())
}

fn admin_devices(app: &App) -> protocol::Result<Vec<Value>> {
    app.store
        .devices()?
        .into_iter()
        .map(|device| {
            let sessions = app.connections.list(&device.id);
            let mut value = json!(device);
            value["online"] = json!(!sessions.is_empty());
            value["sessions"] = json!(sessions);
            Ok(value)
        })
        .collect()
}
