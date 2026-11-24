//! Servicios de aplicación de Fake Services.
//!
//! [`FakesApp`] es la única puerta para leer y modificar el sistema. La UI
//! (vía API REST), la API REST y el servidor MCP llaman a estos mismos
//! métodos, así que todos tienen exactamente las mismas capacidades.
//!
//! Cada modificación: valida → guarda en disco (fuente de verdad) →
//! recompila la configuración → la publica en el servidor de simulación.

pub mod bundle;
mod error;
mod history;
mod merge;
mod resources;
mod seed;
pub mod specs;
pub mod system;
pub mod templates;
pub mod views;
mod workflows;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use fakes_config::{LoadProblem, Registry, RegistryEntry};
use fakes_domain::validate::{validate_endpoint, validate_service, validate_workspace};
use fakes_domain::{
    Endpoint, Issue, Service, Workspace, endpoint_id_hint, normalize_prefix, slugify, unique_id,
};
use fakes_matching::CompiledWorkspace;
use fakes_server::{MockServer, ServerState, Snapshot, display_host};
use fakes_store::{RequestFilter, RequestRecord, RequestSummary, Store};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::{Mutex, broadcast};

pub use bundle::{Bundle, ExportOptions, ImportReport, ImportRequest};
pub use error::{AppError, AppResult, ErrorBody};
pub use fakes_import::detect::DetectionReport;
pub use fakes_store::{CollectionInfo, ResourceRecord};
pub use specs::{SpecImportReport, SpecImportRequest};
pub use system::AutostartStatus;
pub use templates::{TemplateInfo, TemplateInstallReport, TemplateInstallRequest};
pub use views::*;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Evento para la UI en vivo.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum AppEvent {
    /// Llegó una solicitud al servidor de simulación.
    Request(RequestSummary),
    /// Cambió la configuración (servicios, endpoints, espacio).
    ConfigChanged { workspace_id: String },
    /// Cambió el estado de ejecución (servicios iniciados/detenidos, puertos).
    RuntimeChanged { workspace_id: String },
    /// Una ejecución de workflow cambió de estado.
    Execution(fakes_store::ExecutionSummary),
    /// Un workflow emitió un evento (paso `emit`).
    WorkflowEvent {
        execution_id: String,
        workflow_id: String,
        service_id: String,
        event: String,
        data: Value,
    },
}

/// Opciones de arranque.
#[derive(Debug, Clone)]
pub struct AppOptions {
    pub data_dir: PathBuf,
    /// Puertos que los servicios no pueden usar (p. ej. la API de administración).
    pub reserved_ports: Vec<u16>,
    /// Sustituye el puerto del servidor central (útil en pruebas con 0).
    pub central_port_override: Option<u16>,
    /// Crear el servicio de ejemplo en el espacio inicial.
    pub seed_example: bool,
}

impl AppOptions {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            reserved_ports: Vec::new(),
            central_port_override: None,
            seed_example: true,
        }
    }
}

pub(crate) struct Active {
    dir: PathBuf,
    workspace: Workspace,
    services: Vec<Service>,
    problems: Vec<LoadProblem>,
    running: HashSet<String>,
}

struct Inner {
    registry: Registry,
    active: Active,
}

pub struct FakesApp {
    opts: AppOptions,
    store: Store,
    server: Arc<MockServer>,
    inner: Mutex<Inner>,
    events: broadcast::Sender<AppEvent>,
    history_keep: Arc<AtomicU32>,
    shutdown_notify: tokio::sync::Notify,
    engine: Arc<fakes_workflow::WorkflowEngine>,
}

fn default_workspaces_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("workspaces")
}

