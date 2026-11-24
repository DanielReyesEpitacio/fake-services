//! Servidor MCP de Fake Services.
//!
//! Implementa el Model Context Protocol (JSON-RPC 2.0) con herramientas que
//! llaman a los mismos casos de uso que la interfaz y la API REST. Los
//! esquemas de entrada y salida se generan de los mismos tipos (`schemars`)
//! y son estrictos: las entradas no aceptan campos desconocidos.
//!
//! Transportes: stdio (`fakes mcp`) y HTTP (`POST /mcp` en la API local).

mod tools;
mod tools_phase4;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use fakes_app::{AppError, FakesApp};
use schemars::JsonSchema;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

pub const SUPPORTED_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

pub const INSTRUCTIONS: &str = "Fake Services simula localmente servicios externos (APIs de terceros) para desarrollo. \
Flujo recomendado para un proyecto: 1) detectar_integraciones_proyecto con la ruta del proyecto; \
2) para cada integración, instalar_plantilla si hay plantilla (ver listar_plantillas) o crear_servicio con sus \
endpoints, escenarios y workflows (o importar_openapi / importar_coleccion_postman si hay especificación); \
3) iniciar_servicio y obtener_estado para conocer las URLs locales (las simulaciones responden en \
<URL central>/<prefijo>); 4) probar_endpoint para verificar; 5) consultar listar_solicitudes y listar_ejecuciones. \
Las respuestas admiten plantillas MiniJinja: {{ request.params.id }}, {{ request.body.campo }}, {{ request.query.x }}, \
{{ request.headers['x-h'] }}, {{ vars.NOMBRE }}, {{ uuid() }}, {{ now() }}, {{ fake('rfc') }}. Un texto JSON que es \
solo '{{ expr }}' conserva su tipo. Los escenarios se evalúan en orden (gana el primero cuyas reglas se cumplen); \
uno sin reglas es la respuesta por defecto. Para webhooks asíncronos usa workflows (pasos wait, http, condition, switch, \
setVariables, emit, runWorkflow, resource, fail, end) vinculados desde scenario.workflows. Usa obtener_esquema \
para ver el JSON Schema completo de service, endpoint, workflow o bundle. Para actualizar se usa JSON Merge Patch \
en el campo `changes`.";

type ToolFuture = Pin<Box<dyn Future<Output = Result<Value, AppError>> + Send>>;
type Handler = Arc<dyn Fn(Arc<FakesApp>, Value) -> ToolFuture + Send + Sync>;

pub struct Tool {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    pub output_schema: Value,
    wrap_output: bool,
    read_only: bool,
    handler: Handler,
}

/// Esquema JSON de un tipo, sin `$schema` (algunos clientes no lo aceptan).
fn schema_of<T: JsonSchema>() -> Value {
    let mut v = serde_json::to_value(schemars::schema_for!(T)).unwrap_or(json!({"type": "object"}));
    if let Value::Object(m) = &mut v {
        m.remove("$schema");
    }
    v
}

