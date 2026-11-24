//! API REST de administración y servidor de la interfaz web.
//!
//! Seguridad:
//! - Escucha en `127.0.0.1` por defecto.
//! - Todas las rutas `/api/v1/*` exigen `Authorization: Bearer <token>`
//!   (o `?token=` para EventSource). El token vive en la carpeta de datos
//!   con permisos restringidos.
//! - Se valida el header `Host` para impedir ataques de DNS rebinding desde
//!   páginas web abiertas en el navegador.

mod routes;
mod ui;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Router;
use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use fakes_app::{AppOptions, FakesApp};
use serde_json::json;
use tokio::sync::oneshot;

pub use routes::router as api_router;

pub const DEFAULT_ADMIN_PORT: u16 = 4010;
pub const TOKEN_FILE: &str = "admin-token";

#[derive(Debug, Clone)]
pub struct AdminOptions {
    pub host: String,
    pub port: u16,
    pub token: String,
}

#[derive(Clone)]
struct Guard {
    token: Arc<String>,
    allowed_hosts: Arc<Vec<String>>,
}

/// Construye el router completo: `/api/v1` protegido + interfaz web.
pub fn build_router(app: Arc<FakesApp>, token: String, port: u16) -> Router {
    let guard = Guard {
        token: Arc::new(token.clone()),
        allowed_hosts: Arc::new(allowed_hosts(port)),
    };
    let mcp = fakes_mcp::McpServer::new(app.clone());
    let api = routes::router()
        .with_state(app)
        .layer(middleware::from_fn_with_state(guard.clone(), require_token));
    let mcp_routes = Router::new()
        .route(
            "/mcp",
            axum::routing::post(mcp_post)
                .get(mcp_get)
                .delete(mcp_delete),
        )
        .with_state(mcp)
        .layer(axum::extract::DefaultBodyLimit::max(64 * 1024 * 1024))
        .layer(middleware::from_fn_with_state(guard.clone(), require_token));
    Router::new()
        .nest("/api/v1", api)
        .merge(mcp_routes)
        .route("/healthz", axum::routing::get(|| async { "ok" }))
        .fallback(ui::serve_ui)
        .layer(axum::Extension(ui::UiToken(Arc::new(token))))
        .layer(middleware::from_fn_with_state(guard, check_host))
}

/// MCP por HTTP (transporte "streamable HTTP" con respuestas JSON).
async fn mcp_post(
    State(mcp): State<Arc<fakes_mcp::McpServer>>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let payload: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                axum::Json(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": format!("JSON inválido: {e}")}})),
            )
                .into_response();
        }
    };
    let is_init = payload.get("method").and_then(|m| m.as_str()) == Some("initialize");
    match mcp.handle_payload(payload).await {
        Some(resp) => {
            let mut r = axum::Json(resp).into_response();
            let session = headers
                .get("mcp-session-id")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
                .or_else(|| is_init.then(|| format!("{:032x}", fastrand::u128(..))));
            if let Some(s) = session.and_then(|s| axum::http::HeaderValue::from_str(&s).ok()) {
                r.headers_mut().insert("mcp-session-id", s);
            }
            r
        }
        None => StatusCode::ACCEPTED.into_response(),
    }
}

/// No hay flujo de eventos del servidor: las respuestas van en el POST.
async fn mcp_get() -> Response {
    (StatusCode::METHOD_NOT_ALLOWED, [(header::ALLOW, "POST")]).into_response()
}

async fn mcp_delete() -> StatusCode {
    StatusCode::NO_CONTENT
}

fn allowed_hosts(port: u16) -> Vec<String> {
    let mut v = Vec::new();
    for h in ["127.0.0.1", "localhost", "[::1]", "tauri.localhost"] {
        v.push(h.to_string());
        v.push(format!("{h}:{port}"));
    }
    v
}

async fn check_host(State(g): State<Guard>, req: Request, next: Next) -> Response {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !host.is_empty() && !g.allowed_hosts.contains(&host) {
        return (
            StatusCode::FORBIDDEN,
            axum::Json(json!({
                "error": "forbidden_host",
                "message": "host no permitido para la administración local"
            })),
        )
            .into_response();
    }
    next.run(req).await
}

