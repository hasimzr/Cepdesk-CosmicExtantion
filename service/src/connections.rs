//! Live, authenticated sessions. RAII removes entries even when a transport fails.
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::watch;
use uuid::Uuid;

#[derive(Default)]
pub struct Connections(Mutex<HashMap<String, Entry>>);
struct Entry {
    device: String,
    peer: String,
    connected_at: i64,
    last_seen: i64,
    disconnect: watch::Sender<bool>,
}
pub struct Session {
    id: String,
    registry: Arc<Connections>,
    pub disconnect: watch::Receiver<bool>,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.registry.0.lock().unwrap().remove(&self.id);
    }
}
impl Session {
    pub fn touch(&self) {
        if let Some(entry) = self.registry.0.lock().unwrap().get_mut(&self.id) {
            entry.last_seen = crate::store::now();
        }
    }
}
impl Connections {
    pub fn add(self: &Arc<Self>, device: &str, peer: &str) -> Session {
        let id = Uuid::new_v4().to_string();
        let (tx, rx) = watch::channel(false);
        let now = crate::store::now();
        self.0.lock().unwrap().insert(
            id.clone(),
            Entry {
                device: device.into(),
                peer: peer.into(),
                connected_at: now,
                last_seen: now,
                disconnect: tx,
            },
        );
        Session {
            id,
            registry: self.clone(),
            disconnect: rx,
        }
    }
    pub fn list(&self, device: &str) -> Vec<Value> {
        self.0.lock().unwrap().iter().filter(|(_, entry)| entry.device == device).map(|(id, entry)| json!({"id":id,"peer":entry.peer,"connected_at":entry.connected_at,"last_seen":entry.last_seen})).collect()
    }
    pub fn disconnect(&self, device: &str) -> usize {
        let registry = self.0.lock().unwrap();
        let mut count = 0;
        for entry in registry.values().filter(|entry| entry.device == device) {
            let _ = entry.disconnect.send(true);
            count += 1;
        }
        count
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sessions_are_live_owned_and_disconnect_does_not_revoke() {
        let registry = Arc::new(Connections::default());
        let first = registry.add("a", "192.168.1.5:123");
        let second = registry.add("a", "127.0.0.1:456");
        let other = registry.add("b", "127.0.0.1:789");
        assert_eq!(registry.list("a").len(), 2);
        assert_eq!(registry.disconnect("a"), 2);
        assert!(*first.disconnect.borrow());
        assert!(*second.disconnect.borrow());
        assert!(!*other.disconnect.borrow());
        drop(first);
        drop(second);
        assert!(registry.list("a").is_empty());
        assert_eq!(registry.list("b").len(), 1);
        drop(other);
        assert!(registry.list("b").is_empty());
    }
}
