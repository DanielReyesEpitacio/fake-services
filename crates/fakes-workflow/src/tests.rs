use std::sync::atomic::AtomicUsize;
use std::time::Instant;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;

use super::*;

#[derive(Clone, Default)]
struct Receiver {
    calls: Arc<Mutex<Vec<(String, Value)>>>,
    flaky_failures: Arc<AtomicUsize>,
}

async fn record(State(r): State<Receiver>, uri: axum::http::Uri, body: String) -> StatusCode {
    let v = serde_json::from_str(&body).unwrap_or(Value::String(body));
    r.calls.lock().unwrap().push((uri.path().to_string(), v));
    StatusCode::OK
}

async fn flaky(State(r): State<Receiver>, body: String) -> StatusCode {
    if r.flaky_failures.load(Ordering::SeqCst) > 0 {
        r.flaky_failures.fetch_sub(1, Ordering::SeqCst);
        return StatusCode::INTERNAL_SERVER_ERROR;
    }
    r.calls.lock().unwrap().push(("/flaky".into(), json!(body)));
    StatusCode::OK
}

async fn receiver() -> (String, Receiver) {
    let r = Receiver::default();
    let app = Router::new()
        .route("/hook", post(record))
        .route("/hook/{n}", post(record))
        .route("/flaky", post(flaky))
        .with_state(r.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}"), r)
}

#[derive(Default)]
struct Recorder {
    events: Mutex<Vec<(String, Value)>>,
}
impl ExecutionObserver for Recorder {
    fn changed(&self, _: &ExecutionSummary) {}
    fn emitted(&self, _: &ExecutionSummary, event: &str, data: &Value) {
        self.events
            .lock()
            .unwrap()
            .push((event.to_string(), data.clone()));
    }
}

fn catalog(workflows: Vec<Value>) -> Catalog {
    let mut wfs = HashMap::new();
    for w in workflows {
        let wf: Workflow = serde_json::from_value(w).unwrap();
        let errs = fakes_domain::validate::validate_workflow(&wf, &[]);
        assert!(errs.is_empty(), "workflow inválido: {errs:?}");
        wfs.insert(wf.id.clone(), wf);
    }
    let mut services = HashMap::new();
    services.insert(
        "mifiel".to_string(),
        ServiceInfo {
            id: "mifiel".into(),
            name: "MiFiel".into(),
            url: "http://127.0.0.1:4000/mifiel".into(),
            running: true,
            vars: BTreeMap::from([("TOKEN".to_string(), "secreto".to_string())]),
            workflows: wfs,
        },
    );
    Catalog {
        workspace_id: "ws".into(),
        services,
        webhooks: WebhookSettings::default(),
        redact_headers: vec!["authorization".into()],
    }
}

async fn engine_with(store: Store, cat: Catalog) -> (Arc<WorkflowEngine>, Arc<Recorder>) {
    let obs = Arc::new(Recorder::default());
    let e = WorkflowEngine::new(store, obs.clone());
    e.set_catalog(cat);
    (e, obs)
}

fn start_req(wf: &str, scope: Value, input: BTreeMap<String, Value>) -> StartRequest {
    StartRequest {
        service_id: "mifiel".into(),
        workflow_id: wf.into(),
        trigger: TriggerKind::Manual,
        input,
        scope,
        parent: None,
    }
}

