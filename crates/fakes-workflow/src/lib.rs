//! Motor de workflows de Fake Services.
//!
//! Cada ejecución tiene su propio contexto aislado (solicitud original,
//! respuesta, variables, resultados de pasos) y corre en su propia tarea.
//! Su estado se guarda en SQLite después de cada transición, así que si la
//! aplicación se cierra, al volver a abrirla las ejecuciones pendientes se
//! retoman donde quedaron (las esperas respetan la hora original).
//!
//! Semántica de recuperación: un paso que estaba a la mitad cuando se cerró
//! la aplicación se vuelve a ejecutar (al menos una vez).

pub mod http;

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use arc_swap::ArcSwap;
use fakes_domain::workflow::limits;
use fakes_domain::{
    Action, HttpMethod, RuleMode, StepBody, WebhookSettings, Workflow, lookup_path,
    workflow::status_matches,
};
use fakes_matching::eval_operator;
use fakes_response::{render_json, render_text};
use fakes_store::{ExecutionRecord, ExecutionStatus, ExecutionSummary, Store, TriggerKind};
use minijinja::Value as MjValue;
use serde_json::{Map, Value, json};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

pub use http::{HttpError, WebhookClient};

/// Ejecuciones simultáneas máximas (las demás esperan turno).
const MAX_CONCURRENT: usize = 256;
/// Ejecuciones terminadas que se conservan por espacio.
const KEEP_FINISHED: u32 = 2000;
/// Cada cuánto se revisa el estado de un workflow hijo que se espera.
const CHILD_POLL: Duration = Duration::from_millis(150);

// ---------------------------------------------------------------------------
// Catálogo: lo que el motor necesita saber de la configuración vigente
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct ServiceInfo {
    pub id: String,
    pub name: String,
    pub url: String,
    pub running: bool,
    pub vars: BTreeMap<String, String>,
    pub workflows: HashMap<String, Workflow>,
}

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub workspace_id: String,
    pub services: HashMap<String, ServiceInfo>,
    pub webhooks: WebhookSettings,
    /// Headers cuyo valor se oculta en la línea de tiempo.
    pub redact_headers: Vec<String>,
}

/// Recibe los cambios de las ejecuciones (eventos en vivo para la UI).
pub trait ExecutionObserver: Send + Sync + 'static {
    fn changed(&self, execution: &ExecutionSummary);
    fn emitted(&self, execution: &ExecutionSummary, event: &str, data: &Value);
}

pub struct NullObserver;
impl ExecutionObserver for NullObserver {
    fn changed(&self, _: &ExecutionSummary) {}
    fn emitted(&self, _: &ExecutionSummary, _: &str, _: &Value) {}
}

/// Petición para iniciar una ejecución.
#[derive(Debug, Clone)]
pub struct StartRequest {
    pub service_id: String,
    pub workflow_id: String,
    pub trigger: TriggerKind,
    /// Entrada (`{{ input.X }}`). Los textos se renderizan con `scope`.
    pub input: BTreeMap<String, Value>,
    /// Contexto de origen: `request`, `response`, `endpoint`, `scenario`...
    pub scope: Value,
    pub parent: Option<(String, u32)>,
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("no existe el servicio '{0}'")]
    ServiceNotFound(String),
    #[error("el servicio '{service}' no tiene el workflow '{workflow}'")]
    WorkflowNotFound { service: String, workflow: String },
    #[error("el workflow '{0}' está deshabilitado")]
    Disabled(String),
    #[error("se superó la profundidad máxima de workflows anidados ({0})")]
    TooDeep(u32),
    #[error("no existe la ejecución '{0}'")]
    ExecutionNotFound(String),
    #[error("la ejecución ya terminó")]
    AlreadyFinished,
    #[error("plantilla inválida: {0}")]
    Template(String),
    #[error(transparent)]
    Store(#[from] fakes_store::StoreError),
}

// ---------------------------------------------------------------------------
// Motor
// ---------------------------------------------------------------------------

