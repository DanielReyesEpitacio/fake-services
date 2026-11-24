//! Importadores de especificaciones y detección de integraciones.
//!
//! - [`from_openapi`]: OpenAPI 3.x y Swagger 2.0 (JSON o YAML).
//! - [`from_postman`]: colecciones de Postman v2.x.
//! - [`detect::detect_integrations`]: analiza un proyecto y encuentra los
//!   proveedores externos que usa.
//!
//! Los importadores no escriben nada: producen un [`Service`] que la capa de
//! aplicación valida e importa con las mismas reglas que un bundle.

pub mod detect;
mod openapi;
mod postman;
mod schema;

use fakes_domain::{Operator, Rule, RuleSource, Scenario, Service, slugify, unique_id};
use serde_json::Value;

pub use openapi::from_openapi;
pub use postman::from_postman;

#[derive(Debug, Clone, Default)]
pub struct ImportOptions {
    /// Nombre del servicio (por defecto, el título del documento).
    pub name: Option<String>,
    /// Prefijo de rutas (por defecto, `/<id>`).
    pub prefix: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Imported {
    pub service: Service,
    pub warnings: Vec<String>,
}

/// Formatos de documento que se pueden importar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum SpecFormat {
    Openapi,
    Postman,
    Bundle,
}

/// Reconoce el formato de un documento por su contenido.
pub fn detect_format(text: &str) -> Option<SpecFormat> {
    let v = parse_document(text).ok()?;
    if v.get("format").and_then(Value::as_str) == Some("fake-services") {
        return Some(SpecFormat::Bundle);
    }
    if v.get("openapi").is_some() || v.get("swagger").is_some() {
        return Some(SpecFormat::Openapi);
    }
    if v.get("info").and_then(|i| i.get("schema")).is_some() && v.get("item").is_some() {
        return Some(SpecFormat::Postman);
    }
    None
}

/// Lee JSON o YAML.
pub(crate) fn parse_document(text: &str) -> Result<Value, String> {
    let trimmed = text.trim_start_matches('\u{feff}').trim();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return serde_json::from_str(trimmed).map_err(|e| format!("JSON inválido: {e}"));
    }
    serde_yaml::from_str::<Value>(trimmed).map_err(|e| format!("no es JSON ni YAML válido: {e}"))
}

/// Regla que solo se cumple al forzar el escenario (`x-fakes-scenario: <id>`).
pub(crate) fn forced_rule(id: &str) -> Rule {
    Rule {
        source: RuleSource::Header,
        key: "x-fakes-scenario".into(),
        operator: Operator::Equals,
        value: Some(Value::String(id.into())),
    }
}

pub(crate) fn scenario_id(name: &str, existing: &[Scenario]) -> String {
    let base = slugify(name);
    let base = if base.is_empty() {
        "escenario".to_string()
    } else {
        base
    };
    unique_id(&base, existing.iter().map(|s| s.id.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_formats() {
        assert_eq!(
            detect_format("openapi: 3.0.0\npaths: {}"),
            Some(SpecFormat::Openapi)
        );
        assert_eq!(
            detect_format(r#"{"swagger": "2.0"}"#),
            Some(SpecFormat::Openapi)
        );
        assert_eq!(
            detect_format(r#"{"info": {"schema": "https://schema.getpostman.com/x"}, "item": []}"#),
            Some(SpecFormat::Postman)
        );
        assert_eq!(
            detect_format(r#"{"format": "fake-services", "formatVersion": 1}"#),
            Some(SpecFormat::Bundle)
        );
        assert_eq!(detect_format("hola"), None);
    }
}
