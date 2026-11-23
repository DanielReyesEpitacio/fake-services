//! Procesamiento de una solicitud de simulación, de principio a fin.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::response::Response;
use bytes::Bytes;
use fakes_domain::{AuthConfig, IncomingRequest, Issue, ResponseDef, validate::MAX_DELAY_MS};
use fakes_matching::{CompiledEndpoint, CompiledService, EndpointMatch};
use fakes_response::{RenderedResponse, render_response};
use fakes_store::{CapturedBody, Outcome, RequestRecord, new_id};
use http::{HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode};
use serde_json::{Value, json};

use crate::body::parse_body;
use crate::{ServerState, Snapshot, display_host};

/// Rutas reservadas del propio simulador (`/__fakes/health`).
pub const RESERVED_PREFIX: &str = "/__fakes";

#[derive(Clone)]
struct ListenerCtx {
    state: Arc<ServerState>,
    /// Servicio fijo cuando el listener es un puerto propio.
    bound_service: Option<String>,
}

pub fn build_router(state: Arc<ServerState>, bound_service: Option<String>) -> Router {
    Router::new().fallback(handle).with_state(ListenerCtx {
        state,
        bound_service,
    })
}

/// Resultado intermedio antes de convertirse en respuesta HTTP.
struct Handled {
    rendered: RenderedResponse,
    outcome: Outcome,
    message: Option<String>,
    service_id: Option<String>,
    endpoint_id: Option<String>,
    scenario_id: Option<String>,
}

impl Handled {
    fn error(status: u16, outcome: Outcome, code: &str, message: String, extra: Value) -> Self {
        let mut body = json!({ "error": code, "message": message, "source": "fake-services" });
        if let (Value::Object(b), Value::Object(e)) = (&mut body, extra) {
            b.extend(e);
        }
        Self {
            rendered: RenderedResponse {
                status,
                headers: vec![("content-type".into(), "application/json".into())],
                body: serde_json::to_vec_pretty(&body).unwrap_or_default(),
                delay_ms: 0,
            },
            outcome,
            message: Some(message),
            service_id: None,
            endpoint_id: None,
            scenario_id: None,
        }
    }

    fn for_service(mut self, svc: &CompiledService) -> Self {
        self.service_id = Some(svc.service.id.clone());
        self
    }
}

async fn handle(
    State(ctx): State<ListenerCtx>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    req: Request<Body>,
) -> Response {
    let started = Instant::now();
    let snap = ctx.state.snapshot.load_full();
    let (parts, body) = req.into_parts();
    let path = parts.uri.path().to_string();
    let query_string = parts.uri.query().unwrap_or("").to_string();
    let origin = parts
        .headers
        .get(http::header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let cors = snap.compiled.workspace.server.cors;

    if path == format!("{RESERVED_PREFIX}/health") {
        return json_response(200, &json!({"status": "ok", "app": "fake-services"}));
    }

    // Preflight CORS automático.
    if cors
        && parts.method == Method::OPTIONS
        && parts.headers.contains_key("access-control-request-method")
    {
        let mut resp = Response::new(Body::empty());
        *resp.status_mut() = StatusCode::NO_CONTENT;
        add_cors(resp.headers_mut(), origin.as_deref(), Some(&parts.headers));
        return resp;
    }

    let mut incoming = IncomingRequest {
        method: parts.method.as_str().to_string(),
        path: path.clone(),
        route_path: path.clone(),
        query: form_urlencoded::parse(query_string.as_bytes())
            .into_owned()
            .collect(),
        query_string: query_string.clone(),
        headers: parts
            .headers
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_string(),
                    String::from_utf8_lossy(v.as_bytes()).into_owned(),
                )
            })
            .collect(),
        content_type: parts
            .headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string),
        remote_addr: Some(remote.ip().to_string()),
        ..Default::default()
    };

    let max_body = snap.compiled.workspace.server.max_body_bytes;
    let raw =
        match axum::body::to_bytes(body, usize::try_from(max_body).unwrap_or(usize::MAX)).await {
            Ok(b) => b,
            Err(_) => {
                let h = Handled::error(
                    413,
                    Outcome::Error,
                    "payload_too_large",
                    format!("el body supera el máximo de {max_body} bytes"),
                    json!({}),
                );
                return finish(&ctx, &snap, &incoming, h, started, origin.as_deref(), cors).await;
            }
        };
    incoming.body = parse_body(incoming.content_type.as_deref(), &raw).await;
    incoming.raw_body = raw.to_vec();

    let handled = process(&ctx, &snap, &mut incoming).await;
    finish(
        &ctx,
        &snap,
        &incoming,
        handled,
        started,
        origin.as_deref(),
        cors,
    )
    .await
}

