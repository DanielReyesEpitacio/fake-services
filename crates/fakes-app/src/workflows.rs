//! Casos de uso de workflows y ejecuciones, y el puente con el motor.

use std::collections::{BTreeMap, HashMap};
use std::sync::Weak;

use fakes_domain::validate::validate_workflow;
use fakes_domain::{Action, Issue, Service, StartWhen, StepBody, Workflow, slugify, unique_id};
use fakes_server::{StartFuture, WorkflowStarter, WorkflowTrigger};
use fakes_store::{ExecutionFilter, ExecutionSummary, TriggerKind};
use fakes_workflow::{
    Catalog, EngineError, ExecutionObserver, ServiceInfo, StartRequest, WorkflowEngine,
};
use serde_json::{Value, json};
use tokio::sync::broadcast;

use crate::views::{
    ExecutionDetail, ValidationReport, WorkflowSummary, WorkflowUsage, WorkflowView,
};
use crate::{
    Active, AppError, AppEvent, AppResult, FakesApp, find_service, find_service_mut, service_url,
};

// ---------------------------------------------------------------------------
// Puente con el motor
// ---------------------------------------------------------------------------

/// Reenvía los cambios de ejecuciones como eventos en vivo.
pub(crate) struct EventsObserver(pub broadcast::Sender<AppEvent>);

impl ExecutionObserver for EventsObserver {
    fn changed(&self, e: &ExecutionSummary) {
        let _ = self.0.send(AppEvent::Execution(e.clone()));
    }

    fn emitted(&self, e: &ExecutionSummary, event: &str, data: &Value) {
        let _ = self.0.send(AppEvent::WorkflowEvent {
            execution_id: e.id.clone(),
            workflow_id: e.workflow_id.clone(),
            service_id: e.service_id.clone(),
            event: event.to_string(),
            data: data.clone(),
        });
    }
}

/// Permite al servidor de simulación iniciar workflows sin conocer el motor.
pub(crate) struct EngineStarter(pub Weak<WorkflowEngine>);

impl WorkflowStarter for EngineStarter {
    fn start(&self, t: WorkflowTrigger) -> StartFuture {
        let engine = self.0.upgrade();
        Box::pin(async move {
            let engine =
                engine.ok_or_else(|| "el motor de workflows no está disponible".to_string())?;
            let trigger = match t.when {
                StartWhen::RequestReceived => TriggerKind::RequestReceived,
                StartWhen::ResponseSent => TriggerKind::ResponseSent,
            };
            engine
                .start(StartRequest {
                    service_id: t.service_id,
                    workflow_id: t.workflow_id,
                    trigger,
                    input: t.input,
                    scope: t.scope,
                    parent: None,
                })
                .await
                .map(|s| s.id)
                .map_err(|e| e.to_string())
        })
    }
}

/// Lo que el motor necesita de la configuración activa.
pub(crate) fn build_catalog(a: &Active) -> Catalog {
    let services = a
        .services
        .iter()
        .map(|s| {
            let mut vars = a.workspace.variables.clone();
            vars.extend(s.variables.clone());
            (
                s.id.clone(),
                ServiceInfo {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    url: service_url(a, s),
                    running: a.running.contains(&s.id),
                    vars,
                    workflows: s
                        .workflows
                        .iter()
                        .map(|w| (w.id.clone(), w.clone()))
                        .collect::<HashMap<_, _>>(),
                },
            )
        })
        .collect();
    Catalog {
        workspace_id: a.workspace.id.clone(),
        services,
        webhooks: a.workspace.webhooks.clone(),
        redact_headers: a.workspace.history.redact_headers.clone(),
    }
}

fn engine_err(e: EngineError) -> AppError {
    match e {
        EngineError::ServiceNotFound(_)
        | EngineError::WorkflowNotFound { .. }
        | EngineError::ExecutionNotFound(_) => AppError::NotFound(e.to_string()),
        EngineError::Disabled(_) | EngineError::AlreadyFinished | EngineError::TooDeep(_) => {
            AppError::Conflict(e.to_string())
        }
        EngineError::Template(m) => AppError::BadRequest(format!("plantilla inválida: {m}")),
        EngineError::Store(s) => AppError::Store(s),
    }
}

// ---------------------------------------------------------------------------
// Normalización y validación
// ---------------------------------------------------------------------------

