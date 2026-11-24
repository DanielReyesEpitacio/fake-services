use axum::body::Body;
use http::Request;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use super::*;

const TOKEN: &str = "test-token-0123456789-0123456789-0123456789";

async fn setup() -> (Router, Arc<FakesApp>, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let mut opts = AppOptions::new(tmp.path());
    opts.central_port_override = Some(0);
    let app = FakesApp::start(opts).await.unwrap();
    (build_router(app.clone(), TOKEN.into(), 4010), app, tmp)
}

async fn send(router: &Router, method: &str, uri: &str, body: Option<Value>) -> (u16, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", "127.0.0.1:4010")
        .header("authorization", format!("Bearer {TOKEN}"));
    let body = match body {
        Some(b) => {
            req = req.header("content-type", "application/json");
            Body::from(b.to_string())
        }
        None => Body::empty(),
    };
    let resp = router
        .clone()
        .oneshot(req.body(body).unwrap())
        .await
        .unwrap();
    let status = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn requires_token_and_valid_host() {
    let (router, app, _tmp) = setup().await;
    let req = Request::get("/api/v1/status")
        .header("host", "127.0.0.1:4010")
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 401);

    let req = Request::get("/api/v1/status")
        .header("host", "evil.example.com")
        .header("authorization", format!("Bearer {TOKEN}"))
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 403);

    let req = Request::get(format!("/api/v1/status?token={TOKEN}"))
        .header("host", "localhost:4010")
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    app.shutdown().await;
}

#[tokio::test]
async fn crud_through_rest() {
    let (router, app, _tmp) = setup().await;

    let (s, v) = send(
        &router,
        "POST",
        "/api/v1/services",
        Some(json!({"name": "MiFiel", "auth": {"type": "bearer"}})),
    )
    .await;
    assert_eq!(s, 201, "{v}");
    assert_eq!(v["service"]["id"], "mifiel");

    let (s, v) = send(
        &router,
        "POST",
        "/api/v1/services",
        Some(json!({"name": 5})),
    )
    .await;
    assert_eq!(s, 400);
    assert_eq!(v["error"], "bad_request");

    let (s, v) = send(
        &router,
        "POST",
        "/api/v1/services/mifiel/endpoints",
        Some(json!({
            "method": "POST",
            "path": "/documents/{id}/sign",
            "scenarios": [{"name": "OK", "response": {"status": 201}}]
        })),
    )
    .await;
    assert_eq!(s, 201, "{v}");
    let eid = v["endpoint"]["id"].as_str().unwrap().to_string();

    let (s, v) = send(
        &router,
        "POST",
        "/api/v1/services/mifiel/endpoints",
        Some(json!({"method": "GET", "path": "sin-barra"})),
    )
    .await;
    assert_eq!(s, 422);
    assert_eq!(v["issues"][0]["field"], "path");

    let (s, v) = send(
        &router,
        "POST",
        "/api/v1/services/mifiel/endpoints/validate",
        Some(json!({"method": "GET", "path": "/ok"})),
    )
    .await;
    assert_eq!(s, 200);
    assert_eq!(v["valid"], true);

    let (s, v) = send(
        &router,
        "POST",
        &format!("/api/v1/services/mifiel/endpoints/{eid}/disable"),
        None,
    )
    .await;
    assert_eq!(s, 200);
    assert_eq!(v["endpoint"]["enabled"], false);

    let (s, v) = send(
        &router,
        "PUT",
        &format!("/api/v1/services/mifiel/endpoints/{eid}/active-scenario"),
        Some(json!({"scenarioId": "ok"})),
    )
    .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["endpoint"]["activeScenario"], "ok");

    let (s, _) = send(&router, "POST", "/api/v1/services/mifiel/stop", None).await;
    assert_eq!(s, 200);
    let (_, v) = send(&router, "GET", "/api/v1/services", None).await;
    let mifiel = v
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "mifiel")
        .unwrap();
    assert_eq!(mifiel["running"], false);

    let (s, v) = send(&router, "GET", "/api/v1/services/nope", None).await;
    assert_eq!(s, 404);
    assert_eq!(v["error"], "not_found");

    let (s, v) = send(&router, "GET", "/api/v1/schemas/endpoint", None).await;
    assert_eq!(s, 200);
    assert!(v["properties"]["scenarios"].is_object());

    let (s, v) = send(
        &router,
        "GET",
        "/api/v1/requests?errorsOnly=true&limit=5",
        None,
    )
    .await;
    assert_eq!(s, 200, "{v}");

    let (s, _) = send(&router, "DELETE", "/api/v1/services/mifiel", None).await;
    assert_eq!(s, 204);
    app.shutdown().await;
}

