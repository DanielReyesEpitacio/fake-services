use super::*;

async fn server() -> (Arc<McpServer>, Arc<FakesApp>, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let mut opts = fakes_app::AppOptions::new(tmp.path());
    opts.central_port_override = Some(0);
    let app = FakesApp::start(opts).await.unwrap();
    (McpServer::new(app.clone()), app, tmp)
}

async fn call(s: &McpServer, name: &str, args: Value) -> Value {
    let r = s
        .handle_message(json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": args}}))
        .await
        .unwrap();
    r["result"].clone()
}

#[tokio::test]
async fn protocol_basics() {
    let (s, app, _tmp) = server().await;
    let r = s
        .handle_message(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}}))
        .await
        .unwrap();
    assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(r["result"]["serverInfo"]["name"], "fake-services");
    assert!(r["result"]["capabilities"]["tools"].is_object());

    // Notificación: sin respuesta.
    assert!(
        s.handle_message(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .await
            .is_none()
    );
    // Método desconocido.
    let r = s
        .handle_message(json!({"jsonrpc": "2.0", "id": 2, "method": "nada"}))
        .await
        .unwrap();
    assert_eq!(r["error"]["code"], -32601);
    // Lote.
    let r = s
        .handle_payload(json!([{"jsonrpc": "2.0", "id": 3, "method": "ping"}, {"jsonrpc": "2.0", "method": "notifications/x"}]))
        .await
        .unwrap();
    assert_eq!(r.as_array().unwrap().len(), 1);

    // Todas las herramientas tienen esquemas de objeto y los nombres del documento.
    let r = s
        .handle_message(json!({"jsonrpc": "2.0", "id": 4, "method": "tools/list"}))
        .await
        .unwrap();
    let tools = r["result"]["tools"].as_array().unwrap();
    for t in tools {
        assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
        assert_eq!(t["outputSchema"]["type"], "object", "{}", t["name"]);
    }
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for n in [
        "listar_espacios_trabajo",
        "crear_espacio_trabajo",
        "obtener_espacio_trabajo",
        "listar_servicios",
        "crear_servicio",
        "actualizar_servicio",
        "eliminar_servicio",
        "obtener_servicio",
        "iniciar_servicio",
        "detener_servicio",
        "listar_endpoints",
        "crear_endpoint",
        "actualizar_endpoint",
        "eliminar_endpoint",
        "obtener_endpoint",
        "habilitar_endpoint",
        "deshabilitar_endpoint",
        "listar_workflows",
        "crear_workflow",
        "actualizar_workflow",
        "eliminar_workflow",
        "validar_workflow",
        "ejecutar_workflow",
        "cancelar_workflow",
        "listar_ejecuciones",
        "obtener_ejecucion",
        "obtener_linea_tiempo",
        "importar_openapi",
        "importar_coleccion_postman",
        "validar_configuracion",
        "detectar_integraciones_proyecto",
    ] {
        assert!(names.contains(&n), "falta la herramienta {n}");
    }
    // Entradas estrictas.
    let crear = tools
        .iter()
        .find(|t| t["name"] == "crear_servicio")
        .unwrap();
    assert_eq!(crear["inputSchema"]["additionalProperties"], false);
    app.shutdown().await;
}

#[tokio::test]
async fn agent_builds_and_tests_a_simulation() {
    let (s, app, _tmp) = server().await;
    let r = call(&s, "crear_servicio", json!({"service": {
        "name": "MiFiel",
        "endpoints": [{"method": "POST", "path": "/documents/{id}/sign",
            "scenarios": [{"name": "ok", "response": {"status": 201, "body": {"type": "json", "content": {"id": "{{ request.params.id }}"}}}}]}]
    }})).await;
    assert_eq!(r["isError"], false, "{r}");
    assert_eq!(r["structuredContent"]["service"]["id"], "mifiel");

    let r = call(&s, "probar_endpoint", json!({"serviceId": "mifiel", "method": "POST", "path": "/documents/d7/sign", "body": {"x": 1}})).await;
    assert_eq!(r["structuredContent"]["status"], 201, "{r}");
    assert_eq!(r["structuredContent"]["body"]["id"], "d7");

    // Listas se envuelven en `result`.
    let r = call(&s, "listar_servicios", json!({})).await;
    assert!(r["structuredContent"]["result"].as_array().unwrap().len() >= 2);

    // Errores de validación con detalle legible.
    let r = call(&s, "crear_endpoint", json!({"serviceId": "mifiel", "endpoint": {"method": "GET", "path": "sin-barra", "scenarios": []}})).await;
    assert_eq!(r["isError"], true);
    assert!(r["content"][0]["text"].as_str().unwrap().contains("path"));
    // Campos desconocidos se rechazan.
    let r = call(
        &s,
        "obtener_servicio",
        json!({"serviceId": "mifiel", "extra": 1}),
    )
    .await;
    assert_eq!(r["isError"], true);

    let r = call(&s, "actualizar_endpoint", json!({"serviceId": "mifiel", "endpointId": "post-documents-id-sign", "changes": {"name": "Firmar"}})).await;
    assert_eq!(r["structuredContent"]["endpoint"]["name"], "Firmar");

    let r = call(&s, "validar_configuracion", json!({})).await;
    assert_eq!(r["structuredContent"]["valid"], true, "{r}");
    let r = call(&s, "obtener_esquema", json!({"kind": "workflow"})).await;
    assert!(r["structuredContent"]["result"]["properties"]["steps"].is_object());
    app.shutdown().await;
}