impl FakesApp {
    /// Abre (o inicializa) la aplicación y arranca el servidor de simulación.
    pub async fn start(opts: AppOptions) -> AppResult<Arc<Self>> {
        std::fs::create_dir_all(&opts.data_dir).map_err(|e| {
            AppError::BadRequest(format!(
                "no se pudo crear la carpeta de datos {}: {e}",
                opts.data_dir.display()
            ))
        })?;
        let store = Store::open(&opts.data_dir.join("fakes.db")).await?;
        let mut registry = Registry::load(&opts.data_dir)?;

        // Primer arranque: espacio por defecto con un ejemplo.
        if registry.workspaces.is_empty() {
            let dir = default_workspaces_dir(&opts.data_dir).join("principal");
            if !fakes_config::is_workspace_dir(&dir) {
                let mut ws = Workspace::new("principal", "Mi espacio de trabajo");
                ws.description = "Espacio creado automáticamente al instalar Fake Services.".into();
                fakes_config::create_workspace(&dir, &ws)?;
                if opts.seed_example {
                    fakes_config::save_service_full(&dir, &seed::example_service())?;
                }
            }
            registry.workspaces.push(RegistryEntry {
                id: "principal".into(),
                path: dir,
            });
            registry.active = Some("principal".into());
            registry.save(&opts.data_dir)?;
        }

        let active_id = registry
            .active
            .clone()
            .filter(|id| registry.find(id).is_some())
            .unwrap_or_else(|| registry.workspaces[0].id.clone());
        let entry = registry.find(&active_id).cloned().expect("existe");
        let active = load_active(&entry.path, &opts)?;
        registry.active = Some(active_id);

        let (events, _) = broadcast::channel(1024);
        let history_keep = Arc::new(AtomicU32::new(active.workspace.history.max_entries));
        let sink = history::HistorySink::spawn(store.clone(), events.clone(), history_keep.clone());
        let engine = fakes_workflow::WorkflowEngine::new(
            store.clone(),
            Arc::new(workflows::EventsObserver(events.clone())),
        );
        engine.set_catalog(workflows::build_catalog(&active));
        let state = ServerState::full(
            build_snapshot(&active),
            sink,
            Arc::new(workflows::EngineStarter(Arc::downgrade(&engine))),
            store.clone(),
        );
        let server = MockServer::new(state);

        let app = Arc::new(Self {
            opts,
            store,
            server,
            inner: Mutex::new(Inner { registry, active }),
            events,
            history_keep,
            shutdown_notify: tokio::sync::Notify::new(),
            engine,
        });
        app.server.reconcile().await;
        match app.engine.recover().await {
            Ok(0) => {}
            Ok(n) => tracing::info!("{n} ejecuciones de workflows retomadas"),
            Err(e) => tracing::warn!("no se pudieron retomar ejecuciones: {e}"),
        }
        Ok(app)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.events.subscribe()
    }

    pub fn data_dir(&self) -> &Path {
        &self.opts.data_dir
    }

    pub fn server(&self) -> &Arc<MockServer> {
        &self.server
    }

    /// Pide al proceso que se detenga (lo atiende quien lanzó la aplicación).
    pub fn request_shutdown(&self) {
        self.shutdown_notify.notify_one();
    }

    /// Termina cuando alguien pide detener la aplicación (`fakes detener`, API).
    pub async fn shutdown_requested(&self) {
        self.shutdown_notify.notified().await;
    }

    pub async fn shutdown(&self) {
        self.engine.shutdown().await;
        self.server.shutdown().await;
    }

    pub fn engine(&self) -> &Arc<fakes_workflow::WorkflowEngine> {
        &self.engine
    }

    /// Recompila y publica la configuración activa.
    async fn publish(&self, inner: &Inner, config_changed: bool) {
        self.history_keep.store(
            inner.active.workspace.history.max_entries,
            Ordering::Relaxed,
        );
        self.engine
            .set_catalog(workflows::build_catalog(&inner.active));
        self.server.apply(build_snapshot(&inner.active)).await;
        let workspace_id = inner.active.workspace.id.clone();
        let _ = self.events.send(if config_changed {
            AppEvent::ConfigChanged { workspace_id }
        } else {
            AppEvent::RuntimeChanged { workspace_id }
        });
    }

    // -----------------------------------------------------------------------
    // Estado
    // -----------------------------------------------------------------------

    pub async fn status(&self) -> AppResult<AppStatus> {
        let inner = self.inner.lock().await;
        let a = &inner.active;
        let listeners = self.server.status().await;
        let central_url = listeners
            .iter()
            .find(|l| l.service_id.is_none())
            .map(|l| l.url.clone())
            .unwrap_or_default();
        Ok(AppStatus {
            version: VERSION.into(),
            workspace_id: a.workspace.id.clone(),
            workspace_name: a.workspace.name.clone(),
            workspace_path: a.dir.display().to_string(),
            central_url,
            listeners,
            services_total: a.services.len(),
            services_running: a
                .services
                .iter()
                .filter(|s| a.running.contains(&s.id))
                .count(),
            endpoints_total: a.services.iter().map(|s| s.endpoints.len()).sum(),
            requests_total: self.store.count_requests(&a.workspace.id).await?,
            executions_active: self.store.count_active_executions(&a.workspace.id).await?,
            workflows_total: a.services.iter().map(|s| s.workflows.len()).sum(),
            problems: a.problems.clone(),
        })
    }

