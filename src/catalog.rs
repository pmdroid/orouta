use crate::config::{Config, Upstream};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

const TTL: Duration = Duration::from_secs(15);
const PROBE: Duration = Duration::from_secs(2);

pub struct Catalog {
    inner: RwLock<Inner>,
}

struct Inner {
    by_name: HashMap<String, String>,
    tags: Vec<Value>,
    fetched: Option<Instant>,
}

impl Catalog {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(Inner {
                by_name: HashMap::new(),
                tags: Vec::new(),
                fetched: None,
            }),
        }
    }

    pub async fn refresh(&self, config: &Config, client: &reqwest::Client) {
        let mut by_name = HashMap::new();
        let mut tags = Vec::new();
        for id in &config.upstream_order {
            let Some(up) = config.upstreams.get(id) else {
                continue;
            };
            let url = format!("{}/api/tags", up.base_url);
            let mut req = client.get(&url).timeout(PROBE);
            if let Some(key) = &up.api_key {
                req = req.bearer_auth(key);
            }
            let Ok(resp) = req.send().await else {
                continue;
            };
            if !resp.status().is_success() {
                continue;
            }
            let Ok(v) = resp.json::<Value>().await else {
                continue;
            };
            let Some(models) = v.get("models").and_then(|m| m.as_array()) else {
                continue;
            };
            for m in models {
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
        }
        let mut g = self.inner.write().await;
        g.by_name = by_name;
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

    pub async fn tags_body(&self, config: &Config, client: &reqwest::Client) -> Value {
        self.ensure(config, client, true).await;
        let g = self.inner.read().await;
        json!({ "models": g.tags })
    }

    pub async fn has(&self, config: &Config, client: &reqwest::Client, name: &str) -> bool {
        self.lookup(config, client, name).await.is_some()
    }
}
