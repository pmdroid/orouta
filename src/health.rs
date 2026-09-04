use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct UpstreamHealth {
    pub up: bool,
    pub last_ok: Option<Instant>,
    pub last_error: Option<String>,
    pub consecutive_failures: u32,
}

impl Default for UpstreamHealth {
    fn default() -> Self {
        Self {
            up: true,
            last_ok: None,
            last_error: None,
            consecutive_failures: 0,
        }
    }
}

#[derive(Default)]
pub struct Health {
    inner: RwLock<HashMap<String, UpstreamHealth>>,
}

impl Health {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn record_ok(&self, id: &str) {
        let mut g = self.inner.write().await;
        let h = g.entry(id.to_string()).or_default();
        h.up = true;
        h.last_ok = Some(Instant::now());
        h.last_error = None;
        h.consecutive_failures = 0;
    }

    pub async fn record_error(&self, id: &str, error: String) {
        let mut g = self.inner.write().await;
        let h = g.entry(id.to_string()).or_default();
        h.up = false;
        h.last_error = Some(error);
        h.consecutive_failures += 1;
    }

    pub async fn is_up(&self, id: &str) -> bool {
        self.inner
            .read()
            .await
            .get(id)
            .map(|h| h.up)
            .unwrap_or(true)
    }

    pub async fn first_down(&self, order: &[String]) -> Option<String> {
        let g = self.inner.read().await;
        order
            .iter()
            .find(|id| g.get(*id).is_some_and(|h| !h.up))
            .cloned()
    }
}
