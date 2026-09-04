use serde_json::{json, Value};
use std::sync::Arc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "sk-orouta-alice";
const NDJSON: &str =
    "{\"status\":\"pulling manifest\"}\n{\"status\":\"downloading\",\"completed\":10}\n{\"status\":\"success\"}\n";

fn toml_for(upstreams: &[(&str, &str)], pull_host: Option<&str>) -> String {
    let mut s = r#"
host = "127.0.0.1"
port = 0
"#
    .to_string();
    if let Some(id) = pull_host {
        s.push_str(&format!("\npull_host = \"{id}\"\n"));
    }
    s.push_str(&format!("\n[auth]\nkeys = [\"{KEY}\"]\n"));
    for (id, url) in upstreams {
        s.push_str(&format!(
            "\n[[upstream]]\nid = \"{id}\"\nbase_url = \"{url}\"\n"
        ));
    }
    s
}

async fn mount_pull(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/api/pull"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(NDJSON.as_bytes())
                .insert_header("content-type", "application/x-ndjson"),
        )
        .mount(server)
        .await;
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

async fn pull_count(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/api/pull")
        .count()
}

#[tokio::test]
async fn pull_with_host_param_forwards_to_that_host() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    mount_pull(&home).await;
    mount_pull(&desk).await;
    let toml = toml_for(
        &[("home", home.uri().as_str()), ("desk", desk.uri().as_str())],
        None,
    );
    let base = start(&toml).await;
    let res = client()
        .post(format!("{base}/api/pull?host=desk"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({"name":"mistral"}))
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
    assert_eq!(&bytes[..], NDJSON.as_bytes());
    assert_eq!(pull_count(&desk).await, 1);
    assert_eq!(pull_count(&home).await, 0);
    let got = desk
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.url.path() == "/api/pull")
        .unwrap();
    assert_eq!(got.url.path(), "/api/pull");
    assert!(got.url.query().is_none());
    let body: Value = serde_json::from_slice(&got.body).unwrap();
    assert_eq!(body["name"], "mistral");
}

#[tokio::test]
async fn pull_host_config_selects_host() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    mount_pull(&home).await;
    mount_pull(&desk).await;
    let toml = toml_for(
        &[("home", home.uri().as_str()), ("desk", desk.uri().as_str())],
        Some("home"),
    );
    let base = start(&toml).await;
    let res = client()
        .post(format!("{base}/api/pull"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({"name":"llama3"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(pull_count(&home).await, 1);
    assert_eq!(pull_count(&desk).await, 0);
}

#[tokio::test]
async fn host_param_overrides_pull_host() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    mount_pull(&home).await;
    mount_pull(&desk).await;
    let toml = toml_for(
        &[("home", home.uri().as_str()), ("desk", desk.uri().as_str())],
        Some("home"),
    );
    let base = start(&toml).await;
    let res = client()
        .post(format!("{base}/api/pull?host=desk"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({"name":"llama3"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(pull_count(&home).await, 0);
    assert_eq!(pull_count(&desk).await, 1);
}

#[tokio::test]
async fn pull_without_selection_and_multiple_hosts_is_400() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    mount_pull(&home).await;
    mount_pull(&desk).await;
    let toml = toml_for(
        &[("home", home.uri().as_str()), ("desk", desk.uri().as_str())],
        None,
    );
    let base = start(&toml).await;
    let res = client()
        .post(format!("{base}/api/pull"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({"name":"llama3"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    let v: Value = res.json().await.unwrap();
    let err = v["error"].as_str().unwrap();
    assert!(err.contains("home"));
    assert!(err.contains("desk"));
    assert_eq!(pull_count(&home).await, 0);
    assert_eq!(pull_count(&desk).await, 0);
}

#[tokio::test]
async fn pull_unknown_host_param_is_400() {
    let home = MockServer::start().await;
    let desk = MockServer::start().await;
    mount_pull(&home).await;
    mount_pull(&desk).await;
    let toml = toml_for(
        &[("home", home.uri().as_str()), ("desk", desk.uri().as_str())],
        None,
    );
    let base = start(&toml).await;
    let res = client()
        .post(format!("{base}/api/pull?host=nope"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({"name":"llama3"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    let v: Value = res.json().await.unwrap();
    assert!(v["error"].as_str().unwrap().contains("unknown pull host"));
    assert_eq!(pull_count(&home).await, 0);
    assert_eq!(pull_count(&desk).await, 0);
}

#[tokio::test]
async fn pull_single_host_needs_no_selection() {
    let home = MockServer::start().await;
    mount_pull(&home).await;
    let toml = toml_for(&[("home", home.uri().as_str())], None);
    let base = start(&toml).await;
    let res = client()
        .post(format!("{base}/api/pull"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({"name":"llama3"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(pull_count(&home).await, 1);
}