/// Completa identificadores faltantes del workflow y de sus pasos.
pub(crate) fn normalize_workflow(wf: &mut Workflow, siblings: &[Workflow]) {
    wf.schema_version = fakes_domain::SCHEMA_VERSION;
    if wf.id.trim().is_empty() {
        wf.id = unique_id(&wf.name, siblings.iter().map(|w| w.id.as_str()));
    }
    let mut used: Vec<String> = wf
        .steps
        .iter()
        .filter(|s| !s.id.trim().is_empty())
        .map(|s| s.id.clone())
        .collect();
    for step in wf.steps.iter_mut() {
        if step.id.trim().is_empty() {
            let base = if step.name.trim().is_empty() {
                step.action.kind().to_string()
            } else {
                slugify(&step.name)
            };
            step.id = unique_id(&base, used.iter().map(String::as_str));
            used.push(step.id.clone());
        }
    }
}

/// Validación estructural + plantillas + referencias a otros workflows.
pub(crate) fn full_workflow_issues(
    wf: &Workflow,
    siblings: &[&Workflow],
    all_ids: &[&str],
) -> Vec<Issue> {
    let mut issues = validate_workflow(wf, siblings);
    let t = |issues: &mut Vec<Issue>, field: String, src: &str| {
        issues.extend(fakes_response::validate_template_text(&field, src));
    };
    for (k, v) in &wf.variables {
        issues.extend(fakes_response::validate_template_json(
            &format!("variables.{k}"),
            v,
        ));
    }
    for (i, step) in wf.steps.iter().enumerate() {
        let base = format!("steps[{i}]");
        match &step.action {
            Action::Http(h) => {
                t(&mut issues, format!("{base}.url"), &h.url);
                for (k, v) in &h.headers {
                    t(&mut issues, format!("{base}.headers.{k}"), v);
                }
                for (k, v) in &h.query {
                    t(&mut issues, format!("{base}.query.{k}"), v);
                }
                match &h.body {
                    StepBody::Json { content } => {
                        issues.extend(fakes_response::validate_template_json(
                            &format!("{base}.body.content"),
                            content,
                        ))
                    }
                    StepBody::Text { content, .. } => {
                        t(&mut issues, format!("{base}.body.content"), content)
                    }
                    StepBody::Form { fields } => {
                        for (k, v) in fields {
                            t(&mut issues, format!("{base}.body.fields.{k}"), v);
                        }
                    }
                    StepBody::None => {}
                }
            }
            Action::Emit { event, data } => {
                t(&mut issues, format!("{base}.event"), event);
                issues.extend(fakes_response::validate_template_json(
                    &format!("{base}.data"),
                    data,
                ));
            }
            Action::SetVariables { values } => {
                for (k, v) in values {
                    issues.extend(fakes_response::validate_template_json(
                        &format!("{base}.values.{k}"),
                        v,
                    ));
                }
            }
            Action::RunWorkflow {
                workflow_id, input, ..
            } => {
                if !workflow_id.is_empty()
                    && workflow_id != &wf.id
                    && !all_ids.contains(&workflow_id.as_str())
                {
                    issues.push(Issue::new(
                        format!("{base}.workflowId"),
                        format!("el servicio no tiene el workflow '{workflow_id}'"),
                    ));
                }
                for (k, v) in input {
                    issues.extend(fakes_response::validate_template_json(
                        &format!("{base}.input.{k}"),
                        v,
                    ));
                }
            }
            Action::Fail { message } => t(&mut issues, format!("{base}.message"), message),
            _ => {}
        }
    }
    issues
}

fn usages(svc: &Service, workflow_id: &str) -> Vec<WorkflowUsage> {
    let mut out = Vec::new();
    for ep in &svc.endpoints {
        for sc in &ep.scenarios {
            if sc.workflows.iter().any(|l| l.workflow_id == workflow_id) {
                out.push(WorkflowUsage {
                    endpoint_id: ep.id.clone(),
                    scenario_id: sc.id.clone(),
                });
            }
        }
    }
    out
}

fn summary(svc: &Service, w: &Workflow) -> WorkflowSummary {
    WorkflowSummary {
        id: w.id.clone(),
        name: w.name.clone(),
        description: w.description.clone(),
        enabled: w.enabled,
        step_count: w.steps.len(),
        schedule_every_ms: w.schedule.as_ref().map(|s| s.every_ms),
        used_by: usages(svc, &w.id),
    }
}