    /// Vuelve a leer el espacio activo desde disco (tras un `git pull`, por ejemplo).
    pub async fn reload(&self) -> AppResult<WorkspaceView> {
        let mut inner = self.inner.lock().await;
        let running_before = inner.active.running.clone();
        let mut fresh = load_active(&inner.active.dir, &self.opts)?;
        // Conservar el estado de ejecución de los servicios que siguen existiendo.
        fresh.running = fresh
            .services
            .iter()
            .filter(|s| {
                running_before.contains(&s.id)
                    || (!inner.active.services.iter().any(|o| o.id == s.id) && s.auto_start)
            })
            .map(|s| s.id.clone())
            .collect();
        inner.active = fresh;
        self.publish(&inner, true).await;
        Ok(workspace_view(&inner.active))
    }

    // -----------------------------------------------------------------------
    // Espacios de trabajo
    // -----------------------------------------------------------------------

    pub async fn list_workspaces(&self) -> Vec<WorkspaceInfo> {
        let inner = self.inner.lock().await;
        inner
            .registry
            .workspaces
            .iter()
            .map(|e| {
                let active = inner.registry.active.as_deref() == Some(e.id.as_str());
                let ws = if active {
                    Some(inner.active.workspace.clone())
                } else {
                    fakes_config::read_json::<Workspace>(&e.path.join(fakes_config::WORKSPACE_FILE))
                        .ok()
                };
                WorkspaceInfo {
                    id: e.id.clone(),
                    name: ws
                        .as_ref()
                        .map(|w| w.name.clone())
                        .unwrap_or_else(|| e.id.clone()),
                    description: ws
                        .as_ref()
                        .map(|w| w.description.clone())
                        .unwrap_or_default(),
                    path: e.path.display().to_string(),
                    active,
                    available: ws.is_some(),
                }
            })
            .collect()
    }

    /// Espacio activo con sus servicios.
    pub async fn active_workspace(&self) -> WorkspaceView {
        let inner = self.inner.lock().await;
        workspace_view(&inner.active)
    }

    /// Crea un espacio nuevo. Si `path` es `None`, se crea en la carpeta de datos.
    pub async fn create_workspace(
        &self,
        mut ws: Workspace,
        path: Option<PathBuf>,
        activate: bool,
    ) -> AppResult<WorkspaceInfo> {
        let mut inner = self.inner.lock().await;
        if ws.id.trim().is_empty() {
            ws.id = unique_id(
                &ws.name,
                inner.registry.workspaces.iter().map(|w| w.id.as_str()),
            );
        } else if inner.registry.find(&ws.id).is_some() {
            return Err(AppError::Conflict(format!(
                "ya existe el espacio '{}'",
                ws.id
            )));
        }
        ws.schema_version = fakes_domain::SCHEMA_VERSION;
        let issues = validate_workspace(&ws);
        if !issues.is_empty() {
            return Err(AppError::Validation(issues));
        }
        let dir = path.unwrap_or_else(|| default_workspaces_dir(&self.opts.data_dir).join(&ws.id));
        fakes_config::create_workspace(&dir, &ws)?;
        inner.registry.workspaces.push(RegistryEntry {
            id: ws.id.clone(),
            path: dir.clone(),
        });
        inner.registry.save(&self.opts.data_dir)?;
        drop(inner);
        if activate {
            self.activate_workspace(&ws.id).await?;
        }
        Ok(WorkspaceInfo {
            id: ws.id.clone(),
            name: ws.name.clone(),
            description: ws.description.clone(),
            path: dir.display().to_string(),
            active: activate,
            available: true,
        })
    }

    /// Registra una carpeta existente (por ejemplo, versionada en Git).
    pub async fn register_workspace(
        &self,
        path: PathBuf,
        activate: bool,
    ) -> AppResult<WorkspaceInfo> {
        let ws: Workspace = fakes_config::read_json(&path.join(fakes_config::WORKSPACE_FILE))
            .map_err(|e| {
                AppError::BadRequest(format!("la carpeta no contiene un espacio válido: {e}"))
            })?;
        let mut inner = self.inner.lock().await;
        if let Some(existing) = inner.registry.find(&ws.id) {
            if existing.path != path {
                return Err(AppError::Conflict(format!(
                    "ya hay un espacio registrado con id '{}' en {}",
                    ws.id,
                    existing.path.display()
                )));
            }
        } else {
            inner.registry.workspaces.push(RegistryEntry {
                id: ws.id.clone(),
                path: path.clone(),
            });
            inner.registry.save(&self.opts.data_dir)?;
        }
        drop(inner);
        if activate {
            self.activate_workspace(&ws.id).await?;
        }
        Ok(WorkspaceInfo {
            id: ws.id,
            name: ws.name,
            description: ws.description,
            path: path.display().to_string(),
            active: activate,
            available: true,
        })
    }

