use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

pub const WINDOW: usize = 50;
pub const TAIL: usize = 64 * 1024;

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub eval_count: u64,
    pub eval_duration_ns: u64,
    pub prompt_eval_count: Option<u64>,
    pub prompt_eval_duration_ns: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EvalFields {
    pub eval_count: u64,
    pub eval_duration_ns: u64,
    pub prompt_eval_count: Option<u64>,
    pub prompt_eval_duration_ns: Option<u64>,
}

#[derive(Clone)]
pub struct ModelTps {
    pub model: String,
    pub avg: f64,
    pub last: f64,
    pub prompt: Option<f64>,
    pub samples: usize,
}

#[derive(Default)]
pub struct TpsStore {
    samples: Mutex<HashMap<(String, String), VecDeque<Sample>>>,
}

impl TpsStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&self, host_id: &str, model: &str, fields: EvalFields) {
        let sample = Sample {
            eval_count: fields.eval_count,
            eval_duration_ns: fields.eval_duration_ns,
            prompt_eval_count: fields.prompt_eval_count,
            prompt_eval_duration_ns: fields.prompt_eval_duration_ns,
        };
        let mut map = self.samples.lock().unwrap_or_else(|p| p.into_inner());
        let q = map
            .entry((host_id.to_string(), model.to_string()))
            .or_default();
        if q.len() == WINDOW {
            q.pop_front();
        }
        q.push_back(sample);
    }

    pub fn per_host(&self, host_id: &str) -> Vec<ModelTps> {
        let map = self.samples.lock().unwrap_or_else(|p| p.into_inner());
        let mut out: Vec<ModelTps> = map
            .iter()
            .filter(|((host, _), _)| host == host_id)
            .map(|((_, model), q)| summarize(model.clone(), q))
            .collect();
        out.sort_by(|a, b| a.model.cmp(&b.model));
        out
    }
}

fn summarize(model: String, q: &VecDeque<Sample>) -> ModelTps {
    let tps_values: Vec<f64> = q
        .iter()
        .map(|s| rate(s.eval_count, s.eval_duration_ns))
        .collect();
    let avg = tps_values.iter().sum::<f64>() / tps_values.len() as f64;
    let last = tps_values[tps_values.len() - 1];
    let prompt =
        q.iter()
            .rev()
            .find_map(|s| match (s.prompt_eval_count, s.prompt_eval_duration_ns) {
                (Some(c), Some(d)) => Some(rate(c, d)),
                _ => None,
            });
    ModelTps {
        model,
        avg,
        last,
        prompt,
        samples: q.len(),
    }
}

fn rate(count: u64, duration_ns: u64) -> f64 {
    if duration_ns == 0 {
        0.0
    } else {
        count as f64 / (duration_ns as f64 / 1e9)
    }
}

pub fn capture(tail: &[u8], content_type: Option<&str>) -> Option<EvalFields> {
    let tail = String::from_utf8_lossy(tail);
    let ct = content_type.unwrap_or("");
    let value = if ct.contains("ndjson") || ct.contains("jsonl") {
        last_ndjson_object(&tail)
    } else if ct.contains("json") {
        last_json_object(&tail)
    } else {
        return None;
    };
    eval_fields(&value?)
}

fn last_ndjson_object(tail: &str) -> Option<Value> {
    tail.rsplit('\n')
        .map(str::trim)
        .find(|line| !line.is_empty())
        .and_then(|line| serde_json::from_str(line).ok())
}

fn last_json_object(tail: &str) -> Option<Value> {
    let bytes = tail.as_bytes();
    let mut best = None;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            match object_span(tail, i) {
                Some((end, v)) => {
                    best = Some(v);
                    i = end;
                }
                None => i += 1,
            }
        } else {
            i += 1;
        }
    }
    best
}