pub struct WorkflowEngine {
    store: Store,
    http: WebhookClient,
    catalog: ArcSwap<Catalog>,
    observer: Arc<dyn ExecutionObserver>,
    tasks: Mutex<HashMap<String, CancellationToken>>,
    cancelled_by_user: Mutex<std::collections::HashSet<String>>,
    semaphore: Arc<Semaphore>,
    shutting_down: AtomicBool,
    schedule: Mutex<Option<CancellationToken>>,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn iso(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_default()
}

/// Resultado de ejecutar un paso.
enum Outcome {
    /// Continuar; `Some(destino)` sobrescribe el salto.
    Next(Option<String>),
    /// Dormir hasta ese momento y volver a evaluar el mismo paso.
    Sleep(i64),
    /// Esperar a una ejecución hija.
    WaitChild(String),
    Complete,
    Fail(String),
}

impl WorkflowEngine {
    pub fn new(store: Store, observer: Arc<dyn ExecutionObserver>) -> Arc<Self> {
        Arc::new(Self {
            store,
            http: WebhookClient::new(),
            catalog: ArcSwap::from_pointee(Catalog::default()),
            observer,
            tasks: Mutex::new(HashMap::new()),
            cancelled_by_user: Mutex::new(Default::default()),
            semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT)),
            shutting_down: AtomicBool::new(false),
            schedule: Mutex::new(None),
        })
    }

    pub fn catalog(&self) -> Arc<Catalog> {
        self.catalog.load_full()
    }

    /// Publica la configuración vigente (workflows, servicios iniciados, política de red).
    pub fn set_catalog(self: &Arc<Self>, catalog: Catalog) {
        self.http.set_policy(catalog.webhooks.clone());
        self.catalog.store(Arc::new(catalog));
        self.ensure_scheduler();
    }

    pub fn active_count(&self) -> usize {
        self.tasks.lock().unwrap().len()
    }

    /// Inicia una ejecución nueva.
    pub async fn start(
        self: &Arc<Self>,
        req: StartRequest,
    ) -> Result<ExecutionSummary, EngineError> {
        let cat = self.catalog.load_full();
        let svc = cat
            .services
            .get(&req.service_id)
            .ok_or_else(|| EngineError::ServiceNotFound(req.service_id.clone()))?;
        let wf =
            svc.workflows
                .get(&req.workflow_id)
                .ok_or_else(|| EngineError::WorkflowNotFound {
                    service: req.service_id.clone(),
                    workflow: req.workflow_id.clone(),
                })?;
        if !wf.enabled {
            return Err(EngineError::Disabled(wf.id.clone()));
        }
        let depth = req.parent.as_ref().map(|(_, d)| d + 1).unwrap_or(0);
        if depth > limits::MAX_DEPTH {
            return Err(EngineError::TooDeep(limits::MAX_DEPTH));
        }

        let id = uuid::Uuid::now_v7().to_string();
        let now = now_ms();

        // Entrada: se renderiza con el contexto de origen.
        let mut scope = req.scope.clone();
        if !scope.is_object() {
            scope = json!({});
        }
        scope["vars"] = json!(svc.vars);
        scope["service"] = json!({"id": svc.id, "name": svc.name, "url": svc.url});
        let scope_mj = MjValue::from_serialize(&scope);
        let mut input = Map::new();
        for (k, v) in &req.input {
            let rendered = render_json(v, &scope_mj, &format!("input.{k}"))
                .map_err(|e| EngineError::Template(e.to_string()))?;
            input.insert(k.clone(), rendered);
        }

        let mut ctx = json!({
            "request": scope.get("request").cloned().unwrap_or(Value::Null),
            "response": scope.get("response").cloned().unwrap_or(Value::Null),
            "endpoint": scope.get("endpoint").cloned().unwrap_or(Value::Null),
            "scenario": scope.get("scenario").cloned().unwrap_or(Value::Null),
            "input": input,
            "vars": svc.vars,
            "steps": {},
            "service": {"id": svc.id, "name": svc.name, "url": svc.url},
            "workflow": {"id": wf.id, "name": wf.name},
            "execution": {
                "id": id,
                "trigger": req.trigger.as_str(),
                "startedAt": iso(now),
                "parentId": req.parent.as_ref().map(|(p, _)| p.clone()),
            },
        });
        // Variables iniciales del workflow (pueden usar input y request).
        for (k, v) in &wf.variables {
            let mj = MjValue::from_serialize(&ctx);
            let rendered = render_json(v, &mj, &format!("variables.{k}"))
                .map_err(|e| EngineError::Template(e.to_string()))?;
            ctx["vars"][k] = rendered;
        }

        let rec = ExecutionRecord {
            id: id.clone(),
            workspace_id: cat.workspace_id.clone(),
            service_id: svc.id.clone(),
            workflow_id: wf.id.clone(),
            workflow_name: wf.name.clone(),
            status: ExecutionStatus::Pending,
            trigger: req.trigger,
            parent_id: req.parent.as_ref().map(|(p, _)| p.clone()),
            depth,
            pc: 0,
            phase: 0,
            attempt: 0,
            steps_executed: 0,
            current_step: wf.steps.first().map(|s| s.id.clone()),
            wake_at_ms: None,
            waiting_for: None,
            context: ctx,
            definition: serde_json::to_value(wf).unwrap_or(Value::Null),
            error: None,
            created_ms: now,
            updated_ms: now,
            finished_ms: None,
        };
        self.store.insert_execution(&rec).await?;
        self.timeline(
            &rec,
            "started",
            None,
            &format!("Inició «{}» ({})", wf.name, trigger_label(req.trigger)),
            json!({"input": rec.context["input"]}),
        )
        .await;
        let summary = ExecutionSummary::from(&rec);
        self.observer.changed(&summary);
        self.spawn(rec);
        Ok(summary)
    }

    /// Cancela una ejecución en curso.
    pub async fn cancel(self: &Arc<Self>, id: &str) -> Result<ExecutionSummary, EngineError> {
        let mut rec = self
            .store
            .execution_status(id)
            .await?
            .ok_or_else(|| EngineError::ExecutionNotFound(id.to_string()))?;
        if rec.status.is_finished() {
            return Err(EngineError::AlreadyFinished);
        }
        let token = self.tasks.lock().unwrap().get(id).cloned();
        match token {
            Some(t) => {
                self.cancelled_by_user
                    .lock()
                    .unwrap()
                    .insert(id.to_string());
                t.cancel();
                // La tarea marca la cancelación; esperamos a que quede registrada.
                for _ in 0..50 {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    if let Some(r) = self.store.execution_status(id).await?
                        && r.status.is_finished()
                    {
                        return Ok(ExecutionSummary::from(&r));
                    }
                }
                let r = self.store.execution_status(id).await?.unwrap_or(rec);
                Ok(ExecutionSummary::from(&r))
            }
            None => {
                // Sin tarea (p. ej. de otro espacio): se marca directamente.
                self.finish(
                    &mut rec,
                    ExecutionStatus::Cancelled,
                    Some("cancelada por el usuario".into()),
                )
                .await;
                Ok(ExecutionSummary::from(&rec))
            }
        }
    }

    /// Retoma las ejecuciones sin terminar del espacio del catálogo actual.
    pub async fn recover(self: &Arc<Self>) -> Result<usize, EngineError> {
        let ws = self.catalog.load().workspace_id.clone();
        let pending = self.store.unfinished_executions(&ws).await?;
        let mut n = 0;
        for rec in pending {
            if self.tasks.lock().unwrap().contains_key(&rec.id) {
                continue;
            }
            let wake = rec
                .wake_at_ms
                .map(|w| format!(" (despierta {})", iso(w)))
                .unwrap_or_default();
            self.timeline(
                &rec,
                "recovered",
                rec.current_step.as_deref(),
                &format!("Retomada tras reiniciar la aplicación{wake}"),
                Value::Null,
            )
            .await;
            self.spawn(rec);
            n += 1;
        }
        Ok(n)
    }

    /// Detiene las tareas sin cambiar su estado: se retoman con `recover`.
    pub async fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        if let Some(t) = self.schedule.lock().unwrap().take() {
            t.cancel();
        }
        let tokens: Vec<CancellationToken> = self.tasks.lock().unwrap().values().cloned().collect();
        for t in tokens {
            t.cancel();
        }
        for _ in 0..100 {
            if self.tasks.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Detiene las tareas del espacio actual sin cambiar su estado (al cambiar de espacio).
    pub async fn suspend_all(&self) {
        let tokens: Vec<CancellationToken> = self.tasks.lock().unwrap().values().cloned().collect();
        self.shutting_down.store(true, Ordering::SeqCst);
        for t in tokens {
            t.cancel();
        }
        for _ in 0..100 {
            if self.tasks.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        self.shutting_down.store(false, Ordering::SeqCst);
    }

    fn spawn(self: &Arc<Self>, rec: ExecutionRecord) {
        let token = CancellationToken::new();
        self.tasks
            .lock()
            .unwrap()
            .insert(rec.id.clone(), token.clone());
        let engine = self.clone();
        tokio::spawn(async move {
            let id = rec.id.clone();
            engine.drive(rec, token).await;
            engine.tasks.lock().unwrap().remove(&id);
            engine.cancelled_by_user.lock().unwrap().remove(&id);
        });
    }

    async fn timeline(
        &self,
        rec: &ExecutionRecord,
        kind: &str,
        step: Option<&str>,
        message: &str,
        data: Value,
    ) {
        if let Err(e) = self
            .store
            .append_timeline(&rec.id, now_ms(), kind, step, message, &data)
            .await
        {
            tracing::warn!("no se pudo guardar la línea de tiempo: {e}");
        }
    }

    async fn persist(&self, rec: &mut ExecutionRecord) {
        rec.updated_ms = now_ms();
        if let Err(e) = self.store.save_execution(rec).await {
            tracing::error!(execution = %rec.id, "no se pudo guardar la ejecución: {e}");
        }
        self.observer.changed(&ExecutionSummary::from(&*rec));
    }

    async fn finish(
        &self,
        rec: &mut ExecutionRecord,
        status: ExecutionStatus,
        error: Option<String>,
    ) {
        rec.status = status;
        rec.error = error.clone();
        rec.wake_at_ms = None;
        rec.waiting_for = None;
        rec.finished_ms = Some(now_ms());
        rec.context["execution"]["status"] = json!(status.as_str());
        let (kind, msg) = match status {
            ExecutionStatus::Completed => ("completed", "Ejecución completada".to_string()),
            ExecutionStatus::Cancelled => ("cancelled", "Ejecución cancelada".to_string()),
            _ => (
                "failed",
                format!("Ejecución fallida: {}", error.unwrap_or_default()),
            ),
        };
        self.persist(rec).await;
        self.timeline(
            rec,
            kind,
            rec.current_step.clone().as_deref(),
            &msg,
            Value::Null,
        )
        .await;
        if let Err(e) = self
            .store
            .prune_executions(&rec.workspace_id, KEEP_FINISHED)
            .await
        {
            tracing::warn!("no se pudieron recortar ejecuciones: {e}");
        }
    }

    /// Bucle principal de una ejecución.
    async fn drive(self: &Arc<Self>, mut rec: ExecutionRecord, token: CancellationToken) {
        let _permit = tokio::select! {
            p = self.semaphore.clone().acquire_owned() => p.ok(),
            _ = token.cancelled() => None,
        };
        let def: Workflow = match serde_json::from_value(rec.definition.clone()) {
            Ok(d) => d,
            Err(e) => {
                self.finish(
                    &mut rec,
                    ExecutionStatus::Failed,
                    Some(format!("definición inválida: {e}")),
                )
                .await;
                return;
            }
        };

        loop {
            if token.is_cancelled() {
                return self.on_cancelled(&mut rec).await;
            }

            // Esperas pendientes (también tras recuperar).
            if let Some(wake) = rec.wake_at_ms {
                let remaining = wake - now_ms();
                if remaining > 0 {
                    if rec.status != ExecutionStatus::Waiting {
                        rec.status = ExecutionStatus::Waiting;
                        self.persist(&mut rec).await;
                    }
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_millis(remaining as u64)) => {}
                        _ = token.cancelled() => return self.on_cancelled(&mut rec).await,
                    }
                }
                rec.wake_at_ms = None;
            }

            // Workflow hijo pendiente.
            if let Some(child_id) = rec.waiting_for.clone() {
                if rec.status != ExecutionStatus::Waiting {
                    rec.status = ExecutionStatus::Waiting;
                    self.persist(&mut rec).await;
                }
                let child = loop {
                    match self.store.execution_status(&child_id).await {
                        Ok(Some(c)) if c.status.is_finished() => break Some(c),
                        Ok(Some(_)) => {}
                        Ok(None) => break None,
                        Err(e) => tracing::warn!("no se pudo consultar la ejecución hija: {e}"),
                    }
                    tokio::select! {
                        _ = tokio::time::sleep(CHILD_POLL) => {}
                        _ = token.cancelled() => return self.on_cancelled(&mut rec).await,
                    }
                };
                rec.waiting_for = None;
                let step_id = def
                    .steps
                    .get(rec.pc as usize)
                    .map(|s| s.id.clone())
                    .unwrap_or_default();
                let result = match &child {
                    Some(c) => json!({
                        "executionId": c.id,
                        "status": c.status.as_str(),
                        "vars": c.context.get("vars").cloned().unwrap_or(Value::Null),
                        "error": c.error,
                    }),
                    None => json!({"executionId": child_id, "status": "missing"}),
                };
                rec.context["steps"][&step_id] = result.clone();
                let ok = child
                    .as_ref()
                    .is_some_and(|c| c.status == ExecutionStatus::Completed);
                self.timeline(
                    &rec,
                    if ok { "step_completed" } else { "step_failed" },
                    Some(&step_id),
                    &format!(
                        "El workflow hijo terminó: {}",
                        child
                            .as_ref()
                            .map(|c| c.status.as_str())
                            .unwrap_or("no encontrado")
                    ),
                    result,
                )
                .await;
                if !ok {
                    let msg = format!("el workflow hijo {child_id} no se completó");
                    self.finish(&mut rec, ExecutionStatus::Failed, Some(msg))
                        .await;
                    return;
                }
                if !self.advance(&mut rec, &def, None).await {
                    return;
                }
                continue;
            }

            if rec.status != ExecutionStatus::Running {
                rec.status = ExecutionStatus::Running;
                self.persist(&mut rec).await;
            }

            if rec.steps_executed >= def.max_steps {
                let msg = format!(
                    "se alcanzó el límite de {} pasos (¿un bucle infinito?)",
                    def.max_steps
                );
                self.finish(&mut rec, ExecutionStatus::Failed, Some(msg))
                    .await;
                return;
            }

            let Some(step) = def.steps.get(rec.pc as usize) else {
                self.finish(&mut rec, ExecutionStatus::Completed, None)
                    .await;
                return;
            };
            rec.current_step = Some(step.id.clone());

            let pc = rec.pc as usize;
            let outcome = tokio::select! {
                o = self.run_step(&mut rec, &def, pc) => Some(o),
                _ = token.cancelled() => None,
            };
            let Some(outcome) = outcome else {
                return self.on_cancelled(&mut rec).await;
            };

            match outcome {
                Outcome::Next(jump) => {
                    if !self.advance(&mut rec, &def, jump).await {
                        return;
                    }
                }
                Outcome::Sleep(until) => {
                    rec.wake_at_ms = Some(until);
                    rec.status = ExecutionStatus::Waiting;
                    self.persist(&mut rec).await;
                }
                Outcome::WaitChild(child) => {
                    rec.waiting_for = Some(child);
                    rec.status = ExecutionStatus::Waiting;
                    self.persist(&mut rec).await;
                }
                Outcome::Complete => {
                    rec.steps_executed += 1;
                    self.finish(&mut rec, ExecutionStatus::Completed, None)
                        .await;
                    return;
                }
                Outcome::Fail(msg) => {
                    rec.steps_executed += 1;
                    self.finish(&mut rec, ExecutionStatus::Failed, Some(msg))
                        .await;
                    return;
                }
            }
        }
    }

    /// Pasa al siguiente paso. Devuelve `false` si la ejecución terminó.
    async fn advance(
        &self,
        rec: &mut ExecutionRecord,
        def: &Workflow,
        jump: Option<String>,
    ) -> bool {
        let pc = rec.pc as usize;
        let target = jump.or_else(|| def.steps.get(pc).and_then(|s| s.next.clone()));
        rec.steps_executed += 1;
        rec.phase = 0;
        rec.attempt = 0;
        match def.resolve_jump(pc, target.as_deref()) {
            Some(next) if next < def.steps.len() => {
                rec.pc = next as u32;
                rec.current_step = Some(def.steps[next].id.clone());
                self.persist(rec).await;
                true
            }
            // Sin más pasos o salto a `end`.
            _ => {
                self.finish(rec, ExecutionStatus::Completed, None).await;
                false
            }
        }
    }

    async fn on_cancelled(&self, rec: &mut ExecutionRecord) {
        let by_user = self.cancelled_by_user.lock().unwrap().contains(&rec.id);
        if by_user || !self.shutting_down.load(Ordering::SeqCst) {
            self.finish(
                rec,
                ExecutionStatus::Cancelled,
                Some("cancelada por el usuario".into()),
            )
            .await;
        } else {
            // Apagado: se conserva el estado para retomarla después.
            self.persist(rec).await;
        }
    }

    fn ctx_mj(rec: &ExecutionRecord) -> MjValue {
        let mut ctx = rec.context.clone();
        ctx["execution"]["status"] = json!(rec.status.as_str());
        // Ojo: no agregar claves con nombres de funciones (`now`, `uuid`...) porque
        // las ocultarían en las plantillas.
        MjValue::from_serialize(&ctx)
    }

    async fn run_step(
        self: &Arc<Self>,
        rec: &mut ExecutionRecord,
        def: &Workflow,
        idx: usize,
    ) -> Outcome {
        let step = &def.steps[idx];
        let sid = step.id.clone();
        match &step.action {
            Action::Wait { duration_ms } => {
                if rec.phase == 0 {
                    rec.phase = 1;
                    let until = now_ms() + *duration_ms as i64;
                    self.timeline(
                        rec,
                        "waiting",
                        Some(&sid),
                        &format!("Esperando {}", human_ms(*duration_ms)),
                        json!({"durationMs": duration_ms, "until": iso(until)}),
                    )
                    .await;
                    return Outcome::Sleep(until);
                }
                rec.context["steps"][&sid] =
                    json!({"status": "completed", "waitedMs": duration_ms});
                self.timeline(
                    rec,
                    "step_completed",
                    Some(&sid),
                    "Espera terminada",
                    Value::Null,
                )
                .await;
                Outcome::Next(None)
            }

            Action::Http(h) => {
                if rec.phase == 0 {
                    rec.phase = 1;
                    if h.delay_ms > 0 {
                        let until = now_ms() + h.delay_ms as i64;
                        self.timeline(
                            rec,
                            "waiting",
                            Some(&sid),
                            &format!("Retraso de {} antes de la solicitud", human_ms(h.delay_ms)),
                            Value::Null,
                        )
                        .await;
                        return Outcome::Sleep(until);
                    }
                }
                self.run_http(rec, &sid, h).await
            }

            Action::Emit { event, data } => {
                let mj = Self::ctx_mj(rec);
                let name = match render_text(event, &mj, "event") {
                    Ok(n) => n,
                    Err(e) => return Outcome::Fail(e.to_string()),
                };
                let payload = match render_json(data, &mj, "data") {
                    Ok(p) => p,
                    Err(e) => return Outcome::Fail(e.to_string()),
                };
                rec.context["steps"][&sid] =
                    json!({"status": "completed", "event": name, "data": payload});
                self.timeline(
                    rec,
                    "event_emitted",
                    Some(&sid),
                    &format!("Evento «{name}»"),
                    payload.clone(),
                )
                .await;
                self.observer
                    .emitted(&ExecutionSummary::from(&*rec), &name, &payload);
                Outcome::Next(None)
            }

            Action::SetVariables { values } => {
                let mut set = Map::new();
                for (k, v) in values {
                    let mj = Self::ctx_mj(rec);
                    match render_json(v, &mj, &format!("values.{k}")) {
                        Ok(val) => {
                            rec.context["vars"][k] = val.clone();
                            set.insert(k.clone(), val);
                        }
                        Err(e) => return Outcome::Fail(e.to_string()),
                    }
                }
                rec.context["steps"][&sid] = json!({"status": "completed", "values": set});
                self.timeline(
                    rec,
                    "variables_set",
                    Some(&sid),
                    &format!(
                        "Variables: {}",
                        set.keys().cloned().collect::<Vec<_>>().join(", ")
                    ),
                    Value::Object(set),
                )
                .await;
                Outcome::Next(None)
            }

            Action::Condition {
                conditions,
                mode,
                on_true,
                on_false,
            } => {
                let mut ctx = rec.context.clone();
                ctx["execution"]["status"] = json!(rec.status.as_str());
                let mut details = Vec::new();
                for c in conditions {
                    let actual = lookup_path(&ctx, &c.path).cloned();
                    let ok = eval_operator(c.operator, actual.as_ref(), c.value.as_ref(), None);
                    details.push(json!({
                        "path": c.path, "operator": c.operator, "expected": c.value,
                        "actual": actual, "result": ok,
                    }));
                }
                let results = details
                    .iter()
                    .map(|d| d["result"].as_bool().unwrap_or(false));
                let passed = match mode {
                    RuleMode::All => results.clone().all(|b| b),
                    RuleMode::Any => results.clone().any(|b| b),
                };
                let target = if passed {
                    on_true.clone()
                } else {
                    on_false.clone()
                };
                rec.context["steps"][&sid] = json!({"status": "completed", "result": passed});
                self.timeline(
                    rec,
                    "condition_evaluated",
                    Some(&sid),
                    &format!(
                        "Condición {} → {}",
                        if passed { "verdadera" } else { "falsa" },
                        target.clone().unwrap_or_else(|| "siguiente paso".into())
                    ),
                    json!({"result": passed, "conditions": details}),
                )
                .await;
                Outcome::Next(Some(target.unwrap_or_else(|| "next".into())))
            }

            Action::RunWorkflow {
                workflow_id,
                input,
                wait,
            } => {
                let scope = rec.context.clone();
                let req = StartRequest {
                    service_id: rec.service_id.clone(),
                    workflow_id: workflow_id.clone(),
                    trigger: TriggerKind::Workflow,
                    input: input.clone(),
                    scope,
                    parent: Some((rec.id.clone(), rec.depth)),
                };
                // Se encapsula para romper la recursión de tipos start → run_step → start.
                let fut: std::pin::Pin<
                    Box<
                        dyn std::future::Future<Output = Result<ExecutionSummary, EngineError>>
                            + Send
                            + '_,
                    >,
                > = Box::pin(self.start(req));
                match fut.await {
                    Ok(child) => {
                        self.timeline(
                            rec,
                            "child_started",
                            Some(&sid),
                            &format!("Inició el workflow «{}»", child.workflow_name),
                            json!({"executionId": child.id}),
                        )
                        .await;
                        if *wait {
                            Outcome::WaitChild(child.id)
                        } else {
                            rec.context["steps"][&sid] =
                                json!({"status": "started", "executionId": child.id});
                            Outcome::Next(None)
                        }
                    }
                    Err(e) => Outcome::Fail(format!("no se pudo iniciar «{workflow_id}»: {e}")),
                }
            }

            Action::Resource {
                collection,
                operation,
                record_id,
                data,
                fail_if_missing,
            } => {
                let mj = Self::ctx_mj(rec);
                let rid = match render_text(record_id, &mj, "recordId") {
                    Ok(v) => v,
                    Err(e) => return Outcome::Fail(e.to_string()),
                };
                let rdata = match data
                    .as_ref()
                    .map(|d| render_json(d, &mj, "data"))
                    .transpose()
                {
                    Ok(v) => v,
                    Err(e) => return Outcome::Fail(e.to_string()),
                };
                let outcome = self
                    .store
                    .resource_apply(
                        &rec.workspace_id,
                        &rec.service_id,
                        collection,
                        resource_op(*operation),
                        Some(rid.as_str()),
                        rdata,
                    )
                    .await;
                let (result, msg, missing) = match outcome {
                    Ok(fakes_store::ResourceOutcome::Record(r)) => (
                        json!({"status": "completed", "found": true, "resource": r}),
                        format!(
                            "{} {collection}/{}",
                            resource_verb(*operation),
                            r["id"].as_str().unwrap_or(&rid)
                        ),
                        false,
                    ),
                    Ok(fakes_store::ResourceOutcome::List(items)) => (
                        json!({"status": "completed", "found": true, "count": items.len(), "resources": items}),
                        format!("{} registros en {collection}", items.len()),
                        false,
                    ),
                    Ok(fakes_store::ResourceOutcome::Missing) => (
                        json!({"status": "completed", "found": false}),
                        format!("{collection}/{rid} no existe"),
                        true,
                    ),
                    Err(e) => return Outcome::Fail(format!("recurso {collection}: {e}")),
                };
                rec.context["steps"][&sid] = result.clone();
                if missing && *fail_if_missing {
                    self.timeline(rec, "step_failed", Some(&sid), &msg, Value::Null)
                        .await;
                    return Outcome::Fail(msg);
                }
                self.timeline(rec, "resource", Some(&sid), &msg, result)
                    .await;
                Outcome::Next(None)
            }

            Action::Fail { message } => {
                let mj = Self::ctx_mj(rec);
                let msg = render_text(message, &mj, "message").unwrap_or_else(|_| message.clone());
                self.timeline(
                    rec,
                    "step_failed",
                    Some(&sid),
                    &format!("Error simulado: {msg}"),
                    Value::Null,
                )
                .await;
                Outcome::Fail(msg)
            }

            Action::End => {
                self.timeline(
                    rec,
                    "step_completed",
                    Some(&sid),
                    "Fin del workflow",
                    Value::Null,
                )
                .await;
                Outcome::Complete
            }
        }
    }

    async fn run_http(
        &self,
        rec: &mut ExecutionRecord,
        sid: &str,
        h: &fakes_domain::HttpAction,
    ) -> Outcome {
        let mj = Self::ctx_mj(rec);
        let req = match build_request(h, &mj) {
            Ok(r) => r,
            Err(e) => {
                self.timeline(rec, "step_failed", Some(sid), &e, Value::Null)
                    .await;
                return Outcome::Fail(e);
            }
        };
        let attempt = rec.attempt + 1;
        let cat = self.catalog.load();
        let shown_headers: Map<String, Value> = req
            .headers
            .iter()
            .map(|(k, v)| {
                let sensitive = cat.redact_headers.iter().any(|r| r.eq_ignore_ascii_case(k));
                (
                    k.clone(),
                    json!(if sensitive {
                        "••••"
                    } else {
                        v.as_str()
                    }),
                )
            })
            .collect();
        let body_preview = req
            .body
            .as_ref()
            .map(|b| String::from_utf8_lossy(&b[..b.len().min(4096)]).into_owned());
        self.timeline(
            rec,
            "http_request",
            Some(sid),
            &format!("{} {} (intento {attempt})", req.method, req.url),
            json!({"method": req.method, "url": req.url, "headers": shown_headers, "body": body_preview, "attempt": attempt}),
        )
        .await;

        let result = self.http.send(&req).await;
        let (ok, retryable, summary, data) = match &result {
            Ok(r) => {
                let ok = status_matches(&h.expected_status, r.status);
                (
                    ok,
                    true,
                    format!("HTTP {} en {} ms", r.status, r.duration_ms),
                    json!({"status": r.status, "headers": r.headers, "body": r.body, "durationMs": r.duration_ms}),
                )
            }
            Err(e) => (
                false,
                e.retryable(),
                e.to_string(),
                json!({"error": e.to_string()}),
            ),
        };

        rec.context["steps"][sid] = json!({
            "status": if ok { "completed" } else { "failed" },
            "attempts": attempt,
            "request": {"method": req.method, "url": req.url},
            "response": result.as_ref().ok().map(|r| json!({"status": r.status, "headers": r.headers, "body": r.body})),
            "error": result.as_ref().err().map(|e| e.to_string()),
        });

        self.timeline(
            rec,
            "http_response",
            Some(sid),
            &if ok {
                summary.clone()
            } else if result.is_ok() {
                format!("{summary}: código no esperado")
            } else {
                summary.clone()
            },
            data,
        )
        .await;

        if ok {
            return Outcome::Next(None);
        }
        if retryable && rec.attempt < h.retry.max_attempts {
            rec.attempt += 1;
            let delay = h.retry.delay_for(rec.attempt);
            self.timeline(
                rec,
                "retry_scheduled",
                Some(sid),
                &format!(
                    "Reintento {}/{} en {}",
                    rec.attempt,
                    h.retry.max_attempts,
                    human_ms(delay)
                ),
                Value::Null,
            )
            .await;
            return Outcome::Sleep(now_ms() + delay as i64);
        }
        let msg = format!("la solicitud de «{sid}» falló: {summary}");
        if h.continue_on_error {
            self.timeline(
                rec,
                "step_failed",
                Some(sid),
                &format!("{msg} (se continúa)"),
                Value::Null,
            )
            .await;
            Outcome::Next(None)
        } else {
            self.timeline(rec, "step_failed", Some(sid), &msg, Value::Null)
                .await;
            Outcome::Fail(msg)
        }
    }

    // -- ejecuciones periódicas -------------------------------------------

    fn ensure_scheduler(self: &Arc<Self>) {
        let mut slot = self.schedule.lock().unwrap();
        if slot.is_some() || self.shutting_down.load(Ordering::SeqCst) {
            return;
        }
        let token = CancellationToken::new();
        *slot = Some(token.clone());
        let engine = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut last: HashMap<(String, String, String), i64> = HashMap::new();
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(250)) => {}
                    _ = token.cancelled() => return,
                }
                let Some(engine) = engine.upgrade() else {
                    return;
                };
                let cat = engine.catalog.load_full();
                let now = now_ms();
                let mut due = Vec::new();
                for svc in cat.services.values().filter(|s| s.running) {
                    for wf in svc.workflows.values().filter(|w| w.enabled) {
                        let Some(sch) = &wf.schedule else { continue };
                        let key = (cat.workspace_id.clone(), svc.id.clone(), wf.id.clone());
                        let started = *last.entry(key.clone()).or_insert(now);
                        if now - started >= sch.every_ms as i64 {
                            last.insert(key, now);
                            due.push((svc.id.clone(), wf.id.clone()));
                        }
                    }
                }
                for (service_id, workflow_id) in due {
                    let req = StartRequest {
                        service_id,
                        workflow_id: workflow_id.clone(),
                        trigger: TriggerKind::Schedule,
                        input: BTreeMap::new(),
                        scope: json!({}),
                        parent: None,
                    };
                    if let Err(e) = engine.start(req).await {
                        tracing::warn!(workflow = %workflow_id, "no se pudo iniciar la ejecución periódica: {e}");
                    }
                }
            }
        });
    }
}