    pub async fn activate_workspace(&self, id: &str) -> AppResult<WorkspaceView> {
        let mut inner = self.inner.lock().await;
        let entry = inner
            .registry
            .find(id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("no existe el espacio '{id}'")))?;
        let active = load_active(&entry.path, &self.opts)?;
        // Las ejecuciones del espacio anterior quedan pausadas y se retoman al volver.
        let switching = inner.active.workspace.id != active.workspace.id;
        if switching {
            self.engine.suspend_all().await;
        }
        inner.active = active;
        inner.registry.active = Some(id.to_string());
        inner.registry.save(&self.opts.data_dir)?;
        self.publish(&inner, true).await;
        if switching && let Err(e) = self.engine.recover().await {
            tracing::warn!("no se pudieron retomar ejecuciones: {e}");
        }
        Ok(workspace_view(&inner.active))
    }

    /// Quita un espacio del registro (no borra archivos).
    pub async fn forget_workspace(&self, id: &str) -> AppResult<()> {
        let mut inner = self.inner.lock().await;
        if inner.registry.active.as_deref() == Some(id) {
            return Err(AppError::Conflict(
                "no se puede quitar el espacio activo; activa otro primero".into(),
            ));
        }
        let before = inner.registry.workspaces.len();
        inner.registry.workspaces.retain(|w| w.id != id);
        if inner.registry.workspaces.len() == before {
            return Err(AppError::NotFound(format!("no existe el espacio '{id}'")));
        }
        inner.registry.save(&self.opts.data_dir)?;
        Ok(())
    }

    /// Actualiza nombre, descripción, servidor, variables e historial del espacio activo.
    pub async fn update_workspace(&self, mut ws: Workspace) -> AppResult<Workspace> {
        let mut inner = self.inner.lock().await;
        ws.id = inner.active.workspace.id.clone();
        ws.schema_version = fakes_domain::SCHEMA_VERSION;
        let mut issues = validate_workspace(&ws);
        if self.opts.reserved_ports.contains(&ws.server.port) {
            issues.push(Issue::new(
                "server.port",
                "el puerto está reservado para la administración",
            ));
        }
        for s in &inner.active.services {
            if s.port == Some(ws.server.port) {
                issues.push(Issue::new(
                    "server.port",
                    format!("el puerto lo usa el servicio '{}'", s.name),
                ));
            }
        }
        if !issues.is_empty() {
            return Err(AppError::Validation(issues));
        }
        fakes_config::save_workspace_meta(&inner.active.dir, &ws)?;
        inner.active.workspace = ws.clone();
        self.publish(&inner, true).await;
        Ok(ws)
    }

    pub async fn patch_workspace(&self, patch: Value) -> AppResult<Workspace> {
        let current = self.inner.lock().await.active.workspace.clone();
        let merged: Workspace = merge::apply(&current, patch)?;
        self.update_workspace(merged).await
    }

    // -----------------------------------------------------------------------
    // Servicios
    // -----------------------------------------------------------------------

    pub async fn list_services(&self) -> Vec<ServiceSummary> {
        let inner = self.inner.lock().await;
        workspace_view(&inner.active).services
    }

    pub async fn get_service(&self, id: &str) -> AppResult<ServiceView> {
        let inner = self.inner.lock().await;
        let svc = find_service(&inner.active, id)?;
        Ok(service_view(&inner.active, svc))
    }

    fn check_service(&self, active: &Active, svc: &Service) -> Vec<Issue> {
        let others: Vec<&Service> = active.services.iter().collect();
        let mut issues = validate_service(svc, &others, &active.workspace);
        if let Some(p) = svc.port
            && self.opts.reserved_ports.contains(&p)
        {
            issues.push(Issue::new(
                "port",
                "el puerto está reservado para la administración",
            ));
        }
        issues
    }

    /// Crea un servicio. Los endpoints incluidos también se crean.
    pub async fn create_service(&self, mut svc: Service) -> AppResult<ServiceView> {
        let mut inner = self.inner.lock().await;
        let a = &inner.active;
        if svc.id.trim().is_empty() {
            svc.id = unique_id(&svc.name, a.services.iter().map(|s| s.id.as_str()));
        } else if a.services.iter().any(|s| s.id == svc.id) {
            return Err(AppError::Conflict(format!(
                "ya existe el servicio '{}'",
                svc.id
            )));
        }
        svc.prefix = if svc.prefix.trim().is_empty() {
            format!("/{}", svc.id)
        } else {
            normalize_prefix(&svc.prefix)
        };
        svc.schema_version = fakes_domain::SCHEMA_VERSION;
        let endpoints = std::mem::take(&mut svc.endpoints);
        let workflows = std::mem::take(&mut svc.workflows);
        let mut issues = self.check_service(a, &svc);
        let mut wf_prepared: Vec<fakes_domain::Workflow> = Vec::new();
        for mut wf in workflows {
            workflows::normalize_workflow(&mut wf, &wf_prepared);
            wf_prepared.push(wf);
        }
        let wf_ids: Vec<&str> = wf_prepared.iter().map(|w| w.id.as_str()).collect();
        for (i, wf) in wf_prepared.iter().enumerate() {
            let siblings: Vec<&fakes_domain::Workflow> = wf_prepared[..i].iter().collect();
            issues.extend(
                workflows::full_workflow_issues(wf, &siblings, &wf_ids)
                    .into_iter()
                    .map(|is| Issue::new(format!("workflows[{i}].{}", is.field), is.message)),
            );
        }
        let mut prepared: Vec<Endpoint> = Vec::new();
        for (i, mut ep) in endpoints.into_iter().enumerate() {
            normalize_endpoint(&mut ep, &prepared);
            let siblings: Vec<&Endpoint> = prepared.iter().collect();
            issues.extend(
                full_endpoint_issues(&ep, &siblings, &wf_ids)
                    .into_iter()
                    .map(|is| Issue::new(format!("endpoints[{i}].{}", is.field), is.message)),
            );
            prepared.push(ep);
        }
        if !issues.is_empty() {
            return Err(AppError::Validation(issues));
        }
        svc.endpoints = prepared;
        svc.workflows = wf_prepared;
        fakes_config::save_service_full(&a.dir, &svc)?;
        let id = svc.id.clone();
        if svc.auto_start {
            inner.active.running.insert(id.clone());
        }
        inner.active.services.push(svc);
        sort_services(&mut inner.active.services);
        self.publish(&inner, true).await;
        let svc = find_service(&inner.active, &id)?;
        Ok(service_view(&inner.active, svc))
    }

    /// Reemplaza la configuración del servicio. Si `endpoints` viene vacío
    /// se conservan los actuales (los endpoints se editan por separado).
    pub async fn update_service(&self, id: &str, mut svc: Service) -> AppResult<ServiceView> {
        let mut inner = self.inner.lock().await;
        let current = find_service(&inner.active, id)?.clone();
        svc.id = current.id.clone();
        svc.schema_version = fakes_domain::SCHEMA_VERSION;
        svc.prefix = normalize_prefix(&svc.prefix);
        if svc.endpoints.is_empty() {
            svc.endpoints = current.endpoints.clone();
        }
        if svc.workflows.is_empty() {
            svc.workflows = current.workflows.clone();
        }
        let issues = self.check_service(&inner.active, &svc);
        if !issues.is_empty() {
            return Err(AppError::Validation(issues));
        }
        fakes_config::save_service(&inner.active.dir, &svc)?;
        let slot = inner
            .active
            .services
            .iter_mut()
            .find(|s| s.id == id)
            .expect("existe");
        *slot = svc;
        sort_services(&mut inner.active.services);
        self.publish(&inner, true).await;
        let svc = find_service(&inner.active, id)?;
        Ok(service_view(&inner.active, svc))
    }

    /// Actualización parcial con JSON Merge Patch (RFC 7396).
    pub async fn patch_service(&self, id: &str, patch: Value) -> AppResult<ServiceView> {
        let current = {
            let inner = self.inner.lock().await;
            let mut s = find_service(&inner.active, id)?.clone();
            s.endpoints.clear();
            s.workflows.clear();
            s
        };
        let merged: Service = merge::apply(&current, patch)?;
        self.update_service(id, merged).await
    }

    pub async fn delete_service(&self, id: &str) -> AppResult<()> {
        let mut inner = self.inner.lock().await;
        find_service(&inner.active, id)?;
        fakes_config::delete_service(&inner.active.dir, id)?;
        self.store
            .resource_clear(&inner.active.workspace.id, id, None)
            .await?;
        inner.active.services.retain(|s| s.id != id);
        inner.active.running.remove(id);
        self.publish(&inner, true).await;
        Ok(())
    }

    pub async fn start_service(&self, id: &str) -> AppResult<ServiceView> {
        self.set_running(id, true).await
    }

    pub async fn stop_service(&self, id: &str) -> AppResult<ServiceView> {
        self.set_running(id, false).await
    }

    async fn set_running(&self, id: &str, running: bool) -> AppResult<ServiceView> {
        let mut inner = self.inner.lock().await;
        find_service(&inner.active, id)?;
        if running {
            inner.active.running.insert(id.to_string());
        } else {
            inner.active.running.remove(id);
        }
        self.publish(&inner, false).await;
        let svc = find_service(&inner.active, id)?;
        Ok(service_view(&inner.active, svc))
    }

    /// Inicia o detiene todos los servicios.
    pub async fn set_all_running(&self, running: bool) -> AppResult<()> {
        let mut inner = self.inner.lock().await;
        inner.active.running = if running {
            inner.active.services.iter().map(|s| s.id.clone()).collect()
        } else {
            HashSet::new()
        };
        self.publish(&inner, false).await;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Endpoints
    // -----------------------------------------------------------------------

    pub async fn list_endpoints(&self, service_id: &str) -> AppResult<Vec<EndpointSummary>> {
        let inner = self.inner.lock().await;
        let svc = find_service(&inner.active, service_id)?;
        let base = service_url(&inner.active, svc);
        Ok(svc
            .endpoints
            .iter()
            .map(|e| EndpointSummary {
                id: e.id.clone(),
                name: e.name.clone(),
                method: e.method,
                path: e.path.clone(),
                enabled: e.enabled,
                scenario_count: e.scenarios.len(),
                active_scenario: e.active_scenario.clone(),
                url: join_url(&base, &e.path),
            })
            .collect())
    }

    pub async fn get_endpoint(
        &self,
        service_id: &str,
        endpoint_id: &str,
    ) -> AppResult<EndpointView> {
        let inner = self.inner.lock().await;
        let svc = find_service(&inner.active, service_id)?;
        let ep = svc
            .endpoints
            .iter()
            .find(|e| e.id == endpoint_id)
            .ok_or_else(|| error::not_found_endpoint(service_id, endpoint_id))?;
        Ok(EndpointView {
            service_id: svc.id.clone(),
            url: join_url(&service_url(&inner.active, svc), &ep.path),
            endpoint: ep.clone(),
        })
    }

    pub async fn create_endpoint(
        &self,
        service_id: &str,
        mut ep: Endpoint,
    ) -> AppResult<EndpointView> {
        let mut inner = self.inner.lock().await;
        let svc = find_service(&inner.active, service_id)?;
        if !ep.id.trim().is_empty() && svc.endpoints.iter().any(|e| e.id == ep.id) {
            return Err(AppError::Conflict(format!(
                "el servicio '{service_id}' ya tiene el endpoint '{}'",
                ep.id
            )));
        }
        normalize_endpoint(&mut ep, &svc.endpoints);
        let siblings: Vec<&Endpoint> = svc.endpoints.iter().collect();
        let issues = full_endpoint_issues(&ep, &siblings, &workflow_ids(svc));
        if !issues.is_empty() {
            return Err(AppError::Validation(issues));
        }
        fakes_config::save_endpoint(&inner.active.dir, service_id, &ep)?;
        let id = ep.id.clone();
        let svc = find_service_mut(&mut inner.active, service_id)?;
        svc.endpoints.push(ep);
        fakes_config::sort_endpoints(&mut svc.endpoints);
        self.publish(&inner, true).await;
        drop(inner);
        self.get_endpoint(service_id, &id).await
    }

    pub async fn update_endpoint(
        &self,
        service_id: &str,
        endpoint_id: &str,
        mut ep: Endpoint,
    ) -> AppResult<EndpointView> {
        let mut inner = self.inner.lock().await;
        let svc = find_service(&inner.active, service_id)?;
        if !svc.endpoints.iter().any(|e| e.id == endpoint_id) {
            return Err(error::not_found_endpoint(service_id, endpoint_id));
        }
        ep.id = endpoint_id.to_string();
        let others: Vec<Endpoint> = svc
            .endpoints
            .iter()
            .filter(|e| e.id != endpoint_id)
            .cloned()
            .collect();
        normalize_endpoint(&mut ep, &others);
        let siblings: Vec<&Endpoint> = others.iter().collect();
        let issues = full_endpoint_issues(&ep, &siblings, &workflow_ids(svc));
        if !issues.is_empty() {
            return Err(AppError::Validation(issues));
        }
        fakes_config::save_endpoint(&inner.active.dir, service_id, &ep)?;
        let svc = find_service_mut(&mut inner.active, service_id)?;
        if let Some(slot) = svc.endpoints.iter_mut().find(|e| e.id == endpoint_id) {
            *slot = ep;
        }
        fakes_config::sort_endpoints(&mut svc.endpoints);
        self.publish(&inner, true).await;
        drop(inner);
        self.get_endpoint(service_id, endpoint_id).await
    }

    pub async fn patch_endpoint(
        &self,
        service_id: &str,
        endpoint_id: &str,
        patch: Value,
    ) -> AppResult<EndpointView> {
        let current = self.get_endpoint(service_id, endpoint_id).await?.endpoint;
        let merged: Endpoint = merge::apply(&current, patch)?;
        self.update_endpoint(service_id, endpoint_id, merged).await
    }

    pub async fn set_endpoint_enabled(
        &self,
        service_id: &str,
        endpoint_id: &str,
        enabled: bool,
    ) -> AppResult<EndpointView> {
        self.patch_endpoint(
            service_id,
            endpoint_id,
            serde_json::json!({ "enabled": enabled }),
        )
        .await
    }

    /// Fija (o quita, con `None`) el escenario forzado de un endpoint.
    pub async fn set_active_scenario(
        &self,
        service_id: &str,
        endpoint_id: &str,
        scenario: Option<String>,
    ) -> AppResult<EndpointView> {
        self.patch_endpoint(
            service_id,
            endpoint_id,
            serde_json::json!({ "activeScenario": scenario }),
        )
        .await
    }

    pub async fn delete_endpoint(&self, service_id: &str, endpoint_id: &str) -> AppResult<()> {
        let mut inner = self.inner.lock().await;
        let svc = find_service_mut(&mut inner.active, service_id)?;
        let before = svc.endpoints.len();
        svc.endpoints.retain(|e| e.id != endpoint_id);
        if svc.endpoints.len() == before {
            return Err(error::not_found_endpoint(service_id, endpoint_id));
        }
        fakes_config::delete_endpoint(&inner.active.dir, service_id, endpoint_id)?;
        self.publish(&inner, true).await;
        Ok(())
    }

    /// Valida un endpoint sin guardarlo.
    pub async fn validate_endpoint(
        &self,
        service_id: &str,
        mut ep: Endpoint,
    ) -> AppResult<ValidationReport> {
        let inner = self.inner.lock().await;
        let svc = find_service(&inner.active, service_id)?;
        let others: Vec<Endpoint> = svc
            .endpoints
            .iter()
            .filter(|e| e.id != ep.id)
            .cloned()
            .collect();
        normalize_endpoint(&mut ep, &others);
        let siblings: Vec<&Endpoint> = others.iter().collect();
        let issues = full_endpoint_issues(&ep, &siblings, &workflow_ids(svc));
        Ok(ValidationReport {
            valid: issues.is_empty(),
            issues,
        })
    }

    // -----------------------------------------------------------------------
    // Historial
    // -----------------------------------------------------------------------

    async fn active_id(&self) -> String {
        self.inner.lock().await.active.workspace.id.clone()
    }

    pub async fn list_requests(&self, filter: &RequestFilter) -> AppResult<Vec<RequestSummary>> {
        let ws = self.active_id().await;
        Ok(self.store.list_requests(&ws, filter).await?)
    }

    pub async fn get_request(&self, id: &str) -> AppResult<RequestRecord> {
        let ws = self.active_id().await;
        self.store
            .get_request(&ws, id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("no existe la solicitud '{id}'")))
    }

    pub async fn clear_requests(&self, service_id: Option<&str>) -> AppResult<u64> {
        let ws = self.active_id().await;
        Ok(self.store.clear_requests(&ws, service_id).await?)
    }
}