async fn process(ctx: &ListenerCtx, snap: &Snapshot, req: &mut IncomingRequest) -> Handled {
    // 1. Servicio
    let found = match &ctx.bound_service {
        Some(id) => snap.compiled.service(id).map(|s| {
            // En puerto propio se aceptan rutas con o sin prefijo.
            let rest = if !s.prefix.is_empty() && req.path.starts_with(&s.prefix) {
                let r = &req.path[s.prefix.len()..];
                if r.is_empty() {
                    "/".to_string()
                } else if r.starts_with('/') {
                    r.to_string()
                } else {
                    req.path.clone()
                }
            } else {
                req.path.clone()
            };
            (s, rest)
        }),
        None => snap.compiled.service_for_path(&req.path),
    };
    let Some((svc, rest)) = found else {
        let prefixes: Vec<&str> = snap
            .compiled
            .services
            .iter()
            .filter(|s| !s.prefix.is_empty())
            .map(|s| s.prefix.as_str())
            .collect();
        return Handled::error(
            404,
            Outcome::NotFound,
            "service_not_found",
            format!("ningún servicio simulado usa la ruta {}", req.path),
            json!({ "availablePrefixes": prefixes }),
        );
    };
    req.route_path = rest;

    if !snap.running.contains(&svc.service.id) {
        return Handled::error(
            503,
            Outcome::ServiceStopped,
            "service_stopped",
            format!("el servicio '{}' está detenido", svc.service.name),
            json!({}),
        )
        .for_service(svc);
    }

    // 2. Endpoint
    let (endpoint, params) = match svc.find_endpoint(&req.method, &req.route_path) {
        EndpointMatch::Found { endpoint, params } => (endpoint, params),
        EndpointMatch::MethodNotAllowed { allowed } => {
            let mut h = Handled::error(
                405,
                Outcome::MethodNotAllowed,
                "method_not_allowed",
                format!(
                    "{} {} no está configurado; métodos disponibles: {}",
                    req.method,
                    req.route_path,
                    allowed.join(", ")
                ),
                json!({ "allowed": allowed }),
            )
            .for_service(svc);
            h.rendered
                .headers
                .push(("allow".into(), allowed.join(", ")));
            return h;
        }
        EndpointMatch::NotFound => {
            return Handled::error(
                404,
                Outcome::NotFound,
                "endpoint_not_found",
                format!(
                    "el servicio '{}' no tiene un endpoint para {} {}",
                    svc.service.name, req.method, req.route_path
                ),
                json!({}),
            )
            .for_service(svc);
        }
    };
    req.params = params;

    // 3. Autenticación
    if let Err(reason) = svc.check_auth(req) {
        let mut h = Handled::error(
            401,
            Outcome::Unauthorized,
            "unauthorized",
            reason,
            json!({}),
        )
        .for_service(svc);
        h.endpoint_id = Some(endpoint.endpoint.id.clone());
        match &svc.service.auth {
            AuthConfig::Bearer { .. } => h
                .rendered
                .headers
                .push(("www-authenticate".into(), "Bearer".into())),
            AuthConfig::Basic { .. } => h.rendered.headers.push((
                "www-authenticate".into(),
                "Basic realm=\"fake-services\"".into(),
            )),
            _ => {}
        }
        return h;
    }

    let base_ctx = template_context(snap, svc, endpoint, req);

    // 4. Validación
    let issues = endpoint.validate(req);
    if !issues.is_empty() {
        let mut h = validation_response(snap, endpoint, &base_ctx, &issues).for_service(svc);
        h.endpoint_id = Some(endpoint.endpoint.id.clone());
        return h;
    }

    // 5. Escenario
    let Some((scenario, _why)) = endpoint.select_scenario(req, &svc.vars) else {
        let mut h = Handled::error(
            501,
            Outcome::NoScenario,
            "no_scenario",
            format!(
                "ningún escenario de '{}' coincide con la solicitud",
                endpoint.endpoint.id
            ),
            json!({}),
        )
        .for_service(svc);
        h.endpoint_id = Some(endpoint.endpoint.id.clone());
        return h;
    };

    let mut ctx_value = base_ctx;
    ctx_value["scenario"] = json!({ "id": scenario.id, "name": scenario.name });

    let mut handled =
        match render_response(&scenario.response, &ctx_value, snap.files_root.as_deref()) {
            Ok(r) => Handled {
                rendered: r,
                outcome: Outcome::Matched,
                message: None,
                service_id: None,
                endpoint_id: None,
                scenario_id: None,
            },
            Err(e) => Handled::error(
                500,
                Outcome::Error,
                "template_error",
                e.to_string(),
                json!({ "scenario": scenario.id }),
            ),
        }
        .for_service(svc);
    handled.endpoint_id = Some(endpoint.endpoint.id.clone());
    handled.scenario_id = Some(scenario.id.clone());
    handled
}