/// Operación del dominio → operación del almacén.
pub fn resource_op(op: fakes_domain::ResourceOp) -> fakes_store::ResourceOperation {
    use fakes_domain::ResourceOp as D;
    use fakes_store::ResourceOperation as S;
    match op {
        D::Create => S::Create,
        D::Upsert => S::Upsert,
        D::Update => S::Update,
        D::Get => S::Get,
        D::Delete => S::Delete,
        D::List => S::List,
    }
}

fn resource_verb(op: fakes_domain::ResourceOp) -> &'static str {
    use fakes_domain::ResourceOp as D;
    match op {
        D::Create => "Creado",
        D::Upsert => "Guardado",
        D::Update => "Actualizado",
        D::Get => "Leído",
        D::Delete => "Borrado",
        D::List => "Listado",
    }
}

fn trigger_label(t: TriggerKind) -> &'static str {
    match t {
        TriggerKind::Manual => "manual",
        TriggerKind::RequestReceived => "solicitud recibida",
        TriggerKind::ResponseSent => "respuesta completada",
        TriggerKind::Schedule => "periódica",
        TriggerKind::Workflow => "desde otro workflow",
    }
}

fn human_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms} ms")
    } else if ms < 60_000 {
        let s = ms as f64 / 1000.0;
        if s.fract() == 0.0 {
            format!("{s:.0} s")
        } else {
            format!("{s:.1} s")
        }
    } else if ms < 3_600_000 {
        format!("{:.1} min", ms as f64 / 60_000.0)
    } else {
        format!("{:.1} h", ms as f64 / 3_600_000.0)
    }
}