fn object_span(tail: &str, start: usize) -> Option<(usize, Value)> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (i, ch) in tail[start..].char_indices() {
        let abs = start + i;
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
        } else {
            match ch {
                '"' => in_string = true,
                '{' => depth += 1,
                '}' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        let v: Value = serde_json::from_str(&tail[start..=abs]).ok()?;
                        return Some((abs + 1, v));
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn eval_fields(v: &Value) -> Option<EvalFields> {
    let eval_count = num_u64(v.get("eval_count")?)?;
    let eval_duration_ns = num_u64(v.get("eval_duration")?)?;
    if eval_count == 0 || eval_duration_ns == 0 {
        return None;
    }
    Some(EvalFields {
        eval_count,
        eval_duration_ns,
        prompt_eval_count: v.get("prompt_eval_count").and_then(num_u64),
        prompt_eval_duration_ns: v.get("prompt_eval_duration").and_then(num_u64),
    })
}

pub(crate) fn num_u64(v: &Value) -> Option<u64> {
    let n = v.as_number()?;
    n.as_u64().or_else(|| n.as_f64().map(|f| f as u64))
}

pub fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const STREAM_TAIL: &str = r#"{"model":"llama3","message":{"role":"assistant","content":"hi"},"done":false}
{"model":"llama3","done":true,"done_reason":"stop","prompt_eval_count":12,"prompt_eval_duration":300000000,"eval_count":136,"eval_duration":3459000000}
"#;

    #[test]
    fn ndjson_final_line_with_eval_fields() {
        let f = capture(STREAM_TAIL.as_bytes(), Some("application/x-ndjson")).unwrap();
        assert_eq!(f.eval_count, 136);
        assert_eq!(f.eval_duration_ns, 3459000000);
        assert_eq!(f.prompt_eval_count, Some(12));
        assert_eq!(f.prompt_eval_duration_ns, Some(300000000));
    }

    #[test]
    fn ndjson_truncated_head_still_finds_final_line() {
        let tail = format!(r#"{{"partial_chunk_cut"{}"#, STREAM_TAIL);
        let f = capture(tail.as_bytes(), Some("application/x-ndjson")).unwrap();
        assert_eq!(f.eval_count, 136);
    }

    #[test]
    fn non_streaming_json() {
        let body = r#"{"model":"llama3","done":true,"eval_count":88,"eval_duration":2281000000}"#;
        let f = capture(body.as_bytes(), Some("application/json")).unwrap();
        assert_eq!(f.eval_count, 88);
        assert_eq!(f.prompt_eval_count, None);
    }

    #[test]
    fn json_body_truncated_mid_object_yields_none_without_panic() {
        let filler = "9".repeat(TAIL);
        let body = format!(
            r#"{{"model":"llama3","context":[{filler}],"eval_count":88,"eval_duration":2281000000}}"#
        );
        let tail = &body.as_bytes()[body.len() - TAIL..];
        assert_eq!(capture(tail, Some("application/json")), None);
    }

    #[test]
    fn json_tail_drops_leading_partial_bytes_and_finds_last_object() {
        let junk = format!(r#"1234,"junk":"{}""#, "x".repeat(TAIL));
        let body = format!("{junk}{{\"eval_count\":88,\"eval_duration\":2281000000}}");
        let tail = &body.as_bytes()[body.len() - TAIL..];
        let f = capture(tail, Some("application/json")).unwrap();
        assert_eq!(f.eval_count, 88);
    }

    #[test]
    fn utf8_split_cut_still_finds_eval_fields() {
        let filler = "日".repeat(TAIL / 3 + 2);
        let body = format!(
            "{{\"note\":\"{filler}\",\"done\":false}}\n{{\"done\":true,\"eval_count\":136,\"eval_duration\":3459000000}}\n"
        );
        let mut cut = body.len() - TAIL;
        while body.is_char_boundary(cut) {
            cut -= 1;
        }
        assert!(!body.is_char_boundary(cut));
        let tail = &body.as_bytes()[cut..];
        let f = capture(tail, Some("application/x-ndjson")).unwrap();
        assert_eq!(f.eval_count, 136);
        assert_eq!(f.eval_duration_ns, 3459000000);
    }

    #[test]
    fn json_body_larger_than_tail_prefers_last_complete_object() {
        let big_string = "x".repeat(TAIL);
        let body = format!(
            r#"{{"note":"{big_string}","msg":{{"role":"assistant"}},"eval_count":88,"eval_duration":2281000000}}"#
        );
        let tail = &body.as_bytes()[body.len() - TAIL..];
        let as_str = std::str::from_utf8(tail).unwrap();
        let obj = last_json_object(as_str).unwrap();
        assert_eq!(obj.get("role").and_then(Value::as_str), Some("assistant"));
    }

    #[test]
    fn missing_eval_fields() {
        let body = r#"{"model":"llama3","done":true}"#;
        assert_eq!(capture(body.as_bytes(), Some("application/json")), None);
        assert_eq!(capture(STREAM_TAIL.as_bytes(), Some("text/plain")), None);
        assert_eq!(capture(STREAM_TAIL.as_bytes(), None), None);
    }

    #[test]
    fn zero_eval_fields() {
        let body = r#"{"done":true,"eval_count":0,"eval_duration":0}"#;
        assert_eq!(capture(body.as_bytes(), Some("application/json")), None);
    }

    #[test]
    fn malformed_tail_never_panics() {
        for tail in [
            "",
            "{",
            "}",
            "{{{{",
            "not json at all",
            "{\"a\": \"unterminated}",
            "{\"a\": [1, 2, 3",
            "\u{0}\u{fffd}{garbage}",
        ] {
            assert_eq!(capture(tail.as_bytes(), Some("application/json")), None);
            assert_eq!(capture(tail.as_bytes(), Some("application/x-ndjson")), None);
        }
    }

    #[test]
    fn window_caps_at_50_samples() {
        let store = TpsStore::new();
        for _ in 0..60 {
            store.record(
                "home",
                "llama3",
                EvalFields {
                    eval_count: 10,
                    eval_duration_ns: 1_000_000_000,
                    prompt_eval_count: None,
                    prompt_eval_duration_ns: None,
                },
            );
        }
        let tps = store.per_host("home");
        assert_eq!(tps.len(), 1);
        assert_eq!(tps[0].samples, WINDOW);
        assert!((tps[0].avg - 10.0).abs() < 1e-9);
        assert!(store.per_host("desk").is_empty());
    }
}