// ---------------------------------------------------------------------------
// Utilidades internas
// ---------------------------------------------------------------------------

fn load_active(dir: &Path, opts: &AppOptions) -> AppResult<Active> {
    let mut loaded = fakes_config::load_workspace(dir)?;
    if let Some(p) = opts.central_port_override {
        loaded.workspace.server.port = p;
    }
    let running = loaded
        .services
        .iter()
        .filter(|s| s.auto_start)
        .map(|s| s.id.clone())
        .collect();
    Ok(Active {
        dir: loaded.dir,
        workspace: loaded.workspace,
        services: loaded.services,
        problems: loaded.problems,
        running,
    })
}

fn build_snapshot(a: &Active) -> Snapshot {
    Snapshot {
        compiled: CompiledWorkspace::compile(a.workspace.clone(), a.services.clone()),
        running: a.running.clone(),
        files_root: Some(a.dir.join(fakes_config::FILES_DIR)),
        call_counts: Default::default(),
    }
}

pub(crate) fn find_service<'a>(a: &'a Active, id: &str) -> AppResult<&'a Service> {
    a.services
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| error::not_found_service(id))
}

pub(crate) fn find_service_mut<'a>(a: &'a mut Active, id: &str) -> AppResult<&'a mut Service> {
    a.services
        .iter_mut()
        .find(|s| s.id == id)
        .ok_or_else(|| error::not_found_service(id))
}

