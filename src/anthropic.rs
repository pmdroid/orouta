use crate::config::Upstream;
use crate::proxy;
use crate::AppState;
use axum::body::Body;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use bytes::Bytes;
use futures_util::StreamExt;
use serde_json::{json, Map, Value};
use std::pin::Pin;
use std::task::{Context, Poll};

pub async fn messages(state: AppState, body: Bytes) -> Response {
    let parsed: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "invalid json"})),
            )
                .into_response();
        }
    };
    let client_model = match parsed.get("model").and_then(|m| m.as_str()) {
        Some(m) => m.to_string(),
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "unknown model"})),
            )
                .into_response();
        }
    };
    let Some(upstream) = state
        .catalog
        .lookup(&state.config, &state.client, &client_model)
        .await
    else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "unknown model"})),
        )
            .into_response();
    };
    if parsed.get("tools").is_some() || parsed.get("tool_choice").is_some() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "unsupported content"})),
        )
            .into_response();
    }
    let ollama_model = client_model.clone();
    let ollama_body = match to_ollama(&parsed, &ollama_model) {
        Ok(v) => v,
        Err(msg) => {
            return (StatusCode::BAD_REQUEST, Json(json!({"error": msg}))).into_response();
        }
    };
    let stream = parsed
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let payload = match serde_json::to_vec(&ollama_body) {
        Ok(p) => Bytes::from(p),
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "encode"})),
            )
                .into_response();
        }
    };
    if stream {
        stream_chat(&state, &upstream, payload, client_model).await
    } else {
        complete_chat(&state, &upstream, payload, client_model).await
    }
}

fn to_ollama(body: &Value, model: &str) -> Result<Value, &'static str> {
    let mut messages = Vec::new();
    if let Some(system) = extract_system(body)? {
        messages.push(json!({"role": "system", "content": system}));
    }
    let incoming = body
        .get("messages")
        .and_then(|m| m.as_array())
        .ok_or("unsupported content")?;
    for m in incoming {
        let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("user");
        let content = match m.get("content") {
            None => String::new(),
            Some(c) => text_from_content(c)?,
        };
        messages.push(json!({"role": role, "content": content}));
    }
    let mut options = Map::new();
    if let Some(v) = body.get("max_tokens") {
        options.insert("num_predict".into(), v.clone());
    }
    if let Some(v) = body.get("temperature") {
        options.insert("temperature".into(), v.clone());
    }
    if let Some(v) = body.get("top_p") {
        options.insert("top_p".into(), v.clone());
    }
    if let Some(v) = body.get("top_k") {
        options.insert("top_k".into(), v.clone());
    }
    if let Some(v) = body.get("stop_sequences") {
        options.insert("stop".into(), v.clone());
    }
    let stream = body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let mut ollama = json!({
        "model": model,
        "messages": messages,
        "stream": stream,
    });
    if !options.is_empty() {
        ollama["options"] = Value::Object(options);
    }
    Ok(ollama)
}

fn extract_system(body: &Value) -> Result<Option<String>, &'static str> {
    match body.get("system") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(v) => Ok(Some(text_from_content(v)?)),
    }
}

fn text_from_content(value: &Value) -> Result<String, &'static str> {
    match value {
        Value::Null => Ok(String::new()),
        Value::String(s) => Ok(s.clone()),
        Value::Array(arr) => {
            let mut out = String::new();
            for block in arr {
                let ty = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                if ty != "text" {
                    return Err("unsupported content");
                }
                if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                    out.push_str(t);
                }
            }
            Ok(out)
        }
        _ => Err("unsupported content"),
    }
}

fn stop_reason(done_reason: &str) -> &'static str {
    match done_reason {
        "length" => "max_tokens",
        "stop_sequence" => "stop_sequence",
        _ => "end_turn",
    }
}

async fn complete_chat(
    state: &AppState,
    upstream: &Upstream,
    payload: Bytes,
    client_model: String,
) -> Response {
    let url = format!("{}/api/chat", upstream.base_url);
    let mut builder = state.client.post(url).body(payload);
    builder = builder.header(header::CONTENT_TYPE, "application/json");
    if let Some(key) = &upstream.api_key {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {key}"));
    }
    let stats = &state.stats[&upstream.id];
    stats.request_started();
    let start = std::time::Instant::now();
    let resp = match builder.send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "upstream");
            stats.request_finished(start.elapsed(), Some(e.to_string()));
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "upstream unavailable"})),
            )
                .into_response();
        }
    };
    if !resp.status().is_success() {
        stats.request_finished(
            start.elapsed(),
            Some(format!("http {}", resp.status().as_u16())),
        );
        return proxy::pipe_response(resp).await;
    }
    stats.request_finished(start.elapsed(), None);
    let ollama: Value = match resp.json().await {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "upstream invalid json"})),
            )
                .into_response();
        }
    };
    let text = ollama
        .pointer("/message/content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let done_reason = ollama
        .get("done_reason")
        .and_then(|v| v.as_str())
        .unwrap_or("stop");
    let input_tokens = ollama
        .get("prompt_eval_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let output_tokens = ollama
        .get("eval_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let id = format!("msg_orouta_{}", uuid::Uuid::new_v4());
    Json(json!({
        "id": id,
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": text}],
        "model": client_model,
        "stop_reason": stop_reason(done_reason),
        "usage": {
            "input_tokens": input_tokens,
            "output_tokens": output_tokens
        }
    }))
    .into_response()
}

async fn stream_chat(
    state: &AppState,
    upstream: &Upstream,
    payload: Bytes,
    client_model: String,
) -> Response {
    let url = format!("{}/api/chat", upstream.base_url);
    let mut builder = state.client.post(url).body(payload);
    builder = builder.header(header::CONTENT_TYPE, "application/json");
    if let Some(key) = &upstream.api_key {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {key}"));
    }
    let stats = &state.stats[&upstream.id];
    stats.request_started();
    let start = std::time::Instant::now();
    let resp = match builder.send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "upstream");
            stats.request_finished(start.elapsed(), Some(e.to_string()));
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "upstream unavailable"})),
            )
                .into_response();
        }
    };
    if !resp.status().is_success() {
        stats.request_finished(
            start.elapsed(),
            Some(format!("http {}", resp.status().as_u16())),
        );
        return proxy::pipe_response(resp).await;
    }
    stats.request_finished(start.elapsed(), None);
    let id = format!("msg_orouta_{}", uuid::Uuid::new_v4());
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(32);
    tokio::spawn(async move {
        translate_ndjson(resp, tx, id, client_model).await;
    });
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    let mut response = Response::new(Body::from_stream(RxStream(rx)));
    *response.headers_mut() = headers;
    *response.status_mut() = StatusCode::OK;
    response
}

