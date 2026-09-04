use serde_json::Value;

pub fn extract_name(body: &[u8]) -> Option<String> {
    let v: Value = serde_json::from_slice(body).ok()?;
    let obj = v.as_object()?;
    if let Some(m) = obj.get("model").and_then(|x| x.as_str()) {
        return Some(m.to_string());
    }
    obj.get("name").and_then(|x| x.as_str()).map(str::to_string)
}

pub fn is_inference(path: &str) -> bool {
    matches!(
        path,
        "/api/chat"
            | "/api/generate"
            | "/api/embed"
            | "/api/embeddings"
            | "/v1/chat/completions"
            | "/v1/completions"
            | "/v1/embeddings"
            | "/v1/messages"
    )
}

pub fn is_management(path: &str) -> bool {
    matches!(
        path,
        "/api/pull" | "/api/push" | "/api/show" | "/api/delete" | "/api/create" | "/api/copy"
    )
}

pub fn copy_names(body: &[u8]) -> (Option<String>, Option<String>) {
    let v: Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => return (None, None),
    };
    let source = v.get("source").and_then(|x| x.as_str()).map(str::to_string);
    let destination = v
        .get("destination")
        .and_then(|x| x.as_str())
        .map(str::to_string);
    (source, destination)
}
