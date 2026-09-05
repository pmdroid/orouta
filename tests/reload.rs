use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const KEY: &str = "sk-orouta-alice";

struct DynamicTags(Arc<std::sync::RwLock<Value>>);

impl wiremock::Respond for DynamicTags {
    fn respond(&self, _req: &Request) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(self.0.read().unwrap().clone())
    }
}

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
    let dir = std::env::temp_dir().join(format!("orouta-reload-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    TempDir(dir)
}

async fn start(cfg_path: &PathBuf, cfg: &str) -> String {
    std::fs::write(cfg_path, cfg).unwrap();
    let config = orouta::Config::load(cfg_path).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = orouta::app(
        Arc::new(config),
        reqwest::Client::new(),
        Some(cfg_path.to_path_buf()),
    );
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

async fn write_config(cfg_path: &PathBuf, cfg: &str) {
    std::fs::write(cfg_path, cfg).unwrap();
}

async fn chat_hits(base: &str, model: &str) -> reqwest::Response {
    client()
        .post(format!("{base}/api/chat"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({"model": model, "messages": [{"role": "user", "content": "hi"}]}))
        .send()
        .await
        .unwrap()
}

async fn chat_count(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/api/chat")
        .count()
}

async fn wait_for_chat(base: &str, server: &MockServer, expected: usize) -> bool {
    for _ in 0..40 {
        chat_hits(base, "llama3").await;
        if chat_count(server).await >= expected {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    chat_count(server).await >= expected
}

#[tokio::test]
async fn upstream_base_url_change_reroutes() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    mount_tags(&home, &["llama3"]).await;
    mount_tags(&desk, &["llama3"]).await;
    mount_chat(&home).await;
    mount_chat(&desk).await;
    let dir = temp_dir();
    let cfg_path = dir.join("orouta.toml");
    let base = start(
        &cfg_path,
        &toml_for(&[("home", &home.uri())], &format!("\"{KEY}\"")),
    )
    .await;

    let res = chat_hits(&base, "llama3").await;
    assert_eq!(res.status(), 200);
    assert_eq!(chat_count(&home).await, 1);
    assert_eq!(chat_count(&desk).await, 0);

    write_config(
        &cfg_path,
        &toml_for(&[("home", &desk.uri())], &format!("\"{KEY}\"")),
    )
    .await;

    assert!(wait_for_chat(&base, &desk, 1).await);
}

#[tokio::test]
async fn upstream_set_change_resets_catalog() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    let home_tags = Arc::new(std::sync::RwLock::new(names_tag(&["llama3"])));
    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(DynamicTags(home_tags.clone()))
        .mount(&home)
        .await;
    mount_chat(&home).await;
    mount_chat(&desk).await;
    let dir = temp_dir();
    let cfg_path = dir.join("orouta.toml");
    let base = start(
        &cfg_path,
        &toml_for(&[("home", &home.uri())], &format!("\"{KEY}\"")),
    )
    .await;

    let res = chat_hits(&base, "llama3").await;
    assert_eq!(res.status(), 200);
    assert_eq!(chat_count(&home).await, 1);

    *home_tags.write().unwrap() = names_tag(&[]);
    mount_tags(&desk, &["llama3"]).await;
    write_config(
        &cfg_path,
        &toml_for(
            &[("home", &home.uri()), ("desk", &desk.uri())],
            &format!("\"{KEY}\""),
        ),
    )
    .await;

    assert!(wait_for_chat(&base, &desk, 1).await);
}

#[tokio::test]
async fn invalid_config_keeps_old() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    mount_tags(&home, &["llama3"]).await;
    mount_chat(&home).await;
    mount_chat(&desk).await;
    let dir = temp_dir();
    let cfg_path = dir.join("orouta.toml");
    let base = start(
        &cfg_path,
        &toml_for(&[("home", &home.uri())], &format!("\"{KEY}\"")),
    )
    .await;

    let res = chat_hits(&base, "llama3").await;
    assert_eq!(res.status(), 200);

    write_config(&cfg_path, "not [ valid toml").await;
    tokio::time::sleep(Duration::from_millis(1600)).await;

    let res = chat_hits(&base, "llama3").await;
    assert_eq!(res.status(), 200);
    assert_eq!(chat_count(&home).await, 2);
    assert_eq!(chat_count(&desk).await, 0);
}

#[tokio::test]
async fn key_change_takes_effect() {
    let home = MockServer::start().await;
    mount_tags(&home, &["llama3"]).await;
    let dir = temp_dir();
    let cfg_path = dir.join("orouta.toml");
    let base = start(
        &cfg_path,
        &toml_for(&[("home", &home.uri())], &format!("\"{KEY}\"")),
    )
    .await;

    let res = client()
        .get(format!("{base}/api/tags"))
        .header("Authorization", format!("Bearer {KEY}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    write_config(
        &cfg_path,
        &toml_for(&[("home", &home.uri())], "\"sk-orouta-new\""),
    )
    .await;
    let mut reloaded = false;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let res = client()
            .get(format!("{base}/api/tags"))
            .header("Authorization", "Bearer sk-orouta-new")
            .send()
            .await
            .unwrap();
        if res.status() == 200 {
            reloaded = true;
            break;
        }
    }
    assert!(reloaded, "reload poll never picked up the new key");

    let res = client()
        .get(format!("{base}/api/tags"))
        .header("Authorization", format!("Bearer {KEY}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

async fn mount_tags(server: &MockServer, names: &[&str]) {
    let models: Vec<Value> = names
        .iter()
        .map(|n| json!({"name": n, "model": n}))
        .collect();
    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"models": models})))
        .mount(server)
        .await;
}

async fn mount_chat(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"done": true})))
        .mount(server)
        .await;
}
