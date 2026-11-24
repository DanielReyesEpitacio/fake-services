//! Rutas `/api/v1`. Cada handler delega en `FakesApp`; aquí no hay lógica
//! de negocio, solo traducción HTTP ↔ casos de uso.

use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use fakes_app::{AppError, FakesApp};
use fakes_domain::{Endpoint, Service, Workspace};
use fakes_store::RequestFilter;
use futures_util::Stream;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

pub type AppState = Arc<FakesApp>;

/// Error HTTP con el cuerpo estándar `{error, message, issues?}`.
pub struct ApiError(AppError);

impl From<AppError> for ApiError {
    fn from(e: AppError) -> Self {
        Self(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::Config(_) | AppError::Store(_) => {
                tracing::error!("error interno: {}", self.0);
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        (status, Json(self.0.to_body())).into_response()
    }
}

type ApiResult<T> = Result<Json<T>, ApiError>;

fn bad_request(msg: impl Into<String>) -> ApiError {
    ApiError(AppError::BadRequest(msg.into()))
}

/// Lee un body JSON con mensajes de error legibles.
fn body<T: DeserializeOwned>(payload: Result<Json<Value>, JsonRejection>) -> Result<T, ApiError> {
    let Json(v) = payload.map_err(|e| bad_request(format!("JSON inválido: {}", e.body_text())))?;
    serde_json::from_value(v).map_err(|e| bad_request(format!("datos inválidos: {e}")))
}

fn raw(payload: Result<Json<Value>, JsonRejection>) -> Result<Value, ApiError> {
    payload
        .map(|Json(v)| v)
        .map_err(|e| bad_request(format!("JSON inválido: {}", e.body_text())))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/status", get(status))
        .route("/reload", post(reload))
        .route("/events", get(events))
        .route("/schemas/{name}", get(schema))
        .route("/system/autostart", get(get_autostart).put(set_autostart))
        .route("/system/shutdown", post(request_shutdown))
        // Importar / exportar
        .route("/export", get(export_bundle))
        .route(
            "/import/spec",
            post(import_spec).layer(axum::extract::DefaultBodyLimit::max(64 * 1024 * 1024)),
        )
        .route("/validate", get(validate_all))
        .route("/detect", post(detect_integrations))
        .route(
            "/import",
            post(import_bundle).layer(axum::extract::DefaultBodyLimit::max(64 * 1024 * 1024)),
        )
        // Plantillas de proveedores
        .route("/templates", get(list_templates))
        .route("/templates/{id}", get(get_template))
        .route("/templates/{id}/install", post(install_template))
        // Espacios de trabajo
        .route("/workspaces", get(list_workspaces).post(create_workspace))
        .route("/workspaces/register", post(register_workspace))
        .route("/workspaces/{id}", axum::routing::delete(forget_workspace))
        .route("/workspaces/{id}/activate", post(activate_workspace))
        .route(
            "/workspace",
            get(active_workspace)
                .put(update_workspace)
                .patch(patch_workspace),
        )
        // Servicios
        .route("/services", get(list_services).post(create_service))
        .route("/services/start-all", post(start_all))
        .route("/services/stop-all", post(stop_all))
        .route(
            "/services/{sid}",
            get(get_service)
                .put(update_service)
                .patch(patch_service)
                .delete(delete_service),
        )
        .route("/services/{sid}/start", post(start_service))
        .route("/services/{sid}/stop", post(stop_service))
        // Endpoints
        .route(
            "/services/{sid}/endpoints",
            get(list_endpoints).post(create_endpoint),
        )
        .route(
            "/services/{sid}/endpoints/validate",
            post(validate_endpoint),
        )
        .route(
            "/services/{sid}/endpoints/{eid}",
            get(get_endpoint)
                .put(update_endpoint)
                .patch(patch_endpoint)
                .delete(delete_endpoint),
        )
        .route(
            "/services/{sid}/endpoints/{eid}/enable",
            post(enable_endpoint),
        )
        .route(
            "/services/{sid}/endpoints/{eid}/disable",
            post(disable_endpoint),
        )
        .route(
            "/services/{sid}/endpoints/{eid}/active-scenario",
            put(set_active_scenario),
        )
        // Recursos simulados persistentes
        .route(
            "/services/{sid}/resources",
            get(list_collections).delete(clear_all_resources),
        )
        .route(
            "/services/{sid}/resources/{col}",
            get(list_resources).delete(clear_collection),
        )
        .route(
            "/services/{sid}/resources/{col}/{*id}",
            get(get_resource).put(put_resource).delete(delete_resource),
        )
        // Workflows
        .route(
            "/services/{sid}/workflows",
            get(list_workflows).post(create_workflow),
        )
        .route(
            "/services/{sid}/workflows/validate",
            post(validate_workflow),
        )
        .route(
            "/services/{sid}/workflows/{wid}",
            get(get_workflow)
                .put(update_workflow)
                .patch(patch_workflow)
                .delete(delete_workflow),
        )
        .route("/services/{sid}/workflows/{wid}/run", post(run_workflow))
        // Ejecuciones
        .route("/executions", get(list_executions).delete(clear_executions))
        .route("/executions/{id}", get(get_execution))
        .route("/executions/{id}/cancel", post(cancel_execution))
        // Historial
        .route("/requests", get(list_requests).delete(clear_requests))
        .route("/requests/{id}", get(get_request))
}

// -- estado ---------------------------------------------------------------

async fn status(State(app): State<AppState>) -> ApiResult<fakes_app::AppStatus> {
    Ok(Json(app.status().await?))
}

async fn reload(State(app): State<AppState>) -> ApiResult<fakes_app::WorkspaceView> {
    Ok(Json(app.reload().await?))
}

/// Eventos en vivo (Server-Sent Events).
async fn events(State(app): State<AppState>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(app.subscribe()).filter_map(|msg| {
        msg.ok().map(|ev| {
            Ok(Event::default()
                .json_data(&ev)
                .unwrap_or_else(|_| Event::default().data("{}")))
        })
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// JSON Schema de los documentos de configuración.
async fn schema(Path(name): Path<String>) -> Result<Json<Value>, ApiError> {
    let schema = match name.as_str() {
        "workspace" => schemars::schema_for!(Workspace),
        "service" => schemars::schema_for!(Service),
        "endpoint" => schemars::schema_for!(Endpoint),
        "workflow" => schemars::schema_for!(fakes_domain::Workflow),
        "bundle" => schemars::schema_for!(fakes_app::Bundle),
        _ => {
            return Err(ApiError(AppError::NotFound(format!(
                "esquema '{name}' desconocido; opciones: workspace, service, endpoint, workflow, bundle"
            ))));
        }
    };
    Ok(Json(serde_json::to_value(schema).unwrap_or(Value::Null)))
}

// -- espacios -------------------------------------------------------------

async fn list_workspaces(State(app): State<AppState>) -> ApiResult<Vec<fakes_app::WorkspaceInfo>> {
    Ok(Json(app.list_workspaces().await))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateWorkspaceBody {
    #[serde(flatten)]
    workspace: Workspace,
    #[serde(default)]
    path: Option<PathBuf>,
    #[serde(default)]
    activate: bool,
}

async fn create_workspace(
    State(app): State<AppState>,
    payload: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<fakes_app::WorkspaceInfo>), ApiError> {
    let b: CreateWorkspaceBody = body(payload)?;
    let info = app
        .create_workspace(b.workspace, b.path, b.activate)
        .await?;
    Ok((StatusCode::CREATED, Json(info)))
}

#[derive(Deserialize)]
struct RegisterBody {
    path: PathBuf,
    #[serde(default)]
    activate: bool,
}

async fn register_workspace(
    State(app): State<AppState>,
    payload: Result<Json<Value>, JsonRejection>,
) -> ApiResult<fakes_app::WorkspaceInfo> {
    let b: RegisterBody = body(payload)?;
    Ok(Json(app.register_workspace(b.path, b.activate).await?))
}

async fn forget_workspace(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    app.forget_workspace(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn activate_workspace(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<fakes_app::WorkspaceView> {
    Ok(Json(app.activate_workspace(&id).await?))
}

async fn active_workspace(State(app): State<AppState>) -> ApiResult<fakes_app::WorkspaceView> {
    Ok(Json(app.active_workspace().await))
}

async fn update_workspace(
    State(app): State<AppState>,
    payload: Result<Json<Value>, JsonRejection>,
) -> ApiResult<Workspace> {
    Ok(Json(app.update_workspace(body(payload)?).await?))
}

async fn patch_workspace(
    State(app): State<AppState>,
    payload: Result<Json<Value>, JsonRejection>,
) -> ApiResult<Workspace> {
    Ok(Json(app.patch_workspace(raw(payload)?).await?))
}

// -- servicios ------------------------------------------------------------

async fn list_services(State(app): State<AppState>) -> ApiResult<Vec<fakes_app::ServiceSummary>> {
    Ok(Json(app.list_services().await))
}

async fn create_service(
    State(app): State<AppState>,
    payload: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<fakes_app::ServiceView>), ApiError> {
    let v = app.create_service(body(payload)?).await?;
    Ok((StatusCode::CREATED, Json(v)))
}

async fn get_service(
    State(app): State<AppState>,
    Path(sid): Path<String>,
) -> ApiResult<fakes_app::ServiceView> {
    Ok(Json(app.get_service(&sid).await?))
}

async fn update_service(
    State(app): State<AppState>,
    Path(sid): Path<String>,
    payload: Result<Json<Value>, JsonRejection>,
) -> ApiResult<fakes_app::ServiceView> {
    Ok(Json(app.update_service(&sid, body(payload)?).await?))
}

async fn patch_service(
    State(app): State<AppState>,
    Path(sid): Path<String>,
    payload: Result<Json<Value>, JsonRejection>,
) -> ApiResult<fakes_app::ServiceView> {
    Ok(Json(app.patch_service(&sid, raw(payload)?).await?))
}

async fn delete_service(
    State(app): State<AppState>,
    Path(sid): Path<String>,
) -> Result<StatusCode, ApiError> {
    app.delete_service(&sid).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn start_service(
    State(app): State<AppState>,
    Path(sid): Path<String>,
) -> ApiResult<fakes_app::ServiceView> {
    Ok(Json(app.start_service(&sid).await?))
}

async fn stop_service(
    State(app): State<AppState>,
    Path(sid): Path<String>,
) -> ApiResult<fakes_app::ServiceView> {
    Ok(Json(app.stop_service(&sid).await?))
}

async fn start_all(State(app): State<AppState>) -> Result<StatusCode, ApiError> {
    app.set_all_running(true).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn stop_all(State(app): State<AppState>) -> Result<StatusCode, ApiError> {
    app.set_all_running(false).await?;
    Ok(StatusCode::NO_CONTENT)
}

// -- endpoints ------------------------------------------------------------

async fn list_endpoints(
    State(app): State<AppState>,
    Path(sid): Path<String>,
) -> ApiResult<Vec<fakes_app::EndpointSummary>> {
    Ok(Json(app.list_endpoints(&sid).await?))
}

async fn create_endpoint(
    State(app): State<AppState>,
    Path(sid): Path<String>,
    payload: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<fakes_app::EndpointView>), ApiError> {
    let v = app.create_endpoint(&sid, body(payload)?).await?;
    Ok((StatusCode::CREATED, Json(v)))
}

async fn validate_endpoint(
    State(app): State<AppState>,
    Path(sid): Path<String>,
    payload: Result<Json<Value>, JsonRejection>,
) -> ApiResult<fakes_app::ValidationReport> {
    Ok(Json(app.validate_endpoint(&sid, body(payload)?).await?))
}

async fn get_endpoint(
    State(app): State<AppState>,
    Path((sid, eid)): Path<(String, String)>,
) -> ApiResult<fakes_app::EndpointView> {
    Ok(Json(app.get_endpoint(&sid, &eid).await?))
}

async fn update_endpoint(
    State(app): State<AppState>,
    Path((sid, eid)): Path<(String, String)>,
    payload: Result<Json<Value>, JsonRejection>,
) -> ApiResult<fakes_app::EndpointView> {
    Ok(Json(app.update_endpoint(&sid, &eid, body(payload)?).await?))
}

async fn patch_endpoint(
    State(app): State<AppState>,
    Path((sid, eid)): Path<(String, String)>,
    payload: Result<Json<Value>, JsonRejection>,
) -> ApiResult<fakes_app::EndpointView> {
    Ok(Json(app.patch_endpoint(&sid, &eid, raw(payload)?).await?))
}

async fn delete_endpoint(
    State(app): State<AppState>,
    Path((sid, eid)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    app.delete_endpoint(&sid, &eid).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn enable_endpoint(
    State(app): State<AppState>,
    Path((sid, eid)): Path<(String, String)>,
) -> ApiResult<fakes_app::EndpointView> {
    Ok(Json(app.set_endpoint_enabled(&sid, &eid, true).await?))
}

async fn disable_endpoint(
    State(app): State<AppState>,
    Path((sid, eid)): Path<(String, String)>,
) -> ApiResult<fakes_app::EndpointView> {
    Ok(Json(app.set_endpoint_enabled(&sid, &eid, false).await?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActiveScenarioBody {
    scenario_id: Option<String>,
}

async fn set_active_scenario(
    State(app): State<AppState>,
    Path((sid, eid)): Path<(String, String)>,
    payload: Result<Json<Value>, JsonRejection>,
) -> ApiResult<fakes_app::EndpointView> {
    let b: ActiveScenarioBody = body(payload)?;
    Ok(Json(
        app.set_active_scenario(&sid, &eid, b.scenario_id).await?,
    ))
}

// -- historial ------------------------------------------------------------

async fn list_requests(
    State(app): State<AppState>,
    Query(filter): Query<RequestFilter>,
) -> ApiResult<Vec<fakes_store::RequestSummary>> {
    Ok(Json(app.list_requests(&filter).await?))
}

async fn get_request(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<fakes_store::RequestRecord> {
    Ok(Json(app.get_request(&id).await?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClearQuery {
    service_id: Option<String>,
}

async fn clear_requests(
    State(app): State<AppState>,
    Query(q): Query<ClearQuery>,
) -> ApiResult<Value> {
    let deleted = app.clear_requests(q.service_id.as_deref()).await?;
    Ok(Json(json!({ "deleted": deleted })))
}

// -- workflows ------------------------------------------------------------

async fn list_workflows(
    State(app): State<AppState>,
    Path(sid): Path<String>,
) -> ApiResult<Vec<fakes_app::WorkflowSummary>> {
    Ok(Json(app.list_workflows(&sid).await?))
}

async fn create_workflow(
    State(app): State<AppState>,
    Path(sid): Path<String>,
    payload: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<fakes_app::WorkflowView>), ApiError> {
    let v = app.create_workflow(&sid, body(payload)?).await?;
    Ok((StatusCode::CREATED, Json(v)))
}

async fn validate_workflow(
    State(app): State<AppState>,
    Path(sid): Path<String>,
    payload: Result<Json<Value>, JsonRejection>,
) -> ApiResult<fakes_app::ValidationReport> {
    Ok(Json(app.validate_workflow(&sid, body(payload)?).await?))
}

async fn get_workflow(
    State(app): State<AppState>,
    Path((sid, wid)): Path<(String, String)>,
) -> ApiResult<fakes_app::WorkflowView> {
    Ok(Json(app.get_workflow(&sid, &wid).await?))
}

async fn update_workflow(
    State(app): State<AppState>,
    Path((sid, wid)): Path<(String, String)>,
    payload: Result<Json<Value>, JsonRejection>,
) -> ApiResult<fakes_app::WorkflowView> {
    Ok(Json(app.update_workflow(&sid, &wid, body(payload)?).await?))
}

async fn patch_workflow(
    State(app): State<AppState>,
    Path((sid, wid)): Path<(String, String)>,
    payload: Result<Json<Value>, JsonRejection>,
) -> ApiResult<fakes_app::WorkflowView> {
    Ok(Json(app.patch_workflow(&sid, &wid, raw(payload)?).await?))
}

async fn delete_workflow(
    State(app): State<AppState>,
    Path((sid, wid)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    app.delete_workflow(&sid, &wid).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RunBody {
    /// Entrada del workflow (`{{ input.X }}`).
    #[serde(default)]
    input: std::collections::BTreeMap<String, Value>,
    /// Contexto simulado opcional: `{"request": {...}, "response": {...}}`.
    #[serde(default)]
    context: Option<Value>,
}

async fn run_workflow(
    State(app): State<AppState>,
    Path((sid, wid)): Path<(String, String)>,
    payload: Option<Json<Value>>,
) -> Result<(StatusCode, Json<fakes_store::ExecutionSummary>), ApiError> {
    let b: RunBody = match payload {
        Some(Json(v)) if !v.is_null() => {
            serde_json::from_value(v).map_err(|e| bad_request(format!("datos inválidos: {e}")))?
        }
        _ => RunBody::default(),
    };
    let s = app.run_workflow(&sid, &wid, b.input, b.context).await?;
    Ok((StatusCode::ACCEPTED, Json(s)))
}

// -- ejecuciones ------------------------------------------------------------

async fn list_executions(
    State(app): State<AppState>,
    Query(filter): Query<fakes_store::ExecutionFilter>,
) -> ApiResult<Vec<fakes_store::ExecutionSummary>> {
    Ok(Json(app.list_executions(&filter).await?))
}

async fn get_execution(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<fakes_app::ExecutionDetail> {
    Ok(Json(app.get_execution(&id).await?))
}

async fn cancel_execution(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<fakes_store::ExecutionSummary> {
    Ok(Json(app.cancel_execution(&id).await?))
}

async fn clear_executions(State(app): State<AppState>) -> ApiResult<Value> {
    let deleted = app.clear_executions().await?;
    Ok(Json(json!({ "deleted": deleted })))
}

// -- sistema ----------------------------------------------------------------

async fn get_autostart() -> ApiResult<fakes_app::AutostartStatus> {
    Ok(Json(fakes_app::system::autostart_status()?))
}

#[derive(Deserialize)]
struct AutostartBody {
    enabled: bool,
}

async fn set_autostart(
    payload: Result<Json<Value>, JsonRejection>,
) -> ApiResult<fakes_app::AutostartStatus> {
    let b: AutostartBody = body(payload)?;
    Ok(Json(fakes_app::system::set_autostart(b.enabled)?))
}

// -- importar / exportar ------------------------------------------------------

async fn import_spec(
    State(app): State<AppState>,
    payload: Result<Json<Value>, JsonRejection>,
) -> ApiResult<fakes_app::SpecImportReport> {
    Ok(Json(app.import_spec(body(payload)?).await?))
}

// -- plantillas -------------------------------------------------------------

async fn list_templates(State(app): State<AppState>) -> ApiResult<Vec<fakes_app::TemplateInfo>> {
    Ok(Json(app.list_templates().await))
}

async fn get_template(Path(id): Path<String>) -> ApiResult<fakes_app::Bundle> {
    Ok(Json(fakes_app::templates::template_bundle(&id)?))
}

async fn install_template(
    State(app): State<AppState>,
    Path(id): Path<String>,
    payload: Option<Json<Value>>,
) -> ApiResult<fakes_app::TemplateInstallReport> {
    let req: fakes_app::TemplateInstallRequest = match payload {
        Some(Json(v)) if !v.is_null() => {
            serde_json::from_value(v).map_err(|e| bad_request(format!("datos inválidos: {e}")))?
        }
        _ => Default::default(),
    };
    Ok(Json(app.install_template(&id, req).await?))
}

// -- recursos -----------------------------------------------------------------

#[derive(Deserialize)]
struct Page {
    limit: Option<u32>,
    offset: Option<u32>,
}

#[derive(serde::Serialize)]
struct Cleared {
    deleted: u64,
}

async fn list_collections(
    State(app): State<AppState>,
    Path(sid): Path<String>,
) -> ApiResult<Vec<fakes_app::CollectionInfo>> {
    Ok(Json(app.list_resource_collections(&sid).await?))
}

async fn clear_all_resources(
    State(app): State<AppState>,
    Path(sid): Path<String>,
) -> ApiResult<Cleared> {
    Ok(Json(Cleared {
        deleted: app.clear_resources(&sid, None).await?,
    }))
}

async fn list_resources(
    State(app): State<AppState>,
    Path((sid, col)): Path<(String, String)>,
    Query(p): Query<Page>,
) -> ApiResult<Vec<fakes_app::ResourceRecord>> {
    Ok(Json(
        app.list_resources(&sid, &col, p.limit, p.offset).await?,
    ))
}

async fn clear_collection(
    State(app): State<AppState>,
    Path((sid, col)): Path<(String, String)>,
) -> ApiResult<Cleared> {
    Ok(Json(Cleared {
        deleted: app.clear_resources(&sid, Some(&col)).await?,
    }))
}

async fn get_resource(
    State(app): State<AppState>,
    Path((sid, col, id)): Path<(String, String, String)>,
) -> ApiResult<fakes_app::ResourceRecord> {
    Ok(Json(app.get_resource(&sid, &col, &id).await?))
}

async fn put_resource(
    State(app): State<AppState>,
    Path((sid, col, id)): Path<(String, String, String)>,
    payload: Result<Json<Value>, JsonRejection>,
) -> ApiResult<fakes_app::ResourceRecord> {
    Ok(Json(
        app.put_resource(&sid, &col, &id, raw(payload)?).await?,
    ))
}

async fn delete_resource(
    State(app): State<AppState>,
    Path((sid, col, id)): Path<(String, String, String)>,
) -> Result<StatusCode, ApiError> {
    app.delete_resource(&sid, &col, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn validate_all(State(app): State<AppState>) -> ApiResult<fakes_app::ValidationReport> {
    Ok(Json(app.validate_all().await?))
}

#[derive(Deserialize)]
struct DetectBody {
    path: std::path::PathBuf,
}

async fn detect_integrations(
    State(app): State<AppState>,
    payload: Result<Json<Value>, JsonRejection>,
) -> ApiResult<fakes_app::DetectionReport> {
    let b: DetectBody = body(payload)?;
    Ok(Json(app.detect_integrations(b.path).await?))
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ExportQuery {
    /// Ids separados por comas; vacío = todos.
    #[serde(default)]
    services: Option<String>,
    #[serde(default)]
    workspace: bool,
    #[serde(default)]
    files: bool,
    #[serde(default)]
    resources: bool,
}

async fn export_bundle(
    State(app): State<AppState>,
    Query(q): Query<ExportQuery>,
) -> Result<Response, ApiError> {
    let opts = fakes_app::ExportOptions {
        services: q
            .services
            .as_deref()
            .unwrap_or("")
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        include_workspace: q.workspace,
        include_files: q.files,
        include_resources: q.resources,
    };
    let bundle = app.export_bundle(&opts).await?;
    let ws = app.status().await?.workspace_id;
    let scope = match opts.services.as_slice() {
        [one] => one.clone(),
        _ => ws,
    };
    let filename = format!("fake-services-{scope}-{}.json", chrono_like_date());
    let mut body = serde_json::to_vec_pretty(&bundle).unwrap_or_default();
    body.push(b'\n');
    Ok((
        [
            (
                axum::http::header::CONTENT_TYPE,
                "application/json".to_string(),
            ),
            (
                axum::http::header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        body,
    )
        .into_response())
}

fn chrono_like_date() -> String {
    // AAAA-MM-DD sin dependencias extra.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Algoritmo de Howard Hinnant: días desde 1970-01-01 → fecha civil.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

async fn import_bundle(
    State(app): State<AppState>,
    payload: Result<Json<Value>, JsonRejection>,
) -> ApiResult<fakes_app::ImportReport> {
    let req: fakes_app::ImportRequest = body(payload)?;
    Ok(Json(app.import_bundle(req).await?))
}

/// Detiene Fake Services de forma ordenada (lo usan `fakes detener` y los instaladores).
async fn request_shutdown(State(app): State<AppState>) -> (StatusCode, Json<Value>) {
    let app = app.clone();
    tokio::spawn(async move {
        // Deja salir la respuesta antes de cerrar.
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        app.request_shutdown();
    });
    (
        StatusCode::ACCEPTED,
        Json(json!({ "message": "Fake Services se está deteniendo" })),
    )
}