pub(crate) fn sort_services(services: &mut [Service]) {
    services.sort_by_key(|a| a.name.to_lowercase());
}

fn central_url(a: &Active) -> String {
    let s = &a.workspace.server;
    format!("http://{}:{}", display_host(&s.host), s.port)
}

pub(crate) fn service_url(a: &Active, svc: &Service) -> String {
    format!("{}{}", central_url(a), normalize_prefix(&svc.prefix))
}

fn join_url(base: &str, path: &str) -> String {
    format!("{}{}", base.trim_end_matches('/'), path)
}

fn service_view(a: &Active, svc: &Service) -> ServiceView {
    ServiceView {
        service: svc.clone(),
        running: a.running.contains(&svc.id),
        url: service_url(a, svc),
        port_url: svc
            .port
            .map(|p| format!("http://{}:{p}", display_host(&a.workspace.server.host))),
    }
}

fn workspace_view(a: &Active) -> WorkspaceView {
    WorkspaceView {
        path: a.dir.display().to_string(),
        workspace: a.workspace.clone(),
        problems: a.problems.clone(),
        services: a
            .services
            .iter()
            .map(|s| ServiceSummary {
                id: s.id.clone(),
                name: s.name.clone(),
                description: s.description.clone(),
                prefix: s.prefix.clone(),
                port: s.port,
                running: a.running.contains(&s.id),
                url: service_url(a, s),
                endpoint_count: s.endpoints.len(),
                workflow_count: s.workflows.len(),
            })
            .collect(),
    }
}

