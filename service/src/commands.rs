use crate::{
    protocol::{internal, ApiError, Event, Result},
    store::{now, SavedCommand, Store},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{broadcast, mpsc, watch, OwnedSemaphorePermit, Semaphore};
use uuid::Uuid;

pub const MAX_OUTPUT: usize = 128 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Spec {
    pub command: String,
    pub shell: String,
    pub cwd: String,
    pub timeout_seconds: u64,
    pub saved_id: Option<String>,
    #[serde(default)]
    pub keep_history: bool,
}
impl Spec {
    pub fn validate(&self) -> Result<()> {
        if self.command.is_empty()
            || self.command.len() > 8192
            || self.command.contains('\0')
            || !(1..=3600).contains(&self.timeout_seconds)
            || !Path::new(&self.shell).is_absolute()
            || !Path::new(&self.cwd).is_absolute()
            || !Path::new(&self.cwd).is_dir()
            || !Path::new(&self.shell).is_file()
        {
            return Err(ApiError::new("INVALID_ARGUMENT","Komut en fazla 8192 bayt; shell ve mevcut çalışma dizini mutlak yol; zaman aşımı 1–3600 saniye olmalı."));
        }
        Ok(())
    }
}
impl From<SavedCommand> for Spec {
    fn from(s: SavedCommand) -> Self {
        Self {
            command: s.command,
            shell: s.shell,
            cwd: s.cwd,
            timeout_seconds: s.timeout_seconds,
            saved_id: Some(s.id),
            keep_history: false,
        }
    }
}
#[derive(Clone)]
struct Active {
    owner: String,
    cancel: watch::Sender<bool>,
    input: mpsc::Sender<Vec<u8>>,
}
pub struct Jobs {
    store: Arc<Store>,
    events: broadcast::Sender<Event>,
    running: Mutex<HashMap<String, Active>>,
    slots: Arc<Semaphore>,
    records: Mutex<HashMap<String, Value>>,
}
impl Jobs {
    pub fn new(store: Arc<Store>, events: broadcast::Sender<Event>) -> Arc<Self> {
        Arc::new(Self {
            store,
            events,
            running: Mutex::new(HashMap::new()),
            slots: Arc::new(Semaphore::new(4)),
            records: Mutex::new(HashMap::new()),
        })
    }
    pub async fn start(self: &Arc<Self>, owner: &str, spec: Spec) -> Result<Value> {
        spec.validate()?;
        let permit = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| ApiError::new("BUSY", "Aynı anda en fazla dört komut çalışabilir."))?;
        let mut running = self.running.lock().unwrap();
        if running.values().filter(|a| a.owner == owner).count() >= 2 {
            return Err(ApiError::new(
                "BUSY",
                "Cihaz başına en fazla iki komut çalışabilir.",
            ));
        }
        let id = Uuid::new_v4().to_string();
        let (cancel, rx) = watch::channel(false);
        let job = json!({"id":id,"owner":owner,"spec":spec,"status":"running","created_at":now(),"finished_at":null,"exit_code":null,"stdout":"","stderr":"","output_truncated":false});
        if spec.keep_history {
            self.store.put_job(&id, owner, &job)?;
        }
        self.records.lock().unwrap().insert(id.clone(), job.clone());
        let (input, input_rx) = mpsc::channel(16);
        running.insert(
            id.clone(),
            Active {
                owner: owner.into(),
                cancel,
                input,
            },
        );
        drop(running);
        let shared = self.clone();
        let owner = owner.to_string();
        let initial = job.clone();
        tokio::spawn(async move {
            shared.run(owner, spec, initial, rx, input_rx, permit).await;
        });
        Ok(job)
    }
    pub fn cancel(&self, owner: &str, id: &str) -> Result<()> {
        let running = self.running.lock().unwrap();
        let active = running
            .get(id)
            .filter(|j| j.owner == owner)
            .ok_or_else(|| ApiError::new("NOT_FOUND", "Çalışan iş bulunamadı."))?;
        let _ = active.cancel.send(true);
        Ok(())
    }
    pub fn revoke(&self, owner: &str) {
        for active in self
            .running
            .lock()
            .unwrap()
            .values()
            .filter(|j| j.owner == owner)
        {
            let _ = active.cancel.send(true);
        }
    }
    pub fn shutdown(&self) {
        for active in self.running.lock().unwrap().values() {
            let _ = active.cancel.send(true);
        }
    }
    fn publish(&self, owner: &str, job: &Value) {
        if job["spec"]["keep_history"].as_bool() == Some(true) {
            let _ = self.store.put_job(job["id"].as_str().unwrap(), owner, job);
        }
        let mut records = self.records.lock().unwrap();
        records.insert(job["id"].as_str().unwrap().into(), job.clone());
        if records.len() > 104 {
            let oldest = records
                .iter()
                .filter(|(_, job)| job["status"] != "running")
                .min_by_key(|(_, job)| job["created_at"].as_u64().unwrap_or(0))
                .map(|(id, _)| id.clone());
            if let Some(id) = oldest {
                records.remove(&id);
            }
        }
        drop(records);
        let _ = self.events.send(Event {
            name: "job.changed",
            data: json!({"id":job["id"],"status":job["status"]}),
            owner: Some(owner.into()),
        });
    }
    async fn run(
        self: Arc<Self>,
        owner: String,
        spec: Spec,
        mut job: Value,
        mut cancel: watch::Receiver<bool>,
        mut input: mpsc::Receiver<Vec<u8>>,
        _permit: OwnedSemaphorePermit,
    ) {
        let (mut child, terminal) =
            match crate::terminal::Terminal::spawn(&spec.shell, &spec.command, &spec.cwd) {
                Ok(v) => v,
                Err(_) => {
                    job["status"] = json!("failed");
                    job["stderr"] = json!("Shell başlatılamadı.");
                    job["finished_at"] = json!(now());
                    self.publish(&owner, &job);
                    self.running
                        .lock()
                        .unwrap()
                        .remove(job["id"].as_str().unwrap());
                    return;
                }
            };
        let pid = child.id().unwrap() as i32;
        let (tx, mut output) = mpsc::channel::<(bool, Vec<u8>)>(16);
        let out_task = tokio::spawn(reader(terminal.clone(), tx));
        let deadline = tokio::time::sleep(Duration::from_secs(spec.timeout_seconds));
        tokio::pin!(deadline);
        let mut bytes = 0;
        let mut output_closed = false;
        let mut out_bytes = Vec::new();
        let mut err_bytes = Vec::new();
        self.publish(&owner, &job);
        let mut updates = tokio::time::interval(Duration::from_millis(120));
        let mut dirty = false;
        let (status, exit) = loop {
            tokio::select! {
                result=child.wait()=>break("finished",result.ok().and_then(|s|s.code())),
                _=&mut deadline=>{signal_group(pid);let _=child.wait().await;break("timed_out",None);},
                changed=cancel.changed()=>{if changed.is_err() || *cancel.borrow(){signal_group(pid);let _=child.wait().await;break("cancelled",None);}},
                Some(data)=input.recv()=>{ let _=tokio::time::timeout(Duration::from_millis(250), terminal.write(&data)).await; },
                _=updates.tick(),if dirty=>{self.publish(&owner,&job);dirty=false;},
                chunk=output.recv(),if !output_closed=>{
                    if let Some((err,data))=chunk {
                        let n=data.len().min(MAX_OUTPUT.saturating_sub(bytes));bytes+=n;
                        if err {err_bytes.extend_from_slice(&data[..n]);}else{out_bytes.extend_from_slice(&data[..n]);}
                        job["stdout"]=json!(String::from_utf8_lossy(&out_bytes));job["stderr"]=json!(String::from_utf8_lossy(&err_bytes));
                        if n<data.len(){job["output_truncated"]=json!(true);}
                        dirty=true;
                        if bytes >= MAX_OUTPUT { job["output_truncated"]=json!(true); signal_group(pid); let _=child.wait().await; break ("output_limit",None); }
                    }else{output_closed=true;}
                }
            }
        };
        // A shell may exit while grandchildren still hold pipes. Kill remaining group, then drain.
        signal_group(pid);
        let drain = async {
            while let Some((err, data)) = output.recv().await {
                let n = data.len().min(MAX_OUTPUT.saturating_sub(bytes));
                bytes += n;
                if err {
                    err_bytes.extend_from_slice(&data[..n]);
                } else {
                    out_bytes.extend_from_slice(&data[..n]);
                }
                if n < data.len() {
                    job["output_truncated"] = json!(true);
                }
            }
        };
        let _ = tokio::time::timeout(Duration::from_secs(1), drain).await;
        out_task.abort();
        job["stdout"] = json!(String::from_utf8_lossy(&out_bytes));
        job["stderr"] = json!(String::from_utf8_lossy(&err_bytes));
        job["status"] = json!(status);
        job["exit_code"] = json!(exit);
        job["finished_at"] = json!(now());
        self.publish(&owner, &job);
        self.running
            .lock()
            .unwrap()
            .remove(job["id"].as_str().unwrap());
    }
    pub fn input(&self, owner: &str, id: &str, data: &str) -> Result<()> {
        if data.is_empty() || data.len() > 4096 {
            return Err(ApiError::new(
                "INVALID_ARGUMENT",
                "Terminal girdisi 1–4096 bayt olmalı.",
            ));
        }
        let running = self.running.lock().unwrap();
        let active = running
            .get(id)
            .filter(|job| job.owner == owner)
            .ok_or_else(|| ApiError::new("NOT_FOUND", "Çalışan iş bulunamadı."))?;
        active
            .input
            .try_send(data.as_bytes().to_vec())
            .map_err(|_| ApiError::new("BUSY", "Terminal giriş kuyruğu dolu; tekrar deneyin."))
    }
    pub fn list(&self, owner: &str) -> Result<Vec<Value>> {
        let mut jobs = self.store.jobs(owner)?;
        for job in self
            .records
            .lock()
            .unwrap()
            .values()
            .filter(|job| job["owner"] == owner)
        {
            jobs.retain(|old| old["id"] != job["id"]);
            jobs.push(job.clone());
        }
        jobs.sort_by_key(|job| std::cmp::Reverse(job["created_at"].as_u64().unwrap_or(0)));
        Ok(jobs)
    }
    pub fn delete(&self, owner: &str, id: &str) -> Result<()> {
        if self.running.lock().unwrap().contains_key(id) {
            return Err(ApiError::new("BUSY", "Önce çalışan işi durdurun."));
        }
        self.get(owner, id)?;
        self.store.delete_job(owner, id)?;
        self.records.lock().unwrap().remove(id);
        let _ = self.events.send(Event {
            name: "job.changed",
            data: json!({"id":id,"status":"deleted"}),
            owner: Some(owner.into()),
        });
        Ok(())
    }
    pub fn get(&self, owner: &str, id: &str) -> Result<Value> {
        if let Some(job) = self
            .records
            .lock()
            .unwrap()
            .get(id)
            .filter(|job| job["owner"] == owner)
        {
            return Ok(job.clone());
        }
        self.store
            .jobs(owner)?
            .into_iter()
            .find(|j| j["id"] == id)
            .ok_or_else(|| ApiError::new("NOT_FOUND", "İş bulunamadı."))
    }
}
fn signal_group(pid: i32) {
    // SAFETY: negative pid targets the process group created specifically for this job.
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }
}
async fn reader(terminal: Arc<crate::terminal::Terminal>, tx: mpsc::Sender<(bool, Vec<u8>)>) {
    let mut buf = [0u8; 4096];
    while let Ok(n) = terminal.read(&mut buf).await {
        if n == 0 || tx.send((false, buf[..n].to_vec())).await.is_err() {
            break;
        }
    }
}
pub fn parse_spec(value: &Value) -> Result<Spec> {
    serde_json::from_value(value.clone()).map_err(|_| {
        ApiError::new(
            "INVALID_ARGUMENT",
            "Komut, shell, cwd ve timeout_seconds alanları gerekli.",
        )
    })
}
pub fn validate_saved(saved: &SavedCommand) -> Result<()> {
    if saved.name.is_empty() || saved.name.len() > 120 || saved.id.len() > 128 {
        return Err(ApiError::new(
            "INVALID_ARGUMENT",
            "Komut adı veya kimliği geçersiz.",
        ));
    }
    Spec::from(saved.clone()).validate().map_err(internal)
}