/// Define una herramienta a partir de una función tipada.
pub(crate) fn tool<I, O, F, Fut>(
    name: &'static str,
    title: &'static str,
    description: &'static str,
    read_only: bool,
    f: F,
) -> Tool
where
    I: DeserializeOwned + JsonSchema + Send + 'static,
    O: Serialize + JsonSchema + 'static,
    F: Fn(Arc<FakesApp>, I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<O, AppError>> + Send + 'static,
{
    let mut input_schema = schema_of::<I>();
    if input_schema.get("type").is_none() {
        input_schema["type"] = json!("object");
    }
    let out = schema_of::<O>();
    // structuredContent debe ser un objeto: los demás tipos se envuelven en `result`.
    let is_object = out.get("type").and_then(Value::as_str) == Some("object");
    let (output_schema, wrap_output) = if is_object {
        (out, false)
    } else {
        let defs = out.get("$defs").cloned();
        let mut inner = out;
        if let Value::Object(m) = &mut inner {
            m.remove("$defs");
        }
        let mut wrapped = json!({
            "type": "object",
            "properties": {"result": inner},
            "required": ["result"],
        });
        if let Some(d) = defs {
            wrapped["$defs"] = d;
        }
        (wrapped, true)
    };
    let f = Arc::new(f);
    let handler: Handler = Arc::new(move |app, args| {
        let f = f.clone();
        Box::pin(async move {
            let input: I = serde_json::from_value(if args.is_null() { json!({}) } else { args })
                .map_err(|e| AppError::BadRequest(format!("argumentos inválidos: {e}")))?;
            let out = f(app, input).await?;
            serde_json::to_value(out).map_err(|e| AppError::BadRequest(e.to_string()))
        })
    });
    Tool {
        name,
        title,
        description,
        input_schema,
        output_schema,
        wrap_output,
        read_only,
        handler,
    }
}

pub struct McpServer {
    app: Arc<FakesApp>,
    tools: Vec<Tool>,
}

impl McpServer {
    pub fn new(app: Arc<FakesApp>) -> Arc<Self> {
        Arc::new(Self {
            app,
            tools: tools::all(),
        })
    }

    pub fn tools(&self) -> &[Tool] {
        &self.tools
    }

    /// Procesa un mensaje o un lote. `None` si no hay respuesta (notificaciones).
    pub async fn handle_payload(&self, payload: Value) -> Option<Value> {
        match payload {
            Value::Array(items) => {
                let mut out = Vec::new();
                for item in items {
                    if let Some(r) = self.handle_message(item).await {
                        out.push(r);
                    }
                }
                (!out.is_empty()).then_some(Value::Array(out))
            }
            other => self.handle_message(other).await,
        }
    }

    pub async fn handle_message(&self, msg: Value) -> Option<Value> {
        let id = msg.get("id").cloned();
        let Some(method) = msg.get("method").and_then(Value::as_str) else {
            // Respuestas del cliente u objetos inválidos.
            return id.map(|id| error(id, -32600, "solicitud inválida"));
        };
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let Some(id) = id else {
            // Notificación (notifications/initialized, cancelled...): sin respuesta.
            return None;
        };
        let result = match method {
            "initialize" => Ok(self.initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(self.list_tools()),
            "tools/call" => Ok(self.call_tool(&params).await),
            "resources/list" => Ok(json!({"resources": []})),
            "resources/templates/list" => Ok(json!({"resourceTemplates": []})),
            "prompts/list" => Ok(json!({"prompts": []})),
            other => Err((-32601, format!("método no soportado: {other}"))),
        };
        Some(match result {
            Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
            Err((code, msg)) => error(id, code, &msg),
        })
    }

    fn initialize(&self, params: &Value) -> Value {
        let requested = params
            .get("protocolVersion")
            .and_then(Value::as_str)
            .unwrap_or("");
        let version = if SUPPORTED_VERSIONS.contains(&requested) {
            requested
        } else {
            SUPPORTED_VERSIONS[0]
        };
        json!({
            "protocolVersion": version,
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {"name": "fake-services", "title": "Fake Services", "version": fakes_app::VERSION},
            "instructions": INSTRUCTIONS,
        })
    }

    fn list_tools(&self) -> Value {
        let tools: Vec<Value> = self
            .tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "title": t.title,
                    "description": t.description,
                    "inputSchema": t.input_schema,
                    "outputSchema": t.output_schema,
                    "annotations": {"readOnlyHint": t.read_only, "openWorldHint": false},
                })
            })
            .collect();
        json!({"tools": tools})
    }

    async fn call_tool(&self, params: &Value) -> Value {
        let name = params.get("name").and_then(Value::as_str).unwrap_or("");
        let args = params.get("arguments").cloned().unwrap_or(Value::Null);
        let Some(tool) = self.tools.iter().find(|t| t.name == name) else {
            return tool_error(&AppError::NotFound(format!(
                "no existe la herramienta '{name}'"
            )));
        };
        match (tool.handler)(self.app.clone(), args).await {
            Ok(v) => {
                let structured = if tool.wrap_output {
                    json!({"result": v})
                } else {
                    v
                };
                let text = serde_json::to_string_pretty(&structured).unwrap_or_default();
                json!({
                    "content": [{"type": "text", "text": text}],
                    "structuredContent": structured,
                    "isError": false,
                })
            }
            Err(e) => tool_error(&e),
        }
    }
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn tool_error(e: &AppError) -> Value {
    let body = e.to_body();
    let mut text = body.message.clone();
    for i in &body.issues {
        text.push_str(&format!("\n- {}: {}", i.field, i.message));
    }
    // Sin structuredContent: el esquema de salida describe solo los resultados correctos.
    json!({
        "content": [{"type": "text", "text": text}],
        "isError": true,
    })
}

#[cfg(test)]
mod tests;
