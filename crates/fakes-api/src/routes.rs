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
        _ => {
            return Err(ApiError(AppError::NotFound(format!(
                "esquema '{name}' desconocido; opciones: workspace, service, endpoint"
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
