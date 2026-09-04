use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

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
    let dir = std::env::temp_dir().join(format!("orouta-keys-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    TempDir(dir)
}

fn prepare(keys: &[&str]) -> TempDir {
    let dir = temp_dir();
    std::fs::write(dir.join("orouta.toml"), toml_for(keys)).unwrap();
    dir
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

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

fn aget(url: String) -> reqwest::RequestBuilder {
    client()
        .get(url)
        .header("Authorization", format!("Bearer {KEY}"))
}

fn apost(url: String) -> reqwest::RequestBuilder {
    client()
        .post(url)
        .header("Authorization", format!("Bearer {KEY}"))
}

fn adelete(url: String) -> reqwest::RequestBuilder {
    client()
        .delete(url)
        .header("Authorization", format!("Bearer {KEY}"))
}

fn get_with(url: String, key: &str) -> reqwest::RequestBuilder {
    client()
        .get(url)
        .header("Authorization", format!("Bearer {key}"))
}

fn overlay_path(dir: &TempDir) -> PathBuf {
    dir.join("orouta.overlay.json")
}

async fn start(keys: &[&str]) -> (String, TempDir) {
    let dir = prepare(keys);
    let base = serve(&dir).await;
    (base, dir)
}

async fn text_at(url: String, key: Option<&str>) -> String {
    let req = match key {
        Some(k) => get_with(url, k),
        None => client().get(url),
    };
    req.send().await.unwrap().text().await.unwrap()
}

async fn status_code(req: reqwest::RequestBuilder) -> u16 {
    req.send().await.unwrap().status().as_u16()
}

async fn create_key(base: &str, label: &str) -> Value {
    let v: Value = apost(format!("{base}/api/keys"))
        .json(&json!({"label": label}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    v
}

#[tokio::test]
async fn create_key_returns_secret_once_and_records_overlay() {
    let (base, dir) = start(&[KEY]).await;
    let v = create_key(&base, "ci").await;
    let secret = v["secret"].as_str().unwrap();
    assert!(secret.starts_with("orouta_"));
    let hex = secret.trim_start_matches("orouta_");
    assert_eq!(hex.len(), 32);
    assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(v["secret"].as_array().map(|a| a.len()), None);
    let raw = serde_json::to_string(&v).unwrap();
    assert_eq!(raw.matches(secret).count(), 1);
    let keys = v["keys"].as_array().unwrap();
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0]["id"], "k1");
    assert_eq!(keys[0]["label"], "from orouta.toml");
    assert_eq!(keys[1]["id"], "k2");
    assert_eq!(keys[1]["label"], "ci");
    for entry in keys {
        let text = serde_json::to_string(entry).unwrap();
        assert!(!text.contains(secret));
        assert!(entry["prefix"].as_str().unwrap().len() == 12);
    }
    let overlay: Value =
        serde_json::from_str(&std::fs::read_to_string(overlay_path(&dir)).unwrap()).unwrap();
    assert_eq!(overlay["keys"]["added"][0]["label"], "ci");
    assert_eq!(overlay["keys"]["added"][0]["secret"], secret);
    assert!(overlay["keys"]["added"][0]["created"].is_string());
    let perms = std::fs::metadata(overlay_path(&dir)).unwrap().permissions();
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(perms.mode() & 0o777, 0o600);
    assert_eq!(
        status_code(get_with(format!("{base}/status.json"), secret)).await,
        200
    );
    assert_eq!(status_code(aget(format!("{base}/status.json"))).await, 200);
}

#[tokio::test]
async fn keys_page_lists_keys_without_secrets() {
    let (base, _dir) = start(&[KEY]).await;
    let v = create_key(&base, "ci").await;
    let secret = v["secret"].as_str().unwrap().to_string();
    assert_eq!(
        status_code(get_with(format!("{base}/status.json"), &secret)).await,
        200
    );
    let _ = create_key(&base, "unused").await;
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let page = text_at(format!("{base}/keys"), Some(KEY)).await;
    assert!(page.contains("from orouta.toml"));
    assert!(page.contains("in config file"));
    assert!(page.contains("ci"));
    assert!(page.contains("unused"));
    assert!(page.contains(&format!("{}&hellip;", &secret[..12])));
    assert!(!page.contains(secret.as_str()));
    assert!(page.contains("just now"));
    assert!(page.contains("never"));
}

#[tokio::test]
async fn revoke_stops_key_on_next_request() {
    let (base, dir) = start(&[KEY]).await;
    let v = create_key(&base, "ci").await;
    let secret = v["secret"].as_str().unwrap().to_string();
    let res = adelete(format!("{base}/api/keys/k2")).send().await.unwrap();
    assert_eq!(res.status(), 200);
    let overlay: Value =
        serde_json::from_str(&std::fs::read_to_string(overlay_path(&dir)).unwrap()).unwrap();
    assert_eq!(overlay["keys"]["revoked"][0], secret.as_str());
    assert_eq!(overlay["keys"]["added"].as_array().unwrap().len(), 0);
    assert_eq!(
        status_code(get_with(format!("{base}/status.json"), &secret)).await,
        401
    );
    assert_eq!(status_code(aget(format!("{base}/status.json"))).await, 200);
}

#[tokio::test]
async fn toml_edits_cannot_resurrect_revoked_key() {
    let (base, dir) = start(&[KEY]).await;
    let res = adelete(format!("{base}/api/keys/k1")).send().await.unwrap();
    assert_eq!(res.status(), 200);
    std::fs::write(dir.join("orouta.toml"), toml_for(&[KEY, "sk-orouta-bob"])).unwrap();
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert_eq!(
        status_code(get_with(format!("{base}/status.json"), "sk-orouta-bob")).await,
        200
    );
    assert_eq!(
        status_code(get_with(format!("{base}/status.json"), KEY)).await,
        401
    );
}

#[tokio::test]
async fn unknown_key_id_returns_404() {
    let (base, _dir) = start(&[KEY]).await;
    let res = adelete(format!("{base}/api/keys/k9")).send().await.unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn revoking_last_key_leaves_proxy_open() {
    let (base, dir) = start(&[KEY]).await;
    let res = adelete(format!("{base}/api/keys/k1")).send().await.unwrap();
    assert_eq!(res.status(), 200);
    let overlay: Value =
        serde_json::from_str(&std::fs::read_to_string(overlay_path(&dir)).unwrap()).unwrap();
    assert_eq!(overlay["keys"]["revoked"][0], KEY);
    assert_eq!(
        status_code(client().get(format!("{base}/status.json"))).await,
        200
    );
}

#[tokio::test]
async fn open_proxy_refuses_key_mutations() {
    let (base, _dir) = start(&[]).await;
    let res = client()
        .post(format!("{base}/api/keys"))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
    let res = client()
        .delete(format!("{base}/api/keys/k1"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
    let res = client().get(format!("{base}/keys")).send().await.unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn corrupt_overlay_shows_error_banner() {
    let (base, dir) = start(&[KEY]).await;
    std::fs::write(overlay_path(&dir), "{not json").unwrap();
    let page = text_at(format!("{base}/keys"), Some(KEY)).await;
    assert!(page.contains("overlay error"));
    assert!(!page.contains("from orouta.toml"));
    let res = apost(format!("{base}/api/keys"))
        .json(&json!({"label": "x"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn status_page_links_to_keys_page() {
    let (base, _dir) = start(&[KEY]).await;
    let page = text_at(format!("{base}/status"), Some(KEY)).await;
    assert!(page.contains("href=\"/keys\""));
    assert!(page.contains("api keys"));
    let keys_page = text_at(format!("{base}/keys"), Some(KEY)).await;
    assert!(keys_page.contains("href=\"/status\""));
    assert!(keys_page.contains("hosts"));
}

#[tokio::test]
async fn status_json_has_no_keys() {
    let (base, _dir) = start(&[KEY]).await;
    let _ = create_key(&base, "ci").await;
    let v: Value = aget(format!("{base}/status.json"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(v.get("keys").is_none());
}
