use crate::config::Config;
use crate::AppState;
use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

#[derive(Default)]
pub struct HostStats {
    reachable: AtomicBool,
    latency_ms: AtomicU64,
    requests_total: AtomicU64,
    errors_total: AtomicU64,
    in_flight: AtomicI64,
    last_error: Mutex<Option<String>>,
}

impl HostStats {
    pub fn request_started(&self) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
        self.in_flight.fetch_add(1, Ordering::Relaxed);
    }

    pub fn request_finished(&self, latency: Duration, error: Option<String>) {
        self.in_flight.fetch_sub(1, Ordering::Relaxed);
        match error {
            None => {
                self.reachable.store(true, Ordering::Relaxed);
                self.latency_ms
                    .store(latency.as_millis() as u64, Ordering::Relaxed);
            }
            Some(msg) => {
                self.errors_total.fetch_add(1, Ordering::Relaxed);
                self.set_last_error(msg);
            }
        }
    }

    pub fn probe_finished(&self, latency: Duration, error: Option<String>) {
        match error {
            None => {
                self.reachable.store(true, Ordering::Relaxed);
                self.latency_ms
                    .store(latency.as_millis() as u64, Ordering::Relaxed);
            }
            Some(msg) => {
                self.reachable.store(false, Ordering::Relaxed);
                self.set_last_error(msg);
            }
        }
    }

    fn set_last_error(&self, msg: String) {
        *self.last_error.lock().unwrap_or_else(|p| p.into_inner()) = Some(msg);
    }

    pub fn reachable(&self) -> bool {
        self.reachable.load(Ordering::Relaxed)
    }

    pub fn latency_ms(&self) -> u64 {
        self.latency_ms.load(Ordering::Relaxed)
    }

    pub fn requests_total(&self) -> u64 {
        self.requests_total.load(Ordering::Relaxed)
    }

    pub fn errors_total(&self) -> u64 {
        self.errors_total.load(Ordering::Relaxed)
    }

    pub fn in_flight(&self) -> i64 {
        self.in_flight.load(Ordering::Relaxed)
    }

    pub fn last_error(&self) -> Option<String> {
        self.last_error
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
}

fn host_models(by_host: &HashMap<String, Vec<String>>, id: &str) -> Vec<String> {
    by_host.get(id).cloned().unwrap_or_default()
}

