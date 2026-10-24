//! In-memory TTL cache for tool results, keyed by canonical arguments JSON.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct TtlCache {
    ttl: Duration,
    entries: Mutex<HashMap<String, (Instant, String)>>,
}

impl TtlCache {
    pub fn new(ttl: Duration) -> Self {
        Self { ttl, entries: Mutex::default() }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        let entries = self.entries.lock().unwrap();
        entries.get(key).filter(|(at, _)| at.elapsed() < self.ttl).map(|(_, v)| v.clone())
    }

    pub fn put(&self, key: String, value: String) {
        let mut entries = self.entries.lock().unwrap();
        entries.retain(|_, (at, _)| at.elapsed() < self.ttl);
        entries.insert(key, (Instant::now(), value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expires_after_ttl() {
        let cache = TtlCache::new(Duration::from_millis(30));
        cache.put("k".into(), "v".into());
        assert_eq!(cache.get("k").as_deref(), Some("v"));
        std::thread::sleep(Duration::from_millis(40));
        assert_eq!(cache.get("k"), None);
    }
}