fn validation_response(
    snap: &Snapshot,
    endpoint: &CompiledEndpoint,
    base_ctx: &Value,
    issues: &[Issue],
) -> Handled {
    let errors = serde_json::to_value(issues).unwrap_or(Value::Null);
    let message = issues
        .iter()
        .map(|i| format!("{}: {}", i.field, i.message))
        .collect::<Vec<_>>()
        .join("; ");
    match &endpoint.endpoint.on_validation_error {
        Some(def) => {
            let mut ctx = base_ctx.clone();
            ctx["validation"] = json!({ "errors": errors });
            match render_response(def, &ctx, snap.files_root.as_deref()) {
                Ok(r) => Handled {
                    rendered: r,
                    outcome: Outcome::ValidationFailed,
                    message: Some(message),
                    service_id: None,
                    endpoint_id: None,
                    scenario_id: None,
                },
                Err(e) => Handled::error(
                    500,
                    Outcome::Error,
                    "template_error",
                    e.to_string(),
                    json!({}),
                ),
            }
        }
        None => {
            let def = ResponseDef::json(
                422,
                json!({
                    "error": "validation_failed",
                    "message": "la solicitud no cumple lo que espera el endpoint",
                    "source": "fake-services",
                    "errors": errors,
                }),
            );
            let rendered = render_response(&def, &Value::Null, None).unwrap_or(RenderedResponse {
                status: 422,
                headers: vec![],
                body: vec![],
                delay_ms: 0,
            });
            Handled {
                rendered,
                outcome: Outcome::ValidationFailed,
                message: Some(message),
                service_id: None,
                endpoint_id: None,
                scenario_id: None,
            }
        }
    }
}

/// Contexto disponible en plantillas.
fn template_context(
    snap: &Snapshot,
    svc: &CompiledService,
    endpoint: &CompiledEndpoint,
    req: &IncomingRequest,
) -> Value {
    let server = &snap.compiled.workspace.server;
    let service_url = match svc.service.port {
        Some(p) => format!("http://{}:{}", display_host(&server.host), p),
        None => format!("{}{}", snap.central_url(), svc.prefix),
    };
    json!({
        "request": {
            "method": req.method,
            "path": req.path,
            "routePath": req.route_path,
            "params": req.params,
            "query": req.query_map(),
            "queryAll": req.query_all(),
            "queryString": req.query_string,
            "headers": req.headers_map(),
            "contentType": req.content_type,
            "body": req.body,
            "rawBody": req.body_text(),
            "ip": req.remote_addr,
        },
        "vars": svc.vars,
        "service": {
            "id": svc.service.id,
            "name": svc.service.name,
            "prefix": svc.prefix,
            "url": service_url,
        },
        "endpoint": {
            "id": endpoint.endpoint.id,
            "name": endpoint.endpoint.name,
            "method": endpoint.endpoint.method.as_str(),
            "path": endpoint.endpoint.path,
        },
        "workspace": { "id": snap.compiled.workspace.id, "name": snap.compiled.workspace.name },
    })
}

