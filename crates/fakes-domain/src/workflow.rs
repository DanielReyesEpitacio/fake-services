//! Workflows declarativos: secuencias de pasos que una simulación ejecuta
//! de forma asíncrona (esperas, webhooks, condiciones, variables...).
//!
//! Un workflow pertenece a un servicio. Se inicia desde un escenario de un
//! endpoint (al recibir la solicitud o al completar la respuesta), de forma
//! manual, desde otro workflow o periódicamente.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{Operator, RuleMode, SCHEMA_VERSION};

fn default_true() -> bool {
    true
}
fn is_true(v: &bool) -> bool {
    *v
}
fn is_false(v: &bool) -> bool {
    !*v
}
fn is_zero(v: &u64) -> bool {
    *v == 0
}
fn schema_version() -> u32 {
    SCHEMA_VERSION
}

/// Límites duros del motor (protegen contra configuraciones peligrosas).
pub mod limits {
    /// Pasos máximos que puede ejecutar una ejecución (incluye repeticiones por saltos).
    pub const MAX_STEPS: u32 = 10_000;
    pub const DEFAULT_MAX_STEPS: u32 = 500;
    /// Espera máxima de un paso `wait`: 24 horas.
    pub const MAX_WAIT_MS: u64 = 24 * 60 * 60 * 1000;
    /// Tiempo máximo de una solicitud HTTP.
    pub const MAX_HTTP_TIMEOUT_MS: u64 = 120_000;
    pub const DEFAULT_HTTP_TIMEOUT_MS: u64 = 10_000;
    pub const MAX_RETRIES: u32 = 10;
    /// Profundidad máxima de workflows que inician otros workflows.
    pub const MAX_DEPTH: u32 = 5;
    /// Intervalo mínimo de un workflow periódico.
    pub const MIN_SCHEDULE_MS: u64 = 1000;
}

/// Un workflow de un servicio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Workflow {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    /// Identificador dentro del servicio (slug). Se genera del nombre si se omite.
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Variables iniciales de cada ejecución (`{{ vars.X }}`). Los textos son plantillas.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, Value>,
    /// Ejecución periódica opcional mientras el servicio está iniciado.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<Schedule>,
    /// Pasos en orden. Sin saltos, se ejecutan uno tras otro.
    #[serde(default)]
    pub steps: Vec<Step>,
    /// Pasos máximos por ejecución; evita bucles infinitos.
    #[serde(default = "default_max_steps")]
    pub max_steps: u32,
    /// Posición de cada paso en el editor visual (solo presentación).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub layout: BTreeMap<String, Position>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

fn default_max_steps() -> u32 {
    limits::DEFAULT_MAX_STEPS
}

/// Disparador por tiempo: inicia el workflow cada `everyMs`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Schedule {
    pub every_ms: u64,
}

/// Un paso del workflow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    /// Identificador del paso (slug). Sus resultados quedan en `{{ steps.<id> }}`.
    #[serde(default)]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Paso al que saltar al terminar (`end` termina). Por defecto, el siguiente.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    #[serde(flatten)]
    pub action: Action,
}

/// Destino especial de un salto: terminar la ejecución.
pub const JUMP_END: &str = "end";
/// Destino especial de un salto: continuar con el paso siguiente.
pub const JUMP_NEXT: &str = "next";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Action {
    /// Esperar una duración.
    #[serde(rename_all = "camelCase")]
    Wait { duration_ms: u64 },
    /// Ejecutar una solicitud HTTP (webhook).
    Http(HttpAction),
    /// Emitir un evento visible en la línea de tiempo y en los eventos en vivo.
    #[serde(rename_all = "camelCase")]
    Emit {
        event: String,
        #[serde(default, skip_serializing_if = "Value::is_null")]
        data: Value,
    },
    /// Asignar variables de la ejecución. Los textos son plantillas.
    #[serde(rename_all = "camelCase")]
    SetVariables { values: BTreeMap<String, Value> },
    /// Evaluar condiciones y saltar según el resultado.
    #[serde(rename_all = "camelCase")]
    Condition {
        conditions: Vec<Condition>,
        #[serde(default, skip_serializing_if = "RuleMode::is_all")]
        mode: RuleMode,
        /// Destino si se cumple (`next` por defecto).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        on_true: Option<String>,
        /// Destino si no se cumple (`next` por defecto).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        on_false: Option<String>,
    },
    /// Ejecutar otro workflow del mismo servicio.
    #[serde(rename_all = "camelCase")]
    RunWorkflow {
        workflow_id: String,
        /// Entrada del workflow hijo (`{{ input.X }}`). Los textos son plantillas.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        input: BTreeMap<String, Value>,
        /// Esperar a que el hijo termine antes de continuar.
        #[serde(default, skip_serializing_if = "is_false")]
        wait: bool,
    },
    /// Leer o modificar un recurso simulado persistente del servicio.
    #[serde(rename_all = "camelCase")]
    Resource {
        collection: String,
        operation: crate::model::ResourceOp,
        /// Identificador del registro (plantilla). Se llama `recordId` para no
        /// chocar con el `id` del paso.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        record_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<Value>,
        /// Fallar la ejecución si el registro no existe (por defecto se continúa
        /// y `steps.<id>.found` es `false`).
        #[serde(default, skip_serializing_if = "is_false")]
        fail_if_missing: bool,
    },
    /// Simular un error: la ejecución termina como fallida.
    #[serde(rename_all = "camelCase")]
    Fail { message: String },
    /// Finalizar la ejecución correctamente.
    End,
}