#[tokio::test]
async fn ui_fallback_serves_html() {
    let (router, app, _tmp) = setup().await;
    let req = Request::get("/servicios/x")
        .header("host", "127.0.0.1:4010")
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&bytes);
    assert!(html.contains("Fake Services"));
    app.shutdown().await;
}

#[test]
fn token_is_created_once_with_private_permissions() {
    let tmp = tempfile::tempdir().unwrap();
    let a = load_or_create_token(tmp.path()).unwrap();
    let b = load_or_create_token(tmp.path()).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.len(), 48);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(tmp.path().join(TOKEN_FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[tokio::test]
async fn workflows_and_executions_through_rest() {
    let (router, app, _tmp) = setup().await;

    let (s, v) = send(&router, "GET", "/api/v1/services/ejemplo/workflows", None).await;
    assert_eq!(s, 200);
    assert_eq!(v.as_array().unwrap().len(), 2);

    let (s, v) = send(
        &router,
        "POST",
        "/api/v1/services/ejemplo/workflows",
        Some(json!({"name": "Rápido", "steps": [{"type": "setVariables", "values": {"x": "{{ input.n }}"}}]})),
    )
    .await;
    assert_eq!(s, 201, "{v}");
    assert_eq!(v["workflow"]["steps"][0]["id"], "setvariables");

    let (s, v) = send(
        &router,
        "POST",
        "/api/v1/services/ejemplo/workflows/validate",
        Some(json!({"name": "Mal", "steps": [{"id": "a", "type": "http", "method": "POST", "url": ""}]})),
    )
    .await;
    assert_eq!(s, 200);
    assert_eq!(v["valid"], false);

    let (s, v) = send(
        &router,
        "POST",
        "/api/v1/services/ejemplo/workflows/rapido/run",
        Some(json!({"input": {"n": 7}})),
    )
    .await;
    assert_eq!(s, 202, "{v}");
    let id = v["id"].as_str().unwrap().to_string();

    let mut detail = Value::Null;
    for _ in 0..100 {
        let (_, d) = send(&router, "GET", &format!("/api/v1/executions/{id}"), None).await;
        if d["execution"]["status"] == "completed" {
            detail = d;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(detail["execution"]["context"]["vars"]["x"], 7, "{detail}");
    assert!(detail["timeline"].as_array().unwrap().len() >= 3);

    let (s, v) = send(&router, "GET", "/api/v1/executions?status=completed", None).await;
    assert_eq!(s, 200);
    assert_eq!(v.as_array().unwrap().len(), 1);

    let (s, _) = send(
        &router,
        "DELETE",
        "/api/v1/services/ejemplo/workflows/notificar-firma",
        None,
    )
    .await;
    assert_eq!(s, 409);
    let (s, _) = send(&router, "GET", "/api/v1/schemas/workflow", None).await;
    assert_eq!(s, 200);
    app.shutdown().await;
}

#[tokio::test]
async fn export_then_import_through_rest() {
    let (router, app, _tmp) = setup().await;
    let resp = router
        .clone()
        .oneshot(
            Request::get("/api/v1/export?services=ejemplo&workspace=true")
                .header("host", "127.0.0.1:4010")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let cd = resp.headers()["content-disposition"]
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        cd.starts_with("attachment; filename=\"fake-services-ejemplo-20"),
        "{cd}"
    );
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let bundle: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(bundle["format"], "fake-services");
    assert_eq!(bundle["services"][0]["id"], "ejemplo");
    assert!(bundle["services"][0]["workflows"].as_array().unwrap().len() >= 2);

    let (s, v) = send(
        &router,
        "POST",
        "/api/v1/import",
        Some(json!({"bundle": bundle, "dryRun": true})),
    )
    .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["renamed"][0]["to"], "ejemplo-2");
    let (s, v) = send(
        &router,
        "POST",
        "/api/v1/import",
        Some(json!({"bundle": bundle, "onConflict": "skip"})),
    )
    .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["skipped"], json!(["ejemplo"]));
    let (s, v) = send(
        &router,
        "POST",
        "/api/v1/import",
        Some(json!({"bundle": {"format": "otro", "formatVersion": 1}})),
    )
    .await;
    assert_eq!(s, 400, "{v}");
    let (s, _) = send(&router, "GET", "/api/v1/schemas/bundle", None).await;
    assert_eq!(s, 200);
    let (s, v) = send(&router, "GET", "/api/v1/system/autostart", None).await;
    assert_eq!(s, 200);
    assert!(v["supported"].is_boolean());
    app.shutdown().await;
}
