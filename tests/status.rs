use serde_json::{json, Value};
use std::sync::Arc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "sk-orouta-alice";

fn toml_for(home: &str, desk: &str, keys: &str) -> String {
    format!(
        r#"
host = "127.0.0.1"
port = 0

[auth]
keys = [{keys}]

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

async fn start_with(
    home: &MockServer,
    desk: &MockServer,
    keys: &str,
    home_models: &[&str],
    desk_models: &[&str],
) -> String {
    mount_tags(home, home_models).await;
    mount_tags(desk, desk_models).await;
    serve(&home.uri(), &desk.uri(), keys).await
}

async fn serve(home: &str, desk: &str, keys: &str) -> String {
    let cfg = orouta::Config::parse(&toml_for(home, desk, keys)).unwrap();
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
async fn html_lists_hosts_models_and_base_urls() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    let base = start_with(
        &home,
        &desk,
        r#""sk-orouta-alice""#,
        &["llama3:latest"],
        &["mistral"],
    )
    .await;
    let res = client()
        .get(format!("{base}/status"))
        .header("Authorization", format!("Bearer {KEY}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let ct = res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ct.contains("text/html"));
    let body = res.text().await.unwrap();
    assert!(body.contains("http-equiv=\"refresh\""));
    assert!(body.contains("home"));
    assert!(body.contains(&home.uri()));
    assert!(body.contains("llama3:latest"));
    assert!(body.contains("desk"));
    assert!(body.contains(&desk.uri()));
    assert!(body.contains("mistral"));
}

#[tokio::test]
async fn json_reports_per_host_state() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    let base = start_with(
        &home,
        &desk,
        r#""sk-orouta-alice""#,
        &["llama3:latest"],
        &["mistral"],
    )
    .await;
    let v: Value = client()
        .get(format!("{base}/status.json"))
        .header("Authorization", format!("Bearer {KEY}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let hosts = v["hosts"].as_array().unwrap();
    assert_eq!(hosts.len(), 2);
    assert_eq!(hosts[0]["id"], "home");
    assert_eq!(hosts[0]["base_url"], home.uri().as_str());
    assert_eq!(hosts[0]["reachable"], true);
    assert!(hosts[0]["latency_ms"].is_u64());
    assert_eq!(hosts[0]["models"][0], "llama3:latest");
    assert_eq!(hosts[0]["requests_total"], 0);
    assert_eq!(hosts[0]["errors_total"], 0);
    assert_eq!(hosts[0]["in_flight"], 0);
    assert!(hosts[0]["last_error"].is_null());
    assert_eq!(hosts[1]["id"], "desk");
    assert_eq!(hosts[1]["reachable"], true);
    assert_eq!(hosts[1]["models"][0], "mistral");
}

#[tokio::test]
async fn probe_failure_marks_host_down() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&home)
        .await;
    mount_tags(&desk, &["mistral"]).await;
    let base = serve(&home.uri(), &desk.uri(), r#""sk-orouta-alice""#).await;
    let v: Value = client()
        .get(format!("{base}/status.json"))
        .header("Authorization", format!("Bearer {KEY}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let hosts = v["hosts"].as_array().unwrap();
    let home_stats = hosts.iter().find(|h| h["id"] == "home").unwrap();
    let desk_stats = hosts.iter().find(|h| h["id"] == "desk").unwrap();
    assert_eq!(home_stats["reachable"], false);
    assert_eq!(home_stats["last_error"], "tags http 500");
    assert_eq!(home_stats["requests_total"], 0);
    assert_eq!(desk_stats["reachable"], true);
    assert!(desk_stats["last_error"].is_null());
}

#[tokio::test]
async fn status_requires_key() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    let base = start_with(&home, &desk, r#""sk-orouta-alice""#, &[], &[]).await;
    for route in ["/status", "/status.json"] {
        let res = client().get(format!("{base}{route}")).send().await.unwrap();
        assert_eq!(res.status(), 401);
    }
}

#[tokio::test]
async fn empty_keys_open() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    let base = start_with(&home, &desk, "", &[], &[]).await;
    for route in ["/status", "/status.json"] {
        let res = client().get(format!("{base}{route}")).send().await.unwrap();
        assert_eq!(res.status(), 200);
    }
}

#[tokio::test]
async fn forwarded_request_updates_counters() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"done": true})))
        .mount(&home)
        .await;
    let base = start_with(
        &home,
        &desk,
        r#""sk-orouta-alice""#,
        &["llama3:latest"],
        &[],
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
    let v: Value = client()
        .get(format!("{base}/status.json"))
        .header("Authorization", format!("Bearer {KEY}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let hosts = v["hosts"].as_array().unwrap();
    let home_stats = hosts.iter().find(|h| h["id"] == "home").unwrap();
    let desk_stats = hosts.iter().find(|h| h["id"] == "desk").unwrap();
    assert_eq!(home_stats["requests_total"], 1);
    assert_eq!(home_stats["errors_total"], 0);
    assert_eq!(home_stats["reachable"], true);
    assert_eq!(desk_stats["requests_total"], 0);
}

#[tokio::test]
async fn upstream_error_counts_and_records_last_error() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&home)
        .await;
    let base = start_with(
        &home,
        &desk,
        r#""sk-orouta-alice""#,
        &["llama3:latest"],
        &[],
    )
    .await;
    let res = client()
        .post(format!("{base}/api/chat"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({"model":"llama3","messages":[{"role":"user","content":"hi"}]}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
    let v: Value = client()
        .get(format!("{base}/status.json"))
        .header("Authorization", format!("Bearer {KEY}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let home_stats = v["hosts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["id"] == "home")
        .unwrap();
    assert_eq!(home_stats["requests_total"], 1);
    assert_eq!(home_stats["errors_total"], 1);
    assert_eq!(home_stats["in_flight"], 0);
    assert_eq!(home_stats["last_error"], "http 500");
}

#[tokio::test]
async fn messages_request_counts_toward_host_stats() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "message": {"content": "hi"},
            "done": true,
            "done_reason": "stop",
            "prompt_eval_count": 1,
            "eval_count": 2
        })))
        .mount(&home)
        .await;
    let base = start_with(
        &home,
        &desk,
        r#""sk-orouta-alice""#,
        &["llama3:latest"],
        &[],
    )
    .await;
    let res = client()
        .post(format!("{base}/v1/messages"))
        .header("x-api-key", KEY)
        .json(
            &json!({"model":"llama3","max_tokens":10,"messages":[{"role":"user","content":"hi"}]}),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let v: Value = client()
        .get(format!("{base}/status.json"))
        .header("Authorization", format!("Bearer {KEY}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let home_stats = v["hosts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["id"] == "home")
        .unwrap();
    assert_eq!(home_stats["requests_total"], 1);
    assert_eq!(home_stats["errors_total"], 0);
    assert_eq!(home_stats["reachable"], true);
}
