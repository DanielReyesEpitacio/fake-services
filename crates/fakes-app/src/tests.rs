use std::time::Duration;

use fakes_domain::*;
use serde_json::json;

use super::*;

async fn app(dir: &Path) -> Arc<FakesApp> {
    let mut opts = AppOptions::new(dir);
    opts.central_port_override = Some(0);
    opts.reserved_ports = vec![4010];
    FakesApp::start(opts).await.unwrap()
}

fn service_input(name: &str) -> Service {
    serde_json::from_value(json!({ "name": name })).unwrap()
}

fn endpoint_input(v: serde_json::Value) -> Endpoint {
    serde_json::from_value(v).unwrap()
}

async fn http(app: &FakesApp, raw: &str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let addr = app.server().central_addr().await.unwrap();
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    s.write_all(raw.as_bytes()).await.unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).await.unwrap();
    out
}

#[tokio::test]
async fn first_start_creates_example_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path()).await;
    let ws = app.list_workspaces().await;
    assert_eq!(ws.len(), 1);
    assert!(ws[0].active);
    let services = app.list_services().await;
    assert_eq!(services[0].id, "ejemplo");
    assert!(services[0].running);
    let status = app.status().await.unwrap();
    assert_eq!(status.endpoints_total, 3);
    assert!(status.listeners[0].listening);

    let resp = http(
        &app,
        "GET /ejemplo/usuarios/0 HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(resp.starts_with("HTTP/1.1 404"), "{resp}");
    let resp = http(
        &app,
        "GET /ejemplo/hola?nombre=Ana HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(resp.contains("Hola Ana"), "{resp}");
    app.shutdown().await;
}

#[tokio::test]
async fn crud_services_and_endpoints_persist_to_disk() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path()).await;

    // Crear servicio con id y prefijo generados
    let svc = app.create_service(service_input("MiFiel")).await.unwrap();
    assert_eq!(svc.service.id, "mifiel");
    assert_eq!(svc.service.prefix, "/mifiel");
    assert!(svc.running);
    assert!(svc.url.ends_with("/mifiel"));

    // Conflicto de prefijo
    let mut dup = service_input("Otro");
    dup.prefix = "/mifiel".into();
    match app.create_service(dup).await {
        Err(AppError::Validation(issues)) => assert_eq!(issues[0].field, "prefix"),
        other => panic!("{other:?}"),
    }

    // Crear endpoint con id y escenarios generados
    let ep = app
        .create_endpoint(
            "mifiel",
            endpoint_input(json!({
                "method": "POST",
                "path": "/documents/{id}/sign",
                "scenarios": [
                    {"name": "Firma exitosa", "response": {"status": 201, "body": {"type": "json", "content": {"id": "{{ request.params.id }}", "status": "pending"}}}},
                    {"name": "", "response": {"status": 500}}
                ]
            })),
        )
        .await
        .unwrap();
    assert_eq!(ep.endpoint.id, "post-documents-id-sign");
    assert_eq!(ep.endpoint.scenarios[0].id, "firma-exitosa");
    assert_eq!(ep.endpoint.scenarios[1].id, "http-500");
    assert_eq!(ep.endpoint.scenarios[1].name, "HTTP 500");

    // Plantilla inválida se rechaza
    let bad = endpoint_input(json!({
        "method": "GET", "path": "/x",
        "scenarios": [{"name": "a", "response": {"status": 200, "body": {"type": "text", "content": "{{ roto "}}}]
    }));
    assert!(matches!(
        app.create_endpoint("mifiel", bad).await,
        Err(AppError::Validation(_))
    ));

    // El servidor ya responde con la nueva configuración
    let resp = http(&app, "POST /mifiel/documents/d1/sign HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
    assert!(resp.starts_with("HTTP/1.1 201"), "{resp}");
    assert!(resp.contains("\"id\": \"d1\""));

    // Forzar escenario
    app.set_active_scenario("mifiel", "post-documents-id-sign", Some("http-500".into()))
        .await
        .unwrap();
    let resp = http(&app, "POST /mifiel/documents/d1/sign HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
    assert!(resp.starts_with("HTTP/1.1 500"), "{resp}");

    // Patch parcial
    let ep = app
        .patch_endpoint(
            "mifiel",
            "post-documents-id-sign",
            json!({"activeScenario": null, "name": "Firmar"}),
        )
        .await
        .unwrap();
    assert_eq!(ep.endpoint.name, "Firmar");
    assert_eq!(ep.endpoint.active_scenario, None);

    // Detener servicio
    app.stop_service("mifiel").await.unwrap();
    let resp = http(&app, "POST /mifiel/documents/d1/sign HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
    assert!(resp.starts_with("HTTP/1.1 503"), "{resp}");
    app.start_service("mifiel").await.unwrap();

    // Persistencia: un proceso nuevo ve lo mismo
    let ws_dir = PathBuf::from(&app.status().await.unwrap().workspace_path);
    let loaded = fakes_config::load_workspace(&ws_dir).unwrap();
    let mifiel = loaded.services.iter().find(|s| s.id == "mifiel").unwrap();
    assert_eq!(mifiel.endpoints[0].name, "Firmar");

    // Patch de servicio no pierde endpoints
    app.patch_service("mifiel", json!({"description": "Firma electrónica"}))
        .await
        .unwrap();
    assert_eq!(app.list_endpoints("mifiel").await.unwrap().len(), 1);

    // Eliminar
    app.delete_endpoint("mifiel", "post-documents-id-sign")
        .await
        .unwrap();
    app.delete_service("mifiel").await.unwrap();
    assert!(!ws_dir.join("services/mifiel").exists());
    assert!(matches!(
        app.get_service("mifiel").await,
        Err(AppError::NotFound(_))
    ));
    app.shutdown().await;
}

#[tokio::test]
async fn history_is_recorded_and_queryable() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path()).await;
    let mut events = app.subscribe();
    http(&app, "GET /ejemplo/hola HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer supersecreto123\r\nConnection: close\r\n\r\n").await;
    http(
        &app,
        "GET /nada HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    )
    .await;

    // Esperar a que el escritor asíncrono procese
    let mut seen = 0;
    while seen < 2 {
        match tokio::time::timeout(Duration::from_secs(5), events.recv()).await {
            Ok(Ok(AppEvent::Request(_))) => seen += 1,
            Ok(Ok(_)) => {}
            other => panic!("{other:?}"),
        }
    }
    let all = app.list_requests(&RequestFilter::default()).await.unwrap();
    assert_eq!(all.len(), 2);
    let errors = app
        .list_requests(&RequestFilter {
            errors_only: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].status, 404);

    let ok = all.iter().find(|r| r.status == 200).unwrap();
    let full = app.get_request(&ok.id).await.unwrap();
    assert_eq!(full.service_id.as_deref(), Some("ejemplo"));
    let auth = full
        .request_headers
        .iter()
        .find(|(k, _)| k == "authorization")
        .unwrap();
    assert!(!auth.1.contains("supersecreto"));

    assert_eq!(app.clear_requests(None).await.unwrap(), 2);
    app.shutdown().await;
}

#[tokio::test]
async fn workspaces_create_activate_and_reload() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path()).await;

    let repo = tmp.path().join("mi-repo/.fakes");
    let info = app
        .create_workspace(
            Workspace::new("", "Proyecto Pagos"),
            Some(repo.clone()),
            true,
        )
        .await
        .unwrap();
    assert_eq!(info.id, "proyecto-pagos");
    assert!(repo.join("fakes.workspace.json").exists());
    assert!(app.list_services().await.is_empty());

    app.create_service(service_input("Belvo")).await.unwrap();

    // Edición externa (p. ej. git pull) + reload
    let mut svc: Service =
        fakes_config::read_json(&repo.join("services/belvo/service.json")).unwrap();
    svc.description = "editado fuera".into();
    fakes_config::save_service(&repo, &svc).unwrap();
    let view = app.reload().await.unwrap();
    assert_eq!(view.services[0].description, "editado fuera");
    assert!(view.services[0].running);

    // Volver al principal
    app.activate_workspace("principal").await.unwrap();
    assert_eq!(app.list_services().await[0].id, "ejemplo");
    assert!(app.forget_workspace("principal").await.is_err());
    app.forget_workspace("proyecto-pagos").await.unwrap();

    // Registrar carpeta existente
    let info = app.register_workspace(repo.clone(), false).await.unwrap();
    assert_eq!(info.id, "proyecto-pagos");
    assert_eq!(app.list_workspaces().await.len(), 2);

    // Puerto reservado
    let mut ws = app.active_workspace().await.workspace;
    ws.server.port = 4010;
    assert!(matches!(
        app.update_workspace(ws).await,
        Err(AppError::Validation(_))
    ));
    app.shutdown().await;
}

#[tokio::test]
async fn service_with_own_port() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path()).await;
    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let mut s = service_input("S3");
    s.port = Some(port);
    let view = app.create_service(s).await.unwrap();
    assert_eq!(view.port_url, Some(format!("http://127.0.0.1:{port}")));
    let status = app.status().await.unwrap();
    let own = status
        .listeners
        .iter()
        .find(|l| l.service_id.as_deref() == Some("s3"))
        .unwrap();
    assert!(own.listening, "{own:?}");
    app.delete_service("s3").await.unwrap();
    let status = app.status().await.unwrap();
    assert_eq!(status.listeners.len(), 1);
    app.shutdown().await;
}
