use serde_json::{json, Value};
use std::sync::Arc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "sk-orouta-alice";

fn toml_for(home: &str, desk: &str, keys: &str, home_api_key: &str) -> String {
    format!(
        r#"
host = "127.0.0.1"
port = 0

[auth]
keys = [{keys}]

[[upstream]]
id = "home"
base_url = "{home}"
api_key = "{home_api_key}"

[[upstream]]
id = "desk"
base_url = "{desk}"
"#
    )
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

async fn start_with(
    home: &MockServer,
    desk: &MockServer,
    keys: &str,
    home_api_key: &str,
    home_models: &[&str],
    desk_models: &[&str],
) -> String {
    mount_tags(home, home_models).await;
    mount_tags(desk, desk_models).await;
    start(&home.uri(), &desk.uri(), keys, home_api_key).await
}

async fn start(home: &str, desk: &str, keys: &str, home_api_key: &str) -> String {
    let cfg = orouta::Config::parse(&toml_for(home, desk, keys, home_api_key)).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = orouta::app(Arc::new(cfg), reqwest::Client::new(), None);
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

#[tokio::test]
async fn bearer_accepted() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    let base = start_with(&home, &desk, r#""sk-orouta-alice""#, "", &[], &[]).await;
    let res = client()
        .get(format!("{base}/api/tags"))
        .header("Authorization", format!("Bearer {KEY}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn x_api_key_accepted() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    let base = start_with(&home, &desk, r#""sk-orouta-alice""#, "", &[], &[]).await;
    let res = client()
        .get(format!("{base}/api/tags"))
        .header("x-api-key", KEY)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn missing_key_unauthorized() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    let base = start_with(&home, &desk, r#""sk-orouta-alice""#, "", &[], &[]).await;
    let res = client()
        .get(format!("{base}/api/tags"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn wrong_key_unauthorized() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    let base = start_with(&home, &desk, r#""sk-orouta-alice""#, "", &[], &[]).await;
    let res = client()
        .get(format!("{base}/api/tags"))
        .header("Authorization", "Bearer sk-wrong")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn empty_keys_open() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    let base = start_with(&home, &desk, "", "", &[], &[]).await;
    let res = client()
        .get(format!("{base}/api/tags"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn chat_llama3_hits_home_only() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"done": true})))
        .mount(&home)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"done": true})))
        .mount(&desk)
        .await;
    let base = start_with(
        &home,
        &desk,
        r#""sk-orouta-alice""#,
        "",
        &["llama3:latest"],
        &["mistral"],
    )
    .await;
    let res = client()
        .post(format!("{base}/api/chat"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({"model":"llama3","messages":[{"role":"user","content":"hi"}]}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let home_chat = home
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/api/chat")
        .count();
    let desk_chat = desk
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/api/chat")
        .count();
    assert_eq!(home_chat, 1);
    assert_eq!(desk_chat, 0);
}

#[tokio::test]
async fn chat_completions_forwards_upstream_api_key() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&home)
        .await;
    let base = start_with(
        &home,
        &desk,
        r#""sk-orouta-alice""#,
        "sk-home",
        &["llama3"],
        &[],
    )
    .await;
    let body = json!({"model":"llama3","messages":[{"role":"user","content":"hi"}]});
    let res = client()
        .post(format!("{base}/v1/chat/completions"))
        .header("Authorization", format!("Bearer {KEY}"))
        .header("x-api-key", KEY)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let got = home
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.url.path() == "/v1/chat/completions")
        .unwrap();
    let auth = got
        .headers
        .get("authorization")
        .and_then(|v| v.to_str().ok());
    assert_eq!(auth, Some("Bearer sk-home"));
    assert!(got.headers.get("x-api-key").is_none());
    let forwarded: Value = serde_json::from_slice(&got.body).unwrap();
    assert_eq!(forwarded["model"], "llama3");
    let desk_chat = desk
        .received_requests()
        .await
        .unwrap()
        .iter()
        .any(|r| r.url.path() == "/v1/chat/completions");
    assert!(!desk_chat);
}

#[tokio::test]
async fn stream_body_matches_upstream() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    let upstream_bytes = b"{\"message\":{\"content\":\"Hel\"},\"done\":false}\n{\"message\":{\"content\":\"lo\"},\"done\":true}\n";
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(upstream_bytes.as_slice())
                .insert_header("content-type", "application/x-ndjson"),
        )
        .mount(&home)
        .await;
    let base = start_with(&home, &desk, r#""sk-orouta-alice""#, "", &["llama3"], &[]).await;
    let res = client()
        .post(format!("{base}/api/chat"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({"model":"llama3","stream":true,"messages":[{"role":"user","content":"hi"}]}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let ct = res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ct.contains("application/x-ndjson"));
    let bytes = res.bytes().await.unwrap();
    assert_eq!(&bytes[..], upstream_bytes.as_slice());
}

#[tokio::test]
async fn unknown_inference_model_is_404() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&home)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&desk)
        .await;
    let base = start_with(&home, &desk, r#""sk-orouta-alice""#, "", &["llama3"], &[]).await;
    let res = client()
        .post(format!("{base}/api/chat"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({"model":"does-not-exist","messages":[]}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
    let v: Value = res.json().await.unwrap();
    assert_eq!(v["error"], "unknown model");
    let home_chat = home
        .received_requests()
        .await
        .unwrap()
        .iter()
        .any(|r| r.url.path() == "/api/chat");
    let desk_chat = desk
        .received_requests()
        .await
        .unwrap()
        .iter()
        .any(|r| r.url.path() == "/api/chat");
    assert!(!home_chat);
    assert!(!desk_chat);
}

#[tokio::test]
async fn pull_unknown_name_forwards_to_selected_host() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status":"ok"})))
        .mount(&home)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status":"ok"})))
        .mount(&desk)
        .await;
    let base = start_with(&home, &desk, r#""sk-orouta-alice""#, "", &["llama3"], &[]).await;
    let res = client()
        .post(format!("{base}/api/pull?host=home"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({"name":"does-not-exist"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let home_pull = home
        .received_requests()
        .await
        .unwrap()
        .iter()
        .any(|r| r.url.path() == "/api/pull");
    let desk_pull = desk
        .received_requests()
        .await
        .unwrap()
        .iter()
        .any(|r| r.url.path() == "/api/pull");
    assert!(home_pull);
    assert!(!desk_pull);
}

#[tokio::test]
async fn tags_and_models_from_hosts() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    let base = start_with(
        &home,
        &desk,
        r#""sk-orouta-alice""#,
        "",
        &["llama3:latest"],
        &["claude-sonnet"],
    )
    .await;
    let tags: Value = client()
        .get(format!("{base}/api/tags"))
        .header("Authorization", format!("Bearer {KEY}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let names: Vec<&str> = tags["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"llama3:latest"));
    assert!(names.contains(&"claude-sonnet"));
    let models: Value = client()
        .get(format!("{base}/v1/models"))
        .header("Authorization", format!("Bearer {KEY}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let ids: Vec<&str> = models["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"llama3:latest"));
    assert!(ids.contains(&"claude-sonnet"));
}
