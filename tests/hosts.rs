use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "sk-orouta-alice";

fn toml_for(upstreams: &[(&str, &str)], keys: &str) -> String {
    let mut out = format!("host = \"127.0.0.1\"\nport = 0\n\n[auth]\nkeys = [{keys}]\n\n");
    for (id, url) in upstreams {
        out.push_str(&format!(
            "[[upstream]]\nid = \"{id}\"\nbase_url = \"{url}\"\n\n"
        ));
    }
    out
}

fn names_tag(names: &[&str]) -> Value {
    let models: Vec<Value> = names
        .iter()
        .map(|n| json!({"name": n, "model": n}))
        .collect();
    json!({"models": models})
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
    let dir = std::env::temp_dir().join(format!("orouta-hosts-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    TempDir(dir)
}

fn prepare(upstreams: &[(&str, &str)], keys: &str) -> TempDir {
    let dir = temp_dir();
    std::fs::write(dir.join("orouta.toml"), toml_for(upstreams, keys)).unwrap();
    dir
}

fn write_overlay(dir: &TempDir, overlay: &Value) {
    std::fs::write(overlay_path(dir), overlay.to_string()).unwrap();
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

async fn start(upstreams: &[(&str, &str)]) -> (String, TempDir) {
    let dir = prepare(upstreams, r#""sk-orouta-alice""#);
    let base = serve(&dir).await;
    (base, dir)
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

async fn mount_tags(server: &MockServer, names: &[&str]) {
    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(names_tag(names)))
        .mount(server)
        .await;
}

async fn mount_chat(server: &MockServer, delay: Duration) {
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"done": true}))
                .set_delay(delay),
        )
        .mount(server)
        .await;
}