async fn finish(
    ctx: &ListenerCtx,
    snap: &Snapshot,
    req: &IncomingRequest,
    handled: Handled,
    started: Instant,
    origin: Option<&str>,
    cors: bool,
) -> Response {
    let delay = handled.rendered.delay_ms.min(MAX_DELAY_MS);
    if delay > 0 {
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }

    let mut resp = Response::new(Body::from(Bytes::from(handled.rendered.body.clone())));
    *resp.status_mut() =
        StatusCode::from_u16(handled.rendered.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let headers = resp.headers_mut();
    for (k, v) in &handled.rendered.headers {
        if let (Ok(name), Ok(value)) = (
            HeaderName::try_from(k.as_str()),
            HeaderValue::try_from(v.as_str()),
        ) {
            headers.append(name, value);
        }
    }
    if let Some(s) = &handled.scenario_id
        && let Ok(v) = HeaderValue::try_from(s.as_str())
    {
        headers.insert("x-fakes-scenario", v);
    }
    if cors {
        add_cors(headers, origin, None);
    }

    let history = &snap.compiled.workspace.history;
    if history.enabled {
        let redact = |hs: Vec<(String, String)>| -> Vec<(String, String)> {
            hs.into_iter()
                .map(|(k, v)| {
                    if history
                        .redact_headers
                        .iter()
                        .any(|r| r.eq_ignore_ascii_case(&k))
                    {
                        (k, redact_value(&v))
                    } else {
                        (k, v)
                    }
                })
                .collect()
        };
        let resp_headers: Vec<(String, String)> = resp
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_string(),
                    String::from_utf8_lossy(v.as_bytes()).into_owned(),
                )
            })
            .collect();
        let record = RequestRecord {
            id: new_id(),
            ts_ms: chrono::Utc::now().timestamp_millis(),
            workspace_id: snap.workspace_id().to_string(),
            service_id: handled.service_id,
            endpoint_id: handled.endpoint_id,
            scenario_id: handled.scenario_id,
            method: req.method.clone(),
            path: req.path.clone(),
            query: req.query_string.clone(),
            remote_addr: req.remote_addr.clone(),
            request_headers: redact(req.headers.clone()),
            request_body: CapturedBody::capture(&req.raw_body, history.max_body_capture_bytes),
            status: handled.rendered.status,
            response_headers: redact(resp_headers),
            response_body: CapturedBody::capture(
                &handled.rendered.body,
                history.max_body_capture_bytes,
            ),
            duration_ms: started.elapsed().as_millis() as u64,
            outcome: handled.outcome,
            message: handled.message,
        };
        ctx.state.sink.record(record);
    }
    resp
}

/// Conserva el esquema (`Bearer`) y unos caracteres para poder distinguir valores.
fn redact_value(v: &str) -> String {
    let (scheme, secret) = match v.split_once(' ') {
        Some((s, rest)) if s.len() <= 10 => (format!("{s} "), rest),
        _ => (String::new(), v),
    };
    let visible: String = secret.chars().take(4).collect();
    if secret.chars().count() <= 8 {
        format!("{scheme}••••")
    } else {
        format!("{scheme}{visible}••••")
    }
}

fn add_cors(headers: &mut HeaderMap, origin: Option<&str>, preflight: Option<&HeaderMap>) {
    let allow_origin = origin.unwrap_or("*");
    if let Ok(v) = HeaderValue::try_from(allow_origin) {
        headers.insert("access-control-allow-origin", v);
    }
    if origin.is_some() {
        headers.insert(
            "access-control-allow-credentials",
            HeaderValue::from_static("true"),
        );
        headers.append("vary", HeaderValue::from_static("Origin"));
    }
    headers.insert(
        "access-control-expose-headers",
        HeaderValue::from_static("*"),
    );
    if let Some(req) = preflight {
        let methods =
            req.get("access-control-request-method")
                .cloned()
                .unwrap_or(HeaderValue::from_static(
                    "GET, POST, PUT, PATCH, DELETE, OPTIONS",
                ));
        headers.insert("access-control-allow-methods", methods);
        let allow_headers = req
            .get("access-control-request-headers")
            .cloned()
            .unwrap_or(HeaderValue::from_static("*"));
        headers.insert("access-control-allow-headers", allow_headers);
        headers.insert("access-control-max-age", HeaderValue::from_static("600"));
    }
}

fn json_response(status: u16, v: &Value) -> Response {
    let mut resp = Response::new(Body::from(serde_json::to_vec(v).unwrap_or_default()));
    *resp.status_mut() = StatusCode::from_u16(status).unwrap_or(StatusCode::OK);
    resp.headers_mut().insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    resp
}

#[cfg(test)]
mod tests {
    use super::redact_value;

    #[test]
    fn redacts_secrets() {
        assert_eq!(redact_value("Bearer abcdefghijkl"), "Bearer abcd••••");
        assert_eq!(redact_value("short"), "••••");
        assert_eq!(redact_value("sk_live_1234567890"), "sk_l••••");
    }
}
