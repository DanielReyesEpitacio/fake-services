//! Catálogo de herramientas MCP. Cada una traduce sus argumentos a un caso de
//! uso de `FakesApp`; no hay lógica de negocio aquí.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use fakes_app::bundle::{ExportOptions, ImportMode, ImportRequest, OnConflict};
use fakes_app::specs::{SpecFormatInput, SpecImportRequest};
use fakes_app::{AppError, FakesApp};
use fakes_domain::{Endpoint, HttpMethod, Service, Workflow, Workspace};
use fakes_store::{ExecutionFilter, RequestFilter};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Tool, tool};

// -- argumentos ---------------------------------------------------------------

#[derive(Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct Empty {}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceRef {
    /// Identificador del servicio (p. ej. `mifiel`).
    pub service_id: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EndpointRef {
    pub service_id: String,
    pub endpoint_id: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowRef {
    pub service_id: String,
    pub workflow_id: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionRef {
    pub execution_id: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceRef {
    pub workspace_id: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateWorkspace {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Carpeta donde guardarlo (p. ej. `<proyecto>/.fakes` para versionarlo en Git).
    /// Vacía = carpeta de datos de Fake Services.
    #[serde(default)]
    pub path: Option<PathBuf>,
    /// Activarlo al crearlo (por defecto, sí).
    #[serde(default = "yes")]
    pub activate: bool,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateService {
    /// Servicio completo. `id` y `prefix` se generan si faltan. Puede incluir
    /// `endpoints` y `workflows` para crear toda la simulación en una llamada.
    pub service: Service,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PatchService {
    pub service_id: String,
    /// Cambios en formato JSON Merge Patch (null elimina un campo).
    pub changes: Value,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateEndpoint {
    pub service_id: String,
    /// Endpoint completo. `id` y los `id` de escenarios se generan si faltan.
    pub endpoint: Endpoint,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PatchEndpoint {
    pub service_id: String,
    pub endpoint_id: String,
    /// Cambios en formato JSON Merge Patch. Los arreglos (p. ej. `scenarios`) se reemplazan completos.
    pub changes: Value,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForceScenario {
    pub service_id: String,
    pub endpoint_id: String,
    /// Escenario a forzar; `null` vuelve a evaluar las reglas.
    #[serde(default)]
    pub scenario_id: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateWorkflow {
    pub service_id: String,
    /// Workflow completo. `id` y los `id` de pasos se generan si faltan.
    pub workflow: Workflow,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PatchWorkflow {
    pub service_id: String,
    pub workflow_id: String,
    /// Cambios en formato JSON Merge Patch. `steps` se reemplaza completo.
    pub changes: Value,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunWorkflow {
    pub service_id: String,
    pub workflow_id: String,
    /// Entrada disponible como `input.*`.
    #[serde(default)]
    pub input: BTreeMap<String, Value>,
    /// Contexto simulado opcional: `{"request": {...}, "response": {...}}`.
    #[serde(default)]
    pub context: Option<Value>,
}

#[derive(Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListExecutions {
    #[serde(default)]
    pub service_id: Option<String>,
    #[serde(default)]
    pub workflow_id: Option<String>,
    /// Estados separados por comas: pending, running, waiting, completed, failed, cancelled.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListRequests {
    #[serde(default)]
    pub service_id: Option<String>,
    #[serde(default)]
    pub endpoint_id: Option<String>,
    #[serde(default)]
    pub errors_only: bool,
    /// Texto contenido en la ruta.
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestRef {
    pub request_id: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportSpec {
    /// Documento completo (JSON o YAML).
    pub content: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub on_conflict: OnConflict,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportBundle {
    /// Exportación de Fake Services (`format: "fake-services"`).
    pub bundle: Value,
    #[serde(default)]
    pub mode: ImportMode,
    #[serde(default)]
    pub on_conflict: OnConflict,
    #[serde(default)]
    pub workspace_name: Option<String>,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Export {
    /// Servicios a exportar; vacío = todos.
    #[serde(default)]
    pub service_ids: Vec<String>,
    #[serde(default)]
    pub include_workspace: bool,
    #[serde(default)]
    pub include_files: bool,
    #[serde(default)]
    pub include_resources: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DetectArgs {
    /// Carpeta del proyecto a analizar (ruta absoluta recomendada).
    pub path: PathBuf,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SchemaArgs {
    /// `workspace`, `service`, `endpoint`, `workflow` o `bundle`.
    pub kind: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TryEndpoint {
    pub service_id: String,
    pub method: HttpMethod,
    /// Ruta relativa al servicio, con query si hace falta: `/documents/doc_1/sign?x=1`.
    pub path: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// Body: un objeto o arreglo se envía como JSON; un texto, tal cual.
    #[serde(default)]
    pub body: Option<Value>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TryResult {
    pub url: String,
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    /// JSON si se pudo interpretar; si no, texto.
    pub body: Value,
    pub duration_ms: u64,
}

#[derive(Serialize, JsonSchema)]
pub struct Done {
    pub ok: bool,
    pub message: String,
}

pub(crate) fn done(message: impl Into<String>) -> Done {
    Done {
        ok: true,
        message: message.into(),
    }
}

// -- catálogo -----------------------------------------------------------------

pub fn all() -> Vec<Tool> {
    vec![
        // Estado y espacios
        tool("obtener_estado", "Obtener estado", "Versión, espacio activo, URL central de simulaciones, puertos abiertos y contadores.", true,
            |app: Arc<FakesApp>, _: Empty| async move { app.status().await }),
        tool("listar_espacios_trabajo", "Listar espacios de trabajo", "Lista los espacios de trabajo registrados e indica cuál está activo.", true,
            |app: Arc<FakesApp>, _: Empty| async move { Ok(app.list_workspaces().await) }),
        tool("crear_espacio_trabajo", "Crear espacio de trabajo", "Crea un espacio de trabajo (opcionalmente en la carpeta de un proyecto para versionarlo en Git) y lo activa.", false,
            |app: Arc<FakesApp>, a: CreateWorkspace| async move {
                let mut ws = Workspace::new("", a.name);
                ws.description = a.description.unwrap_or_default();
                app.create_workspace(ws, a.path, a.activate).await
            }),
        tool("obtener_espacio_trabajo", "Obtener espacio de trabajo", "Espacio activo: ajustes, servicios y archivos con problemas.", true,
            |app: Arc<FakesApp>, _: Empty| async move { Ok(app.active_workspace().await) }),
        tool("activar_espacio_trabajo", "Activar espacio de trabajo", "Cambia el espacio activo (las simulaciones pasan a ser las de ese espacio).", false,
            |app: Arc<FakesApp>, a: WorkspaceRef| async move { app.activate_workspace(&a.workspace_id).await }),

        // Servicios
        tool("listar_servicios", "Listar servicios", "Servicios del espacio activo con su URL local, estado y número de endpoints y workflows.", true,
            |app: Arc<FakesApp>, _: Empty| async move { Ok(app.list_services().await) }),
        tool("obtener_servicio", "Obtener servicio", "Configuración completa de un servicio (con endpoints y workflows) y su URL.", true,
            |app: Arc<FakesApp>, a: ServiceRef| async move { app.get_service(&a.service_id).await }),
        tool("crear_servicio", "Crear servicio", "Crea un servicio simulado. Puede incluir endpoints, escenarios y workflows completos. Se inicia automáticamente.", false,
            |app: Arc<FakesApp>, a: CreateService| async move { app.create_service(a.service).await }),
        tool("actualizar_servicio", "Actualizar servicio", "Modifica la configuración de un servicio con JSON Merge Patch (nombre, prefijo, puerto, auth, variables...). Los endpoints y workflows se editan con sus propias herramientas.", false,
            |app: Arc<FakesApp>, a: PatchService| async move { app.patch_service(&a.service_id, a.changes).await }),
        tool("eliminar_servicio", "Eliminar servicio", "Elimina un servicio con sus endpoints y workflows.", false,
            |app: Arc<FakesApp>, a: ServiceRef| async move { app.delete_service(&a.service_id).await.map(|_| done("servicio eliminado")) }),
        tool("iniciar_servicio", "Iniciar servicio", "Inicia un servicio: empieza a responder en su URL.", false,
            |app: Arc<FakesApp>, a: ServiceRef| async move { app.start_service(&a.service_id).await }),
        tool("detener_servicio", "Detener servicio", "Detiene un servicio: responde 503 hasta iniciarlo de nuevo.", false,
            |app: Arc<FakesApp>, a: ServiceRef| async move { app.stop_service(&a.service_id).await }),

        // Endpoints
        tool("listar_endpoints", "Listar endpoints", "Endpoints de un servicio con su URL completa.", true,
            |app: Arc<FakesApp>, a: ServiceRef| async move { app.list_endpoints(&a.service_id).await }),
        tool("obtener_endpoint", "Obtener endpoint", "Configuración completa de un endpoint: solicitud esperada, escenarios, reglas y respuestas.", true,
            |app: Arc<FakesApp>, a: EndpointRef| async move { app.get_endpoint(&a.service_id, &a.endpoint_id).await }),
        tool("crear_endpoint", "Crear endpoint", "Crea un endpoint con sus escenarios. Valida rutas, reglas y plantillas antes de guardar.", false,
            |app: Arc<FakesApp>, a: CreateEndpoint| async move { app.create_endpoint(&a.service_id, a.endpoint).await }),
        tool("actualizar_endpoint", "Actualizar endpoint", "Modifica un endpoint con JSON Merge Patch.", false,
            |app: Arc<FakesApp>, a: PatchEndpoint| async move { app.patch_endpoint(&a.service_id, &a.endpoint_id, a.changes).await }),
        tool("eliminar_endpoint", "Eliminar endpoint", "Elimina un endpoint.", false,
            |app: Arc<FakesApp>, a: EndpointRef| async move { app.delete_endpoint(&a.service_id, &a.endpoint_id).await.map(|_| done("endpoint eliminado")) }),
        tool("habilitar_endpoint", "Habilitar endpoint", "Habilita un endpoint.", false,
            |app: Arc<FakesApp>, a: EndpointRef| async move { app.set_endpoint_enabled(&a.service_id, &a.endpoint_id, true).await }),
        tool("deshabilitar_endpoint", "Deshabilitar endpoint", "Deshabilita un endpoint (deja de responder; la ruta da 404).", false,
            |app: Arc<FakesApp>, a: EndpointRef| async move { app.set_endpoint_enabled(&a.service_id, &a.endpoint_id, false).await }),
        tool("forzar_escenario", "Forzar escenario", "Fuerza un escenario de un endpoint (p. ej. para simular un error) o vuelve a las reglas con null.", false,
            |app: Arc<FakesApp>, a: ForceScenario| async move { app.set_active_scenario(&a.service_id, &a.endpoint_id, a.scenario_id).await }),
        tool("probar_endpoint", "Probar endpoint", "Envía una solicitud real a la simulación y devuelve la respuesta (para verificar la configuración).", false,
            |app: Arc<FakesApp>, a: TryEndpoint| async move { try_endpoint(app, a).await }),

        // Workflows
        tool("listar_workflows", "Listar workflows", "Workflows de un servicio y qué escenarios los inician.", true,
            |app: Arc<FakesApp>, a: ServiceRef| async move { app.list_workflows(&a.service_id).await }),
        tool("obtener_workflow", "Obtener workflow", "Definición completa de un workflow.", true,
            |app: Arc<FakesApp>, a: WorkflowRef| async move { app.get_workflow(&a.service_id, &a.workflow_id).await }),
        tool("crear_workflow", "Crear workflow", "Crea un workflow (pasos: wait, http, condition, setVariables, emit, runWorkflow, resource, fail, end). Para iniciarlo desde un endpoint agrega {workflowId} a scenario.workflows.", false,
            |app: Arc<FakesApp>, a: CreateWorkflow| async move { app.create_workflow(&a.service_id, a.workflow).await }),
        tool("actualizar_workflow", "Actualizar workflow", "Modifica un workflow con JSON Merge Patch.", false,
            |app: Arc<FakesApp>, a: PatchWorkflow| async move { app.patch_workflow(&a.service_id, &a.workflow_id, a.changes).await }),
        tool("eliminar_workflow", "Eliminar workflow", "Elimina un workflow (falla si algún escenario o workflow lo usa).", false,
            |app: Arc<FakesApp>, a: WorkflowRef| async move { app.delete_workflow(&a.service_id, &a.workflow_id).await.map(|_| done("workflow eliminado")) }),
        tool("validar_workflow", "Validar workflow", "Valida un workflow sin guardarlo (estructura, saltos, límites y plantillas).", true,
            |app: Arc<FakesApp>, a: CreateWorkflow| async move { app.validate_workflow(&a.service_id, a.workflow).await }),
        tool("ejecutar_workflow", "Ejecutar workflow", "Ejecuta un workflow manualmente con una entrada y un contexto simulado.", false,
            |app: Arc<FakesApp>, a: RunWorkflow| async move { app.run_workflow(&a.service_id, &a.workflow_id, a.input, a.context).await }),
        tool("cancelar_workflow", "Cancelar ejecución de workflow", "Cancela una ejecución en curso.", false,
            |app: Arc<FakesApp>, a: ExecutionRef| async move { app.cancel_execution(&a.execution_id).await }),

        // Ejecuciones y solicitudes
        tool("listar_ejecuciones", "Listar ejecuciones", "Ejecuciones de workflows, filtrables por servicio, workflow y estado.", true,
            |app: Arc<FakesApp>, a: ListExecutions| async move {
                app.list_executions(&ExecutionFilter { service_id: a.service_id, workflow_id: a.workflow_id, status: a.status, before_ms: None, limit: a.limit }).await
            }),
        tool("obtener_ejecucion", "Obtener ejecución", "Estado completo de una ejecución: paso actual, contexto (variables, resultados de pasos) y error.", true,
            |app: Arc<FakesApp>, a: ExecutionRef| async move { app.get_execution(&a.execution_id).await.map(|d| d.execution) }),
        tool("obtener_linea_tiempo", "Obtener línea de tiempo", "Eventos de una ejecución en orden: esperas, solicitudes HTTP, respuestas, reintentos, condiciones...", true,
            |app: Arc<FakesApp>, a: ExecutionRef| async move { app.get_execution(&a.execution_id).await.map(|d| d.timeline) }),
        tool("listar_solicitudes", "Listar solicitudes", "Historial de solicitudes recibidas por las simulaciones.", true,
            |app: Arc<FakesApp>, a: ListRequests| async move {
                app.list_requests(&RequestFilter { service_id: a.service_id, endpoint_id: a.endpoint_id, errors_only: a.errors_only, search: a.search, before_ms: None, limit: a.limit }).await
            }),
        tool("obtener_solicitud", "Obtener solicitud", "Detalle de una solicitud: headers, body, respuesta y escenario elegido.", true,
            |app: Arc<FakesApp>, a: RequestRef| async move { app.get_request(&a.request_id).await }),

        // Importación y configuración
        tool("importar_openapi", "Importar OpenAPI", "Crea un servicio desde un documento OpenAPI 3.x o Swagger 2.0 (JSON o YAML): endpoints, validación y un escenario por respuesta.", false,
            |app: Arc<FakesApp>, a: ImportSpec| async move { app.import_spec(spec_req(a, SpecFormatInput::Openapi)).await }),
        tool("importar_coleccion_postman", "Importar colección de Postman", "Crea un servicio desde una colección de Postman v2.x; los ejemplos guardados se vuelven escenarios.", false,
            |app: Arc<FakesApp>, a: ImportSpec| async move { app.import_spec(spec_req(a, SpecFormatInput::Postman)).await }),
        tool("importar_paquete", "Importar paquete", "Importa una exportación de Fake Services (servicios, workflows, recursos, ajustes).", false,
            |app: Arc<FakesApp>, a: ImportBundle| async move {
                let bundle = serde_json::from_value(a.bundle).map_err(|e| AppError::BadRequest(format!("paquete inválido: {e}")))?;
                app.import_bundle(ImportRequest { bundle, mode: a.mode, on_conflict: a.on_conflict, workspace_name: a.workspace_name, path: None, dry_run: a.dry_run }).await
            }),
        tool("exportar", "Exportar", "Exporta servicios (y opcionalmente ajustes, archivos y recursos) como un paquete JSON.", true,
            |app: Arc<FakesApp>, a: Export| async move {
                app.export_bundle(&ExportOptions { services: a.service_ids, include_workspace: a.include_workspace, include_files: a.include_files, include_resources: a.include_resources }).await
            }),
        tool("validar_configuracion", "Validar configuración", "Valida toda la configuración del espacio activo y lista los problemas encontrados.", true,
            |app: Arc<FakesApp>, _: Empty| async move { app.validate_all().await }),
        tool("detectar_integraciones_proyecto", "Detectar integraciones del proyecto", "Analiza el código de un proyecto (dependencias, URLs y variables de entorno) y devuelve los servicios externos que usa, con evidencia y plantilla sugerida.", true,
            |app: Arc<FakesApp>, a: DetectArgs| async move { app.detect_integrations(a.path).await }),
        tool("obtener_esquema", "Obtener esquema", "JSON Schema completo de workspace, service, endpoint, workflow o bundle.", true,
            |_app: Arc<FakesApp>, a: SchemaArgs| async move { schema_for(&a.kind) }),
    ]
    .into_iter()
    .chain(crate::tools_phase4::all())
    .collect()
}

fn spec_req(a: ImportSpec, format: SpecFormatInput) -> SpecImportRequest {
    SpecImportRequest {
        content: a.content,
        format: Some(format),
        name: a.name,
        prefix: a.prefix,
        on_conflict: a.on_conflict,
        dry_run: a.dry_run,
    }
}

fn schema_for(kind: &str) -> Result<Value, AppError> {
    let s = match kind {
        "workspace" => schemars::schema_for!(Workspace),
        "service" => schemars::schema_for!(Service),
        "endpoint" => schemars::schema_for!(Endpoint),
        "workflow" => schemars::schema_for!(Workflow),
        "bundle" => schemars::schema_for!(fakes_app::Bundle),
        other => {
            return Err(AppError::BadRequest(format!(
                "esquema '{other}' desconocido; opciones: workspace, service, endpoint, workflow, bundle"
            )));
        }
    };
    Ok(serde_json::to_value(s).unwrap_or(Value::Null))
}

async fn try_endpoint(app: Arc<FakesApp>, a: TryEndpoint) -> Result<TryResult, AppError> {
    let svc = app.get_service(&a.service_id).await?;
    if !svc.running {
        return Err(AppError::Conflict(format!(
            "el servicio '{}' está detenido; inícialo con iniciar_servicio",
            a.service_id
        )));
    }
    let path = if a.path.starts_with('/') {
        a.path.clone()
    } else {
        format!("/{}", a.path)
    };
    // Dirección real del servidor central (puede diferir de la configurada si es 0).
    let central = app.status().await?.central_url;
    let base = if central.is_empty() || svc.service.prefix.is_empty() {
        svc.port_url.clone().unwrap_or_else(|| svc.url.clone())
    } else {
        format!("{}{}", central.trim_end_matches('/'), svc.service.prefix)
    };
    let url = format!("{}{}", base.trim_end_matches('/'), path);
    let method = match a.method {
        HttpMethod::Any => "GET",
        m => m.as_str(),
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .no_proxy()
        .build()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let mut req = client.request(
        reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET),
        &url,
    );
    let has_ct = a
        .headers
        .keys()
        .any(|k| k.eq_ignore_ascii_case("content-type"));
    for (k, v) in &a.headers {
        req = req.header(k, v);
    }
    match a.body {
        Some(Value::String(s)) => req = req.body(s),
        Some(v) if !v.is_null() => {
            if !has_ct {
                req = req.header("content-type", "application/json");
            }
            req = req.body(serde_json::to_vec(&v).unwrap_or_default());
        }
        _ => {}
    }
    let started = Instant::now();
    let resp = req
        .send()
        .await
        .map_err(|e| AppError::BadRequest(format!("no se pudo llamar a {url}: {e}")))?;
    let status = resp.status().as_u16();
    let headers = resp
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    let bytes = resp.bytes().await.unwrap_or_default();
    let body = serde_json::from_slice(&bytes).unwrap_or_else(|_| {
        let text: String = String::from_utf8_lossy(&bytes)
            .chars()
            .take(64 * 1024)
            .collect();
        if text.is_empty() {
            Value::Null
        } else {
            Value::String(text)
        }
    });
    Ok(TryResult {
        url,
        status,
        headers,
        body,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}