// ---------------------------------------------------------------------------
// Casos de uso
// ---------------------------------------------------------------------------

impl FakesApp {
    pub async fn list_workflows(&self, service_id: &str) -> AppResult<Vec<WorkflowSummary>> {
        let inner = self.inner.lock().await;
        let svc = find_service(&inner.active, service_id)?;
        Ok(svc.workflows.iter().map(|w| summary(svc, w)).collect())
    }

    pub async fn get_workflow(
        &self,
        service_id: &str,
        workflow_id: &str,
    ) -> AppResult<WorkflowView> {
        let inner = self.inner.lock().await;
        let svc = find_service(&inner.active, service_id)?;
        let wf = svc
            .workflows
            .iter()
            .find(|w| w.id == workflow_id)
            .ok_or_else(|| not_found_workflow(service_id, workflow_id))?;
        Ok(WorkflowView {
            service_id: svc.id.clone(),
            workflow: wf.clone(),
            used_by: usages(svc, workflow_id),
        })
    }

    pub async fn create_workflow(
        &self,
        service_id: &str,
        mut wf: Workflow,
    ) -> AppResult<WorkflowView> {
        let mut inner = self.inner.lock().await;
        let svc = find_service(&inner.active, service_id)?;
        if !wf.id.trim().is_empty() && svc.workflows.iter().any(|w| w.id == wf.id) {
            return Err(AppError::Conflict(format!(
                "el servicio '{service_id}' ya tiene el workflow '{}'",
                wf.id
            )));
        }
        normalize_workflow(&mut wf, &svc.workflows);
        let siblings: Vec<&Workflow> = svc.workflows.iter().collect();
        let mut ids: Vec<&str> = svc.workflows.iter().map(|w| w.id.as_str()).collect();
        ids.push(&wf.id);
        let issues = full_workflow_issues(&wf, &siblings, &ids);
        if !issues.is_empty() {
            return Err(AppError::Validation(issues));
        }
        fakes_config::save_workflow(&inner.active.dir, service_id, &wf)?;
        let id = wf.id.clone();
        let svc = find_service_mut(&mut inner.active, service_id)?;
        svc.workflows.push(wf);
        svc.workflows.sort_by_key(|a| a.name.to_lowercase());
        self.publish(&inner, true).await;
        drop(inner);
        self.get_workflow(service_id, &id).await
    }

    pub async fn update_workflow(
        &self,
        service_id: &str,
        workflow_id: &str,
        mut wf: Workflow,
    ) -> AppResult<WorkflowView> {
        let mut inner = self.inner.lock().await;
        let svc = find_service(&inner.active, service_id)?;
        if !svc.workflows.iter().any(|w| w.id == workflow_id) {
            return Err(not_found_workflow(service_id, workflow_id));
        }
        wf.id = workflow_id.to_string();
        let others: Vec<Workflow> = svc
            .workflows
            .iter()
            .filter(|w| w.id != workflow_id)
            .cloned()
            .collect();
        normalize_workflow(&mut wf, &others);
        let siblings: Vec<&Workflow> = others.iter().collect();
        let ids: Vec<&str> = svc.workflows.iter().map(|w| w.id.as_str()).collect();
        let issues = full_workflow_issues(&wf, &siblings, &ids);
        if !issues.is_empty() {
            return Err(AppError::Validation(issues));
        }
        fakes_config::save_workflow(&inner.active.dir, service_id, &wf)?;
        let svc = find_service_mut(&mut inner.active, service_id)?;
        if let Some(slot) = svc.workflows.iter_mut().find(|w| w.id == workflow_id) {
            *slot = wf;
        }
        svc.workflows.sort_by_key(|a| a.name.to_lowercase());
        self.publish(&inner, true).await;
        drop(inner);
        self.get_workflow(service_id, workflow_id).await
    }

    pub async fn patch_workflow(
        &self,
        service_id: &str,
        workflow_id: &str,
        patch: Value,
    ) -> AppResult<WorkflowView> {
        let current = self.get_workflow(service_id, workflow_id).await?.workflow;
        let merged: Workflow = crate::merge::apply(&current, patch)?;
        self.update_workflow(service_id, workflow_id, merged).await
    }

