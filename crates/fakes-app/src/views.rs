//! Vistas (DTOs) que devuelven los casos de uso. Son las mismas para la
//! UI, la API REST y el MCP.

use fakes_config::LoadProblem;
use fakes_domain::{Endpoint, HttpMethod, Service, Workspace};
use fakes_server::ListenerStatus;
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub path: String,
    pub active: bool,
    /// `false` si la carpeta ya no existe o no se pudo leer.
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceView {
    pub path: String,
    pub workspace: Workspace,
    pub services: Vec<ServiceSummary>,
    /// Archivos que no se pudieron cargar.
    pub problems: Vec<LoadProblem>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub prefix: String,
    pub port: Option<u16>,
    pub running: bool,
    /// URL base local del servicio simulado.
    pub url: String,
    pub endpoint_count: usize,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceView {
    pub service: Service,
    pub running: bool,
    pub url: String,
    /// URL alternativa en el puerto propio, si lo tiene.
    pub port_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EndpointSummary {
    pub id: String,
    pub name: String,
    pub method: HttpMethod,
    pub path: String,
    pub enabled: bool,
    pub scenario_count: usize,
    pub active_scenario: Option<String>,
    /// URL completa para llamar al endpoint.
    pub url: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EndpointView {
    pub service_id: String,
    pub endpoint: Endpoint,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub version: String,
    pub workspace_id: String,
    pub workspace_name: String,
    pub workspace_path: String,
    pub central_url: String,
    #[schemars(with = "Vec<serde_json::Value>")]
    pub listeners: Vec<ListenerStatus>,
    pub services_total: usize,
    pub services_running: usize,
    pub endpoints_total: usize,
    pub requests_total: i64,
    pub problems: Vec<LoadProblem>,
}

/// Resultado de validar sin guardar.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
    pub valid: bool,
    pub issues: Vec<fakes_domain::Issue>,
}
