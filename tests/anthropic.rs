use serde_json::{json, Value};
use std::sync::Arc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "sk-orouta-alice";

fn toml_for(home: &str, desk: &str) -> String {
    format!(
        r#"
host = "127.0.0.1"
port = 0

[auth]
keys = ["sk-orouta-alice"]

[[upstream]]
id = "home"
base_url = "{home}"

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

async fn start(home: &MockServer, desk: &MockServer) -> String {
    mount_tags(home, &["llama3"]).await;
    mount_tags(desk, &["claude-sonnet"]).await;
    let cfg = orouta::Config::parse(&toml_for(&home.uri(), &desk.uri())).unwrap();
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
async fn maps_to_ollama_chat() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "message": {"role": "assistant", "content": "ok"},
            "done": true,
            "done_reason": "stop",
            "prompt_eval_count": 1,
            "eval_count": 1
        })))
        .mount(&desk)
        .await;
    let base = start(&home, &desk).await;
    let res = client()
        .post(format!("{base}/v1/messages"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({
            "model": "claude-sonnet",
            "max_tokens": 64,
            "system": "be brief",
            "messages": [{"role": "user", "content": "hi"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let home_chat = home
        .received_requests()
        .await
        .unwrap()
        .iter()
        .any(|r| r.url.path() == "/api/chat");
    assert!(!home_chat);
    let got = desk
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.url.path() == "/api/chat")
        .unwrap();
    let body: Value = serde_json::from_slice(&got.body).unwrap();
    assert_eq!(body["model"], "claude-sonnet");
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][0]["content"], "be brief");
    assert_eq!(body["messages"][1]["role"], "user");
    assert_eq!(body["messages"][1]["content"], "hi");
    assert_eq!(body["options"]["num_predict"], 64);
}

#[tokio::test]
async fn non_stream_translates_json() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "message": {"role": "assistant", "content": "hello from ollama"},
            "done": true,
            "done_reason": "stop",
            "prompt_eval_count": 11,
            "eval_count": 4
        })))
        .mount(&desk)
        .await;
    let base = start(&home, &desk).await;
    let res = client()
        .post(format!("{base}/v1/messages"))
        .header("x-api-key", KEY)
        .json(&json!({
            "model": "claude-sonnet",
            "max_tokens": 32,
            "stream": false,
            "messages": [{"role": "user", "content": "hi"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let v: Value = res.json().await.unwrap();
    assert_eq!(v["content"][0]["text"], "hello from ollama");
    assert_eq!(v["model"], "claude-sonnet");
    assert_eq!(v["role"], "assistant");
    assert_eq!(v["stop_reason"], "end_turn");
    assert_eq!(v["usage"]["input_tokens"], 11);
    assert_eq!(v["usage"]["output_tokens"], 4);
    assert!(v["id"].as_str().unwrap().starts_with("msg_orouta_"));
}

#[tokio::test]
async fn stream_ndjson_to_sse() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    let ndjson = concat!(
        r#"{"message":{"role":"assistant","content":"Hel"},"done":false}"#,
        "\n",
        r#"{"message":{"role":"assistant","content":"lo"},"done":false}"#,
        "\n",
        r#"{"message":{"role":"assistant","content":""},"done":true,"done_reason":"stop","prompt_eval_count":3,"eval_count":2}"#,
        "\n"
    );
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(ndjson)
                .insert_header("content-type", "application/x-ndjson"),
        )
        .mount(&desk)
        .await;
    let base = start(&home, &desk).await;
    let res = client()
        .post(format!("{base}/v1/messages"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({
            "model": "claude-sonnet",
            "max_tokens": 16,
            "stream": true,
            "messages": [{"role": "user", "content": "hi"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(ct.contains("text/event-stream"));
    let text = res.text().await.unwrap();
    assert!(text.contains("event: content_block_delta"));
    assert!(text.contains("text_delta"));
    assert!(text.contains("Hel"));
    assert!(text.contains("lo"));
    assert!(text.contains("event: message_stop"));
}

#[tokio::test]
async fn image_body_is_400() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&desk)
        .await;
    let base = start(&home, &desk).await;
    let res = client()
        .post(format!("{base}/v1/messages"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({
            "model": "claude-sonnet",
            "max_tokens": 10,
            "messages": [{
                "role": "user",
                "content": [{"type": "image", "source": {"type": "base64", "data": "xx"}}]
            }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    let desk_chat = desk
        .received_requests()
        .await
        .unwrap()
        .iter()
        .any(|r| r.url.path() == "/api/chat");
    assert!(!desk_chat);
}

#[tokio::test]
async fn tools_body_is_400() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&desk)
        .await;
    let base = start(&home, &desk).await;
    let res = client()
        .post(format!("{base}/v1/messages"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({
            "model": "claude-sonnet",
            "max_tokens": 10,
            "tools": [{"name": "x", "input_schema": {}}],
            "messages": [{"role": "user", "content": "hi"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    let desk_chat = desk
        .received_requests()
        .await
        .unwrap()
        .iter()
        .any(|r| r.url.path() == "/api/chat");
    assert!(!desk_chat);
}

#[tokio::test]
async fn unknown_model_is_404() {
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
    let base = start(&home, &desk).await;
    let res = client()
        .post(format!("{base}/v1/messages"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({
            "model": "does-not-exist",
            "max_tokens": 10,
            "messages": [{"role": "user", "content": "hi"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
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