fn sse(event: &str, data: &Value) -> Bytes {
    Bytes::from(format!("event: {event}\ndata: {data}\n\n"))
}

async fn translate_ndjson(
    resp: reqwest::Response,
    tx: tokio::sync::mpsc::Sender<Result<Bytes, std::io::Error>>,
    id: String,
    client_model: String,
) {
    let start = json!({
        "type": "message_start",
        "message": {
            "id": id,
            "type": "message",
            "role": "assistant",
            "content": [],
            "model": client_model,
            "stop_reason": Value::Null,
            "stop_sequence": Value::Null,
            "usage": {"input_tokens": 0, "output_tokens": 0}
        }
    });
    if tx.send(Ok(sse("message_start", &start))).await.is_err() {
        return;
    }
    let block_start = json!({
        "type": "content_block_start",
        "index": 0,
        "content_block": {"type": "text", "text": ""}
    });
    if tx
        .send(Ok(sse("content_block_start", &block_start)))
        .await
        .is_err()
    {
        return;
    }
    let mut buf: Vec<u8> = Vec::new();
    let mut stream = resp.bytes_stream();
    let mut finished = false;
    let mut input_tokens = 0u64;
    let mut output_tokens = 0u64;
    let mut reason = "end_turn".to_string();
    while let Some(chunk) = stream.next().await {
        let bytes = match chunk {
            Ok(b) => b,
            Err(e) => {
                let _ = tx.send(Err(std::io::Error::other(e))).await;
                return;
            }
        };
        buf.extend_from_slice(&bytes);
        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let mut line: Vec<u8> = buf.drain(..=pos).collect();
            if line.last() == Some(&b'\n') {
                line.pop();
            }
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if line.is_empty() {
                continue;
            }
            let Ok(v) = serde_json::from_slice::<Value>(&line) else {
                continue;
            };
            if apply_chunk(
                &v,
                &tx,
                &mut finished,
                &mut input_tokens,
                &mut output_tokens,
                &mut reason,
            )
            .await
            .is_err()
            {
                return;
            }
        }
        if finished {
            break;
        }
    }
    if !buf.is_empty() {
        if let Ok(v) = serde_json::from_slice::<Value>(&buf) {
            if apply_chunk(
                &v,
                &tx,
                &mut finished,
                &mut input_tokens,
                &mut output_tokens,
                &mut reason,
            )
            .await
            .is_err()
            {
                return;
            }
        }
    }
    if finished {
        let stop = json!({"type": "content_block_stop", "index": 0});
        if tx.send(Ok(sse("content_block_stop", &stop))).await.is_err() {
            return;
        }
        let delta = json!({
            "type": "message_delta",
            "delta": {"stop_reason": reason, "stop_sequence": Value::Null},
            "usage": {"input_tokens": input_tokens, "output_tokens": output_tokens}
        });
        if tx.send(Ok(sse("message_delta", &delta))).await.is_err() {
            return;
        }
        let stop_msg = json!({"type": "message_stop"});
        let _ = tx.send(Ok(sse("message_stop", &stop_msg))).await;
    }
}

async fn apply_chunk(
    v: &Value,
    tx: &tokio::sync::mpsc::Sender<Result<Bytes, std::io::Error>>,
    finished: &mut bool,
    input_tokens: &mut u64,
    output_tokens: &mut u64,
    reason: &mut String,
) -> Result<(), ()> {
    if *finished {
        return Ok(());
    }
    if let Some(delta) = v.pointer("/message/content").and_then(|c| c.as_str()) {
        if !delta.is_empty() {
            let ev = json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": delta}
            });
            tx.send(Ok(sse("content_block_delta", &ev)))
                .await
                .map_err(|_| ())?;
        }
    }
    if v.get("done").and_then(|d| d.as_bool()).unwrap_or(false) {
        if let Some(n) = v.get("prompt_eval_count").and_then(|x| x.as_u64()) {
            *input_tokens = n;
        }
        if let Some(n) = v.get("eval_count").and_then(|x| x.as_u64()) {
            *output_tokens = n;
        }
        if let Some(dr) = v.get("done_reason").and_then(|x| x.as_str()) {
            *reason = stop_reason(dr).to_string();
        }
        *finished = true;
    }
    Ok(())
}

struct RxStream(tokio::sync::mpsc::Receiver<Result<Bytes, std::io::Error>>);

impl futures_util::Stream for RxStream {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.0.poll_recv(cx)
    }
}