async fn wait_finished(store: &Store, id: &str) -> ExecutionRecord {
    let started = Instant::now();
    loop {
        let r = store.execution_status(id).await.unwrap().unwrap();
        if r.status.is_finished() {
            return r;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "la ejecución no terminó: {r:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn mifiel_signature_flow() {
    let (base, recv) = receiver().await;
    let store = Store::in_memory().await.unwrap();
    let (engine, _) = engine_with(
        store.clone(),
        catalog(vec![json!({
            "id": "notificar-firma", "name": "Notificar firma",
            "steps": [
                {"id": "espera-1", "type": "wait", "durationMs": 50},
                {"id": "pendiente", "type": "http", "method": "POST", "url": "{{ request.body.callback_url }}",
                 "headers": {"Authorization": "Bearer {{ vars.TOKEN }}"},
                 "body": {"type": "json", "content": {"document_id": "{{ response.body.id }}", "status": "pending"}}},
                {"id": "espera-2", "type": "wait", "durationMs": 100},
                {"id": "firmado", "type": "http", "method": "POST", "url": "{{ request.body.callback_url }}",
                 "body": {"type": "json", "content": {"document_id": "{{ response.body.id }}", "status": "signed",
                                                       "previous": "{{ steps.pendiente.response.status }}",
                                                       "signed_at": "{{ now('%Y') }}", "nonce": "{{ uuid() | length }}"}}},
                {"id": "fin", "type": "end"}
            ]
        })]),
    )
    .await;

    let scope = json!({
        "request": {"body": {"callback_url": format!("{base}/hook")}},
        "response": {"status": 201, "body": {"id": "doc_123", "status": "pending"}},
    });
    let started = Instant::now();
    let exec = engine
        .start(start_req("notificar-firma", scope, BTreeMap::new()))
        .await
        .unwrap();
    let done = wait_finished(&store, &exec.id).await;
    assert_eq!(done.status, ExecutionStatus::Completed, "{:?}", done.error);
    assert!(started.elapsed() >= Duration::from_millis(150));

    let calls = recv.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[0].1,
        json!({"document_id": "doc_123", "status": "pending"})
    );
    // Las funciones de plantilla (now, uuid) siguen disponibles en los workflows.
    let year = chrono::Utc::now().format("%Y").to_string();
    assert_eq!(
        calls[1].1,
        json!({"document_id": "doc_123", "status": "signed", "previous": 200, "signed_at": year, "nonce": 36})
    );

    let tl = store.timeline(&exec.id).await.unwrap();
    let kinds: Vec<&str> = tl.iter().map(|t| t.kind.as_str()).collect();
    assert_eq!(kinds.first(), Some(&"started"));
    assert_eq!(kinds.last(), Some(&"completed"));
    assert_eq!(kinds.iter().filter(|k| **k == "http_request").count(), 2);
    // El header sensible no queda en la línea de tiempo.
    let req = tl.iter().find(|t| t.kind == "http_request").unwrap();
    assert_eq!(req.data["headers"]["authorization"], "••••");
}

#[tokio::test]
async fn retries_with_backoff_then_success() {
    let (base, recv) = receiver().await;
    recv.flaky_failures.store(2, Ordering::SeqCst);
    let store = Store::in_memory().await.unwrap();
    let (engine, _) = engine_with(
        store.clone(),
        catalog(vec![json!({
            "id": "w", "name": "W",
            "steps": [{"id": "hook", "type": "http", "method": "POST", "url": format!("{base}/flaky"),
                       "retry": {"maxAttempts": 3, "backoffMs": 20, "multiplier": 1.5}}]
        })]),
    )
    .await;
    let exec = engine
        .start(start_req("w", json!({}), BTreeMap::new()))
        .await
        .unwrap();
    let done = wait_finished(&store, &exec.id).await;
    assert_eq!(done.status, ExecutionStatus::Completed, "{:?}", done.error);
    assert_eq!(done.context["steps"]["hook"]["attempts"], 3);
    let tl = store.timeline(&exec.id).await.unwrap();
    assert_eq!(tl.iter().filter(|t| t.kind == "retry_scheduled").count(), 2);
    assert_eq!(recv.calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn retries_exhausted_and_continue_on_error() {
    let (base, recv) = receiver().await;
    recv.flaky_failures.store(100, Ordering::SeqCst);
    let store = Store::in_memory().await.unwrap();
    let (engine, _) = engine_with(
        store.clone(),
        catalog(vec![
            json!({"id": "falla", "name": "Falla", "steps": [
                {"id": "hook", "type": "http", "method": "POST", "url": format!("{base}/flaky"),
                 "retry": {"maxAttempts": 1, "backoffMs": 10}}]}),
            json!({"id": "sigue", "name": "Sigue", "steps": [
                {"id": "hook", "type": "http", "method": "POST", "url": format!("{base}/flaky"), "continueOnError": true},
                {"id": "marca", "type": "setVariables", "values": {"llego": true}}]}),
            json!({"id": "esperado", "name": "Esperado", "steps": [
                {"id": "hook", "type": "http", "method": "POST", "url": format!("{base}/flaky"), "expectedStatus": ["5xx"]}]}),
        ]),
    )
    .await;
    let a = engine
        .start(start_req("falla", json!({}), BTreeMap::new()))
        .await
        .unwrap();
    let a = wait_finished(&store, &a.id).await;
    assert_eq!(a.status, ExecutionStatus::Failed);
    assert!(a.error.unwrap().contains("500"));
    assert_eq!(a.context["steps"]["hook"]["attempts"], 2);

    let b = engine
        .start(start_req("sigue", json!({}), BTreeMap::new()))
        .await
        .unwrap();
    let b = wait_finished(&store, &b.id).await;
    assert_eq!(b.status, ExecutionStatus::Completed);
    assert_eq!(b.context["vars"]["llego"], true);

    let c = engine
        .start(start_req("esperado", json!({}), BTreeMap::new()))
        .await
        .unwrap();
    assert_eq!(
        wait_finished(&store, &c.id).await.status,
        ExecutionStatus::Completed
    );
}

#[tokio::test]
async fn conditions_variables_loops_and_limits() {
    let store = Store::in_memory().await.unwrap();
    let (engine, obs) = engine_with(
        store.clone(),
        catalog(vec![
            json!({"id": "contar", "name": "Contar", "variables": {"n": 0, "meta": "{{ input.meta }}"}, "steps": [
                {"id": "sumar", "type": "setVariables", "values": {"n": "{{ vars.n + 1 }}"}},
                {"id": "avisar", "type": "emit", "event": "vuelta-{{ vars.n }}", "data": {"n": "{{ vars.n }}"}},
                {"id": "seguir", "type": "condition",
                 "conditions": [{"path": "vars.n", "operator": "lessThan", "value": 1}],
                 "onTrue": "sumar", "onFalse": "end"}
            ]}),
            json!({"id": "infinito", "name": "Infinito", "maxSteps": 20, "steps": [
                {"id": "otra-vez", "type": "condition",
                 "conditions": [{"path": "execution.id", "operator": "exists"}], "onTrue": "otra-vez"}
            ]}),
            json!({"id": "error", "name": "Error", "steps": [
                {"id": "x", "type": "fail", "message": "documento {{ input.doc }} expirado"}
            ]}),
        ]),
    )
    .await;

    // n = 1 tras la primera vuelta → la condición `n < 1` es falsa y termina.
    let mut input = BTreeMap::new();
    input.insert("meta".to_string(), json!(3));
    let e = engine
        .start(start_req("contar", json!({}), input))
        .await
        .unwrap();
    let r = wait_finished(&store, &e.id).await;
    assert_eq!(r.status, ExecutionStatus::Completed);
    assert_eq!(r.context["vars"]["n"], 1);
    assert_eq!(r.context["vars"]["meta"], 3);
    assert_eq!(
        obs.events.lock().unwrap()[0],
        ("vuelta-1".to_string(), json!({"n": 1}))
    );

    let e = engine
        .start(start_req("infinito", json!({}), BTreeMap::new()))
        .await
        .unwrap();
    let r = wait_finished(&store, &e.id).await;
    assert_eq!(r.status, ExecutionStatus::Failed);
    assert!(r.error.unwrap().contains("límite de 20 pasos"));

    let mut input = BTreeMap::new();
    input.insert("doc".to_string(), json!("doc_9"));
    let e = engine
        .start(start_req("error", json!({}), input))
        .await
        .unwrap();
    let r = wait_finished(&store, &e.id).await;
    assert_eq!(r.status, ExecutionStatus::Failed);
    assert_eq!(r.error.as_deref(), Some("documento doc_9 expirado"));
}

#[tokio::test]
async fn loop_until_condition() {
    let store = Store::in_memory().await.unwrap();
    let (engine, _) = engine_with(
        store.clone(),
        catalog(vec![
            json!({"id": "c", "name": "C", "variables": {"n": 0}, "steps": [
                {"id": "sumar", "type": "setVariables", "values": {"n": "{{ vars.n + 1 }}"}},
                {"id": "seguir", "type": "condition",
                 "conditions": [{"path": "vars.n", "operator": "lessThan", "value": 3}],
                 "onTrue": "sumar", "onFalse": "end"}
            ]}),
        ]),
    )
    .await;
    let e = engine
        .start(start_req("c", json!({}), BTreeMap::new()))
        .await
        .unwrap();
    let r = wait_finished(&store, &e.id).await;
    assert_eq!(r.status, ExecutionStatus::Completed);
    assert_eq!(r.context["vars"]["n"], 3);
    assert_eq!(r.steps_executed, 6);
}

#[tokio::test]
async fn child_workflows() {
    let store = Store::in_memory().await.unwrap();
    let (engine, _) = engine_with(
        store.clone(),
        catalog(vec![
            json!({"id": "padre", "name": "Padre", "steps": [
                {"id": "hijo", "type": "runWorkflow", "workflowId": "hijo", "input": {"doc": "{{ input.doc }}"}, "wait": true},
                {"id": "copiar", "type": "setVariables", "values": {"resultado": "{{ steps.hijo.vars.salida }}"}}
            ]}),
            json!({"id": "hijo", "name": "Hijo", "steps": [
                {"id": "espera", "type": "wait", "durationMs": 30},
                {"id": "salida", "type": "setVariables", "values": {"salida": "firmado-{{ input.doc }}"}}
            ]}),
        ]),
    )
    .await;
    let mut input = BTreeMap::new();
    input.insert("doc".to_string(), json!("d1"));
    let e = engine
        .start(start_req("padre", json!({}), input))
        .await
        .unwrap();
    let r = wait_finished(&store, &e.id).await;
    assert_eq!(r.status, ExecutionStatus::Completed, "{:?}", r.error);
    assert_eq!(r.context["vars"]["resultado"], "firmado-d1");
    let children = store
        .list_executions(
            "ws",
            &fakes_store::ExecutionFilter {
                workflow_id: Some("hijo".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(children[0].parent_id.as_deref(), Some(e.id.as_str()));
}

#[tokio::test]
async fn cancel_running_execution() {
    let store = Store::in_memory().await.unwrap();
    let (engine, _) = engine_with(
        store.clone(),
        catalog(vec![json!({"id": "larga", "name": "Larga", "steps": [
            {"id": "espera", "type": "wait", "durationMs": 60000}
        ]})]),
    )
    .await;
    let e = engine
        .start(start_req("larga", json!({}), BTreeMap::new()))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let s = engine.cancel(&e.id).await.unwrap();
    assert_eq!(s.status, ExecutionStatus::Cancelled);
    assert!(matches!(
        engine.cancel(&e.id).await,
        Err(EngineError::AlreadyFinished)
    ));
    assert_eq!(engine.active_count(), 0);
}

#[tokio::test]
async fn recovers_after_restart() {
    let (base, recv) = receiver().await;
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("fakes.db");
    let wf = json!({"id": "w", "name": "W", "steps": [
        {"id": "espera", "type": "wait", "durationMs": 400},
        {"id": "hook", "type": "http", "method": "POST", "url": format!("{base}/hook"),
         "body": {"type": "json", "content": {"doc": "{{ input.doc }}"}}}
    ]});

    let store1 = Store::open(&db).await.unwrap();
    let (engine1, _) = engine_with(store1.clone(), catalog(vec![wf.clone()])).await;
    let mut input = BTreeMap::new();
    input.insert("doc".to_string(), json!("d7"));
    let e = engine1
        .start(start_req("w", json!({}), input))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    engine1.shutdown().await;
    let mid = store1.execution_status(&e.id).await.unwrap().unwrap();
    assert_eq!(
        mid.status,
        ExecutionStatus::Waiting,
        "debe quedar esperando"
    );
    assert!(mid.wake_at_ms.is_some());
    drop(engine1);
    drop(store1);
    assert!(recv.calls.lock().unwrap().is_empty());

    // "Reinicio": nuevo motor con la misma base de datos.
    let store2 = Store::open(&db).await.unwrap();
    let (engine2, _) = engine_with(store2.clone(), catalog(vec![wf])).await;
    assert_eq!(engine2.recover().await.unwrap(), 1);
    let r = wait_finished(&store2, &e.id).await;
    assert_eq!(r.status, ExecutionStatus::Completed, "{:?}", r.error);
    assert_eq!(recv.calls.lock().unwrap()[0].1, json!({"doc": "d7"}));
    let kinds: Vec<String> = store2
        .timeline(&e.id)
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.kind)
        .collect();
    assert!(kinds.contains(&"recovered".to_string()));
}

#[tokio::test]
async fn concurrent_executions_are_isolated() {
    let (base, recv) = receiver().await;
    let store = Store::in_memory().await.unwrap();
    let (engine, _) = engine_with(
        store.clone(),
        catalog(vec![json!({"id": "w", "name": "W", "steps": [
            {"id": "doble", "type": "setVariables", "values": {"x": "{{ input.n * 2 }}"}},
            {"id": "espera", "type": "wait", "durationMs": 20},
            {"id": "hook", "type": "http", "method": "POST", "url": format!("{base}/hook/{{{{ input.n }}}}"),
             "body": {"type": "json", "content": {"n": "{{ input.n }}", "x": "{{ vars.x }}"}}}
        ]})]),
    )
    .await;
    let mut ids = Vec::new();
    for n in 0..30 {
        let mut input = BTreeMap::new();
        input.insert("n".to_string(), json!(n));
        ids.push(
            engine
                .start(start_req("w", json!({}), input))
                .await
                .unwrap()
                .id,
        );
    }
    for id in &ids {
        assert_eq!(
            wait_finished(&store, id).await.status,
            ExecutionStatus::Completed
        );
    }
    let calls = recv.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 30);
    for (path, body) in calls {
        let n = body["n"].as_i64().unwrap();
        assert_eq!(body["x"], n * 2);
        assert_eq!(path, format!("/hook/{n}"));
    }
}

#[tokio::test]
async fn ssrf_targets_fail_without_retries() {
    let store = Store::in_memory().await.unwrap();
    let (engine, _) = engine_with(
        store.clone(),
        catalog(vec![json!({"id": "w", "name": "W", "steps": [
            {"id": "hook", "type": "http", "method": "GET", "url": "{{ input.url }}", "retry": {"maxAttempts": 5, "backoffMs": 10}}
        ]})]),
    )
    .await;
    for url in [
        "http://169.254.169.254/latest/meta-data/",
        "http://10.0.0.7/",
        "http://[::ffff:a9fe:a9fe]/",
    ] {
        let mut input = BTreeMap::new();
        input.insert("url".to_string(), json!(url));
        let e = engine
            .start(start_req("w", json!({}), input))
            .await
            .unwrap();
        let r = wait_finished(&store, &e.id).await;
        assert_eq!(r.status, ExecutionStatus::Failed, "{url}");
        assert!(
            r.error.as_deref().unwrap().contains("bloqueado"),
            "{url}: {:?}",
            r.error
        );
        assert_eq!(r.context["steps"]["hook"]["attempts"], 1, "{url}");
    }
}

#[tokio::test]
async fn scheduled_workflows_run_periodically() {
    let store = Store::in_memory().await.unwrap();
    let (engine, _) = engine_with(
        store.clone(),
        catalog(vec![
            json!({"id": "tick", "name": "Tick", "schedule": {"everyMs": 1000}, "steps": [
                {"id": "x", "type": "setVariables", "values": {"ok": true}}
            ]}),
        ]),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(1400)).await;
    let list = store
        .list_executions("ws", &fakes_store::ExecutionFilter::default())
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].trigger, TriggerKind::Schedule);
    engine.shutdown().await;
}

#[tokio::test]
async fn start_errors() {
    let store = Store::in_memory().await.unwrap();
    let (engine, _) = engine_with(
        store.clone(),
        catalog(vec![
            json!({"id": "off", "name": "Off", "enabled": false, "steps": []}),
        ]),
    )
    .await;
    assert!(matches!(
        engine
            .start(start_req("nope", json!({}), BTreeMap::new()))
            .await,
        Err(EngineError::WorkflowNotFound { .. })
    ));
    assert!(matches!(
        engine
            .start(start_req("off", json!({}), BTreeMap::new()))
            .await,
        Err(EngineError::Disabled(_))
    ));
}

#[tokio::test]
async fn switch_cases_and_error_exits() {
    let (base, recv) = receiver().await;
    recv.flaky_failures.store(100, Ordering::SeqCst);
    let store = Store::in_memory().await.unwrap();
    let (engine, _) = engine_with(
        store.clone(),
        catalog(vec![
            json!({"id": "dividir", "name": "Dividir", "steps": [
                {"id": "segun", "type": "switch", "cases": [
                    {"label": "Alto", "conditions": [{"path": "input.monto", "operator": "greaterThan", "value": 1000}], "next": "alto"},
                    {"label": "Medio", "conditions": [{"path": "input.monto", "operator": "greaterThan", "value": 100}], "next": "medio"}
                 ], "otherwise": "bajo"},
                {"id": "alto", "type": "setVariables", "values": {"rama": "alto"}, "next": "end"},
                {"id": "medio", "type": "setVariables", "values": {"rama": "medio"}, "next": "end"},
                {"id": "bajo", "type": "setVariables", "values": {"rama": "bajo"}}]}),
            json!({"id": "rescate", "name": "Rescate", "steps": [
                {"id": "hook", "type": "http", "method": "POST", "url": format!("{base}/flaky"), "onError": "fallback"},
                {"id": "ok", "type": "setVariables", "values": {"rama": "ok"}, "next": "end"},
                {"id": "fallback", "type": "setVariables", "values": {"rama": "fallback", "error": "{{ steps.hook.error }}"}}]}),
            json!({"id": "recurso", "name": "Recurso", "steps": [
                {"id": "leer", "type": "resource", "collection": "docs", "operation": "get", "recordId": "x",
                 "failIfMissing": true, "onError": "end"},
                {"id": "nunca", "type": "fail", "message": "no debió llegar"}]}),
        ]),
    )
    .await;

    for (monto, rama) in [(5000, "alto"), (500, "medio"), (5, "bajo")] {
        let e = engine
            .start(start_req(
                "dividir",
                json!({}),
                BTreeMap::from([("monto".to_string(), json!(monto))]),
            ))
            .await
            .unwrap();
        let e = wait_finished(&store, &e.id).await;
        assert_eq!(e.status, ExecutionStatus::Completed);
        assert_eq!(e.context["vars"]["rama"], rama, "monto {monto}");
    }

    let e = engine
        .start(start_req("rescate", json!({}), BTreeMap::new()))
        .await
        .unwrap();
    let e = wait_finished(&store, &e.id).await;
    assert_eq!(e.status, ExecutionStatus::Completed, "{:?}", e.error);
    assert_eq!(e.context["vars"]["rama"], "fallback");
    assert!(e.context["vars"]["error"].as_str().unwrap().contains("500"));
    assert_eq!(e.context["steps"]["hook"]["status"], "failed");

    let e = engine
        .start(start_req("recurso", json!({}), BTreeMap::new()))
        .await
        .unwrap();
    let e = wait_finished(&store, &e.id).await;
    assert_eq!(e.status, ExecutionStatus::Completed, "{:?}", e.error);
    assert_eq!(e.context["steps"]["leer"]["found"], false);
}
