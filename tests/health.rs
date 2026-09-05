use serde_json::{json, Value};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "sk-orouta-alice";
const KEYS: &str = r#""sk-orouta-alice""#;
const RAW_UP: u8 = 0;
const RAW_REJECT: u8 = 1;

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

async fn anthropic_message(base: &str, model: &str) -> reqwest::Response {
    client()
        .post(format!("{base}/v1/messages"))
        .header("Authorization", format!("Bearer {KEY}"))
        .json(&json!({
            "model": model,
            "max_tokens": 16,
            "messages": [{"role": "user", "content": "hi"}]
        }))
        .send()
        .await
        .unwrap()
}

async fn dead_base_url() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

struct RawHost {
    url: String,
    mode: Arc<AtomicU8>,
}

impl RawHost {
    fn set(&self, mode: u8) {
        self.mode.store(mode, Ordering::SeqCst);
    }
}

async fn raw_host() -> RawHost {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let mode = Arc::new(AtomicU8::new(RAW_UP));
    tokio::spawn(serve_raw(listener, mode.clone()));
    RawHost {
        url: format!("http://{addr}"),
        mode,
    }
}

async fn serve_raw(listener: tokio::net::TcpListener, mode: Arc<AtomicU8>) {
    loop {
        let Ok((sock, _)) = listener.accept().await else {
            return;
        };
        if mode.load(Ordering::SeqCst) == RAW_REJECT {
            drop(sock);
            continue;
        }
        tokio::spawn(handle_raw(sock));
    }
}

async fn handle_raw(mut sock: tokio::net::TcpStream) {
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        match sock.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
        if buf.len() > 1024 * 1024 {
            return;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let clen = head
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .and_then(|v| v.trim().parse::<usize>().ok())
        })
        .unwrap_or(0);
    while buf.len() < head_end + clen {
        match sock.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
    let body = if head.starts_with("GET /api/tags") {
        json!({"models": [{"name": "llama3", "model": "llama3"}]}).to_string()
    } else {
        json!({"message": {"role": "assistant", "content": "ok"}, "done": true}).to_string()
    };
    let resp = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = sock.write_all(resp.as_bytes()).await;
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

#[tokio::test]
async fn known_model_on_down_host_is_503_then_recovers() {
    let host = raw_host().await;
    let base = start(&toml_config(&[("home", &host.url)], KEYS)).await;
    let res = chat(&base, "llama3").await;
    assert_eq!(res.status(), 200);
    host.set(RAW_REJECT);
    let res = chat(&base, "llama3").await;
    assert_eq!(res.status(), 502);
    let v: Value = res.json().await.unwrap();
    assert_eq!(v["error"], "upstream unavailable");
    assert_eq!(v["host"], "home");
    let res = chat(&base, "llama3").await;
    assert_eq!(res.status(), 503);
    let v: Value = res.json().await.unwrap();
    assert_eq!(v["error"], "host unavailable");
    assert_eq!(v["host"], "home");
    host.set(RAW_UP);
    let res = chat(&base, "llama3").await;
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn recovery_on_lookup_miss_within_one_request() {
    let host = raw_host().await;
    host.set(RAW_REJECT);
    let base = start(&toml_config(&[("home", &host.url)], KEYS)).await;
    let res = chat(&base, "llama3").await;
    assert_eq!(res.status(), 503);
    let v: Value = res.json().await.unwrap();
    assert_eq!(v["host"], "home");
    host.set(RAW_UP);
    let res = chat(&base, "llama3").await;
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn anthropic_route_names_down_host() {
    let host = raw_host().await;
    let base = start(&toml_config(&[("home", &host.url)], KEYS)).await;
    let res = anthropic_message(&base, "llama3").await;
    assert_eq!(res.status(), 200);
    host.set(RAW_REJECT);
    let res = anthropic_message(&base, "llama3").await;
    assert_eq!(res.status(), 502);
    let v: Value = res.json().await.unwrap();
    assert_eq!(v["error"], "upstream unavailable");
    assert_eq!(v["host"], "home");
    let res = anthropic_message(&base, "llama3").await;
    assert_eq!(res.status(), 503);
    let v: Value = res.json().await.unwrap();
    assert_eq!(v["error"], "host unavailable");
    assert_eq!(v["host"], "home");
    host.set(RAW_UP);
    let res = anthropic_message(&base, "llama3").await;
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn anthropic_unknown_model_on_down_host_is_503() {
    let base = start(&toml_config(&[("home", &dead_base_url().await)], KEYS)).await;
    let res = anthropic_message(&base, "does-not-exist").await;
    assert_eq!(res.status(), 503);
    let v: Value = res.json().await.unwrap();
    assert_eq!(v["error"], "host unavailable");
    assert_eq!(v["host"], "home");
}

#[tokio::test]
async fn unknown_model_on_down_host_is_503() {
    let base = start(&toml_config(&[("home", &dead_base_url().await)], KEYS)).await;
    let res = chat(&base, "llama3").await;
    assert_eq!(res.status(), 503);
    let v: Value = res.json().await.unwrap();
    assert_eq!(v["error"], "host unavailable");
    assert_eq!(v["host"], "home");
}

#[tokio::test]
async fn forward_connection_error_names_host() {
    let home = MockServer::start().await;
    let base = start(&toml_config(
        &[
            ("dead", &dead_base_url().await),
            ("home", home.uri().as_str()),
        ],
        KEYS,
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
    mount_tags(&home, &["mistral"]).await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"done": true})))
        .mount(&home)
        .await;
    let base = start(&toml_config(
        &[
            ("dead", &dead_base_url().await),
            ("home", home.uri().as_str()),
        ],
        KEYS,
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
        KEYS,
    ))
    .await;
    let res = chat(&base, "does-not-exist").await;
    assert_eq!(res.status(), 404);
    let v: Value = res.json().await.unwrap();
    assert_eq!(v["error"], "unknown model");
}
