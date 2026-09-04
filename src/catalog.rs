use crate::config::{Config, Upstream};
use crate::health::Health;
use crate::status::HostStats;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

const TTL: Duration = Duration::from_secs(15);
const PROBE: Duration = Duration::from_secs(2);

pub struct Catalog {
    inner: RwLock<Inner>,
    stats: Arc<std::sync::RwLock<HashMap<String, Arc<HostStats>>>>,
    pub health: Arc<Health>,
}

struct Inner {
    by_name: HashMap<String, String>,
    by_host: HashMap<String, Vec<Value>>,
    tags: Vec<Value>,
    fetched: Option<Instant>,
    generation: u64,
}

impl Catalog {
    pub fn new(stats: Arc<std::sync::RwLock<HashMap<String, Arc<HostStats>>>>) -> Self {
        Self {
            inner: RwLock::new(Inner {
                by_name: HashMap::new(),
                by_host: HashMap::new(),
                tags: Vec::new(),
                fetched: None,
                generation: 0,
            }),
            stats,
            health: Arc::new(Health::new()),
        }
    }

    pub async fn reset(&self) {
        let mut g = self.inner.write().await;
        g.generation += 1;
        g.fetched = None;
    }

    pub async fn refresh(&self, config: &Config, client: &reqwest::Client) {
        let generation = self.inner.read().await.generation;
        let mut by_name = HashMap::new();
        let mut by_host: HashMap<String, Vec<Value>> = HashMap::new();
        let mut tags = Vec::new();
        for id in &config.upstream_order {
            let Some(up) = config.upstreams.get(id) else {
                continue;
            };
            if up.disabled {
                continue;
            }
            let url = format!("{}/api/tags", up.base_url);
            let mut req = client.get(&url).timeout(PROBE);
            if let Some(key) = &up.api_key {
                req = req.bearer_auth(key);
            }
            let host_stats = self.stats.read().ok().and_then(|m| m.get(id).cloned());
            let start = Instant::now();
            let resp = match req.send().await {
                Ok(r) => {
                    self.health.record_ok(id).await;
                    r
                }
                Err(e) => {
                    self.health.record_error(id, e.to_string()).await;
                    if let Some(s) = host_stats {
                        s.probe_finished(start.elapsed(), Some(e.to_string()));
                    }
                    continue;
                }
            };
            if !resp.status().is_success() {
                if let Some(s) = host_stats {
                    s.probe_finished(
                        start.elapsed(),
                        Some(format!("tags http {}", resp.status().as_u16())),
                    );
                }
                continue;
            }
            if let Some(s) = host_stats {
                s.probe_finished(start.elapsed(), None);
            }
            let Ok(v) = resp.json::<Value>().await else {
                continue;
            };
            let Some(models) = v.get("models").and_then(|m| m.as_array()) else {
                continue;
            };
            let mut host_models = Vec::new();
            for m in models {
                host_models.push(m.clone());
                let Some(name) = m
                    .get("name")
                    .or_else(|| m.get("model"))
                    .and_then(|x| x.as_str())
                else {
                    continue;
                };
                if !by_name.contains_key(name) {
                    by_name.insert(name.to_string(), id.clone());
                    tags.push(m.clone());
                }
                if let Some(stem) = name.strip_suffix(":latest") {
                    by_name.entry(stem.to_string()).or_insert(id.clone());
                }
            }
            by_host.insert(id.clone(), host_models);
        }
        let mut g = self.inner.write().await;
        if g.generation != generation {
            return;
        }
        g.by_name = by_name;
        g.by_host = by_host;
        g.tags = tags;
        g.fetched = Some(Instant::now());
    }

    async fn ensure(&self, config: &Config, client: &reqwest::Client, force: bool) {
        let stale = {
            let g = self.inner.read().await;
            match g.fetched {
                None => true,
                Some(t) => t.elapsed() > TTL,
            }
        };
        if force || stale {
            self.refresh(config, client).await;
        }
    }

    pub async fn lookup(
        &self,
        config: &Config,
        client: &reqwest::Client,
        name: &str,
    ) -> Option<Upstream> {
        self.ensure(config, client, false).await;
        let id = {
            let g = self.inner.read().await;
            g.by_name.get(name).cloned()
        };
        if id.is_none() {
            self.refresh(config, client).await;
        }
        let g = self.inner.read().await;
        let id = g.by_name.get(name)?;
        config.upstreams.get(id).cloned()
    }

    pub async fn names(&self, config: &Config, client: &reqwest::Client) -> Vec<String> {
        self.ensure(config, client, true).await;
        let g = self.inner.read().await;
        g.tags
            .iter()
            .filter_map(|m| {
                m.get("name")
                    .or_else(|| m.get("model"))
                    .and_then(|x| x.as_str())
                    .map(str::to_string)
            })
            .collect()
    }

    pub async fn model_names_by_host(
        &self,
        config: &Config,
        client: &reqwest::Client,
    ) -> HashMap<String, Vec<String>> {
        self.ensure(config, client, false).await;
        let g = self.inner.read().await;
        g.by_host
            .iter()
            .map(|(id, ms)| {
                (
                    id.clone(),
                    ms.iter()
                        .filter_map(|m| {
                            m.get("name")
                                .or_else(|| m.get("model"))
                                .and_then(|x| x.as_str())
                                .map(str::to_string)
                        })
                        .collect(),
                )
            })
            .collect()
    }

    pub async fn tags_body(&self, config: &Config, client: &reqwest::Client) -> Value {
        self.ensure(config, client, true).await;
        let g = self.inner.read().await;
        json!({ "models": g.tags })
    }

    pub async fn has(&self, config: &Config, client: &reqwest::Client, name: &str) -> bool {
        self.lookup(config, client, name).await.is_some()
    }
}