impl Action {
    pub fn kind(&self) -> &'static str {
        match self {
            Action::Wait { .. } => "wait",
            Action::Http(_) => "http",
            Action::Emit { .. } => "emit",
            Action::SetVariables { .. } => "setVariables",
            Action::Condition { .. } => "condition",
            Action::RunWorkflow { .. } => "runWorkflow",
            Action::Resource { .. } => "resource",
            Action::Fail { .. } => "fail",
            Action::End => "end",
        }
    }
}

/// Solicitud HTTP de un paso. URL, headers, query y body admiten plantillas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HttpAction {
    pub method: crate::HttpMethod,
    /// Por ejemplo `{{ request.body.callback_url }}`.
    pub url: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub query: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "StepBody::is_none")]
    pub body: StepBody,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    /// Retraso antes del primer intento.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub delay_ms: u64,
    #[serde(default, skip_serializing_if = "Retry::is_none")]
    pub retry: Retry,
    /// Códigos aceptados: `200`, `2xx`, `200-299`. Vacío = cualquier 2xx.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_status: Vec<String>,
    /// Si falla tras los reintentos, continuar en lugar de fallar la ejecución.
    #[serde(default, skip_serializing_if = "is_false")]
    pub continue_on_error: bool,
}

fn default_timeout() -> u64 {
    limits::DEFAULT_HTTP_TIMEOUT_MS
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StepBody {
    #[default]
    None,
    /// JSON; cada texto es plantilla y `"{{ expr }}"` conserva el tipo.
    Json { content: Value },
    #[serde(rename_all = "camelCase")]
    Text {
        content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content_type: Option<String>,
    },
    /// `application/x-www-form-urlencoded`.
    Form { fields: BTreeMap<String, String> },
}

impl StepBody {
    pub fn is_none(&self) -> bool {
        matches!(self, StepBody::None)
    }
}

/// Reintentos con espera exponencial: `backoffMs * multiplier^(intento-1)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Retry {
    #[serde(default)]
    pub max_attempts: u32,
    #[serde(default = "default_backoff")]
    pub backoff_ms: u64,
    #[serde(default = "default_multiplier")]
    pub multiplier: f64,
}

fn default_backoff() -> u64 {
    1000
}
fn default_multiplier() -> f64 {
    2.0
}

impl Default for Retry {
    fn default() -> Self {
        Self {
            max_attempts: 0,
            backoff_ms: default_backoff(),
            multiplier: default_multiplier(),
        }
    }
}

impl Retry {
    pub fn is_none(&self) -> bool {
        self.max_attempts == 0
    }

    /// Espera antes del reintento número `attempt` (1, 2, 3...).
    pub fn delay_for(&self, attempt: u32) -> u64 {
        let factor = self
            .multiplier
            .max(1.0)
            .powi(attempt.saturating_sub(1) as i32);
        ((self.backoff_ms as f64) * factor).min(limits::MAX_WAIT_MS as f64) as u64
    }
}

/// Condición sobre el contexto de la ejecución.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Condition {
    /// Ruta en el contexto: `request.body.amount`, `request.headers.authorization`,
    /// `vars.intentos`, `steps.notificar.response.status`, `execution.status`...
    pub path: String,
    pub operator: Operator,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

/// Cuándo inicia un escenario su workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub enum StartWhen {
    /// Al recibir la solicitud (antes del retraso de la respuesta).
    RequestReceived,
    /// Al completar la respuesta.
    #[default]
    ResponseSent,
}

impl StartWhen {
    pub fn is_default(&self) -> bool {
        matches!(self, StartWhen::ResponseSent)
    }
}

/// Workflow que inicia un escenario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowLink {
    pub workflow_id: String,
    #[serde(default, skip_serializing_if = "StartWhen::is_default")]
    pub when: StartWhen,
    /// Entrada del workflow (`{{ input.X }}`). Los textos son plantillas.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub input: BTreeMap<String, Value>,
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub enabled: bool,
}

