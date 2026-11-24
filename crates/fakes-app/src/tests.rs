use std::time::Duration;

use fakes_domain::*;
use serde_json::Value;
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
    assert_eq!(status.endpoints_total, 5);
    assert_eq!(status.workflows_total, 2);
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

#[test]
fn seed_example_is_valid() {
    let svc = seed::example_service();
    let wf_ids: Vec<&str> = svc.workflows.iter().map(|w| w.id.as_str()).collect();
    for (i, wf) in svc.workflows.iter().enumerate() {
        let siblings: Vec<&Workflow> = svc.workflows[..i].iter().collect();
        let issues = workflows::full_workflow_issues(wf, &siblings, &wf_ids);
        assert!(issues.is_empty(), "{}: {issues:?}", wf.id);
    }
    for (i, ep) in svc.endpoints.iter().enumerate() {
        let siblings: Vec<&Endpoint> = svc.endpoints[..i].iter().collect();
        let issues = full_endpoint_issues(ep, &siblings, &wf_ids);
        assert!(issues.is_empty(), "{}: {issues:?}", ep.id);
    }
}

/// Criterio de éxito: simulación de MiFiel con respuesta inmediata y webhooks
/// posteriores, configurada solo con datos (sin código).
#[tokio::test]
async fn mifiel_end_to_end_with_webhooks() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path()).await;
    let svc: Service = serde_json::from_value(json!({
        "name": "MiFiel",
        "auth": {"type": "bearer"},
        "workflows": [{
            "name": "Notificar firma",
            "steps": [
                {"id": "espera-1", "type": "wait", "durationMs": 100},
                {"id": "pendiente", "type": "http", "method": "POST", "url": "{{ request.body.callback_url }}",
                 "body": {"type": "json", "content": {"id": "{{ response.body.id }}", "status": "pending"}}},
                {"id": "espera-2", "type": "wait", "durationMs": 200},
                {"id": "firmado", "type": "http", "method": "POST", "url": "{{ request.body.callback_url }}",
                 "body": {"type": "json", "content": {"id": "{{ response.body.id }}", "status": "signed"}}}
            ]
        }],
        "endpoints": [
            {"method": "POST", "path": "/documents/{id}/sign",
             "request": {"body": {"kind": "json", "required": true,
                                  "schema": {"type": "object", "required": ["callback_url"]}}},
             "scenarios": [{"name": "Firma exitosa",
                            "workflows": [{"workflowId": "notificar-firma"}],
                            "response": {"status": 201, "body": {"type": "json",
                                "content": {"id": "doc_{{ request.params.id }}", "status": "pending"}}}}]}
        ]
    }))
    .unwrap();
    let view = app.create_service(svc).await.unwrap();
    assert_eq!(view.service.workflows[0].id, "notificar-firma");

    let addr = app.server().central_addr().await.unwrap();
    // El receptor de webhooks del servicio de ejemplo hace de "tu aplicación".
    let callback = format!("http://{addr}/ejemplo/webhooks");
    let body = json!({ "callback_url": callback }).to_string();
    let resp = http(
        &app,
        &format!(
            "POST /mifiel/documents/42/sign HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer t\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ),
    )
    .await;
    assert!(resp.starts_with("HTTP/1.1 201"), "{resp}");
    let exec_id = resp
        .lines()
        .find_map(|l| l.strip_prefix("x-fakes-executions: "))
        .expect("header con la ejecución")
        .trim()
        .to_string();

    // Esperar a que termine el workflow.
    let started = std::time::Instant::now();
    let detail = loop {
        let d = app.get_execution(&exec_id).await.unwrap();
        if d.execution.status.is_finished() {
            break d;
        }
        assert!(started.elapsed() < Duration::from_secs(10), "no terminó");
        tokio::time::sleep(Duration::from_millis(30)).await;
    };
    assert_eq!(
        detail.execution.status,
        fakes_store::ExecutionStatus::Completed,
        "{:?}",
        detail.execution.error
    );
    assert_eq!(
        detail.execution.trigger,
        fakes_store::TriggerKind::ResponseSent
    );

    // Los webhooks llegaron al receptor (quedan en el historial).
    tokio::time::sleep(Duration::from_millis(100)).await;
    let hits = app
        .list_requests(&RequestFilter {
            search: Some("/ejemplo/webhooks".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(hits.len(), 2);
    let mut bodies = Vec::new();
    for h in hits.iter().rev() {
        let full = app.get_request(&h.id).await.unwrap();
        bodies.push(serde_json::from_slice::<Value>(&full.request_body.bytes).unwrap());
    }
    assert_eq!(bodies[0], json!({"id": "doc_42", "status": "pending"}));
    assert_eq!(bodies[1], json!({"id": "doc_42", "status": "signed"}));

    // No se puede borrar un workflow en uso.
    assert!(matches!(
        app.delete_workflow("mifiel", "notificar-firma").await,
        Err(AppError::Conflict(_))
    ));
    // Un escenario no puede apuntar a un workflow inexistente.
    let bad: Endpoint = serde_json::from_value(json!({
        "method": "GET", "path": "/x",
        "scenarios": [{"name": "a", "workflows": [{"workflowId": "nope"}], "response": {"status": 200}}]
    }))
    .unwrap();
    assert!(matches!(
        app.create_endpoint("mifiel", bad).await,
        Err(AppError::Validation(_))
    ));

    // Ejecución manual y cancelación.
    let wf: Workflow = serde_json::from_value(
        json!({"name": "Larga", "steps": [{"type": "wait", "durationMs": 60000}]}),
    )
    .unwrap();
    app.create_workflow("mifiel", wf).await.unwrap();
    let e = app
        .run_workflow("mifiel", "larga", Default::default(), None)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(app.status().await.unwrap().executions_active, 1);
    let c = app.cancel_execution(&e.id).await.unwrap();
    assert_eq!(c.status, fakes_store::ExecutionStatus::Cancelled);
    app.shutdown().await;
}

#[tokio::test]
async fn export_and_import_bundles() {
    use crate::bundle::*;

    // Máquina A: el ejemplo + un servicio propio + un archivo.
    let tmp_a = tempfile::tempdir().unwrap();
    let a = app(tmp_a.path()).await;
    let svc: Service = serde_json::from_value(json!({
        "name": "Belvo", "port": null,
        "workflows": [{"name": "Avisar", "steps": [{"type": "wait", "durationMs": 1}]}],
        "endpoints": [{"method": "GET", "path": "/accounts",
            "scenarios": [{"name": "ok", "workflows": [{"workflowId": "avisar"}], "response": {"status": 200}}]}]
    }))
    .unwrap();
    a.create_service(svc).await.unwrap();
    let ws_dir = PathBuf::from(a.status().await.unwrap().workspace_path);
    std::fs::create_dir_all(ws_dir.join("files/contratos")).unwrap();
    std::fs::write(ws_dir.join("files/contratos/c1.pdf"), b"%PDF-1.7").unwrap();

    let only_belvo = a
        .export_bundle(&ExportOptions {
            services: vec!["belvo".into()],
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(only_belvo.services.len(), 1);
    assert!(only_belvo.workspace.is_none() && only_belvo.files.is_empty());

    let full = a
        .export_bundle(&ExportOptions {
            services: vec![],
            include_workspace: true,
            include_files: true,
            include_resources: true,
        })
        .await
        .unwrap();
    assert_eq!(full.format, BUNDLE_FORMAT);
    assert_eq!(full.services.len(), 2);
    assert_eq!(full.files.len(), 1);
    // El bundle es JSON estable y se puede volver a leer.
    let text = serde_json::to_string_pretty(&full).unwrap();
    let full: Bundle = serde_json::from_str(&text).unwrap();
    a.shutdown().await;

    // Máquina B: instalación nueva (ya trae "ejemplo").
    let tmp_b = tempfile::tempdir().unwrap();
    let b = app(tmp_b.path()).await;
    let req = |bundle: &Bundle, on_conflict, dry_run| ImportRequest {
        bundle: bundle.clone(),
        mode: ImportMode::Merge,
        on_conflict,
        workspace_name: None,
        path: None,
        dry_run,
    };

    // Simulación: no escribe nada.
    let r = b
        .import_bundle(req(&full, OnConflict::Rename, true))
        .await
        .unwrap();
    assert!(r.dry_run);
    assert_eq!(b.list_services().await.len(), 1);

    // Omitir conflictos: solo entra Belvo.
    let r = b
        .import_bundle(req(&full, OnConflict::Skip, false))
        .await
        .unwrap();
    assert_eq!(r.created, vec!["belvo"]);
    assert_eq!(r.skipped, vec!["ejemplo"]);
    assert_eq!(r.files, 1);
    let b_dir = PathBuf::from(b.status().await.unwrap().workspace_path);
    assert_eq!(
        std::fs::read(b_dir.join("files/contratos/c1.pdf")).unwrap(),
        b"%PDF-1.7"
    );
    assert_eq!(b.list_workflows("belvo").await.unwrap()[0].used_by.len(), 1);

    // Renombrar: entra una copia con otro id y prefijo.
    let r = b
        .import_bundle(req(&only_belvo, OnConflict::Rename, false))
        .await
        .unwrap();
    assert_eq!(r.renamed[0].from, "belvo");
    assert_eq!(r.renamed[0].to, "belvo-2");
    assert!(r.warnings.iter().any(|w| w.contains("prefijo")));
    let copy = b.get_service("belvo-2").await.unwrap();
    assert_eq!(copy.service.prefix, "/belvo-2");
    assert_eq!(copy.service.workflows.len(), 1);

    // Reemplazar.
    let mut changed = only_belvo.clone();
    changed.services[0].description = "versión nueva".into();
    let r = b
        .import_bundle(req(&changed, OnConflict::Replace, false))
        .await
        .unwrap();
    assert_eq!(r.replaced, vec!["belvo"]);
    assert_eq!(
        b.get_service("belvo").await.unwrap().service.description,
        "versión nueva"
    );

    // Un archivo inválido no escribe nada.
    let mut bad = only_belvo.clone();
    bad.services[0].id = "otro".into();
    bad.services[0].prefix = "/otro".into();
    bad.services[0].endpoints[0].path = "sin-barra".into();
    let before = b.list_services().await.len();
    assert!(matches!(
        b.import_bundle(req(&bad, OnConflict::Rename, false)).await,
        Err(AppError::Validation(_))
    ));
    assert_eq!(b.list_services().await.len(), before);
    let mut wrong = only_belvo.clone();
    wrong.format = "mockoon".into();
    assert!(matches!(
        b.import_bundle(req(&wrong, OnConflict::Rename, false))
            .await,
        Err(AppError::BadRequest(_))
    ));

    // Como espacio nuevo.
    let r = b
        .import_bundle(ImportRequest {
            bundle: full.clone(),
            mode: ImportMode::NewWorkspace,
            on_conflict: OnConflict::Rename,
            workspace_name: Some("Copia de A".into()),
            path: None,
            dry_run: false,
        })
        .await
        .unwrap();
    assert_eq!(r.workspace_id, "copia-de-a");
    assert_eq!(r.created.len(), 2);
    assert_eq!(b.status().await.unwrap().workspace_name, "Copia de A");
    assert_eq!(b.list_workspaces().await.len(), 2);
    b.shutdown().await;
}

async fn send(
    app: &FakesApp,
    method: &str,
    path: &str,
    headers: &str,
    body: &[u8],
) -> (u16, String, Vec<u8>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let addr = app.server().central_addr().await.unwrap();
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    let head = format!(
        "{method} {path} HTTP/1.1\r\nHost: x\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    s.write_all(head.as_bytes()).await.unwrap();
    s.write_all(body).await.unwrap();
    let mut out = Vec::new();
    s.read_to_end(&mut out).await.unwrap();
    let split = out.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8_lossy(&out[..split]).to_string();
    let status = head[9..12].parse().unwrap();
    (status, head, out[split + 4..].to_vec())
}

fn json_body(b: &[u8]) -> Value {
    serde_json::from_slice(b).unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(b)))
}

#[tokio::test]
async fn templates_install_and_persistent_resources() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path()).await;
    let list = app.list_templates().await;
    assert!(list.iter().any(|t| t.id == "mifiel" && !t.installed));
    let r = app
        .install_template("mifiel", Default::default())
        .await
        .unwrap();
    assert_eq!(r.service_id, "mifiel");
    assert!(app.get_service("mifiel").await.unwrap().running);
    assert!(
        app.list_templates()
            .await
            .iter()
            .any(|t| t.id == "mifiel" && t.installed)
    );

    let json_h = "Content-Type: application/json\r\n";
    let (st, _, b) = send(
        &app,
        "POST",
        "/mifiel/documents",
        json_h,
        br#"{"file_name":"contrato.pdf"}"#,
    )
    .await;
    assert_eq!(st, 200, "{}", String::from_utf8_lossy(&b));
    let doc = json_body(&b);
    let id = doc["id"].as_str().unwrap().to_string();
    assert_eq!(doc["file_name"], "contrato.pdf");
    assert_eq!(doc["signed"], false);

    let (st, _, b) = send(&app, "GET", &format!("/mifiel/documents/{id}"), "", b"").await;
    assert_eq!(st, 200);
    assert_eq!(json_body(&b)["file_name"], "contrato.pdf");
    let (st, _, b) = send(&app, "GET", "/mifiel/documents", "", b"").await;
    assert_eq!(st, 200);
    assert_eq!(json_body(&b).as_array().unwrap().len(), 1);
    let (st, _, b) = send(&app, "GET", "/mifiel/documents/nope", "", b"").await;
    assert_eq!(st, 404);
    assert_eq!(json_body(&b)["status"], "error");

    // Casos de uso de recursos
    let cols = app.list_resource_collections("mifiel").await.unwrap();
    assert_eq!(
        (cols[0].collection.as_str(), cols[0].count),
        ("documents", 1)
    );
    let rec = app
        .put_resource("mifiel", "documents", "manual", json!({"state": "signed"}))
        .await
        .unwrap();
    assert_eq!(rec.data, json!({"state": "signed", "id": "manual"}));
    assert_eq!(
        app.list_resources("mifiel", "documents", None, None)
            .await
            .unwrap()
            .len(),
        2
    );
    assert!(matches!(
        app.put_resource("mifiel", "mal nombre", "x", json!({}))
            .await,
        Err(AppError::BadRequest(_))
    ));

    // Exportar e importar con recursos
    let bundle = app
        .export_bundle(&ExportOptions {
            services: vec!["mifiel".into()],
            include_resources: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(bundle.resources["mifiel"]["documents"].len(), 2);
    let (st, _, _) = send(&app, "DELETE", &format!("/mifiel/documents/{id}"), "", b"").await;
    assert_eq!(st, 200);
    let (st, _, _) = send(&app, "DELETE", &format!("/mifiel/documents/{id}"), "", b"").await;
    assert_eq!(st, 404);
    assert_eq!(app.clear_resources("mifiel", None).await.unwrap(), 1);

    // Instalar otra vez renombra; reemplazar conserva el id
    let r2 = app
        .install_template("mifiel", Default::default())
        .await
        .unwrap();
    assert_ne!(r2.service_id, "mifiel");
    app.delete_service(&r2.service_id).await.unwrap();

    // S3: objetos binarios reales
    app.install_template("aws-s3", Default::default())
        .await
        .unwrap();
    let data: Vec<u8> = (0u8..=255).collect();
    let (st, head, b) = send(
        &app,
        "PUT",
        "/s3/fotos/2026/a.bin",
        "Content-Type: image/png\r\n",
        &data,
    )
    .await;
    assert_eq!(st, 200, "{}", String::from_utf8_lossy(&b));
    assert!(head.to_lowercase().contains("etag:"), "{head}");
    let (st, head, b) = send(&app, "GET", "/s3/fotos/2026/a.bin", "", b"").await;
    assert_eq!(st, 200);
    assert!(
        head.to_lowercase().contains("content-type: image/png"),
        "{head}"
    );
    assert_eq!(b, data);
    let (st, _, b) = send(&app, "GET", "/s3/fotos?prefix=2026/", "", b"").await;
    assert_eq!(st, 200, "{}", String::from_utf8_lossy(&b));
    let xml = String::from_utf8_lossy(&b);
    assert!(
        xml.contains("<Key>2026/a.bin</Key>") && xml.contains("<KeyCount>1</KeyCount>"),
        "{xml}"
    );
    let (st, _, b) = send(&app, "GET", "/s3/fotos/otro.txt", "", b"").await;
    assert_eq!(st, 404);
    assert!(String::from_utf8_lossy(&b).contains("NoSuchKey"));
    // Con '/' en el id del recurso
    assert_eq!(
        app.get_resource("aws-s3", "objects", "fotos/2026/a.bin")
            .await
            .unwrap()
            .data["size"],
        256
    );

    // Borrar el servicio borra sus recursos
    app.delete_service("aws-s3").await.unwrap();
    app.install_template("aws-s3", Default::default())
        .await
        .unwrap();
    assert!(
        app.list_resource_collections("aws-s3")
            .await
            .unwrap()
            .is_empty()
    );
    app.shutdown().await;
}

#[tokio::test]
async fn all_templates_install_and_answer() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path()).await;
    for t in app.list_templates().await {
        let r = app
            .install_template(&t.id, Default::default())
            .await
            .unwrap();
        assert!(
            r.report.created.len() + r.report.renamed.len() == 1,
            "{}",
            t.id
        );
    }
    let auth =
        "Authorization: Bearer sk_test\r\nContent-Type: application/x-www-form-urlencoded\r\n";
    // Stripe: form-urlencoded, recursos y tarjeta rechazada
    let (st, _, b) = send(
        &app,
        "POST",
        "/stripe/v1/payment_intents",
        auth,
        b"amount=2000&currency=mxn",
    )
    .await;
    assert_eq!(st, 200, "{}", String::from_utf8_lossy(&b));
    let pi = json_body(&b);
    assert_eq!(pi["amount"], 2000);
    assert_eq!(pi["status"], "requires_confirmation");
    let pid = pi["id"].as_str().unwrap();
    let (st, _, b) = send(
        &app,
        "POST",
        &format!("/stripe/v1/payment_intents/{pid}/confirm"),
        auth,
        b"payment_method=pm_card_chargeDeclined",
    )
    .await;
    assert_eq!(st, 402, "{}", String::from_utf8_lossy(&b));
    let (st, _, b) = send(
        &app,
        "POST",
        &format!("/stripe/v1/payment_intents/{pid}/confirm"),
        auth,
        b"payment_method=pm_card_visa",
    )
    .await;
    assert_eq!(st, 200);
    assert_eq!(json_body(&b)["status"], "succeeded");
    let (st, _, _) = send(
        &app,
        "POST",
        "/stripe/v1/payment_intents",
        auth,
        b"currency=mxn",
    )
    .await;
    assert_eq!(st, 400);
    let (st, _, _) = send(&app, "GET", "/stripe/v1/customers", "", b"").await;
    assert_eq!(st, 401);

    // SendGrid
    let sg = "Authorization: Bearer SG.x\r\nContent-Type: application/json\r\n";
    let (st, head, _) = send(&app, "POST", "/sendgrid/v3/mail/send", sg, br#"{"personalizations":[{"to":[{"email":"a@b.c"}]}],"from":{"email":"x@y.z"},"subject":"Hola"}"#).await;
    assert_eq!(st, 202);
    assert!(head.to_lowercase().contains("x-message-id"));
    let (st, _, _) = send(
        &app,
        "POST",
        "/sendgrid/v3/mail/send",
        sg,
        br#"{"from":{"email":"x@y.z"}}"#,
    )
    .await;
    assert_eq!(st, 400);

    // Belvo
    let bv = "Authorization: Basic aWQ6c2VjcmV0\r\nContent-Type: application/json\r\n";
    let (st, _, b) = send(
        &app,
        "POST",
        "/belvo/api/links/",
        bv,
        br#"{"institution":"erebor_mx_retail","username":"bnk100","password":"full"}"#,
    )
    .await;
    assert_eq!(st, 201, "{}", String::from_utf8_lossy(&b));
    let link = json_body(&b)["id"].as_str().unwrap().to_string();
    let (st, _, b) = send(
        &app,
        "POST",
        "/belvo/api/accounts/",
        bv,
        format!(r#"{{"link":"{link}"}}"#).as_bytes(),
    )
    .await;
    assert_eq!(st, 201);
    assert_eq!(json_body(&b).as_array().unwrap().len(), 2);
    let (st, _, _) = send(
        &app,
        "POST",
        "/belvo/api/accounts/",
        bv,
        br#"{"link":"nope"}"#,
    )
    .await;
    assert_eq!(st, 400);
    let (st, _, b) = send(&app, "GET", "/belvo/api/links/", bv, b"").await;
    assert_eq!(st, 200);
    assert_eq!(json_body(&b)["count"], 1);

    // COPOMEX
    let (st, _, b) = send(
        &app,
        "GET",
        "/copomex/query/info_cp/06000?token=pruebas",
        "",
        b"",
    )
    .await;
    assert_eq!(st, 200);
    assert_eq!(json_body(&b)[0]["response"]["cp"], "06000");
    let (st, _, _) = send(&app, "GET", "/copomex/query/info_cp/06000", "", b"").await;
    assert_eq!(st, 400);
    let (st, _, _) = send(&app, "GET", "/copomex/query/info_cp/abc?token=x", "", b"").await;
    assert_eq!(st, 400);
    app.shutdown().await;
}

#[tokio::test]
async fn mifiel_template_signs_document_and_sends_webhook() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path()).await;
    app.install_template("mifiel", Default::default())
        .await
        .unwrap();
    // Acelerar la espera del workflow para la prueba.
    let mut wf = app
        .get_workflow("mifiel", "procesar-firma")
        .await
        .unwrap()
        .workflow;
    for s in &mut wf.steps {
        if let fakes_domain::Action::Wait { duration_ms } = &mut s.action {
            *duration_ms = 50u64;
        }
    }
    app.update_workflow("mifiel", "procesar-firma", wf)
        .await
        .unwrap();

    let addr = app.server().central_addr().await.unwrap();
    let callback = format!("http://{addr}/mifiel/webhook-receptor");
    for (forced, state) in [
        ("", "signed"),
        ("x-fakes-scenario: rechazado\r\n", "rejected"),
    ] {
        let body = json!({"file_name": "c.pdf", "callback_url": callback}).to_string();
        let (st, head, b) = send(
            &app,
            "POST",
            "/mifiel/documents",
            &format!("Content-Type: application/json\r\n{forced}"),
            body.as_bytes(),
        )
        .await;
        assert_eq!(st, 200);
        let id = json_body(&b)["id"].as_str().unwrap().to_string();
        let exec = head
            .lines()
            .find_map(|l| l.strip_prefix("x-fakes-executions: "))
            .unwrap()
            .trim()
            .to_string();
        let started = std::time::Instant::now();
        let d = loop {
            let d = app.get_execution(&exec).await.unwrap();
            if d.execution.status.is_finished() {
                break d;
            }
            assert!(started.elapsed() < Duration::from_secs(10));
            tokio::time::sleep(Duration::from_millis(30)).await;
        };
        assert_eq!(
            d.execution.status,
            fakes_store::ExecutionStatus::Completed,
            "{:?}",
            d.execution.error
        );
        let rec = app.get_resource("mifiel", "documents", &id).await.unwrap();
        assert_eq!(rec.data["state"], state);
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    let hits = app
        .list_requests(&RequestFilter {
            search: Some("/mifiel/webhook-receptor".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(hits.len(), 2);
    let last = app.get_request(&hits[0].id).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&last.request_body.bytes).unwrap()["state"],
        "rejected"
    );
    app.shutdown().await;
}

#[tokio::test]
async fn call_count_and_probability_scenarios() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path()).await;
    app.create_service(service_input("Avanzado")).await.unwrap();
    app.create_endpoint(
        "avanzado",
        endpoint_input(json!({
            "method": "GET", "path": "/reintento",
            "scenarios": [
                {"name": "Falla las dos primeras", "rules": [{"source": "callCount", "operator": "lessThan", "value": 3}],
                 "response": {"status": 503, "body": {"type": "json", "content": {"n": "{{ request.callNumber }}"}}}},
                {"name": "Siempre (probabilidad 1)", "probability": 1.0,
                 "response": {"status": 200, "body": {"type": "json", "content": {"n": "{{ request.callNumber }}"}}}},
                {"name": "Nunca alcanzado", "response": {"status": 500}}
            ]
        })),
    )
    .await
    .unwrap();
    let mut got = Vec::new();
    for _ in 0..4 {
        let (st, _, b) = send(&app, "GET", "/avanzado/reintento", "", b"").await;
        got.push((st, json_body(&b)["n"].as_u64().unwrap()));
    }
    assert_eq!(got, vec![(503, 1), (503, 2), (200, 3), (200, 4)]);

    // Probabilidad fuera de rango
    let bad = endpoint_input(json!({"method": "GET", "path": "/p",
        "scenarios": [{"name": "x", "probability": 0.0, "response": {"status": 200}}]}));
    assert!(matches!(
        app.create_endpoint("avanzado", bad).await,
        Err(AppError::Validation(_))
    ));

    // Paso `resource` en un workflow manual
    let wf: Workflow = serde_json::from_value(json!({
        "name": "Saldo",
        "steps": [
            {"id": "crear", "type": "resource", "collection": "cuentas", "operation": "upsert", "recordId": "{{ input.cuenta }}", "data": {"saldo": "{{ input.saldo }}"}},
            {"id": "leer", "type": "resource", "collection": "cuentas", "operation": "get", "recordId": "{{ input.cuenta }}"},
            {"id": "falta", "type": "resource", "collection": "cuentas", "operation": "get", "recordId": "otra"},
            {"id": "check", "type": "condition", "conditions": [{"path": "steps.leer.resource.saldo", "operator": "equals", "value": 150}], "onFalse": "error"},
            {"id": "ok", "type": "end"},
            {"id": "error", "type": "fail", "message": "saldo incorrecto"}
        ]
    }))
    .unwrap();
    app.create_workflow("avanzado", wf).await.unwrap();
    let input = [
        ("cuenta".to_string(), json!("c1")),
        ("saldo".to_string(), json!(150)),
    ]
    .into_iter()
    .collect();
    let e = app
        .run_workflow("avanzado", "saldo", input, None)
        .await
        .unwrap();
    let started = std::time::Instant::now();
    let d = loop {
        let d = app.get_execution(&e.id).await.unwrap();
        if d.execution.status.is_finished() {
            break d;
        }
        assert!(started.elapsed() < Duration::from_secs(5));
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(
        d.execution.status,
        fakes_store::ExecutionStatus::Completed,
        "{:?}",
        d.execution.error
    );
    assert_eq!(
        app.get_resource("avanzado", "cuentas", "c1")
            .await
            .unwrap()
            .data,
        json!({"saldo": 150, "id": "c1"})
    );
    app.shutdown().await;
}
