use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;

const KEY: &str = "sk-orouta-alice";

fn toml_for(keys: &[&str]) -> String {
    let list: Vec<String> = keys.iter().map(|k| format!("\"{k}\"")).collect();
    format!(
        "host = \"127.0.0.1\"\nport = 0\n\n[auth]\nkeys = [{}]\n\n[[upstream]]\nid = \"home\"\nbase_url = \"http://127.0.0.1:1\"\n",
        list.join(", ")
    )
}

struct TempDir(PathBuf);

impl std::ops::Deref for TempDir {
    type Target = PathBuf;

    fn deref(&self) -> &PathBuf {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

fn temp_dir() -> TempDir {
    let dir = std::env::temp_dir().join(format!("orouta-login-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    TempDir(dir)
}

async fn serve(dir: &TempDir) -> String {
    let cfg_path = dir.join("orouta.toml");
    let config = orouta::Config::load(&cfg_path).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = orouta::app(Arc::new(config), reqwest::Client::new(), Some(cfg_path));
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

async fn start(keys: &[&str]) -> (String, TempDir) {
    let dir = temp_dir();
    std::fs::write(dir.join("orouta.toml"), toml_for(keys)).unwrap();
    let base = serve(&dir).await;
    (base, dir)
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

async fn login(base: &str, key: &str) -> reqwest::Response {
    client()
        .post(format!("{base}/api/login"))
        .json(&json!({ "key": key }))
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn login_with_valid_key_sets_httponly_cookie() {
    let (base, _dir) = start(&[KEY]).await;
    let res = login(&base, KEY).await;
    assert_eq!(res.status(), 200);
    let cookie = res.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(cookie.starts_with("orouta_key=sk-orouta-alice"));
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Lax"));
}

#[tokio::test]
async fn login_with_wrong_key_is_401() {
    let (base, _dir) = start(&[KEY]).await;
    let res = login(&base, "sk-wrong").await;
    assert_eq!(res.status(), 401);
    let cookie = res.headers().get("set-cookie");
    assert!(cookie.is_none());
}

#[tokio::test]
async fn login_with_missing_body_is_400() {
    let (base, _dir) = start(&[KEY]).await;
    let res = client()
        .post(format!("{base}/api/login"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn session_cookie_authorizes_browser_navigation() {
    let (base, _dir) = start(&[KEY]).await;
    let res = login(&base, KEY).await;
    let cookie = res.headers().get("set-cookie").unwrap().to_str().unwrap();
    let pair = cookie.split(';').next().unwrap();
    let res = client()
        .get(format!("{base}/status"))
        .header("Cookie", pair)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert!(res.text().await.unwrap().contains(r#"id="root""#));
    let res = client()
        .get(format!("{base}/keys"))
        .header("Cookie", pair)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let v: Value = client()
        .get(format!("{base}/status.json"))
        .header("Cookie", pair)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(v["hosts"].is_array());
}

#[tokio::test]
async fn session_cookie_stamps_last_used() {
    let (base, _dir) = start(&[KEY]).await;
    let res = login(&base, KEY).await;
    let pair = res
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    client()
        .get(format!("{base}/api/tags"))
        .header("Cookie", &pair)
        .send()
        .await
        .unwrap();
    let v: Value = client()
        .get(format!("{base}/api/keys"))
        .header("Cookie", &pair)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(v["keys"][0]["last_used"], "just now");
}

#[tokio::test]
async fn unauthenticated_browser_gets_shell_but_data_401s() {
    let (base, _dir) = start(&[KEY]).await;
    let res = client()
        .get(format!("{base}/status"))
        .header("Accept", "text/html,application/xhtml+xml")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert!(res.text().await.unwrap().contains(r#"id="root""#));
    let res = client()
        .get(format!("{base}/status.json"))
        .header("Accept", "text/html,application/xhtml+xml")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
    let v: Value = res.json().await.unwrap();
    assert_eq!(v["error"], "unauthorized");
}

#[tokio::test]
async fn api_clients_still_get_401_json() {
    let (base, _dir) = start(&[KEY]).await;
    let res = client()
        .get(format!("{base}/api/tags"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
    let v: Value = res.json().await.unwrap();
    assert_eq!(v["error"], "unauthorized");
}

#[tokio::test]
async fn login_route_serves_shell_with_keys() {
    let (base, _dir) = start(&[KEY]).await;
    let res = client().get(format!("{base}/login")).send().await.unwrap();
    assert_eq!(res.status(), 200);
    let page = res.text().await.unwrap();
    assert!(page.contains(r#"id="root""#));
}

#[tokio::test]
async fn open_proxy_serves_shell_and_empty_keys_list() {
    let (base, _dir) = start(&[]).await;
    let res = client().get(format!("{base}/login")).send().await.unwrap();
    assert_eq!(res.status(), 200);
    let v: Value = client()
        .get(format!("{base}/api/keys"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(v["keys"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn login_refused_when_open() {
    let (base, _dir) = start(&[]).await;
    let res = login(&base, KEY).await;
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn revoking_session_key_kills_the_session() {
    let (base, _dir) = start(&[KEY, "sk-orouta-bob"]).await;
    let res = login(&base, KEY).await;
    let pair = res
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let res = client()
        .delete(format!("{base}/api/keys/{}", key_id(KEY)))
        .header("Authorization", "Bearer sk-orouta-bob")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let res = client()
        .get(format!("{base}/status"))
        .header("Cookie", &pair)
        .header("Accept", "text/html,application/xhtml+xml")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let res = client()
        .get(format!("{base}/status.json"))
        .header("Cookie", &pair)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn cookie_takes_effect_alongside_header_auth() {
    let (base, _dir) = start(&[KEY]).await;
    let res = login(&base, KEY).await;
    let pair = res
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let res = client()
        .get(format!("{base}/api/tags"))
        .header("Cookie", &pair)
        .header("Accept", "*/*")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

fn key_id(secret: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in secret.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("k{h:08x}")
}
