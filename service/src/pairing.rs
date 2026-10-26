use crate::{
    protocol::{ApiError, Result},
    store::{Device, Store},
};
use rand::{rngs::OsRng, Rng, RngCore};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use uuid::Uuid;

pub fn secret() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}
struct Pending {
    name: String,
    poll_secret: String,
    expires: Instant,
    approved: Option<(Device, String)>,
}
#[derive(Default)]
struct Inner {
    code: Option<(String, Instant)>,
    attempts: Vec<Instant>,
    pending: HashMap<String, Pending>,
}
#[derive(Default)]
pub struct Pairing {
    inner: Mutex<Inner>,
}
impl Pairing {
    pub fn open(&self) -> Value {
        let mut p = self.inner.lock().unwrap();
        let code = format!("{:08}", OsRng.gen_range(0..100_000_000u32));
        p.code = Some((code.clone(), Instant::now() + Duration::from_secs(120)));
        json!({"code":code,"expires_seconds":120,"instruction":"Telefona kodu ve sertifika parmak izini girin; ardından pending ve approve komutlarıyla onaylayın."})
    }
    pub fn request(&self, code: &str, name: &str) -> Result<Value> {
        let mut p = self.inner.lock().unwrap();
        let now = Instant::now();
        p.attempts
            .retain(|t| now.duration_since(*t) < Duration::from_secs(60));
        p.pending.retain(|_, v| v.expires > now);
        if p.attempts.len() >= 5 {
            return Err(ApiError::new(
                "RATE_LIMITED",
                "Eşleştirme deneme sınırı; bir dakika bekleyin.",
            ));
        }
        p.attempts.push(now);
        if !p.code.as_ref().is_some_and(|(expected, end)| {
            *end > now && bool::from(expected.as_bytes().ct_eq(code.as_bytes()))
        }) {
            return Err(ApiError::new(
                "PAIR_CODE_INVALID",
                "Kod geçersiz, kullanılmış veya süresi dolmuş.",
            ));
        }
        p.code = None;
        let id = Uuid::new_v4().to_string();
        let poll_secret = secret();
        p.pending.insert(
            id.clone(),
            Pending {
                name: name.into(),
                poll_secret: poll_secret.clone(),
                expires: now + Duration::from_secs(180),
                approved: None,
            },
        );
        Ok(json!({"pending_id":id,"poll_secret":poll_secret,"expires_seconds":180}))
    }
    pub fn pending(&self) -> Value {
        let mut p = self.inner.lock().unwrap();
        p.pending.retain(|_, v| v.expires > Instant::now());
        json!(p
            .pending
            .iter()
            .map(|(id, v)| json!({"id":id,"name":v.name,"approved":v.approved.is_some()}))
            .collect::<Vec<_>>())
    }
    pub fn approve(&self, id: &str, store: &Store) -> Result<Value> {
        let mut p = self.inner.lock().unwrap();
        let v = p
            .pending
            .get_mut(id)
            .filter(|v| v.expires > Instant::now())
            .ok_or_else(|| ApiError::new("NOT_FOUND", "Bekleyen eşleştirme bulunamadı."))?;
        if v.approved.is_some() {
            return Err(ApiError::new("ALREADY_APPROVED", "Zaten onaylandı."));
        }
        let token = secret();
        let d = store.add_device(&v.name, &token)?;
        v.approved = Some((d.clone(), token));
        Ok(json!(d))
    }
    pub fn deny(&self, id: &str) -> Value {
        self.inner.lock().unwrap().pending.remove(id);
        json!({"denied":true})
    }
    pub fn poll(&self, id: &str, secret: &str) -> Result<Value> {
        let mut p = self.inner.lock().unwrap();
        let v = p
            .pending
            .get(id)
            .filter(|v| {
                v.expires > Instant::now()
                    && bool::from(v.poll_secret.as_bytes().ct_eq(secret.as_bytes()))
            })
            .ok_or_else(|| {
                ApiError::new("PAIR_EXPIRED", "Eşleştirme reddedildi veya süresi doldu.")
            })?;
        if let Some((d, token)) = &v.approved {
            let result = json!({"status":"approved","device":d,"token":token});
            p.pending.remove(id);
            Ok(result)
        } else {
            Ok(json!({"status":"pending"}))
        }
    }
}
