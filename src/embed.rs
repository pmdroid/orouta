use axum::extract::Path;
use axum::http::header;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "ui/dist"]
struct Assets;

pub async fn index() -> Response {
    match Assets::get("index.html") {
        Some(file) => serve(&file.data, "text/html; charset=utf-8", "no-cache"),
        None => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "ui/dist is missing; run: cd ui && bun install && bun run build",
        )
            .into_response(),
    }
}

pub async fn asset(Path(rest): Path<String>) -> Response {
    let name = format!("assets/{rest}");
    if name.contains("..") {
        return StatusCode::NOT_FOUND.into_response();
    }
    match Assets::get(&name) {
        Some(file) => serve(
            &file.data,
            mime(&name),
            "public, max-age=31536000, immutable",
        ),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn serve(data: &[u8], content_type: &str, cache: &str) -> Response {
    (
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, cache),
        ],
        data.to_vec(),
    )
        .into_response()
}

fn mime(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("") {
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
