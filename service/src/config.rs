use anyhow::{Context, Result};
use std::{env, fs, os::unix::fs::PermissionsExt, path::PathBuf};

#[derive(Clone)]
pub struct Paths {
    pub config: PathBuf,
    pub data: PathBuf,
    pub state: PathBuf,
    pub socket: PathBuf,
}
impl Paths {
    pub fn new(isolated: Option<PathBuf>) -> Result<Self> {
        let home = PathBuf::from(env::var("HOME").context("HOME eksik")?);
        let paths = if let Some(root) = isolated {
            Self {
                config: root.join("config"),
                data: root.join("data"),
                state: root.join("state"),
                socket: root.join("admin.sock"),
            }
        } else {
            let dir = |name: &str, fallback: &str| {
                env::var_os(name)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join(fallback))
                    .join("cepdesk")
            };
            let runtime = PathBuf::from(
                env::var("XDG_RUNTIME_DIR")
                    .context("XDG_RUNTIME_DIR eksik; kullanıcı oturumunda çalıştırın")?,
            );
            Self {
                config: dir("XDG_CONFIG_HOME", ".config"),
                data: dir("XDG_DATA_HOME", ".local/share"),
                state: dir("XDG_STATE_HOME", ".local/state"),
                socket: runtime.join("cepdesk/admin.sock"),
            }
        };
        for p in [
            &paths.config,
            &paths.data,
            &paths.state,
            paths.socket.parent().unwrap(),
        ] {
            fs::create_dir_all(p)?;
            fs::set_permissions(p, fs::Permissions::from_mode(0o700))?;
        }
        Ok(paths)
    }
}