fn ts_chip(info: Option<&crate::tailscale::TsInfo>) -> String {
    let Some(ts) = info else {
        return String::new();
    };
    if ts.serving {
        let fallback = format!("https://{}", ts.self_dns);
        let url = esc(ts.url.as_deref().unwrap_or(&fallback));
        format!(r#" <span class="ts"><b>TAILSCALE</b> <a href="{url}">{url}</a></span>"#)
    } else if !ts.online {
        r#" <span class="ts dim">TAILSCALE &middot; offline</span>"#.to_string()
    } else {
        r#" <span class="ts dim">TAILSCALE &middot; no serve</span>"#.to_string()
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub async fn page(State(state): State<AppState>) -> Response {
    let config = state.config.load();
    state.tailscale.spawn_refresh_if_stale(&state.client);
    let ts = state.tailscale.info();
    let ts_line = ts_chip(ts.as_ref());
    let by_host = state
        .catalog
        .model_names_by_host(&config, &state.client)
        .await;
    let mut rows = String::new();
    let mut hosts_up = 0u64;
    let mut models_total = 0u64;
    let mut requests_total = 0u64;
    let mut errors_total = 0u64;
    for id in &config.upstream_order {
        let Some(up) = config.upstreams.get(id) else {
            continue;
        };
        let stats = state.stats_for(id);
        let models = host_models(&by_host, id);
        let reqs = stats.requests_total();
        let errs = stats.errors_total();
        requests_total += reqs;
        errors_total += errs;
        models_total += models.len() as u64;
        let key_bit = if up.api_key.is_some() { "set" } else { "unset" };
        let chips = if models.is_empty() {
            r#"<span class="url">&mdash;</span>"#.to_string()
        } else {
            models
                .iter()
                .map(|m| format!(r#"<span class="model">{}</span>"#, esc(m)))
                .collect()
        };
        let reach = if up.disabled {
            r#"<span class="disabled-tag">DISABLED</span><br><span class="url">not probed</span>"#
                .to_string()
        } else if reqs == 0 {
            r#"<span class="url">&mdash;</span>"#.to_string()
        } else if stats.reachable() {
            hosts_up += 1;
            format!(
                r#"<span class="up">&#9679; up</span><br><span class="url">{}ms</span>"#,
                stats.latency_ms()
            )
        } else {
            r#"<span class="dot down"></span><span class="down">down</span>"#.to_string()
        };
        let last_err = match stats.last_error() {
            Some(e) => format!(r#"<span class="err">{}</span>"#, esc(&e)),
            None => r#"<span class="url">&mdash;</span>"#.to_string(),
        };
        let row_class = if up.disabled {
            r#"<tr class="disabled">"#
        } else {
            "<tr>"
        };
        let actions = format!(
            r#"<span class="actions"><label class="switch"><input type="checkbox" {checked} data-id="{id}" onchange="hostAction(this,'{action}')"><span class="slider"></span></label><button class="remove" data-id="{id}" onclick="hostRemove(this)">remove</button></span>"#,
            id = esc(id),
            checked = if up.disabled { "" } else { "checked" },
            action = if up.disabled { "enable" } else { "disable" },
        );
        let _ = write!(
            rows,
            r#"{row_class}<td><b>{}</b><br><span class="url">{}</span><br><span class="url">api_key: {}</span><br>{}</td><td>{}</td><td class="num" data-label="models">{}</td><td class="num" data-label="requests">{}</td><td class="num" data-label="errors">{}</td><td class="num" data-label="in-flight">{}</td><td>{}</td><td>{}</td></tr>"#,
            esc(id),
            esc(&up.base_url),
            key_bit,
            chips,
            reach,
            models.len(),
            reqs,
            errs,
            stats.in_flight(),
            last_err,
            actions
        );
    }
    let n_hosts = config.upstream_order.len();
    let html = format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="refresh" content="15">
<title>orouta — status</title>
<style>{STYLE}</style>
</head>
<body>
<div class="wrap">
<header><h1>orouta <span>/ status</span></h1></header>
<p class="sub">{n_hosts} hosts &middot; reloaded every 15s &middot; <a href="/status.json">JSON</a>{ts_line}</p>
<div class="summary">
<div class="stat"><b>{hosts_up}/{n_hosts}</b><small>hosts up</small></div>
<div class="stat"><b>{models_total}</b><small>models</small></div>
<div class="stat"><b>{requests_total}</b><small>requests</small></div>
<div class="stat"><b>{errors_total}</b><small>errors</small></div>
</div>
<table>
<thead><tr><th>Host</th><th>Reachable</th><th class="num">Models</th><th class="num">Requests</th><th class="num">Errors</th><th class="num">In-flight</th><th>Last error</th><th>Actions</th></tr></thead>
<tbody>
{rows}
</tbody>
</table>
<div class="add">
<h2>Add host</h2>
<div class="row">
<label>id<input type="text" id="add-id"></label>
<label>base_url<input type="text" id="add-url"></label>
<label>api_key (optional)<input type="password" id="add-key"></label>
<button class="btn" onclick="hostAdd(event)">Add</button>
</div>
</div>
<script>
function hostAction(el, action) {{
  fetch('/api/hosts/' + encodeURIComponent(el.dataset.id) + '/' + action, {{method: 'POST'}})
    .then(function(r) {{
      if (r.ok) {{ location.reload(); }} else {{ r.text().then(function(t) {{ alert('host update failed: ' + r.status + ' ' + t); }}); }}
    }});
}}
function hostRemove(el) {{
  if (!confirm('Remove host ' + el.dataset.id + '?')) {{ return; }}
  fetch('/api/hosts/' + encodeURIComponent(el.dataset.id), {{method: 'DELETE'}})
    .then(function(r) {{
      if (r.ok) {{ location.reload(); }} else {{ r.text().then(function(t) {{ alert('remove failed: ' + r.status + ' ' + t); }}); }}
    }});
}}
function hostAdd(e) {{
  e.preventDefault();
  var body = {{id: document.getElementById('add-id').value, base_url: document.getElementById('add-url').value}};
  var key = document.getElementById('add-key').value;
  if (key) {{ body.api_key = key; }}
  fetch('/api/hosts', {{method: 'POST', headers: {{'content-type': 'application/json'}}, body: JSON.stringify(body)}})
    .then(function(r) {{
      if (r.ok) {{ location.reload(); }} else {{ r.text().then(function(t) {{ alert('add failed: ' + r.status + ' ' + t); }}); }}
    }});
}}
</script>
</div>
</body>
</html>
"#,
    );
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], html).into_response()
}

pub async fn hosts_payload(state: &AppState, config: &Config) -> Vec<Value> {
    let by_host = state
        .catalog
        .model_names_by_host(config, &state.client)
        .await;
    config
        .upstream_order
        .iter()
        .filter_map(|id| {
            let up = config.upstreams.get(id)?;
            let stats = state.stats_for(id);
            Some(json!({
                "id": id,
                "base_url": up.base_url,
                "disabled": up.disabled,
                "api_key_set": up.api_key.is_some(),
                "reachable": stats.reachable(),
                "latency_ms": stats.latency_ms(),
                "models": host_models(&by_host, id),
                "requests_total": stats.requests_total(),
                "errors_total": stats.errors_total(),
                "in_flight": stats.in_flight(),
                "last_error": stats.last_error(),
            }))
        })
        .collect()
}

pub async fn json(State(state): State<AppState>) -> Response {
    let config = state.config.load();
    state.tailscale.spawn_refresh_if_stale(&state.client);
    let ts = state.tailscale.info();
    let hosts = hosts_payload(&state, &config).await;
    let tailscale = ts.map(|t| {
        json!({
            "self": t.self_dns,
            "tailnet": t.tailnet,
            "online": t.online,
            "serving": t.serving,
            "url": t.url,
        })
    });
    Json(json!({ "hosts": hosts, "tailscale": tailscale })).into_response()
}

const STYLE: &str = r#"
  :root {
    --bg: #070b09;
    --panel: #101612;
    --line: #2a3d34;
    --chip: #0a0f0c;
    --text: #e8dcc4;
    --muted: #8a9a8c;
    --accent: #c4a574;
    --ok: #4d7d5c;
    --bad: #b4462e;
  }
  @media (prefers-color-scheme: light) {
    :root {
      --bg: #f7f3e8;
      --panel: #fffdf6;
      --line: #ddd2b8;
      --chip: #f1ead9;
      --text: #2a3d34;
      --muted: #6e7d70;
      --accent: #9a7430;
      --ok: #4d7d5c;
      --bad: #b4462e;
    }
  }
  * { box-sizing: border-box; }
  html, body { max-width: 100%; overflow-x: hidden; }
  body {
    margin: 0;
    background: var(--bg);
    color: var(--text);
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 14px;
    line-height: 1.5;
  }
  .wrap { max-width: 960px; margin: 0 auto; padding: 32px 20px 64px; }
  header { display: flex; align-items: baseline; gap: 16px; margin-bottom: 8px; }
  h1 { font-size: 20px; font-weight: 600; margin: 0; letter-spacing: 0.02em; }
  h1 span { color: var(--accent); }
  .sub { color: var(--muted); margin: 0 0 24px; }
  .summary { display: flex; gap: 12px; flex-wrap: wrap; margin-bottom: 24px; }
  .stat {
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 10px 12px;
    flex: 1 1 72px;
    min-width: 0;
  }
  @media (max-width: 480px) { .stat { flex-basis: 40%; } }
  .stat b { display: block; font-size: 20px; font-weight: 600; }
  .stat small { color: var(--muted); }
  table { width: 100%; border-collapse: collapse; background: var(--panel); border: 1px solid var(--line); border-radius: 6px; overflow: hidden; }
  th, td { text-align: left; padding: 10px 12px; border-bottom: 1px solid var(--line); vertical-align: top; }
  th { color: var(--muted); font-weight: 500; font-size: 12px; text-transform: uppercase; letter-spacing: 0.08em; }
  tr:last-child td { border-bottom: none; }
  td b { font-weight: 600; }
  .url { color: var(--muted); }
  .num { text-align: right; font-variant-numeric: tabular-nums; }
  th.num { text-align: right; }
  .up { color: var(--ok); }
  .down { color: var(--bad); }
  .dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; margin-right: 6px; background: var(--ok); }
  .dot.down { background: var(--bad); }
  .err { color: var(--bad); font-size: 12px; }
  .model { display: inline-block; background: var(--chip); border: 1px solid var(--line); border-radius: 4px; padding: 1px 8px; margin: 2px 4px 2px 0; font-size: 12px; color: var(--text); }
  .ts { display: inline-block; background: var(--chip); border: 1px solid var(--line); border-radius: 4px; padding: 1px 8px; margin-left: 12px; font-size: 12px; }
  .ts b { color: var(--accent); letter-spacing: 0.06em; }
  .ts.dim { color: var(--muted); }
  a { color: var(--accent); text-decoration: none; }
  tr.disabled td { opacity: 0.55; }
  .disabled-tag { color: var(--muted); font-size: 12px; text-transform: uppercase; letter-spacing: 0.08em; }
  .actions { display: flex; align-items: center; gap: 10px; }
  .switch { position: relative; display: inline-block; width: 30px; height: 17px; flex: none; }
  .switch input { display: none; }
  .slider { position: absolute; inset: 0; background: var(--chip); border: 1px solid var(--line); border-radius: 17px; cursor: pointer; }
  .slider::before { content: ""; position: absolute; width: 11px; height: 11px; left: 2px; top: 2px; border-radius: 50%; background: var(--muted); transition: 0.15s; }
  .switch input:checked + .slider { background: var(--ok); border-color: var(--ok); }
  .switch input:checked + .slider::before { transform: translateX(13px); background: var(--bg); }
  .remove {
    background: none; border: 1px solid var(--line); color: var(--muted);
    border-radius: 4px; font: inherit; font-size: 12px; padding: 1px 8px; cursor: pointer;
  }
  .remove:hover { color: var(--bad); border-color: var(--bad); }
  .add {
    background: var(--panel); border: 1px solid var(--line); border-radius: 6px;
    padding: 16px; margin-top: 20px;
  }
  .add h2 { font-size: 13px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.08em; color: var(--muted); margin: 0 0 12px; }
  .add .row { display: flex; gap: 12px; flex-wrap: wrap; }
  .add label { display: flex; flex-direction: column; gap: 4px; flex: 1 1 200px; font-size: 12px; color: var(--muted); }
  .add input {
    background: var(--chip); border: 1px solid var(--line); border-radius: 4px;
    color: var(--text); font: inherit; font-size: 13px; padding: 7px 10px;
  }
  .add input:focus { outline: none; border-color: var(--accent); }
  .add .btn {
    align-self: flex-end; background: var(--accent); color: var(--bg); border: none; border-radius: 4px;
    font: inherit; font-weight: 600; font-size: 13px; padding: 8px 18px; cursor: pointer;
  }
  @media (max-width: 720px) {
    table, thead, tbody { display: block; }
    thead { display: none; }
    tr { display: flex; flex-wrap: wrap; align-items: flex-start; border-bottom: 1px solid var(--line); padding: 14px 0 10px; }
    td { border: none; padding: 2px 12px; }
    tr td:nth-child(1) { flex: 1 1 100%; order: 1; }
    tr td:nth-child(2) { flex: 0 1 34%; order: 2; margin-top: 4px; }
    tr td:nth-child(7) { flex: 1 1 60%; order: 3; margin-top: 4px; }
    tr td:nth-child(8) { flex: 1 1 100%; order: 5; margin-top: 8px; padding-top: 8px; border-top: 1px solid var(--line); }
    td.num { flex: 1 1 25%; order: 4; text-align: center; margin-top: 10px; padding-top: 8px; border-top: 1px solid var(--line); }
    .num::before { content: attr(data-label); display: block; color: var(--muted); font-size: 11px; letter-spacing: 0.06em; margin-right: 0; margin-bottom: 2px; }
    .actions { margin-top: 0; }
  }
"#;