async fn require_token(State(g): State<Guard>, req: Request, next: Next) -> Response {
    let from_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(str::trim);
    let from_query = req
        .uri()
        .query()
        .and_then(|q| q.split('&').find_map(|kv| kv.strip_prefix("token=")));
    let given = from_header.or(from_query).unwrap_or("");
    if !constant_time_eq(given.as_bytes(), g.token.as_bytes()) {
        return (
            StatusCode::UNAUTHORIZED,
            axum::Json(json!({
                "error": "unauthorized",
                "message": "token de administración inválido o ausente"
            })),
        )
            .into_response();
    }
    next.run(req).await
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Lee el token de administración o lo genera la primera vez.
pub fn load_or_create_token(data_dir: &Path) -> std::io::Result<String> {
    let path = data_dir.join(TOKEN_FILE);
    if let Ok(t) = std::fs::read_to_string(&path) {
        let t = t.trim().to_string();
        if t.len() >= 32 {
            return Ok(t);
        }
    }
    std::fs::create_dir_all(data_dir)?;
    let token: String = std::iter::repeat_with(fastrand::alphanumeric)
        .take(48)
        .collect();
    write_private(&path, &token)?;
    Ok(token)
}

#[cfg(unix)]
fn write_private(path: &Path, content: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(content.as_bytes())
}

#[cfg(not(unix))]
fn write_private(path: &Path, content: &str) -> std::io::Result<()> {
    // En Windows, %APPDATA% ya es privado para el usuario.
    std::fs::write(path, content)
}

// ---------------------------------------------------------------------------
// Arranque completo (CLI y escritorio)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct LaunchOptions {
    pub data_dir: PathBuf,
    pub admin_host: String,
    pub admin_port: u16,
}

impl LaunchOptions {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            admin_host: "127.0.0.1".into(),
            admin_port: DEFAULT_ADMIN_PORT,
        }
    }
}

pub struct Launched {
    pub app: Arc<FakesApp>,
    pub admin_addr: SocketAddr,
    pub admin_url: String,
    pub token: String,
    shutdown: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

impl Launched {
    /// Detiene la administración y el servidor de simulación.
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        // Las conexiones de eventos en vivo (SSE) no terminan solas: se espera
        // un momento y después se cortan.
        if tokio::time::timeout(std::time::Duration::from_secs(2), &mut self.task)
            .await
            .is_err()
        {
            self.task.abort();
        }
        self.app.shutdown().await;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error(
        "el puerto de administración {0} ya está en uso; ¿Fake Services ya se está ejecutando?"
    )]
    AdminPortInUse(u16),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    App(#[from] fakes_app::AppError),
}

/// Abre el puerto de administración, arranca la aplicación y sirve la API.
pub async fn launch(opts: LaunchOptions) -> Result<Launched, LaunchError> {
    // Primero el puerto de administración: si está ocupado, hay otra instancia.
    let listener = tokio::net::TcpListener::bind((opts.admin_host.as_str(), opts.admin_port))
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AddrInUse => LaunchError::AdminPortInUse(opts.admin_port),
            _ => LaunchError::Io(e),
        })?;
    let admin_addr = listener.local_addr()?;
    let token = load_or_create_token(&opts.data_dir)?;

    let mut app_opts = AppOptions::new(opts.data_dir.clone());
    app_opts.reserved_ports = vec![admin_addr.port()];
    let app = FakesApp::start(app_opts).await?;

    let router = build_router(app.clone(), token.clone(), admin_addr.port());
    let (tx, rx) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let res = axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = rx.await;
            })
            .await;
        if let Err(e) = res {
            tracing::error!("la API de administración terminó con error: {e}");
        }
    });
    let host = if opts.admin_host == "0.0.0.0" {
        "127.0.0.1"
    } else {
        opts.admin_host.as_str()
    };
    Ok(Launched {
        admin_url: format!("http://{host}:{}", admin_addr.port()),
        app,
        admin_addr,
        token,
        shutdown: Some(tx),
        task,
    })
}

#[cfg(test)]
mod tests;
