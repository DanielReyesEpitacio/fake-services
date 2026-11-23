//! Interfaz web embebida en el binario (compilada desde `ui/`).

use std::sync::Arc;

use axum::Extension;
use axum::http::{StatusCode, Uri, header};
use axum::response::{Html, IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../ui/dist"]
struct Assets;

#[derive(Clone)]
pub struct UiToken(pub Arc<String>);

const MISSING_UI: &str = r#"<!doctype html>
<html lang="es"><head><meta charset="utf-8"><title>Fake Services</title>
<style>body{font-family:system-ui,sans-serif;max-width:640px;margin:80px auto;padding:0 24px;color:#15121F}
code{background:#EDE9FE;padding:2px 6px;border-radius:4px}</style></head>
<body><h1>Fake Services está funcionando</h1>
<p>La interfaz web no está compilada en este binario. Para generarla:</p>
<p><code>cd ui && pnpm install && pnpm build</code></p>
<p>La API de administración está disponible en <code>/api/v1</code>.</p></body></html>"#;

pub async fn serve_ui(uri: Uri, Extension(token): Extension<UiToken>) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return (StatusCode::NOT_FOUND, "ruta de API desconocida").into_response();
    }
    if !path.is_empty()
        && path != "index.html"
        && let Some(file) = Assets::get(path)
    {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        let cache = if path.starts_with("assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
        return (
            [
                (header::CONTENT_TYPE, mime.as_ref().to_string()),
                (header::CACHE_CONTROL, cache.to_string()),
            ],
            file.data.into_owned(),
        )
            .into_response();
    }
    // SPA: cualquier otra ruta devuelve index.html con el token inyectado.
    match Assets::get("index.html") {
        Some(file) => {
            let html = String::from_utf8_lossy(&file.data).replace(
                "<head>",
                &format!(
                    "<head>\n    <meta name=\"fakes-admin-token\" content=\"{}\">",
                    token.0
                ),
            );
            (
                [
                    (header::CACHE_CONTROL, "no-store"),
                    (header::REFERRER_POLICY, "no-referrer"),
                ],
                Html(html),
            )
                .into_response()
        }
        None => Html(MISSING_UI).into_response(),
    }
}
