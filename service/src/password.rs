//! Password authenticated first-pairing. Proofs bind both nonces, server identity,
//! TLS certificate and device name. The password/derived key never crosses the wire.
use crate::{
    pairing::secret,
    protocol::{text, ApiError, Result},
    store::Store,
};
use anyhow::Context;
use ring::{hmac, pbkdf2};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::{self, IsTerminal, Write},
    num::NonZeroU32,
    os::unix::{fs::PermissionsExt, io::AsRawFd},
    path::Path,
    sync::Mutex,
    time::{Duration, Instant},
};

pub const ITERATIONS: u32 = 600_000;
#[derive(Clone, Serialize, Deserialize)]
struct Record {
    salt: String,
    key: String,
}
pub struct Password {
    record: Mutex<Option<Record>>,
    dir: std::path::PathBuf,
    attempts: Mutex<Vec<Instant>>,
}
pub struct Challenge {
    transcript: String,
    name: String,
    expires: Instant,
}
impl Password {
    pub fn create(password: &str, dir: &Path) -> anyhow::Result<Self> {
        anyhow::ensure!(
            password.chars().count() >= 12 && password.len() <= 256,
            "Parola 12–256 karakter olmalı"
        );
        let salt = secret();
        let mut key = [0u8; 32];
        pbkdf2::derive(
            pbkdf2::PBKDF2_HMAC_SHA256,
            NonZeroU32::new(ITERATIONS).unwrap(),
            &hex::decode(&salt)?,
            password.as_bytes(),
            &mut key,
        );
        let record = Record {
            salt,
            key: hex::encode(key),
        };
        // Atomic replacement: a failed write must not corrupt an existing password.
        let path = dir.join("password.json");
        let temporary = dir.join(format!("password-{}.tmp", secret()));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.write_all(&serde_json::to_vec(&record)?)?;
        file.sync_all()?;
        fs::rename(temporary, path)?;
        Ok(Self {
            record: Mutex::new(Some(record)),
            dir: dir.to_path_buf(),
            attempts: Mutex::new(Vec::new()),
        })
    }
    pub fn load_or_configure(dir: &Path) -> anyhow::Result<Self> {
        let path = dir.join("password.json");
        if !path.exists() {
            if io::stdin().is_terminal() {
                return Self::configure(dir);
            }
            return Ok(Self {
                record: Mutex::new(None),
                dir: dir.to_path_buf(),
                attempts: Mutex::new(Vec::new()),
            });
        }
        let record: Record = serde_json::from_slice(&fs::read(path)?)?;
        anyhow::ensure!(
            hex::decode(&record.salt)?.len() == 32 && hex::decode(&record.key)?.len() == 32,
            "Parola kaydı geçersiz"
        );
        Ok(Self {
            record: Mutex::new(Some(record)),
            dir: dir.to_path_buf(),
            attempts: Mutex::new(Vec::new()),
        })
    }
    pub fn configure(dir: &Path) -> anyhow::Result<Self> {
        anyhow::ensure!(
            io::stdin().is_terminal(),
            "İlk kurulum için terminalde cepdesk password çalıştırın; ardından servisi başlatın"
        );
        println!("Telefonlarınızda bir kez kullanacağınız ortak bağlantı parolasını belirleyin (en az 12 karakter).");
        let mut first = read_secret("Bağlantı parolası: ")?;
        let mut second = read_secret("Parolayı tekrar girin: ")?;
        let result = if first == second {
            Self::create(&first, dir)
        } else {
            Err(anyhow::anyhow!("Parolalar eşleşmiyor; tekrar deneyin"))
        };
        // Erase prompt buffers after deriving the stored key.
        unsafe {
            first.as_bytes_mut().fill(0);
            second.as_bytes_mut().fill(0);
        }
        result
    }
    pub fn configured(&self) -> bool {
        self.record.lock().unwrap().is_some()
    }
    pub fn change(&self, value: &str) -> anyhow::Result<()> {
        let mut record = self.record.lock().unwrap();
        let password = Self::create(value, &self.dir)?;
        *record = password.record.into_inner().unwrap();
        Ok(())
    }
    fn credentials(&self) -> Result<(hmac::Key, String)> {
        let record = self.record.lock().unwrap();
        let record = record.as_ref().ok_or_else(|| {
            ApiError::new(
                "PASSWORD_NOT_CONFIGURED",
                "Bilgisayarda CepDesk yönetiminden bağlantı parolasını belirleyin.",
            )
        })?;
        Ok((
            hmac::Key::new(
                hmac::HMAC_SHA256,
                &hex::decode(&record.key).expect("validated password key"),
            ),
            record.salt.clone(),
        ))
    }
    fn key(&self) -> Result<hmac::Key> {
        self.credentials().map(|(key, _)| key)
    }
    pub fn hello(
        &self,
        params: &Value,
        server_id: &str,
        fingerprint: &str,
        challenge: &mut Option<Challenge>,
    ) -> Result<Value> {
        *challenge = None;
        let (key, salt) = self.credentials()?;
        let nonce = text(params, "nonce", 64)?;
        if nonce.len() != 64 || hex::decode(nonce).is_err() {
            return Err(ApiError::new(
                "INVALID_ARGUMENT",
                "Geçersiz bağlantı isteği.",
            ));
        }
        let name = text(params, "name", 120)?;
        let now = Instant::now();
        let mut attempts = self.attempts.lock().unwrap();
        attempts.retain(|t| now.duration_since(*t) < Duration::from_secs(60));
        if attempts.len() >= 5 {
            return Err(ApiError::new(
                "RATE_LIMITED",
                "Bağlantı deneme sınırı; bir dakika bekleyin.",
            ));
        }
        attempts.push(now);
        let server_nonce = secret();
        let transcript = serde_json::to_string(&json!([
            "cepdesk-password-v1",
            server_id,
            fingerprint,
            nonce,
            server_nonce,
            name
        ]))
        .unwrap();
        let proof =
            hex::encode(hmac::sign(&key, format!("server:{transcript}").as_bytes()).as_ref());
        *challenge = Some(Challenge {
            transcript,
            name: name.into(),
            expires: now + Duration::from_secs(90),
        });
        Ok(
            json!({"salt":salt,"iterations":ITERATIONS,"server_id":server_id,"fingerprint":fingerprint,"nonce":server_nonce,"proof":proof,"transcript":challenge.as_ref().unwrap().transcript}),
        )
    }
    pub fn pair(
        &self,
        params: &Value,
        store: &Store,
        challenge: &mut Option<Challenge>,
    ) -> Result<Value> {
        let challenge = challenge
            .take()
            .filter(|c| c.expires > Instant::now())
            .ok_or_else(|| {
                ApiError::new(
                    "PASSWORD_INVALID",
                    "Parola doğrulaması geçersiz veya süresi dolmuş.",
                )
            })?;
        let proof = text(params, "proof", 64)?;
        let proof = hex::decode(proof)
            .map_err(|_| ApiError::new("PASSWORD_INVALID", "Parola doğrulanamadı."))?;
        hmac::verify(
            &self.key()?,
            format!("client:{}", challenge.transcript).as_bytes(),
            &proof,
        )
        .map_err(|_| ApiError::new("PASSWORD_INVALID", "Parola doğrulanamadı."))?;
        let token = secret();
        let device = store.add_device(&challenge.name, &token)?;
        Ok(json!({"device":device,"token":token}))
    }
}
fn read_secret(prompt: &str) -> anyhow::Result<String> {
    struct EchoGuard(libc::termios, i32);
    impl Drop for EchoGuard {
        fn drop(&mut self) {
            unsafe {
                libc::tcsetattr(self.1, libc::TCSANOW, &self.0);
            }
        }
    }
    let fd = io::stdin().as_raw_fd();
    let mut original = unsafe { std::mem::zeroed::<libc::termios>() };
    anyhow::ensure!(
        unsafe { libc::tcgetattr(fd, &mut original) } == 0,
        "Terminal okunamadı"
    );
    let mut hidden = original;
    hidden.c_lflag &= !libc::ECHO;
    print!("{prompt}");
    io::stdout().flush()?;
    anyhow::ensure!(
        unsafe { libc::tcsetattr(fd, libc::TCSANOW, &hidden) } == 0,
        "Parola girişi gizlenemedi"
    );
    let guard = EchoGuard(original, fd);
    let mut value = String::new();
    let count = io::stdin()
        .read_line(&mut value)
        .context("Parola okunamadı")?;
    drop(guard);
    println!();
    anyhow::ensure!(count > 0, "Parola girişi iptal edildi");
    while value.ends_with(['\n', '\r']) {
        value.pop();
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (std::path::PathBuf, Password, Store) {
        let dir = std::env::temp_dir().join(format!("cepdesk-password-test-{}", secret()));
        fs::create_dir_all(&dir).unwrap();
        let password = Password::create("Türkçe parola 123!", &dir).unwrap();
        let store = Store::open(&dir.join("test.db")).unwrap();
        (dir, password, store)
    }
    fn hello(password: &Password, challenge: &mut Option<Challenge>) -> Value {
        password
            .hello(
                &json!({"nonce":secret(),"name":"Telefon"}),
                "server",
                &"ab".repeat(32),
                challenge,
            )
            .unwrap()
    }
    fn proof(password: &Password, hello: &Value) -> Value {
        json!({"proof":hex::encode(hmac::sign(&password.key().unwrap(), format!("client:{}", hello["transcript"].as_str().unwrap()).as_bytes()).as_ref())})
    }
    #[test]
    fn password_persists_and_pairs_multiple_devices_with_distinct_revocable_tokens() {
        let (dir, password, store) = fixture();
        let loaded = Password::load_or_configure(&dir).unwrap();
        assert_eq!(
            password.record.lock().unwrap().as_ref().unwrap().key,
            loaded.record.lock().unwrap().as_ref().unwrap().key
        );
        assert_eq!(
            fs::metadata(dir.join("password.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert!(!fs::read_to_string(dir.join("password.json"))
            .unwrap()
            .contains("Türkçe"));
        let mut challenge = None;
        let a = hello(&loaded, &mut challenge);
        let a_proof = proof(&loaded, &a);
        let first = loaded.pair(&a_proof, &store, &mut challenge).unwrap();
        assert!(loaded.pair(&a_proof, &store, &mut challenge).is_err());
        let b = hello(&loaded, &mut challenge);
        let second = loaded
            .pair(&proof(&loaded, &b), &store, &mut challenge)
            .unwrap();
        assert_ne!(first["token"], second["token"]);
        let device = store
            .authenticate(first["token"].as_str().unwrap())
            .unwrap();
        assert!(!device.free_commands);
        store.revoke(&device.id).unwrap();
        assert!(store
            .authenticate(first["token"].as_str().unwrap())
            .is_err());
        assert!(store
            .authenticate(second["token"].as_str().unwrap())
            .is_ok());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn wrong_expired_cross_session_and_reflected_proofs_cannot_register_devices() {
        let (dir, password, store) = fixture();
        let mut a = None;
        let mut b = None;
        let first = hello(&password, &mut a);
        hello(&password, &mut b);
        assert!(password
            .pair(&proof(&password, &first), &store, &mut b)
            .is_err());
        assert!(password
            .pair(&json!({"proof":first["proof"]}), &store, &mut a)
            .is_err());
        hello(&password, &mut a);
        assert!(password
            .pair(&json!({"proof":"00".repeat(32)}), &store, &mut a)
            .is_err());
        let expiring = hello(&password, &mut a);
        a.as_mut().unwrap().expires = Instant::now() - Duration::from_secs(1);
        assert!(password
            .pair(&proof(&password, &expiring), &store, &mut a)
            .is_err());
        assert!(store.devices().unwrap().is_empty());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn password_attempt_limit_is_shared_across_sessions() {
        let (dir, password, _) = fixture();
        for _ in 0..5 {
            hello(&password, &mut None);
        }
        assert_eq!(
            password
                .hello(
                    &json!({"nonce":secret(),"name":"Tablet"}),
                    "server",
                    &"ab".repeat(32),
                    &mut None
                )
                .unwrap_err()
                .code,
            "RATE_LIMITED"
        );
        fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod management_tests {
    use super::*;
    #[test]
    fn local_password_change_is_atomic_and_does_not_revoke_registered_devices() {
        let dir = std::env::temp_dir().join(format!("cepdesk-password-admin-{}", secret()));
        fs::create_dir_all(&dir).unwrap();
        let password = Password {
            record: Mutex::new(None),
            dir: dir.clone(),
            attempts: Mutex::new(Vec::new()),
        };
        assert!(!password.configured());
        assert!(password.key().is_err());
        password.change("initial-password-123").unwrap();
        assert!(password.configured());
        let old = fs::read(dir.join("password.json")).unwrap();
        assert!(password.change("short").is_err());
        assert_eq!(fs::read(dir.join("password.json")).unwrap(), old);
        let store = Store::open(&dir.join("store.db")).unwrap();
        store.add_device("phone", "a-device-token").unwrap();
        password.change("new-password-456").unwrap();
        assert_ne!(fs::read(dir.join("password.json")).unwrap(), old);
        assert!(store.authenticate("a-device-token").is_ok());
        assert_eq!(
            fs::metadata(dir.join("password.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