/// Completa identificadores faltantes de endpoint y escenarios.
pub(crate) fn normalize_endpoint(ep: &mut Endpoint, siblings: &[Endpoint]) {
    ep.schema_version = fakes_domain::SCHEMA_VERSION;
    if ep.id.trim().is_empty() {
        let hint = endpoint_id_hint(ep.method, &ep.path);
        ep.id = unique_id(&hint, siblings.iter().map(|e| e.id.as_str()));
    }
    let mut used: Vec<String> = ep
        .scenarios
        .iter()
        .filter(|s| !s.id.trim().is_empty())
        .map(|s| s.id.clone())
        .collect();
    for sc in ep.scenarios.iter_mut() {
        if sc.id.trim().is_empty() {
            let base = if sc.name.trim().is_empty() {
                format!("http-{}", sc.response.status)
            } else {
                slugify(&sc.name)
            };
            sc.id = unique_id(&base, used.iter().map(String::as_str));
            used.push(sc.id.clone());
        }
        if sc.name.trim().is_empty() {
            sc.name = format!("HTTP {}", sc.response.status);
        }
    }
}

/// Validación estructural + sintaxis de plantillas.
fn workflow_ids(svc: &Service) -> Vec<&str> {
    svc.workflows.iter().map(|w| w.id.as_str()).collect()
}

pub(crate) fn full_endpoint_issues(
    ep: &Endpoint,
    siblings: &[&Endpoint],
    workflow_ids: &[&str],
) -> Vec<Issue> {
    let mut issues = validate_endpoint(ep, siblings);
    for (i, sc) in ep.scenarios.iter().enumerate() {
        issues.extend(fakes_response::validate_templates(
            &format!("scenarios[{i}].response"),
            &sc.response,
        ));
        for (j, link) in sc.workflows.iter().enumerate() {
            let base = format!("scenarios[{i}].workflows[{j}]");
            if !workflow_ids.contains(&link.workflow_id.as_str()) {
                issues.push(Issue::new(
                    format!("{base}.workflowId"),
                    format!("el servicio no tiene el workflow '{}'", link.workflow_id),
                ));
            }
            for (k, v) in &link.input {
                issues.extend(fakes_response::validate_template_json(
                    &format!("{base}.input.{k}"),
                    v,
                ));
            }
        }
    }
    if let Some(r) = &ep.on_validation_error {
        issues.extend(fakes_response::validate_templates("onValidationError", r));
    }
    issues
}

#[cfg(test)]
mod tests;
