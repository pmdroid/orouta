use serde_json::{json, Value};
use std::sync::Arc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "sk-orouta-alice";

fn toml_config(upstreams: &[(&str, &str)], keys: &str) -> String {
    let mut out = format!(
        r#"
host = "127.0.0.1"
port = 0

[auth]
keys = [{keys}]
"#
    );
    for (id, base) in upstreams {
        out.push_str(&format!(
            "\n[[upstream]]\nid = \"{id}\"\nbase_url = \"{base}\"\n"
        ));
    }
    out
}

async fn start(toml: &str) -> String {
    let cfg = orouta::Config::parse(toml).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = orouta::app(Arc::new(cfg), reqwest::Client::new());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

async fn chat(base: &str, model: &str) -> reqwest::Response {
    client()
        .post(format!("{base}/api/chat"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({"model": model, "messages": [{"role": "user", "content": "hi"}]}))
        .send()
        .await
        .unwrap()
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

async fn mount_tags_error(server: &MockServer, status: u16) {
    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(status))
        .mount(server)
        .await;
}

#[tokio::test]
async fn model_on_down_host_is_503() {
    let home = MockServer::start().await;
    mount_tags_error(&home, 500).await;
    let base = start(&toml_config(
        &[("home", home.uri().as_str())],
        r#""sk-orouta-alice""#,
    ))
    .await;
    let res = chat(&base, "llama3").await;
    assert_eq!(res.status(), 503);
    let v: Value = res.json().await.unwrap();
    assert_eq!(v["error"], "host unavailable");
    assert_eq!(v["host"], "home");
}

#[tokio::test]
async fn recovery_is_picked_up_within_one_request() {
    let home = MockServer::start().await;
    mount_tags_error(&home, 500).await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"done": true})))
        .mount(&home)
        .await;
    let base = start(&toml_config(
        &[("home", home.uri().as_str())],
        r#""sk-orouta-alice""#,
    ))
    .await;
    let res = chat(&base, "llama3").await;
    assert_eq!(res.status(), 503);
    home.reset().await;
    mount_tags(&home, &["llama3"]).await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"done": true})))
        .mount(&home)
        .await;
    let res = chat(&base, "llama3").await;
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn forward_connection_error_names_host() {
    let home = MockServer::start().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dead_port = listener.local_addr().unwrap().port();
    drop(listener);
    let base = start(&toml_config(
        &[
            ("dead", &format!("http://127.0.0.1:{dead_port}")),
            ("home", home.uri().as_str()),
        ],
        r#""sk-orouta-alice""#,
    ))
    .await;
    let res = client()
        .get(format!("{base}/api/version"))
        .header("Authorization", format!("Bearer {KEY}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 502);
    let v: Value = res.json().await.unwrap();
    assert_eq!(v["error"], "upstream unavailable");
    assert_eq!(v["host"], "dead");
}

#[tokio::test]
async fn healthy_host_still_routes_while_other_is_down() {
    let home = MockServer::start().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dead_port = listener.local_addr().unwrap().port();
    drop(listener);
    mount_tags(&home, &["mistral"]).await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"done": true})))
        .mount(&home)
        .await;
    let base = start(&toml_config(
        &[
            ("dead", &format!("http://127.0.0.1:{dead_port}")),
            ("home", home.uri().as_str()),
        ],
        r#""sk-orouta-alice""#,
    ))
    .await;
    let res = chat(&base, "mistral").await;
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn unknown_model_with_healthy_hosts_is_404() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    mount_tags(&home, &["llama3"]).await;
    mount_tags(&desk, &["mistral"]).await;
    let base = start(&toml_config(
        &[("home", home.uri().as_str()), ("desk", desk.uri().as_str())],
        r#""sk-orouta-alice""#,
    ))
    .await;
    let res = chat(&base, "does-not-exist").await;
    assert_eq!(res.status(), 404);
    let v: Value = res.json().await.unwrap();
    assert_eq!(v["error"], "unknown model");
}
