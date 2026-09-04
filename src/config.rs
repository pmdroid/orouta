use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: String,
    pub keys: Vec<String>,
    pub default_id: String,
    pub upstreams: HashMap<String, Upstream>,
    pub models: HashMap<String, ModelEntry>,
}

#[derive(Debug, Clone)]
pub struct Upstream {
    pub id: String,
    pub base_url: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ModelEntry {
    pub name: String,
    pub upstream_id: String,
    pub upstream_model: Option<String>,
}

#[derive(Deserialize)]
struct FileConfig {
    #[serde(default = "default_bind")]
    bind: String,
    #[serde(default)]
    auth: FileAuth,
    #[serde(default)]
    upstream: Vec<FileUpstream>,
    #[serde(default)]
    model: Vec<FileModel>,
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
    #[serde(default)]
    default: bool,
}

#[derive(Deserialize)]
struct FileModel {
    name: String,
    upstream: String,
    #[serde(default)]
    upstream_model: Option<String>,
}

fn default_bind() -> String {
    "0.0.0.0:11434".to_string()
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
        let mut default_id: Option<String> = None;
        for u in file.upstream {
            if upstreams.contains_key(&u.id) {
                return Err(format!("duplicate upstream id: {}", u.id));
            }
            let parsed = reqwest::Url::parse(&u.base_url)
                .map_err(|e| format!("upstream {} base_url: {e}", u.id))?;
            if parsed.scheme() != "http" && parsed.scheme() != "https" {
                return Err(format!("upstream {} base_url must be http or https", u.id));
            }
            if parsed.host_str().is_none() {
                return Err(format!("upstream {} base_url has no host", u.id));
            }
            let base_url = u.base_url.trim_end_matches('/').to_string();
            let api_key = u.api_key.and_then(|k| {
                let t = k.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            });
            if u.default {
                if default_id.is_some() {
                    return Err("exactly one upstream must have default = true".into());
                }
                default_id = Some(u.id.clone());
            }
            upstreams.insert(
                u.id.clone(),
                Upstream {
                    id: u.id,
                    base_url,
                    api_key,
                },
            );
        }
        let default_id = default_id
            .ok_or_else(|| "exactly one upstream must have default = true".to_string())?;
        let mut models = HashMap::new();
        for m in file.model {
            if models.contains_key(&m.name) {
                return Err(format!("duplicate model name: {}", m.name));
            }
            if !upstreams.contains_key(&m.upstream) {
                return Err(format!(
                    "model {} references unknown upstream {}",
                    m.name, m.upstream
                ));
            }
            let upstream_model =
                m.upstream_model
                    .and_then(|s| if s.is_empty() { None } else { Some(s) });
            models.insert(
                m.name.clone(),
                ModelEntry {
                    name: m.name,
                    upstream_id: m.upstream,
                    upstream_model,
                },
            );
        }
        Ok(Config {
            bind: file.bind,
            keys: file.auth.keys,
            default_id,
            upstreams,
            models,
        })
    }

    pub fn default_upstream(&self) -> &Upstream {
        &self.upstreams[&self.default_id]
    }

    pub fn model_upstream(&self, name: &str) -> Option<(&ModelEntry, &Upstream)> {
        let entry = self.models.get(name)?;
        let up = self.upstreams.get(&entry.upstream_id)?;
        Some((entry, up))
    }
}
