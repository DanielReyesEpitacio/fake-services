use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use fakes_domain::*;
use fakes_matching::CompiledWorkspace;
use fakes_store::{Outcome, RequestRecord};
use http::Request;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use super::*;

#[derive(Default)]
struct MemSink(Mutex<Vec<RequestRecord>>);
impl RequestSink for MemSink {
    fn record(&self, r: RequestRecord) {
        self.0.lock().unwrap().push(r);
    }
}

fn scenario(id: &str, rules: Vec<Rule>, response: ResponseDef) -> Scenario {
    Scenario {
        id: id.into(),
        name: id.into(),
        description: String::new(),
        enabled: true,
        rule_mode: RuleMode::All,
        workflows: vec![],
        rules,
        response,
    }
}

fn mifiel() -> Service {
    let mut sign = Endpoint {
        schema_version: 1,
        id: "sign".into(),
        name: "Firmar".into(),
        description: String::new(),
        enabled: true,
        method: HttpMethod::Post,
        path: "/documents/{id}/sign".into(),
        request: RequestSpec::default(),
        scenarios: vec![
            scenario(
                "rechazada",
                vec![Rule {
                    source: RuleSource::Body,
                    key: "reject".into(),
                    operator: Operator::Equals,
                    value: Some(json!(true)),
                }],
                ResponseDef::json(409, json!({"status": "rejected"})),
            ),
            scenario(
                "ok",
                vec![],
                ResponseDef::json(
                    201,
                    json!({
                        "id": "{{ request.params.id }}",
                        "status": "pending",
                        "callback": "{{ request.body.callback_url }}",
                        "url": "{{ service.url }}"
                    }),
                ),
            ),
        ],
        active_scenario: None,
        on_validation_error: None,
    };
    sign.request.body = Some(BodySpec {
        kind: BodyKind::Json,
        required: true,
        schema: Some(json!({"type": "object", "required": ["callback_url"]})),
        example: None,
    });
    Service {
        schema_version: 1,
        id: "mifiel".into(),
        name: "MiFiel".into(),
        description: String::new(),
        upstream_url: None,
        prefix: "/mifiel".into(),
        port: None,
        auto_start: true,
        variables: Default::default(),
        auth: AuthConfig::Bearer { token: None },
        workflows: vec![],
        endpoints: vec![sign],
    }
}

fn setup() -> (Arc<ServerState>, Arc<MemSink>) {
    let sink = Arc::new(MemSink::default());
    let compiled = CompiledWorkspace::compile(Workspace::new("demo", "Demo"), vec![mifiel()]);
    let snap = Snapshot {
        compiled,
        running: HashSet::from(["mifiel".to_string()]),
        files_root: None,
    };
    (ServerState::new(snap, sink.clone()), sink)
}

async fn call(state: &Arc<ServerState>, req: Request<Body>) -> (u16, http::HeaderMap, Value) {
    let app = build_router(state.clone(), None)
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 5555))));
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, headers, v)
}

