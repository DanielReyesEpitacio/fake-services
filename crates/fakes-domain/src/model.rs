//! Tipos declarativos. Esta es la fuente de verdad que se guarda en JSON,
//! se versiona en Git y se comparte entre la UI, la API REST y el MCP.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SCHEMA_VERSION: u32 = 1;

fn default_true() -> bool {
    true
}

fn is_true(v: &bool) -> bool {
    *v
}

fn is_zero(v: &u64) -> bool {
    *v == 0
}

// ---------------------------------------------------------------------------
// Espacio de trabajo
// ---------------------------------------------------------------------------

/// Espacio de trabajo: agrupa servicios y su configuración común.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    /// Identificador estable (slug). Si se omite al crear, se genera del nombre.
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default)]
    pub server: ServerSettings,
    /// Variables disponibles en plantillas como `{{ vars.NOMBRE }}`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, String>,
    #[serde(default)]
    pub history: HistorySettings,
    #[serde(default)]
    pub webhooks: WebhookSettings,
}

fn schema_version() -> u32 {
    SCHEMA_VERSION
}

impl Workspace {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            id: id.into(),
            name: name.into(),
            description: String::new(),
            server: ServerSettings::default(),
            variables: BTreeMap::new(),
            history: HistorySettings::default(),
            webhooks: WebhookSettings::default(),
        }
    }
}

/// Política de red para las solicitudes que hacen los workflows (protección SSRF).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WebhookSettings {
    /// Permitir `localhost` / `127.0.0.1` (lo normal para callbacks en desarrollo).
    #[serde(default = "default_true")]
    pub allow_loopback: bool,
    /// Permitir redes privadas (10.x, 172.16-31.x, 192.168.x, fc00::/7).
    #[serde(default)]
    pub allow_private_networks: bool,
    /// Hosts o IPs permitidos aunque la política los bloquee (p. ej. `host.docker.internal`).
    /// Las direcciones de metadatos de nube nunca se permiten.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_hosts: Vec<String>,
}

impl Default for WebhookSettings {
    fn default() -> Self {
        Self {
            allow_loopback: true,
            allow_private_networks: false,
            allowed_hosts: Vec::new(),
        }
    }
}

/// Servidor HTTP central donde se publican todos los servicios con su prefijo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServerSettings {
    /// Dirección de escucha. Por seguridad, `127.0.0.1` por defecto.
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    /// Responder CORS automáticamente (útil para frontends en desarrollo).
    #[serde(default = "default_true")]
    pub cors: bool,
    /// Tamaño máximo del body de una solicitud, en bytes.
    #[serde(default = "default_max_body")]
    pub max_body_bytes: u64,
}

fn default_host() -> String {
    "127.0.0.1".into()
}
fn default_port() -> u16 {
    4000
}
fn default_max_body() -> u64 {
    10 * 1024 * 1024
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            cors: true,
            max_body_bytes: default_max_body(),
        }
    }
}

/// Configuración del historial de solicitudes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HistorySettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Máximo de solicitudes que se conservan.
    #[serde(default = "default_history_limit")]
    pub max_entries: u32,
    /// Headers cuyo valor se oculta en el historial (sin distinguir mayúsculas).
    #[serde(default = "default_redact")]
    pub redact_headers: Vec<String>,
    /// Máximo de bytes de body guardados por solicitud/respuesta.
    #[serde(default = "default_body_capture")]
    pub max_body_capture_bytes: u64,
}

fn default_history_limit() -> u32 {
    5000
}
fn default_body_capture() -> u64 {
    256 * 1024
}
fn default_redact() -> Vec<String> {
    [
        "authorization",
        "proxy-authorization",
        "cookie",
        "set-cookie",
        "x-api-key",
        "api-key",
        "x-auth-token",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

impl Default for HistorySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            max_entries: default_history_limit(),
            redact_headers: default_redact(),
            max_body_capture_bytes: default_body_capture(),
        }
    }
}

// ---------------------------------------------------------------------------
// Servicio
// ---------------------------------------------------------------------------

/// Un proveedor externo simulado (MiFiel, Belvo, AWS S3...).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    /// Identificador estable (slug), también nombre de la carpeta.
    /// Si se omite al crear, se genera del nombre.
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// URL base del proveedor real (informativa, para documentación e importación).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_url: Option<String>,
    /// Prefijo de rutas en el servidor central, p. ej. `/mifiel`.
    /// Si se omite al crear, se usa `/<id>`.
    #[serde(default)]
    pub prefix: String,
    /// Puerto propio opcional. Si se define, el servicio también responde
    /// en `http://host:puerto/` sin prefijo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Iniciar el servicio al arrancar la aplicación.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub auto_start: bool,
    /// Variables del servicio (se combinan con las del espacio de trabajo).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "AuthConfig::is_none")]
    pub auth: AuthConfig,
    /// Los endpoints se guardan en archivos propios; aquí se cargan en memoria.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub endpoints: Vec<Endpoint>,
    /// Los workflows se guardan en archivos propios; aquí se cargan en memoria.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workflows: Vec<crate::workflow::Workflow>,
}

/// Autenticación que el servicio simulado exige a sus clientes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AuthConfig {
    #[default]
    None,
    /// `Authorization: Bearer <token>`. Si `token` es nulo, se acepta cualquiera.
    #[serde(rename_all = "camelCase")]
    Bearer {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        token: Option<String>,
    },
    /// API key en un header.
    #[serde(rename_all = "camelCase")]
    ApiKey {
        header: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<String>,
    },
    /// `Authorization: Basic ...`.
    #[serde(rename_all = "camelCase")]
    Basic {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        username: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        password: Option<String>,
    },
}

