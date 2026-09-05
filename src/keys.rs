use crate::overlay::{self, Overlay, OverlayKey};
use crate::status::{esc, STYLE};
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fmt::Write as _;
use std::sync::Arc;

pub fn stamp(state: &AppState, key: &str) {
    if let Ok(mut map) = state.key_usage.lock() {
        map.insert(key.to_string(), now_secs());
    }
}

struct KeyRow {
    id: String,
    label: String,
    prefix: String,
    created: String,
    last_used: String,
    secret: String,
}

#[derive(Deserialize)]
pub struct AddKey {
    #[serde(default)]
    label: Option<String>,
}

pub async fn page(State(state): State<AppState>) -> Response {
    state.tailscale.spawn_refresh_if_stale(&state.client);
    let (banner, rows_html) = match rows(&state) {
        Ok(rows) => (String::new(), render_rows(&rows)),
        Err(e) => (
            format!(r#"<div class="error">overlay error: {}</div>"#, esc(&e)),
            String::new(),
        ),
    };
    let create_panel = if state.config.load().keys.is_empty() {
        r#"<p class="url" style="margin:14px 0 0">Key management is locked &mdash; configure <span style="color:var(--text)">[auth].keys</span> in orouta.toml first. A proxy without keys is open and can't be administered remotely.</p>"#.to_string()
    } else {
        r#"<div class="row" style="margin-top:14px">
<label>label<input type="text" id="new-label"></label>
<button class="btn" onclick="createKey(event)">Create key</button>
</div>"#
            .to_string()
    };
    let html = format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>orouta — api keys</title>
<style>{STYLE}</style>
</head>
<body>
<div class="wrap">
<header><img class="logo" src="/logo.png" alt="orouta"><h1><span>/ api keys</span></h1><nav><a href="/status">hosts</a> &middot; <a href="/keys" class="active">api keys</a></nav></header>
<p class="sub">keys authorize everything the proxy can do &middot; revoked keys stop working on the next request</p>
{banner}
<div class="add" style="margin-top:0">
<h2>API keys</h2>
<div id="reveal" class="reveal"></div>
<table>
<thead><tr><th>Label</th><th>Key</th><th>Created</th><th>Last used</th><th></th></tr></thead>
<tbody id="key-rows">
{rows_html}
</tbody>
</table>
{create_panel}
</div>
<script>
function renderKeys(keys) {{
  var tb = document.getElementById('key-rows');
  tb.innerHTML = '';
  keys.forEach(function(k) {{
    var tr = document.createElement('tr');
    tr.innerHTML = '<td><span class="klabel"></span></td><td><span class="kprefix"></span></td><td><span class="ktime"></span></td><td><span class="ktime"></span></td><td><button class="revoke">revoke</button></td>';
    tr.children[0].firstChild.textContent = k.label;
    tr.children[1].firstChild.textContent = k.prefix + '\u2026';
    tr.children[2].firstChild.textContent = k.created;
    tr.children[3].firstChild.textContent = k.last_used;
    var btn = tr.querySelector('button');
    if (k.last) {{ btn.setAttribute('data-last', '1'); }}
    btn.onclick = function() {{ revokeKey(k.id, btn); }};
    tb.appendChild(tr);
  }});
}}
function createKey(e) {{
  e.preventDefault();
  var label = document.getElementById('new-label').value;
  fetch('/api/keys', {{method: 'POST', headers: {{'content-type': 'application/json'}}, body: JSON.stringify(label ? {{label: label}} : {{}})}})
    .then(function(r) {{
      if (!r.ok) {{ r.text().then(function(t) {{ alert('create failed: ' + r.status + ' ' + t); }}); return; }}
      r.json().then(function(v) {{
        var el = document.getElementById('reveal');
        el.innerHTML = '<button class="btn" onclick="copySecret(this)">copy</button><b>New key created &mdash; copy it now, it won&#8217;t be shown again</b><code></code>';
        el.querySelector('code').textContent = v.secret;
        renderKeys(v.keys);
        document.getElementById('new-label').value = '';
      }});
    }});
}}
function copySecret(btn) {{
  var code = btn.parentElement.querySelector('code').textContent;
  navigator.clipboard.writeText(code);
  btn.textContent = 'copied';
}}
function revokeKey(id, btn) {{
  if (btn.hasAttribute('data-last') && !confirm('This is the last API key. Revoking it leaves the proxy without keys until you add one to orouta.toml. Revoke anyway?')) {{ return; }}
  fetch('/api/keys/' + encodeURIComponent(id), {{method: 'DELETE'}})
    .then(function(r) {{
      if (r.ok) {{ r.json().then(function(v) {{ renderKeys(v.keys); }}); }}
      else {{ r.text().then(function(t) {{ alert('revoke failed: ' + r.status + ' ' + t); }}); }}
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

pub async fn create(State(state): State<AppState>, body: Option<Json<AddKey>>) -> Response {
    let overlay_path = match overlay_target(&state) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    let label = body
        .and_then(|Json(b)| b.label)
        .map(|l| l.trim().chars().take(LABEL_MAX).collect::<String>())
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| "unnamed".to_string());
    let secret = format!("orouta_{}", hex_secret());
    let _lock = state.overlay_lock.lock().await;
    if let Some(r) = open_proxy(&state) {
        return r;
    }
    let mut o = match overlay::load(overlay_path) {
        Ok(o) => o,
        Err(e) => return server_error(e),
    };
    o.keys.added.push(OverlayKey {
        label,
        secret: secret.clone(),
        created: now_secs().to_string(),
    });
    if let Err(e) = persist(&state, overlay_path, &o).await {
        return server_error(e);
    }
    let keys = match rows(&state) {
        Ok(r) => r,
        Err(e) => return server_error(e),
    };
    Json(json!({ "secret": secret, "keys": views(&keys) })).into_response()
}

pub async fn revoke(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let overlay_path = match overlay_target(&state) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    let _lock = state.overlay_lock.lock().await;
    if let Some(r) = open_proxy(&state) {
        return r;
    }
    let current = match rows(&state) {
        Ok(r) => r,
        Err(e) => return server_error(e),
    };
    let Some(secret) = current
        .iter()
        .find(|r| r.id == id)
        .map(|r| r.secret.clone())
    else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "unknown key", "id": id})),
        )
            .into_response();
    };
    let mut o = match overlay::load(overlay_path) {
        Ok(o) => o,
        Err(e) => return server_error(e),
    };
    o.keys.added.retain(|a| a.secret != secret);
    if !o.keys.revoked.contains(&secret) {
        o.keys.revoked.push(secret.clone());
    }
    if let Err(e) = persist(&state, overlay_path, &o).await {
        return server_error(e);
    }
    if let Ok(mut map) = state.key_usage.lock() {
        map.remove(&secret);
    }
    let keys = match rows(&state) {
        Ok(r) => r,
        Err(e) => return server_error(e),
    };
    Json(json!({ "keys": views(&keys) })).into_response()
}

const LABEL_MAX: usize = 64;

fn overlay_target(state: &AppState) -> Result<&std::path::Path, Box<Response>> {
    match &state.overlay {
        Some(p) => Ok(p),
        None => Err(Box::new(
            (
                StatusCode::FORBIDDEN,
                Json(json!({"error": "key management unavailable"})),
            )
                .into_response(),
        )),
    }
}

fn open_proxy(state: &AppState) -> Option<Response> {
    if state.config.load().keys.is_empty() {
        return Some(
            (
                StatusCode::FORBIDDEN,
                Json(json!({"error": "key management requires configured auth keys"})),
            )
                .into_response(),
        );
    }
    None
}

fn rows(state: &AppState) -> Result<Vec<KeyRow>, String> {
    let overlay = match &state.overlay {
        Some(p) => overlay::load(p)?,
        None => Overlay::default(),
    };
    let config = state.config.load().clone();
    prune_usage(state, &overlay, &config);
    let now = now_secs();
    let mut out = Vec::new();
    for key in config
        .raw_keys
        .iter()
        .filter(|k| !overlay.keys.revoked.contains(k))
    {
        out.push(make_row(
            "from orouta.toml".to_string(),
            key.clone(),
            "in config file".to_string(),
            state,
            now,
        ));
    }
    for added in overlay
        .keys
        .added
        .iter()
        .filter(|a| !overlay.keys.revoked.contains(&a.secret))
    {
        let created = added
            .created
            .parse::<u64>()
            .map(date_from_secs)
            .unwrap_or_else(|_| added.created.clone());
        out.push(make_row(
            added.label.clone(),
            added.secret.clone(),
            created,
            state,
            now,
        ));
    }
    Ok(out)
}

fn prune_usage(state: &AppState, overlay: &Overlay, config: &crate::config::Config) {
    let mut active: Vec<&str> = config
        .raw_keys
        .iter()
        .filter(|k| !overlay.keys.revoked.contains(k))
        .map(|k| k.as_str())
        .collect();
    active.extend(
        overlay
            .keys
            .added
            .iter()
            .filter(|a| !overlay.keys.revoked.contains(&a.secret))
            .map(|a| a.secret.as_str()),
    );
    if let Ok(mut map) = state.key_usage.lock() {
        map.retain(|secret, _| active.contains(&secret.as_str()));
    }
}

fn make_row(label: String, secret: String, created: String, state: &AppState, now: u64) -> KeyRow {
    let last_used = state
        .key_usage
        .lock()
        .ok()
        .and_then(|m| m.get(&secret).copied())
        .map(|s| ago_label(s, now))
        .unwrap_or_else(|| "never".to_string());
    KeyRow {
        id: key_id(&secret),
        label,
        prefix: secret.chars().take(12).collect(),
        created,
        last_used,
        secret,
    }
}

fn views(rows: &[KeyRow]) -> Vec<Value> {
    let last = rows.len() == 1;
    rows.iter()
        .map(|r| {
            json!({
                "id": r.id,
                "label": r.label,
                "prefix": r.prefix,
                "created": r.created,
                "last_used": r.last_used,
                "last": last,
            })
        })
        .collect()
}

fn render_rows(rows: &[KeyRow]) -> String {
    let last = rows.len() == 1;
    let mut out = String::new();
    for r in rows {
        let data_last = if last { r#" data-last="1""# } else { "" };
        let _ = write!(
            out,
            r#"<tr><td><span class="klabel">{}</span></td><td><span class="kprefix">{}&hellip;</span></td><td><span class="ktime">{}</span></td><td><span class="ktime">{}</span></td><td><button class="revoke" data-id="{}"{data_last} onclick="revokeKey('{}', this)">revoke</button></td></tr>"#,
            esc(&r.label),
            esc(&r.prefix),
            esc(&r.created),
            esc(&r.last_used),
            esc(&r.id),
            esc(&r.id),
        );
    }
    out
}

async fn persist(
    state: &AppState,
    overlay_path: &std::path::Path,
    o: &Overlay,
) -> Result<(), String> {
    overlay::save(overlay_path, o)?;
    let merged = overlay::apply(o, &state.config.load().clone());
    state.config.store(Arc::new(merged));
    Ok(())
}

fn server_error(msg: String) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": msg})),
    )
        .into_response()
}

fn hex_secret() -> String {
    uuid::Uuid::new_v4()
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn key_id(secret: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in secret.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("k{h:08x}")
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn date_from_secs(secs: u64) -> String {
    let z = (secs / 86400) as i64 + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

fn ago_label(secs: u64, now: u64) -> String {
    let d = now.saturating_sub(secs);
    match d {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{}m ago", d / 60),
        3600..=86399 => format!("{}h ago", d / 3600),
        _ => format!("{}d ago", d / 86400),
    }
}
