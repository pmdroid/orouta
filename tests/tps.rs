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
keys = ["{KEY}"]

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

async fn serve(home: &MockServer, desk: &MockServer) -> String {
    mount_tags(home, &["llama3:latest"]).await;
    mount_tags(desk, &["mistral"]).await;
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

async fn get_status_json(base: &str) -> Value {
    client()
        .get(format!("{base}/status.json"))
        .header("Authorization", format!("Bearer {KEY}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

fn host<'a>(v: &'a Value, id: &str) -> &'a Value {
    v["hosts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["id"] == *id)
        .unwrap()
}

#[tokio::test]
async fn streamed_chat_records_tps_sample() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    let base = serve(&home, &desk).await;
    let body = concat!(
        r#"{"model":"llama3","message":{"role":"assistant","content":"hi"},"done":false}"#,
        "\n",
        r#"{"model":"llama3","done":true,"done_reason":"stop","prompt_eval_count":12,"prompt_eval_duration":300000000,"eval_count":136,"eval_duration":3459000000}"#,
        "\n",
    );
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/x-ndjson"))
        .mount(&home)
        .await;

    let res = client()
        .post(format!("{base}/api/chat"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({"model": "llama3", "messages": []}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    res.text().await.unwrap();

    let v = get_status_json(&base).await;
    let tps = host(&v, "home")["tps"].as_array().unwrap().clone();
    assert_eq!(tps.len(), 1);
    assert_eq!(tps[0]["model"], "llama3");
    assert_eq!(tps[0]["samples"], 1);
    let avg = tps[0]["avg"].as_f64().unwrap();
    assert!((avg - 39.3).abs() < 0.1, "avg {avg}");
    let last = tps[0]["last"].as_f64().unwrap();
    assert!((last - 39.3).abs() < 0.1, "last {last}");
    let prompt = tps[0]["prompt"].as_f64().unwrap();
    assert!((prompt - 40.0).abs() < 0.1, "prompt {prompt}");
    assert!(host(&v, "desk")["tps"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn non_streaming_chat_records_tps_sample() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    let base = serve(&home, &desk).await;
    let body = r#"{"model":"llama3","done":true,"eval_count":88,"eval_duration":2281000000}"#;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
        .mount(&home)
        .await;

    let res = client()
        .post(format!("{base}/api/chat"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({"model": "llama3", "messages": []}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    res.text().await.unwrap();

    let v = get_status_json(&base).await;
    let tps = host(&v, "home")["tps"].as_array().unwrap().clone();
    assert_eq!(tps.len(), 1);
    let avg = tps[0]["avg"].as_f64().unwrap();
    assert!((avg - 38.6).abs() < 0.1, "avg {avg}");
    assert_eq!(tps[0]["prompt"], Value::Null);
}

#[tokio::test]
async fn response_without_eval_fields_passes_through_unchanged() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    let base = serve(&home, &desk).await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw("plain passthrough body", "text/plain")
                .append_header("x-upstream", "yes"),
        )
        .mount(&home)
        .await;

    let res = client()
        .post(format!("{base}/api/chat"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({"model": "llama3", "messages": []}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body = res.text().await.unwrap();
    assert_eq!(body, "plain passthrough body");

    let v = get_status_json(&base).await;
    assert!(host(&v, "home")["tps"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn vram_snapshot_in_status_json() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    let base = serve(&home, &desk).await;
    Mock::given(method("GET"))
        .and(path("/api/ps"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models": [
                {"name": "gemma4:e2b", "size": 7801585920u64, "size_vram": 7801585920u64}
            ]
        })))
        .mount(&home)
        .await;

    let v = get_status_json(&base).await;
    let home_vram = &host(&v, "home")["vram"];
    assert_eq!(home_vram["loaded_bytes"], 7801585920u64);
    assert_eq!(home_vram["models"][0]["name"], "gemma4:e2b");
    assert_eq!(home_vram["models"][0]["size_vram"], 7801585920u64);
    assert_eq!(host(&v, "desk")["vram"], Value::Null);
}

#[tokio::test]
async fn vram_cleared_when_tags_fetch_fails() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    let base = serve(&home, &desk).await;
    Mock::given(method("GET"))
        .and(path("/api/ps"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models": [
                {"name": "gemma4:e2b", "size": 7801585920u64, "size_vram": 7801585920u64}
            ]
        })))
        .mount(&home)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(500))
        .with_priority(2)
        .mount(&home)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models": [{"name": "llama3:latest", "model": "llama3:latest"}]
        })))
        .with_priority(1)
        .up_to_n_times(1)
        .mount(&home)
        .await;

    let v = get_status_json(&base).await;
    assert_eq!(host(&v, "home")["vram"]["loaded_bytes"], 7801585920u64);

    client()
        .post(format!("{base}/api/chat"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({"model": "no-such-model", "messages": []}))
        .send()
        .await
        .unwrap();

    let v = get_status_json(&base).await;
    assert_eq!(host(&v, "home")["vram"], Value::Null);
}