    /// Elimina un workflow. Falla si algún escenario u otro workflow lo usa.
    pub async fn delete_workflow(&self, service_id: &str, workflow_id: &str) -> AppResult<()> {
        let mut inner = self.inner.lock().await;
        let svc = find_service(&inner.active, service_id)?;
        if !svc.workflows.iter().any(|w| w.id == workflow_id) {
            return Err(not_found_workflow(service_id, workflow_id));
        }
        let mut refs: Vec<String> = usages(svc, workflow_id)
            .into_iter()
            .map(|u| format!("{} › {}", u.endpoint_id, u.scenario_id))
            .collect();
        for w in &svc.workflows {
            let calls = w
                .steps
                .iter()
                .any(|s| matches!(&s.action, Action::RunWorkflow { workflow_id: t, .. } if t == workflow_id));
            if calls && w.id != workflow_id {
                refs.push(format!("workflow {}", w.id));
            }
        }
        if !refs.is_empty() {
            return Err(AppError::Conflict(format!(
                "el workflow se usa en: {}. Quita esas referencias primero.",
                refs.join(", ")
            )));
        }
        fakes_config::delete_workflow(&inner.active.dir, service_id, workflow_id)?;
        let svc = find_service_mut(&mut inner.active, service_id)?;
        svc.workflows.retain(|w| w.id != workflow_id);
        self.publish(&inner, true).await;
        Ok(())
    }

    /// Valida un workflow sin guardarlo.
    pub async fn validate_workflow(
        &self,
        service_id: &str,
        mut wf: Workflow,
    ) -> AppResult<ValidationReport> {
        let inner = self.inner.lock().await;
        let svc = find_service(&inner.active, service_id)?;
        let others: Vec<Workflow> = svc
            .workflows
            .iter()
            .filter(|w| w.id != wf.id)
            .cloned()
            .collect();
        normalize_workflow(&mut wf, &others);
        let siblings: Vec<&Workflow> = others.iter().collect();
        let mut ids: Vec<&str> = svc.workflows.iter().map(|w| w.id.as_str()).collect();
        ids.push(&wf.id);
        let issues = full_workflow_issues(&wf, &siblings, &ids);
        Ok(ValidationReport {
            valid: issues.is_empty(),
            issues,
        })
    }

    /// Ejecuta un workflow manualmente.
    pub async fn run_workflow(
        &self,
        service_id: &str,
        workflow_id: &str,
        input: BTreeMap<String, Value>,
        scope: Option<Value>,
    ) -> AppResult<ExecutionSummary> {
        {
            let inner = self.inner.lock().await;
            find_service(&inner.active, service_id)?;
        }
        self.engine
            .start(StartRequest {
                service_id: service_id.to_string(),
                workflow_id: workflow_id.to_string(),
                trigger: TriggerKind::Manual,
                input,
                scope: scope.unwrap_or_else(|| json!({})),
                parent: None,
            })
            .await
            .map_err(engine_err)
    }

    pub async fn list_executions(
        &self,
        filter: &ExecutionFilter,
    ) -> AppResult<Vec<ExecutionSummary>> {
        let ws = self.inner.lock().await.active.workspace.id.clone();
        Ok(self.store.list_executions(&ws, filter).await?)
    }

    pub async fn get_execution(&self, id: &str) -> AppResult<ExecutionDetail> {
        let ws = self.inner.lock().await.active.workspace.id.clone();
        let execution = self
            .store
            .get_execution(&ws, id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("no existe la ejecución '{id}'")))?;
        let timeline = self.store.timeline(id).await?;
        Ok(ExecutionDetail {
            execution,
            timeline,
        })
    }

    pub async fn cancel_execution(&self, id: &str) -> AppResult<ExecutionSummary> {
        let ws = self.inner.lock().await.active.workspace.id.clone();
        if self.store.get_execution(&ws, id).await?.is_none() {
            return Err(AppError::NotFound(format!("no existe la ejecución '{id}'")));
        }
        self.engine.cancel(id).await.map_err(engine_err)
    }

    /// Borra las ejecuciones terminadas del espacio activo.
    pub async fn clear_executions(&self) -> AppResult<u64> {
        let ws = self.inner.lock().await.active.workspace.id.clone();
        Ok(self.store.clear_finished_executions(&ws).await?)
    }
}

fn not_found_workflow(sid: &str, wid: &str) -> AppError {
    AppError::NotFound(format!("el servicio '{sid}' no tiene el workflow '{wid}'"))
}
