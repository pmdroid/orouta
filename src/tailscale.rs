use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const REFRESH_INTERVAL: Duration = Duration::from_secs(15);
const CMD_TIMEOUT: Duration = Duration::from_secs(3);
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug)]
pub struct TsInfo {
    pub self_dns: String,
    pub tailnet: Option<String>,
    pub online: bool,
    pub serving: bool,
    pub url: Option<String>,
}

struct Cache {
    info: Option<TsInfo>,
    at: Instant,
}

pub struct Tailscale {
    cache: Mutex<Cache>,
    refreshing: AtomicBool,
}

impl Default for Tailscale {
    fn default() -> Self {
        Self::new()
    }
}

impl Tailscale {
    pub fn new() -> Self {
        Self::with_info(None)
    }

    pub fn with_info(info: Option<TsInfo>) -> Self {
        Self {
            cache: Mutex::new(Cache {
                info,
                at: Instant::now(),
            }),
            refreshing: AtomicBool::new(false),
        }
    }

    pub fn info(&self) -> Option<TsInfo> {
        self.cache
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .info
            .clone()
    }

    pub fn spawn_refresh(self: &Arc<Self>, client: &reqwest::Client) {
        self.refreshing.store(true, Ordering::Relaxed);
        self.spawn_refresh_inner(client);
    }

    pub fn spawn_refresh_if_stale(self: &Arc<Self>, client: &reqwest::Client) {
        if self
            .cache
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .at
            .elapsed()
            < REFRESH_INTERVAL
        {
            return;
        }
        if self
            .refreshing
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return;
        }
        self.spawn_refresh_inner(client);
    }

    fn spawn_refresh_inner(self: &Arc<Self>, client: &reqwest::Client) {
        let this = self.clone();
        let client = client.clone();
        tokio::spawn(async move {
            this.refresh(&client).await;
            this.refreshing.store(false, Ordering::Relaxed);
        });
    }

    pub async fn refresh(&self, client: &reqwest::Client) {
        let info = detect(client).await;
        let mut cache = self.cache.lock().unwrap_or_else(|p| p.into_inner());
        cache.info = info;
        cache.at = Instant::now();
    }
}

async fn detect(client: &reqwest::Client) -> Option<TsInfo> {
    let out = tokio::time::timeout(
        CMD_TIMEOUT,
        tokio::process::Command::new("tailscale")
            .args(["status", "--json"])
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    if !out.status.success() {
        return None;
    }
    let v: Value = serde_json::from_slice(&out.stdout).ok()?;
    let self_dns = strip_dot(v.pointer("/Self/DNSName")?.as_str()?)?;
    let online = v
        .pointer("/Self/Online")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let tailnet = v
        .pointer("/CurrentTailnet/Name")
        .and_then(Value::as_str)
        .map(str::to_string);
    if !online {
        return Some(TsInfo {
            self_dns,
            tailnet,
            online: false,
            serving: false,
            url: None,
        });
    }
    let url = format!("https://{self_dns}");
    let serving = client
        .get(&url)
        .timeout(PROBE_TIMEOUT)
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false);
    Some(TsInfo {
        self_dns,
        tailnet,
        online: true,
        serving,
        url: serving.then_some(url),
    })
}

fn strip_dot(s: &str) -> Option<String> {
    let s = s.strip_suffix('.').unwrap_or(s);
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}