impl AuthConfig {
    pub fn is_none(&self) -> bool {
        matches!(self, AuthConfig::None)
    }
}

// ---------------------------------------------------------------------------
// Endpoint
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
    /// Cualquier método.
    #[serde(rename = "*")]
    Any,
}

impl HttpMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Head => "HEAD",
            HttpMethod::Options => "OPTIONS",
            HttpMethod::Any => "*",
        }
    }

    pub fn matches(&self, method: &str) -> bool {
        match self {
            HttpMethod::Any => true,
            m => m.as_str().eq_ignore_ascii_case(method),
        }
    }
}

/// Una ruta simulada dentro de un servicio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    /// Identificador estable dentro del servicio (slug). Si se omite al
    /// crear, se genera de método y ruta (`post-documents-id-sign`).
    #[serde(default)]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub method: HttpMethod,
    /// Ruta relativa al prefijo. Parámetros con `{nombre}` y comodín final `{*resto}`.
    pub path: String,
    #[serde(default)]
    pub request: RequestSpec,
    /// Escenarios evaluados en orden; gana el primero cuyas reglas coinciden.
    /// Un escenario sin reglas funciona como respuesta por defecto.
    #[serde(default)]
    pub scenarios: Vec<Scenario>,
    /// Fuerza un escenario concreto ignorando las reglas (útil para pruebas).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_scenario: Option<String>,
    /// Respuesta cuando la solicitud no cumple `request`. Por defecto HTTP 422.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_validation_error: Option<ResponseDef>,
}

/// Lo que el endpoint espera recibir.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct RequestSpec {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<ParamSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub query: Vec<ParamSpec>,
    /// Documentación/validación de parámetros de ruta (`{id}`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_params: Vec<ParamSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<BodySpec>,
}

impl RequestSpec {
    pub fn is_empty(&self) -> bool {
        self.headers.is_empty()
            && self.query.is_empty()
            && self.path_params.is_empty()
            && self.body.is_none()
    }
}

/// Header, query o parámetro de ruta esperado.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ParamSpec {
    pub name: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Expresión regular que debe cumplir el valor completo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub example: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub enum BodyKind {
    #[default]
    Any,
    Json,
    Xml,
    Text,
    Form,
    Multipart,
    Binary,
}

/// Body esperado.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BodySpec {
    #[serde(default)]
    pub kind: BodyKind,
    #[serde(default)]
    pub required: bool,
    /// JSON Schema que debe cumplir el body (JSON, form o multipart).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub example: Option<Value>,
}

// ---------------------------------------------------------------------------
// Escenarios y reglas
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Scenario {
    /// Identificador dentro del endpoint (slug). Si se omite, se genera del nombre.
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "RuleMode::is_all")]
    pub rule_mode: RuleMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<Rule>,
    pub response: ResponseDef,
    /// Workflows que inicia este escenario.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workflows: Vec<crate::workflow::WorkflowLink>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub enum RuleMode {
    /// Todas las reglas deben cumplirse.
    #[default]
    All,
    /// Basta con una.
    Any,
}

impl RuleMode {
    pub fn is_all(&self) -> bool {
        matches!(self, RuleMode::All)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum RuleSource {
    Header,
    Query,
    Path,
    /// Body: `key` es una ruta con puntos (`user.items.0.id`); vacía = body completo.
    Body,
    Method,
    /// Variable del espacio/servicio.
    Variable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Operator {
    Equals,
    NotEquals,
    Contains,
    NotContains,
    Exists,
    NotExists,
    GreaterThan,
    LessThan,
    Regex,
}

/// Regla de coincidencia sobre un dato de la solicitud.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub source: RuleSource,
    #[serde(default)]
    pub key: String,
    pub operator: Operator,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

// ---------------------------------------------------------------------------
// Respuestas
// ---------------------------------------------------------------------------

/// Respuesta que devuelve un escenario. Headers y body admiten plantillas
/// (`{{ request.params.id }}`, `{{ uuid() }}`, `{{ vars.TOKEN }}`...).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResponseDef {
    pub status: u16,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "ResponseBody::is_empty")]
    pub body: ResponseBody,
    /// Retraso antes de responder, en milisegundos.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub delay_ms: u64,
    /// Variación aleatoria adicional (0..jitter) en milisegundos.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub jitter_ms: u64,
}

impl ResponseDef {
    pub fn json(status: u16, body: Value) -> Self {
        Self {
            status,
            headers: BTreeMap::new(),
            body: ResponseBody::Json { content: body },
            delay_ms: 0,
            jitter_ms: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ResponseBody {
    #[default]
    None,
    /// JSON. Cada texto se procesa como plantilla; si un texto es solo
    /// `{{ expr }}`, se inserta el valor con su tipo (número, objeto...).
    Json { content: Value },
    /// Texto, XML, HTML... procesado como plantilla.
    #[serde(rename_all = "camelCase")]
    Text {
        content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content_type: Option<String>,
    },
    /// Contenido binario en base64 (sin plantillas).
    #[serde(rename_all = "camelCase")]
    Binary {
        base64: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content_type: Option<String>,
    },
    /// Archivo dentro de la carpeta `files/` del espacio de trabajo.
    #[serde(rename_all = "camelCase")]
    File {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content_type: Option<String>,
    },
}

impl ResponseBody {
    pub fn is_empty(&self) -> bool {
        matches!(self, ResponseBody::None)
    }
}
