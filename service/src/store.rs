use crate::protocol::{internal, ApiError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
pub fn hash(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    pub paused: bool,
    pub max_entries: usize,
    pub max_total_bytes: usize,
    pub max_item_bytes: usize,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            paused: false,
            max_entries: 500,
            max_total_bytes: 8 * 1024 * 1024,
            max_item_bytes: 64 * 1024,
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<()> {
        if self.max_entries == 0
            || self.max_entries > 5000
            || self.max_item_bytes == 0
            || self.max_item_bytes > 64 * 1024
            || self.max_total_bytes < self.max_item_bytes
            || self.max_total_bytes > 64 * 1024 * 1024
        {
            return Err(ApiError::new(
                "INVALID_ARGUMENT",
                "Kayıt: 1–5000, içerik: en fazla 64 KiB, toplam: içerik boyutu–64 MiB olmalı.",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub free_commands: bool,
    pub created_at: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Clip {
    pub id: String,
    pub mime: String,
    pub content: String,
    pub created_at: i64,
    pub promoted_at: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedCommand {
    pub id: String,
    pub name: String,
    pub command: String,
    pub shell: String,
    pub cwd: String,
    pub timeout_seconds: u64,
}
pub struct Store {
    conn: Mutex<Connection>,
}
impl Store {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        conn.create_scalar_function(
            "casefold",
            1,
            rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| {
                let value: String = ctx.get(0)?;
                Ok(value
                    .chars()
                    .flat_map(|c| match c {
                        'İ' => 'i'.to_lowercase(),
                        'I' => 'ı'.to_lowercase(),
                        _ => c.to_lowercase(),
                    })
                    .collect::<String>())
            },
        )?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA secure_delete=ON; PRAGMA busy_timeout=5000;
          CREATE TABLE IF NOT EXISTS settings (id INTEGER PRIMARY KEY CHECK(id=1), value TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS devices (id TEXT PRIMARY KEY, name TEXT NOT NULL, token_hash TEXT UNIQUE NOT NULL, free_commands INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL);
          CREATE TABLE IF NOT EXISTS clipboard (id TEXT PRIMARY KEY, hash TEXT UNIQUE NOT NULL, mime TEXT NOT NULL, content TEXT NOT NULL, bytes INTEGER NOT NULL, created_at INTEGER NOT NULL, promoted_at INTEGER NOT NULL);
          CREATE TABLE IF NOT EXISTS saved_commands (id TEXT PRIMARY KEY, value TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS jobs (id TEXT PRIMARY KEY, owner TEXT NOT NULL, value TEXT NOT NULL);")?;
        conn.execute(
            "INSERT OR IGNORE INTO settings VALUES (1, ?1)",
            [serde_json::to_string(&Settings::default())?],
        )?;
        // Processes cannot survive a service restart reliably. Never leave stale 'running' jobs.
        let jobs: Vec<(String, String)> = conn
            .prepare("SELECT id,value FROM jobs")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?;
        for (id, raw) in jobs {
            let mut v: Value = serde_json::from_str(&raw)?;
            if v["status"] == "running" {
                v["status"] = json!("interrupted");
                v["finished_at"] = json!(now());
                conn.execute(
                    "UPDATE jobs SET value=?2 WHERE id=?1",
                    params![id, v.to_string()],
                )?;
            }
        }
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
    pub fn settings(&self) -> Result<Settings> {
        let c = self.conn.lock().unwrap();
        let s: String = c
            .query_row("SELECT value FROM settings WHERE id=1", [], |r| r.get(0))
            .map_err(internal)?;
        serde_json::from_str(&s).map_err(internal)
    }
    pub fn set_settings(&self, settings: Settings) -> Result<()> {
        settings.validate()?;
        let mut c = self.conn.lock().unwrap();
        let tx = c.transaction().map_err(internal)?;
        tx.execute(
            "UPDATE settings SET value=?1 WHERE id=1",
            [serde_json::to_string(&settings).map_err(internal)?],
        )
        .map_err(internal)?;
        Self::prune(&tx, &settings)?;
        tx.commit().map_err(internal)
    }
    fn prune(c: &Connection, s: &Settings) -> Result<()> {
        c.execute("DELETE FROM clipboard WHERE bytes > ?1", [s.max_item_bytes])
            .map_err(internal)?;
        c.execute("DELETE FROM clipboard WHERE id NOT IN (SELECT id FROM clipboard ORDER BY promoted_at DESC, rowid DESC LIMIT ?1)", [s.max_entries]).map_err(internal)?;
        loop {
            let bytes: i64 = c
                .query_row("SELECT COALESCE(SUM(bytes),0) FROM clipboard", [], |r| {
                    r.get(0)
                })
                .map_err(internal)?;
            if bytes <= s.max_total_bytes as i64 {
                break;
            }
            c.execute("DELETE FROM clipboard WHERE id=(SELECT id FROM clipboard ORDER BY promoted_at, rowid LIMIT 1)", []).map_err(internal)?;
        }
        Ok(())
    }
    pub fn record(&self, content: &str) -> Result<bool> {
        let s = self.settings()?;
        if s.paused || content.is_empty() {
            return Ok(false);
        }
        if content.len() > s.max_item_bytes {
            return Err(ApiError::new(
                "CONTENT_TOO_LARGE",
                "Pano içeriği kayıt sınırını aşıyor.",
            ));
        }
        let mut c = self.conn.lock().unwrap();
        let tx = c.transaction().map_err(internal)?;
        let inserted = tx.execute("INSERT OR IGNORE INTO clipboard VALUES (?1,?2,'text/plain;charset=utf-8',?3,?4,?5,?5)", params![Uuid::new_v4().to_string(), hash(content.as_bytes()), content, content.len(), now()]).map_err(internal)? > 0;
        Self::prune(&tx, &s)?;
        tx.commit().map_err(internal)?;
        Ok(inserted)
    }
    pub fn clips(&self, query: &str, offset: usize) -> Result<Vec<Clip>> {
        let c = self.conn.lock().unwrap();
        let mut stmt = c.prepare("SELECT id,mime,content,created_at,promoted_at FROM clipboard WHERE instr(casefold(content),casefold(?1)) > 0 ORDER BY promoted_at DESC, rowid DESC LIMIT 50 OFFSET ?2").map_err(internal)?;
        let result = stmt
            .query_map(params![query, offset], |r| {
                Ok(Clip {
                    id: r.get(0)?,
                    mime: r.get(1)?,
                    content: r.get(2)?,
                    created_at: r.get(3)?,
                    promoted_at: r.get(4)?,
                })
            })
            .map_err(internal)?
            .collect::<std::result::Result<_, _>>()
            .map_err(internal);
        result
    }
    pub fn clip(&self, id: &str) -> Result<Clip> {
        self.conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT id,mime,content,created_at,promoted_at FROM clipboard WHERE id=?1",
                [id],
                |r| {
                    Ok(Clip {
                        id: r.get(0)?,
                        mime: r.get(1)?,
                        content: r.get(2)?,
                        created_at: r.get(3)?,
                        promoted_at: r.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(internal)?
            .ok_or_else(|| ApiError::new("NOT_FOUND", "Pano kaydı bulunamadı."))
    }
    pub fn promote(&self, id: &str) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE clipboard SET promoted_at=?2 WHERE id=?1",
                params![id, now()],
            )
            .map_err(internal)?;
        Ok(())
    }
    pub fn delete_clip(&self, id: Option<&str>) -> Result<()> {
        let c = self.conn.lock().unwrap();
        match id {
            Some(id) => c.execute("DELETE FROM clipboard WHERE id=?1", [id]),
            None => c.execute("DELETE FROM clipboard", []),
        }
        .map_err(internal)?;
        Ok(())
    }
    pub fn add_device(&self, name: &str, token: &str) -> Result<Device> {
        let d = Device {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            free_commands: false,
            created_at: now(),
        };
        self.conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO devices VALUES (?1,?2,?3,0,?4)",
                params![d.id, d.name, hash(token.as_bytes()), d.created_at],
            )
            .map_err(internal)?;
        Ok(d)
    }
    pub fn devices(&self) -> Result<Vec<Device>> {
        let c = self.conn.lock().unwrap();
        let mut s = c
            .prepare("SELECT id,name,free_commands,created_at FROM devices ORDER BY created_at")
            .map_err(internal)?;
        let result = s
            .query_map([], |r| {
                Ok(Device {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    free_commands: r.get(2)?,
                    created_at: r.get(3)?,
                })
            })
            .map_err(internal)?
            .collect::<std::result::Result<_, _>>()
            .map_err(internal);
        result
    }
    pub fn authenticate(&self, token: &str) -> Result<Device> {
        let c = self.conn.lock().unwrap();
        c.query_row(
            "SELECT id,name,free_commands,created_at FROM devices WHERE token_hash=?1",
            [hash(token.as_bytes())],
            |r| {
                Ok(Device {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    free_commands: r.get(2)?,
                    created_at: r.get(3)?,
                })
            },
        )
        .optional()
        .map_err(internal)?
        .ok_or_else(|| {
            ApiError::new(
                "UNAUTHORIZED",
                "Cihaz eşleşmemiş veya yetkisi iptal edilmiş.",
            )
        })
    }
    pub fn device(&self, id: &str) -> Result<Device> {
        self.devices()?
            .into_iter()
            .find(|d| d.id == id)
            .ok_or_else(|| ApiError::new("UNAUTHORIZED", "Cihaz yetkisi iptal edildi."))
    }
    pub fn revoke(&self, id: &str) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM devices WHERE id=?1", [id])
            .map_err(internal)?;
        Ok(())
    }
    pub fn permission(&self, id: &str, enabled: bool) -> Result<()> {
        self.device(id)?;
        self.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE devices SET free_commands=?2 WHERE id=?1",
                params![id, enabled],
            )
            .map_err(internal)?;
        Ok(())
    }
    pub fn saved(&self) -> Result<Vec<SavedCommand>> {
        let c = self.conn.lock().unwrap();
        let mut s = c
            .prepare("SELECT value FROM saved_commands ORDER BY rowid")
            .map_err(internal)?;
        let raw = s
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(internal)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(internal)?;
        raw.into_iter()
            .map(|v| serde_json::from_str(&v).map_err(internal))
            .collect()
    }
    pub fn save_command(&self, v: &SavedCommand) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute(
                "INSERT OR REPLACE INTO saved_commands VALUES (?1,?2)",
                params![v.id, serde_json::to_string(v).map_err(internal)?],
            )
            .map_err(internal)?;
        Ok(())
    }
    pub fn delete_command(&self, id: &str) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM saved_commands WHERE id=?1", [id])
            .map_err(internal)?;
        Ok(())
    }
    pub fn put_job(&self, id: &str, owner: &str, value: &Value) -> Result<()> {
        let c = self.conn.lock().unwrap();
        c.execute(
            "INSERT OR REPLACE INTO jobs VALUES (?1,?2,?3)",
            params![id, owner, value.to_string()],
        )
        .map_err(internal)?;
        c.execute("DELETE FROM jobs WHERE id IN (SELECT id FROM jobs WHERE json_extract(value,'$.status') != 'running' ORDER BY json_extract(value,'$.created_at') DESC LIMIT -1 OFFSET 100)",[]).map_err(internal)?;
        Ok(())
    }
    pub fn delete_job(&self, owner: &str, id: &str) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute(
                "DELETE FROM jobs WHERE owner=?1 AND id=?2",
                params![owner, id],
            )
            .map_err(internal)?;
        Ok(())
    }
    pub fn jobs(&self, owner: &str) -> Result<Vec<Value>> {
        let c = self.conn.lock().unwrap();
        let mut s=c.prepare("SELECT value FROM jobs WHERE owner=?1 ORDER BY json_extract(value,'$.created_at') DESC LIMIT 100").map_err(internal)?;
        let raw = s
            .query_map([owner], |r| r.get::<_, String>(0))
            .map_err(internal)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(internal)?;
        raw.into_iter()
            .map(|v| serde_json::from_str(&v).map_err(internal))
            .collect()
    }
}
