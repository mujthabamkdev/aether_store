use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Per-key sliding-window token bucket. Keys are typically `(caller_ip, endpoint)`
/// or just `caller_ip`. Locks are short; fine for the load a self-hosted engine
/// will see.
pub struct RateLimiter {
    inner: Mutex<HashMap<String, Bucket>>,
    capacity: u32,
    refill: Duration,
}

struct Bucket {
    tokens: f64,
    last: Instant,
}

impl RateLimiter {
    pub fn new(capacity: u32, refill: Duration) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            capacity,
            refill,
        }
    }

    /// Returns true if the request is allowed.
    pub fn allow(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut g = self.inner.lock().unwrap();
        let b = g.entry(key.to_string()).or_insert_with(|| Bucket {
            tokens: self.capacity as f64,
            last: now,
        });
        let elapsed = now.duration_since(b.last).as_secs_f64();
        b.tokens = (b.tokens + elapsed * (self.capacity as f64) / self.refill.as_secs_f64())
            .min(self.capacity as f64);
        b.last = now;
        if b.tokens >= 1.0 {
            b.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Hard caps enforced on manifests / LLM requests / DAGs to bound worst-case
/// memory + latency. Editable in one place.
#[derive(Clone)]
pub struct Limits {
    pub manifest_max_bytes: usize,
    pub manifest_max_nodes: usize,
    pub manifest_max_depth: usize,
    pub chat_max_tokens: u32,
    pub inventory_max_items: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            manifest_max_bytes: 256 * 1024,
            manifest_max_nodes: 128,
            manifest_max_depth: 16,
            chat_max_tokens: 2048,
            inventory_max_items: 500,
        }
    }
}