/// Construye la solicitud HTTP renderizando plantillas.
fn build_request(
    h: &fakes_domain::HttpAction,
    ctx: &MjValue,
) -> Result<http::OutgoingRequest, String> {
    let mut url = render_text(&h.url, ctx, "url")
        .map_err(|e| e.to_string())?
        .trim()
        .to_string();
    if url.is_empty() {
        return Err("la URL quedó vacía al renderizar la plantilla".into());
    }
    if !h.query.is_empty() {
        let mut ser = form_urlencoded::Serializer::new(String::new());
        for (k, v) in &h.query {
            let val = render_text(v, ctx, &format!("query.{k}")).map_err(|e| e.to_string())?;
            ser.append_pair(k, &val);
        }
        let qs = ser.finish();
        url.push(if url.contains('?') { '&' } else { '?' });
        url.push_str(&qs);
    }
    let mut headers = BTreeMap::new();
    for (k, v) in &h.headers {
        headers.insert(
            k.to_ascii_lowercase(),
            render_text(v, ctx, &format!("headers.{k}")).map_err(|e| e.to_string())?,
        );
    }
    let body = match &h.body {
        StepBody::None => None,
        StepBody::Json { content } => {
            let v = render_json(content, ctx, "body").map_err(|e| e.to_string())?;
            headers
                .entry("content-type".into())
                .or_insert_with(|| "application/json".into());
            Some(serde_json::to_vec(&v).unwrap_or_default())
        }
        StepBody::Text {
            content,
            content_type,
        } => {
            let t = render_text(content, ctx, "body").map_err(|e| e.to_string())?;
            headers.entry("content-type".into()).or_insert_with(|| {
                content_type
                    .clone()
                    .unwrap_or_else(|| "text/plain; charset=utf-8".into())
            });
            Some(t.into_bytes())
        }
        StepBody::Form { fields } => {
            let mut ser = form_urlencoded::Serializer::new(String::new());
            for (k, v) in fields {
                let val = render_text(v, ctx, &format!("body.{k}")).map_err(|e| e.to_string())?;
                ser.append_pair(k, &val);
            }
            headers
                .entry("content-type".into())
                .or_insert_with(|| "application/x-www-form-urlencoded".into());
            Some(ser.finish().into_bytes())
        }
    };
    let method = match h.method {
        HttpMethod::Any => "POST",
        m => m.as_str(),
    };
    Ok(http::OutgoingRequest {
        method: method.to_string(),
        url,
        headers,
        body,
        timeout: Duration::from_millis(h.timeout_ms.clamp(1, limits::MAX_HTTP_TIMEOUT_MS)),
    })
}

#[cfg(test)]
mod tests;