fn post(path: &str, body: Value) -> Request<Body> {
    Request::post(path)
        .header("authorization", "Bearer secreto-largo-123")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn full_flow_with_templates_and_history() {
    let (state, sink) = setup();
    let (status, headers, body) = call(
        &state,
        post(
            "/mifiel/documents/doc_9/sign",
            json!({"callback_url": "http://localhost:3000/hook"}),
        ),
    )
    .await;
    assert_eq!(status, 201);
    assert_eq!(headers["x-fakes-scenario"], "ok");
    assert_eq!(
        body,
        json!({
            "id": "doc_9",
            "status": "pending",
            "callback": "http://localhost:3000/hook",
            "url": "http://127.0.0.1:4000/mifiel"
        })
    );
    let recs = sink.0.lock().unwrap();
    assert_eq!(recs.len(), 1);
    let r = &recs[0];
    assert_eq!(r.outcome, Outcome::Matched);
    assert_eq!(r.scenario_id.as_deref(), Some("ok"));
    assert_eq!(r.endpoint_id.as_deref(), Some("sign"));
    // El token queda oculto en el historial
    let auth = r
        .request_headers
        .iter()
        .find(|(k, _)| k == "authorization")
        .unwrap();
    assert_eq!(auth.1, "Bearer secr••••");
}

#[tokio::test]
async fn scenario_rules_and_forced_header() {
    let (state, _) = setup();
    let (status, _, _) = call(
        &state,
        post(
            "/mifiel/documents/1/sign",
            json!({"callback_url": "x", "reject": true}),
        ),
    )
    .await;
    assert_eq!(status, 409);

    let mut req = post("/mifiel/documents/1/sign", json!({"callback_url": "x"}));
    req.headers_mut()
        .insert("x-fakes-scenario", "rechazada".parse().unwrap());
    let (status, _, _) = call(&state, req).await;
    assert_eq!(status, 409);
}

#[tokio::test]
async fn errors_are_explicit() {
    let (state, sink) = setup();

    // sin token
    let req = Request::post("/mifiel/documents/1/sign")
        .body(Body::empty())
        .unwrap();
    let (status, headers, body) = call(&state, req).await;
    assert_eq!(status, 401);
    assert_eq!(headers["www-authenticate"], "Bearer");
    assert_eq!(body["error"], "unauthorized");

    // validación
    let (status, _, body) = call(&state, post("/mifiel/documents/1/sign", json!({}))).await;
    assert_eq!(status, 422);
    assert_eq!(body["errors"][0]["field"], "body");

    // método no permitido
    let req = Request::get("/mifiel/documents/1/sign")
        .header("authorization", "Bearer x")
        .body(Body::empty())
        .unwrap();
    let (status, headers, _) = call(&state, req).await;
    assert_eq!(status, 405);
    assert_eq!(headers["allow"], "POST");

    // servicio inexistente
    let req = Request::get("/otro/x").body(Body::empty()).unwrap();
    let (status, _, body) = call(&state, req).await;
    assert_eq!(status, 404);
    assert_eq!(body["availablePrefixes"], json!(["/mifiel"]));

    let outcomes: Vec<Outcome> = sink.0.lock().unwrap().iter().map(|r| r.outcome).collect();
    assert_eq!(
        outcomes,
        vec![
            Outcome::Unauthorized,
            Outcome::ValidationFailed,
            Outcome::MethodNotAllowed,
            Outcome::NotFound
        ]
    );
}

#[tokio::test]
async fn stopped_service_returns_503_and_hot_reload() {
    let (state, _) = setup();
    let current = state.snapshot.load_full();
    let compiled =
        CompiledWorkspace::compile((*current.compiled.workspace).clone(), vec![mifiel()]);
    state.snapshot.store(Arc::new(Snapshot {
        compiled,
        running: HashSet::new(),
        files_root: None,
    }));
    let (status, _, body) = call(
        &state,
        post("/mifiel/documents/1/sign", json!({"callback_url": "x"})),
    )
    .await;
    assert_eq!(status, 503);
    assert_eq!(body["error"], "service_stopped");
}

#[tokio::test]
async fn cors_preflight_and_headers() {
    let (state, _) = setup();
    let req = Request::options("/mifiel/documents/1/sign")
        .header("origin", "http://localhost:5173")
        .header("access-control-request-method", "POST")
        .header(
            "access-control-request-headers",
            "authorization,content-type",
        )
        .body(Body::empty())
        .unwrap();
    let (status, headers, _) = call(&state, req).await;
    assert_eq!(status, 204);
    assert_eq!(
        headers["access-control-allow-origin"],
        "http://localhost:5173"
    );
    assert_eq!(
        headers["access-control-allow-headers"],
        "authorization,content-type"
    );

    let mut req = post("/mifiel/documents/1/sign", json!({"callback_url": "x"}));
    req.headers_mut()
        .insert("origin", "http://localhost:5173".parse().unwrap());
    let (_, headers, _) = call(&state, req).await;
    assert_eq!(
        headers["access-control-allow-origin"],
        "http://localhost:5173"
    );
}

#[tokio::test]
async fn real_listeners_open_and_close() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let (state, _) = setup();
    let server = MockServer::new(state.clone());
    let mut ws = Workspace::new("demo", "Demo");
    ws.server.port = 0;
    let snap = |ws: Workspace| Snapshot {
        compiled: CompiledWorkspace::compile(ws, vec![mifiel()]),
        running: HashSet::from(["mifiel".to_string()]),
        files_root: None,
    };
    let status = server.apply(snap(ws.clone())).await;
    assert_eq!(status.len(), 1);
    assert!(status[0].listening, "{status:?}");
    let addr = server.central_addr().await.unwrap();

    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(b"GET /__fakes/health HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut buf = String::new();
    stream.read_to_string(&mut buf).await.unwrap();
    assert!(buf.starts_with("HTTP/1.1 200"), "{buf}");
    assert!(buf.contains("fake-services"));

    // Puerto ocupado → error legible, sin pánico.
    let blocker = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let busy = blocker.local_addr().unwrap().port();
    let mut svc = mifiel();
    svc.port = Some(busy);
    let status = server
        .apply(Snapshot {
            compiled: CompiledWorkspace::compile(ws.clone(), vec![svc]),
            running: HashSet::new(),
            files_root: None,
        })
        .await;
    let own = status.iter().find(|s| s.service_id.is_some()).unwrap();
    assert!(!own.listening);
    assert!(own.error.as_deref().unwrap().contains("en uso"));

    server.shutdown().await;
    assert!(tokio::net::TcpStream::connect(addr).await.is_err());
}