impl Workflow {
    /// Índice de un paso por id.
    pub fn step_index(&self, id: &str) -> Option<usize> {
        self.steps.iter().position(|s| s.id == id)
    }

    /// Resuelve un destino de salto desde el paso `current`.
    /// `None` = terminar la ejecución.
    pub fn resolve_jump(&self, current: usize, target: Option<&str>) -> Option<usize> {
        match target {
            None | Some(JUMP_NEXT) | Some("") => Some(current + 1),
            Some(JUMP_END) => None,
            Some(id) => self.step_index(id),
        }
    }
}

/// `"2xx"`, `"200"`, `"200-299"`; lista vacía = 2xx.
pub fn status_matches(expected: &[String], status: u16) -> bool {
    if expected.is_empty() {
        return (200..300).contains(&status);
    }
    expected
        .iter()
        .any(|e| status_pattern_matches(e.trim(), status))
}

pub fn status_pattern_matches(pattern: &str, status: u16) -> bool {
    let p = pattern.to_ascii_lowercase();
    if let Some(prefix) = p.strip_suffix("xx") {
        return prefix.parse::<u16>().is_ok_and(|d| status / 100 == d);
    }
    if let Some((a, b)) = p.split_once('-') {
        return match (a.trim().parse::<u16>(), b.trim().parse::<u16>()) {
            (Ok(a), Ok(b)) => (a..=b).contains(&status),
            _ => false,
        };
    }
    p.parse::<u16>().is_ok_and(|s| s == status)
}

pub fn valid_status_pattern(pattern: &str) -> bool {
    let p = pattern.trim().to_ascii_lowercase();
    if let Some(prefix) = p.strip_suffix("xx") {
        return prefix.parse::<u16>().is_ok_and(|d| (1..=5).contains(&d));
    }
    if let Some((a, b)) = p.split_once('-') {
        return matches!((a.trim().parse::<u16>(), b.trim().parse::<u16>()), (Ok(a), Ok(b)) if a <= b && (100..=599).contains(&a) && (100..=599).contains(&b));
    }
    p.parse::<u16>().is_ok_and(|s| (100..=599).contains(&s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn status_patterns() {
        assert!(status_matches(&[], 204));
        assert!(!status_matches(&[], 302));
        let e = vec!["2xx".to_string(), "409".to_string(), "500-503".to_string()];
        assert!(status_matches(&e, 201));
        assert!(status_matches(&e, 409));
        assert!(status_matches(&e, 502));
        assert!(!status_matches(&e, 404));
        assert!(valid_status_pattern("4xx"));
        assert!(!valid_status_pattern("9xx"));
        assert!(!valid_status_pattern("abc"));
    }

    #[test]
    fn retry_backoff() {
        let r = Retry {
            max_attempts: 3,
            backoff_ms: 100,
            multiplier: 2.0,
        };
        assert_eq!(
            (r.delay_for(1), r.delay_for(2), r.delay_for(3)),
            (100, 200, 400)
        );
    }

    #[test]
    fn serde_shape_is_flat_and_readable() {
        let wf: Workflow = serde_json::from_value(json!({
            "name": "Notificar firma",
            "steps": [
                {"id": "espera", "type": "wait", "durationMs": 5000},
                {"id": "pendiente", "type": "http", "method": "POST", "url": "{{ request.body.callback_url }}",
                 "body": {"type": "json", "content": {"status": "pending"}},
                 "retry": {"maxAttempts": 3}},
                {"id": "fin", "type": "end"}
            ]
        }))
        .unwrap();
        assert_eq!(wf.steps.len(), 3);
        assert_eq!(wf.max_steps, limits::DEFAULT_MAX_STEPS);
        match &wf.steps[1].action {
            Action::Http(h) => {
                assert_eq!(h.timeout_ms, limits::DEFAULT_HTTP_TIMEOUT_MS);
                assert_eq!(h.retry.max_attempts, 3);
                assert_eq!(h.retry.backoff_ms, 1000);
            }
            other => panic!("{other:?}"),
        }
        let back = serde_json::to_value(&wf).unwrap();
        assert_eq!(
            back["steps"][0],
            json!({"id": "espera", "type": "wait", "durationMs": 5000})
        );
        assert_eq!(back["steps"][2], json!({"id": "fin", "type": "end"}));
    }

    #[test]
    fn jumps() {
        let wf: Workflow = serde_json::from_value(json!({
            "name": "x",
            "steps": [{"id": "a", "type": "end"}, {"id": "b", "type": "end"}]
        }))
        .unwrap();
        assert_eq!(wf.resolve_jump(0, None), Some(1));
        assert_eq!(wf.resolve_jump(0, Some("end")), None);
        assert_eq!(wf.resolve_jump(1, Some("a")), Some(0));
        assert_eq!(wf.resolve_jump(0, Some("zzz")), None);
    }
}