async fn status_hosts(base: &str) -> Vec<Value> {
    let v: Value = aget(format!("{base}/status.json"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    v["hosts"].as_array().unwrap().clone()
}

async fn tag_names(base: &str) -> Vec<String> {
    let v: Value = aget(format!("{base}/api/tags"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    v["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap().to_string())
        .collect()
}

async fn wait_for_tag(base: &str, name: &str) -> bool {
    for _ in 0..40 {
        if tag_names(base).await.iter().any(|n| n == name) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    false
}

fn overlay_path(dir: &TempDir) -> PathBuf {
    dir.join("orouta.overlay.json")
}

#[tokio::test]
async fn add_host_persists_and_serves() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    mount_tags(&home, &["llama3:latest"]).await;
    mount_tags(&desk, &["mistral"]).await;
    mount_chat(&desk, Duration::ZERO).await;
    let (base, dir) = start(&[("home", &home.uri())]).await;

    let res = apost(format!("{base}/api/hosts"))
        .json(&json!({"id": "desk", "base_url": desk.uri()}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let v: Value = res.json().await.unwrap();
    let hosts = v["hosts"].as_array().unwrap();
    assert_eq!(hosts.len(), 2);
    let desk_host = hosts.iter().find(|h| h["id"] == "desk").unwrap();
    assert_eq!(desk_host["base_url"], desk.uri().as_str());
    assert_eq!(desk_host["disabled"], false);
    assert_eq!(desk_host["api_key_set"], false);

    assert!(wait_for_tag(&base, "mistral").await);

    let res = client()
        .post(format!("{base}/api/chat"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({"model": "mistral", "messages": [{"role": "user", "content": "hi"}]}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let overlay: Value =
        serde_json::from_str(&std::fs::read_to_string(overlay_path(&dir)).unwrap()).unwrap();
    assert_eq!(overlay["hosts"]["added"][0]["id"], "desk");
    assert_eq!(
        overlay["hosts"]["added"][0]["base_url"],
        desk.uri().as_str()
    );
}

#[tokio::test]
async fn add_host_rejects_invalid_input() {
    let home = MockServer::start().await;
    mount_tags(&home, &["llama3:latest"]).await;
    let (base, dir) = start(&[("home", &home.uri())]).await;

    for base_url in ["ftp://example.com", "not a url", "http://"] {
        let res = apost(format!("{base}/api/hosts"))
            .json(&json!({"id": "x", "base_url": base_url}))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400, "base_url {base_url}");
    }
    let res = apost(format!("{base}/api/hosts"))
        .json(&json!({"id": "", "base_url": "http://example.com"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    let res = apost(format!("{base}/api/hosts"))
        .json(&json!({"id": "home", "base_url": "http://example.com"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    assert!(!overlay_path(&dir).exists());
}

#[tokio::test]
async fn add_host_with_api_key_not_echoed() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    mount_tags(&home, &["llama3:latest"]).await;
    mount_tags(&desk, &[]).await;
    let (base, _dir) = start(&[("home", &home.uri())]).await;

    let res = apost(format!("{base}/api/hosts"))
        .json(&json!({"id": "desk", "base_url": desk.uri(), "api_key": "sk-secret-123"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body = res.text().await.unwrap();
    assert!(!body.contains("sk-secret-123"));
    let v: Value = serde_json::from_str(&body).unwrap();
    let desk_host = v["hosts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["id"] == "desk")
        .unwrap();
    assert_eq!(desk_host["api_key_set"], true);

    let body = aget(format!("{base}/status.json"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!body.contains("sk-secret-123"));
    assert!(body.contains("api_key_set"));
}

#[tokio::test]
async fn disable_excludes_host_enable_restores() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    mount_tags(&home, &["llama3:latest"]).await;
    mount_tags(&desk, &["mistral"]).await;
    let (base, dir) = start(&[("home", &home.uri()), ("desk", &desk.uri())]).await;
    assert!(wait_for_tag(&base, "mistral").await);

    let res = apost(format!("{base}/api/hosts/desk/disable"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let hosts = status_hosts(&base).await;
    let desk_host = hosts.iter().find(|h| h["id"] == "desk").unwrap();
    assert_eq!(desk_host["disabled"], true);
    assert!(!tag_names(&base).await.iter().any(|n| n == "mistral"));

    let overlay: Value =
        serde_json::from_str(&std::fs::read_to_string(overlay_path(&dir)).unwrap()).unwrap();
    assert_eq!(overlay["hosts"]["disabled"][0], "desk");

    let res = apost(format!("{base}/api/hosts/desk/enable"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert!(wait_for_tag(&base, "mistral").await);
}

#[tokio::test]
async fn remove_host_and_toml_readd_stays_removed() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    mount_tags(&home, &["llama3:latest"]).await;
    mount_tags(&desk, &["mistral"]).await;
    let (base, dir) = start(&[("home", &home.uri()), ("desk", &desk.uri())]).await;
    assert!(wait_for_tag(&base, "mistral").await);

    let res = adelete(format!("{base}/api/hosts/desk"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let hosts = status_hosts(&base).await;
    assert!(hosts.iter().all(|h| h["id"] != "desk"));
    assert!(!tag_names(&base).await.iter().any(|n| n == "mistral"));

    let overlay: Value =
        serde_json::from_str(&std::fs::read_to_string(overlay_path(&dir)).unwrap()).unwrap();
    assert_eq!(overlay["hosts"]["removed"][0], "desk");

    std::fs::write(
        dir.join("orouta.toml"),
        toml_for(
            &[("home", &home.uri()), ("desk", &desk.uri())],
            r#""sk-orouta-alice""#,
        ),
    )
    .unwrap();
    tokio::time::sleep(Duration::from_millis(1600)).await;
    let hosts = status_hosts(&base).await;
    assert!(hosts.iter().all(|h| h["id"] != "desk"));
}

#[tokio::test]
async fn overlay_removed_at_startup_wins() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    mount_tags(&home, &["llama3:latest"]).await;
    mount_tags(&desk, &["mistral"]).await;
    let dir = prepare(
        &[("home", &home.uri()), ("desk", &desk.uri())],
        r#""sk-orouta-alice""#,
    );
    write_overlay(
        &dir,
        &json!({"hosts": {"disabled": [], "removed": ["desk"], "added": []}}),
    );
    let base = serve(&dir).await;
    let hosts = status_hosts(&base).await;
    assert!(hosts.iter().all(|h| h["id"] != "desk"));
    assert!(!tag_names(&base).await.iter().any(|n| n == "mistral"));
}

#[tokio::test]
async fn remove_refuses_in_flight() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    mount_tags(&home, &["llama3:latest"]).await;
    mount_tags(&desk, &["mistral"]).await;
    mount_chat(&home, Duration::from_millis(600)).await;
    let (base, _dir) = start(&[("home", &home.uri()), ("desk", &desk.uri())]).await;
    assert!(wait_for_tag(&base, "llama3:latest").await);

    let chat = tokio::spawn({
        let base = base.clone();
        async move {
            client()
                .post(format!("{base}/api/chat"))
                .header("Authorization", format!("Bearer {KEY}"))
                .json(&json!({"model": "llama3", "messages": [{"role": "user", "content": "hi"}]}))
                .send()
                .await
                .unwrap()
        }
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    let res = adelete(format!("{base}/api/hosts/home"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 409);
    let res = chat.await.unwrap();
    assert_eq!(res.status(), 200);

    let res = adelete(format!("{base}/api/hosts/home"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn mutations_forbidden_without_keys() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    mount_tags(&home, &["llama3:latest"]).await;
    mount_tags(&desk, &[]).await;
    let dir = prepare(&[("home", &home.uri())], "");
    let base = serve(&dir).await;
    let res = client()
        .post(format!("{base}/api/hosts"))
        .json(&json!({"id": "desk", "base_url": desk.uri()}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
    let res = client()
        .post(format!("{base}/api/hosts/home/disable"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
    let res = client()
        .delete(format!("{base}/api/hosts/home"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
    let res = client()
        .post(format!("{base}/api/hosts/home/enable"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
    let hosts = status_hosts(&base).await;
    assert_eq!(hosts.len(), 1);
}

#[tokio::test]
async fn mutations_require_auth_when_keys_set() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    mount_tags(&home, &["llama3:latest"]).await;
    mount_tags(&desk, &[]).await;
    let (base, _dir) = start(&[("home", &home.uri())]).await;
    let res = client()
        .post(format!("{base}/api/hosts"))
        .json(&json!({"id": "desk", "base_url": desk.uri()}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
    let res = client()
        .delete(format!("{base}/api/hosts/home"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn unknown_host_returns_404() {
    let home = MockServer::start().await;
    mount_tags(&home, &["llama3:latest"]).await;
    let (base, _dir) = start(&[("home", &home.uri())]).await;
    let reqs = [
        apost(format!("{base}/api/hosts/ghost/disable")),
        apost(format!("{base}/api/hosts/ghost/enable")),
        adelete(format!("{base}/api/hosts/ghost")),
    ];
    for req in reqs {
        let res = req.send().await.unwrap();
        assert_eq!(res.status(), 404);
    }
}

#[tokio::test]
async fn add_reactivates_overlay_removed_host() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    mount_tags(&home, &["llama3:latest"]).await;
    mount_tags(&desk, &["mistral"]).await;
    let dir = prepare(
        &[("home", &home.uri()), ("desk", &desk.uri())],
        r#""sk-orouta-alice""#,
    );
    write_overlay(
        &dir,
        &json!({"hosts": {"disabled": [], "removed": ["desk"], "added": []}}),
    );
    let base = serve(&dir).await;
    assert!(status_hosts(&base).await.iter().all(|h| h["id"] != "desk"));

    let res = apost(format!("{base}/api/hosts"))
        .json(&json!({"id": "desk", "base_url": desk.uri()}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let hosts = status_hosts(&base).await;
    assert!(hosts.iter().any(|h| h["id"] == "desk"));
    assert!(wait_for_tag(&base, "mistral").await);

    let overlay: Value =
        serde_json::from_str(&std::fs::read_to_string(overlay_path(&dir)).unwrap()).unwrap();
    assert_eq!(overlay["hosts"]["removed"].as_array().unwrap().len(), 0);
    assert_eq!(overlay["hosts"]["added"][0]["id"], "desk");
}

#[tokio::test]
async fn concurrent_adds_both_persist() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    let attic = MockServer::start().await;
    mount_tags(&home, &["llama3:latest"]).await;
    mount_tags(&desk, &["mistral"]).await;
    mount_tags(&attic, &["phi4"]).await;
    let (base, dir) = start(&[("home", &home.uri())]).await;

    let (r1, r2) = tokio::join!(
        apost(format!("{base}/api/hosts"))
            .json(&json!({"id": "desk", "base_url": desk.uri()}))
            .send(),
        apost(format!("{base}/api/hosts"))
            .json(&json!({"id": "attic", "base_url": attic.uri()}))
            .send()
    );
    assert_eq!(r1.unwrap().status(), 200);
    assert_eq!(r2.unwrap().status(), 200);
    let hosts = status_hosts(&base).await;
    assert!(hosts.iter().any(|h| h["id"] == "desk"));
    assert!(hosts.iter().any(|h| h["id"] == "attic"));
    let overlay: Value =
        serde_json::from_str(&std::fs::read_to_string(overlay_path(&dir)).unwrap()).unwrap();
    assert_eq!(overlay["hosts"]["added"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn corrupt_overlay_keeps_last_config_and_refuses_mutations() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    mount_tags(&home, &["llama3:latest"]).await;
    mount_tags(&desk, &["mistral"]).await;
    let (base, dir) = start(&[("home", &home.uri()), ("desk", &desk.uri())]).await;
    assert!(wait_for_tag(&base, "mistral").await);

    let res = adelete(format!("{base}/api/hosts/desk"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert!(status_hosts(&base).await.iter().all(|h| h["id"] != "desk"));

    std::fs::write(overlay_path(&dir), "not json at all").unwrap();
    tokio::time::sleep(Duration::from_millis(1600)).await;
    assert!(status_hosts(&base).await.iter().all(|h| h["id"] != "desk"));

    let res = apost(format!("{base}/api/hosts"))
        .json(&json!({"id": "attic", "base_url": "http://127.0.0.1:1"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn disabled_first_upstream_skips_fallback() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    mount_tags(&home, &["llama3:latest"]).await;
    mount_tags(&desk, &["mistral"]).await;
    mount_chat(&desk, Duration::ZERO).await;
    let (base, _dir) = start(&[("home", &home.uri()), ("desk", &desk.uri())]).await;

    let res = apost(format!("{base}/api/hosts/home/disable"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let res = client()
        .post(format!("{base}/api/chat"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({"messages": [{"role": "user", "content": "hi"}]}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let hits = desk
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/api/chat")
        .count();
    assert_eq!(hits, 1);
    let home_hits = home
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/api/chat")
        .count();
    assert_eq!(home_hits, 0);
}

#[tokio::test]
async fn disabled_only_upstream_returns_503_on_fallback() {
    let home = MockServer::start().await;
    mount_tags(&home, &["llama3:latest"]).await;
    mount_chat(&home, Duration::ZERO).await;
    let (base, _dir) = start(&[("home", &home.uri())]).await;

    let res = apost(format!("{base}/api/hosts/home/disable"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let res = client()
        .post(format!("{base}/api/chat"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({"messages": [{"role": "user", "content": "hi"}]}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 503);
}
