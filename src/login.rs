use crate::auth::{cookie_safe, token_eq, COOKIE};
use crate::status::STYLE;
use crate::AppState;
use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Redirect, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

pub async fn page(State(state): State<AppState>) -> Response {
    if state.config.load().keys.is_empty() {
        return Redirect::to("/status").into_response();
    }
    let html = format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>orouta — login</title>
<style>{STYLE}</style>
</head>
<body>
<div class="wrap">
<header><img class="logo" src="/logo.png" alt="orouta"><h1><span>/ login</span></h1></header>
<form class="add" style="max-width:420px;margin:14vh auto 0" onsubmit="doLogin(event)">
<h2>API key</h2>
<div class="row">
<label>key<input type="password" id="key" autofocus></label>
<button class="btn" type="submit">Sign in</button>
</div>
<p class="url" id="err" style="margin:8px 0 0"></p>
</form>
<script>
function doLogin(e) {{
  e.preventDefault();
  var key = document.getElementById('key').value;
  fetch('/api/login', {{method: 'POST', headers: {{'content-type': 'application/json'}}, body: JSON.stringify({{key: key}})}})
    .then(function(r) {{
      if (r.ok) {{ location.href = '/status'; }}
      else {{ document.getElementById('err').textContent = 'invalid api key'; }}
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

#[derive(Deserialize)]
pub struct LoginBody {
    key: String,
}

pub async fn api(State(state): State<AppState>, body: Option<Json<LoginBody>>) -> Response {
    let keys = state.config.load().keys.clone();
    let Some(Json(body)) = body else {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({"error": "key required"})),
        )
            .into_response();
    };
    let presented = body.key.trim();
    let Some(matched) = keys.iter().find(|k| token_eq(presented, k)) else {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            Json(json!({"error": "invalid api key"})),
        )
            .into_response();
    };
    if !cookie_safe(matched) {
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "configured key cannot be stored in a cookie"})),
        )
            .into_response();
    }
    let cookie = format!("{COOKIE}={matched}; HttpOnly; SameSite=Lax; Path=/");
    (
        [(header::SET_COOKIE, cookie)],
        Json(json!({"ok": true})),
    )
        .into_response()
}
