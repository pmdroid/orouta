use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub keys: Vec<String>,
    pub upstreams: HashMap<String, Upstream>,
    pub upstream_order: Vec<String>,
    pub pull_host: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Upstream {
    pub id: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub disabled: bool,
}

pub fn build_upstream(
    id: &str,
    base_url: &str,
    api_key: Option<String>,
) -> Result<Upstream, String> {
    if id.trim().is_empty() {
        return Err("upstream id is required".into());
    }
    let parsed =
        reqwest::Url::parse(base_url).map_err(|e| format!("upstream {id} base_url: {e}"))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(format!("upstream {id} base_url must be http or https"));
    }
    if parsed.host_str().is_none() {
        return Err(format!("upstream {id} base_url has no host"));
    }
    let base_url = base_url.trim_end_matches('/').to_string();
    let api_key = api_key.and_then(|k| {
        let t = k.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    });
    Ok(Upstream {
        id: id.to_string(),
        base_url,
        api_key,
        disabled: false,
    })
}

#[derive(Deserialize)]
struct FileConfig {
    #[serde(default = "default_host")]
    host: String,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default)]
    auth: FileAuth,
    #[serde(default)]
    upstream: Vec<FileUpstream>,
    #[serde(default)]
    pull_host: Option<String>,
}

#[derive(Deserialize, Default)]
struct FileAuth {
    #[serde(default)]
    keys: Vec<String>,
}

#[derive(Deserialize)]
struct FileUpstream {
    id: String,
    base_url: String,
    #[serde(default)]
    api_key: Option<String>,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    11434
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text =
            std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let file: FileConfig = toml::from_str(text).map_err(|e| format!("toml: {e}"))?;
        Self::from_file(file)
    }

    fn from_file(file: FileConfig) -> Result<Self, String> {
        let mut upstreams = HashMap::new();
        let mut upstream_order = Vec::new();
        for u in file.upstream {
            if upstreams.contains_key(&u.id) {
                return Err(format!("duplicate upstream id: {}", u.id));
            }
            let upstream = build_upstream(&u.id, &u.base_url, u.api_key)?;
            upstream_order.push(u.id.clone());
            upstreams.insert(u.id.clone(), upstream);
        }
        if upstream_order.is_empty() {
            return Err("at least one [[upstream]] is required".into());
        }
        if file.host.trim().is_empty() {
            return Err("host is required".into());
        }
        if let Some(id) = &file.pull_host {
            if id.trim().is_empty() || !upstream_order.contains(id) {
                return Err(format!(
                    "pull_host must be one of: {}",
                    upstream_order.join(", ")
                ));
            }
        }
        Ok(Config {
            host: file.host,
            port: file.port,
            keys: file.auth.keys,
            upstreams,
            upstream_order,
            pull_host: file.pull_host,
        })
    }

    pub fn listen_addr(&self) -> String {
        if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    pub fn first_upstream(&self) -> Option<&Upstream> {
        self.upstream_order
            .iter()
            .find_map(|id| self.upstreams.get(id).filter(|up| !up.disabled))
    }

    pub fn resolve_pull_host(&self, host: Option<&str>) -> Result<Upstream, String> {
        let selected = match host.or(self.pull_host.as_deref()) {
            Some(id) => self.upstreams.get(id).ok_or_else(|| {
                format!(
                    "unknown pull host {id}; available hosts: {}",
                    self.upstream_order.join(", ")
                )
            })?,
            None => {
                if self.upstream_order.len() != 1 {
                    return Err(format!(
                        "no pull host selected; available hosts: {}",
                        self.upstream_order.join(", ")
                    ));
                }
                &self.upstreams[&self.upstream_order[0]]
            }
        };
        Ok(selected.clone())
    }
}
