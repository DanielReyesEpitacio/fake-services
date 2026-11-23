//! Validación estructural del modelo. No depende de motores externos:
//! la sintaxis de plantillas la valida `fakes-response`.

use std::collections::HashSet;

use regex::Regex;
use schemars::JsonSchema;
use serde::Serialize;

use crate::model::*;
use crate::path::{PathTemplate, normalize_prefix};

/// Problema de validación con la ruta del campo afectado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    /// Ruta del campo, p. ej. `scenarios[1].response.status`.
    pub field: String,
    pub message: String,
}

impl Issue {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

pub const MAX_DELAY_MS: u64 = 5 * 60 * 1000;

pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        && !id.starts_with(['-', '_'])
}

fn check_id(issues: &mut Vec<Issue>, field: &str, id: &str) {
    if !is_valid_id(id) {
        issues.push(Issue::new(
            field,
            "identificador inválido: usa minúsculas, números, '-' o '_' (máx. 64)",
        ));
    }
}

pub fn validate_workspace(ws: &Workspace) -> Vec<Issue> {
    let mut issues = Vec::new();
    check_id(&mut issues, "id", &ws.id);
    if ws.name.trim().is_empty() {
        issues.push(Issue::new("name", "el nombre es obligatorio"));
    }
    if ws.server.port == 0 {
        issues.push(Issue::new("server.port", "el puerto debe ser mayor que 0"));
    }
    if ws.server.host.trim().is_empty() {
        issues.push(Issue::new("server.host", "la dirección es obligatoria"));
    }
    for name in ws.variables.keys() {
        if !valid_var_name(name) {
            issues.push(Issue::new(
                format!("variables.{name}"),
                "nombre de variable inválido: letras, números y '_'",
            ));
        }
    }
    issues
}

fn valid_var_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.starts_with(|c: char| c.is_ascii_digit())
}

/// Valida un servicio (sin sus endpoints) contra los demás servicios del espacio.
pub fn validate_service(svc: &Service, others: &[&Service], ws: &Workspace) -> Vec<Issue> {
    let mut issues = Vec::new();
    check_id(&mut issues, "id", &svc.id);
    if svc.name.trim().is_empty() {
        issues.push(Issue::new("name", "el nombre es obligatorio"));
    }
    let prefix = normalize_prefix(&svc.prefix);
    if prefix != svc.prefix {
        issues.push(Issue::new(
            "prefix",
            format!("el prefijo debe estar normalizado como '{prefix}'"),
        ));
    }
    if prefix.is_empty() && svc.port.is_none() {
        issues.push(Issue::new(
            "prefix",
            "el prefijo es obligatorio si el servicio no tiene puerto propio",
        ));
    }
    if prefix.contains(['{', '}', '?', '#', ' ']) {
        issues.push(Issue::new(
            "prefix",
            "el prefijo contiene caracteres inválidos",
        ));
    }
    if prefix.starts_with("/__fakes") {
        issues.push(Issue::new("prefix", "el prefijo '/__fakes' está reservado"));
    }
    for other in others {
        if other.id == svc.id {
            continue;
        }
        if !prefix.is_empty() && other.prefix == prefix {
            issues.push(Issue::new(
                "prefix",
                format!("el prefijo ya lo usa el servicio '{}'", other.name),
            ));
        }
        if svc.port.is_some() && other.port == svc.port {
            issues.push(Issue::new(
                "port",
                format!("el puerto ya lo usa el servicio '{}'", other.name),
            ));
        }
    }
    if let Some(port) = svc.port {
        if port < 1024 {
            issues.push(Issue::new("port", "usa un puerto entre 1024 y 65535"));
        }
        if port == ws.server.port {
            issues.push(Issue::new(
                "port",
                "el puerto coincide con el del servidor central",
            ));
        }
    }
    for name in svc.variables.keys() {
        if !valid_var_name(name) {
            issues.push(Issue::new(
                format!("variables.{name}"),
                "nombre de variable inválido: letras, números y '_'",
            ));
        }
    }
    if let AuthConfig::ApiKey { header, .. } = &svc.auth
        && http_header_name_invalid(header)
    {
        issues.push(Issue::new("auth.header", "nombre de header inválido"));
    }
    if let Some(url) = &svc.upstream_url
        && !(url.starts_with("http://") || url.starts_with("https://"))
    {
        issues.push(Issue::new(
            "upstreamUrl",
            "la URL debe comenzar con http:// o https://",
        ));
    }
    issues
}

fn http_header_name_invalid(name: &str) -> bool {
    name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

/// Valida un endpoint contra los demás endpoints del mismo servicio.
pub fn validate_endpoint(ep: &Endpoint, siblings: &[&Endpoint]) -> Vec<Issue> {
    let mut issues = Vec::new();
    check_id(&mut issues, "id", &ep.id);

    let template = match PathTemplate::parse(&ep.path) {
        Ok(t) => Some(t),
        Err(e) => {
            issues.push(Issue::new("path", e.0));
            None
        }
    };

    if let Some(t) = &template {
        for other in siblings {
            if other.id == ep.id {
                continue;
            }
            if other.path == ep.path
                && (other.method == ep.method
                    || other.method == HttpMethod::Any
                    || ep.method == HttpMethod::Any)
            {
                issues.push(Issue::new(
                    "path",
                    format!(
                        "ya existe un endpoint {} {} ('{}')",
                        other.method.as_str(),
                        other.path,
                        other.id
                    ),
                ));
            }
        }
        let names = t.param_names();
        for (i, p) in ep.request.path_params.iter().enumerate() {
            if !names.contains(&p.name.as_str()) {
                issues.push(Issue::new(
                    format!("request.pathParams[{i}].name"),
                    format!("la ruta no tiene el parámetro '{{{}}}'", p.name),
                ));
            }
        }
    }

    validate_params(&mut issues, "request.headers", &ep.request.headers, true);
    validate_params(&mut issues, "request.query", &ep.request.query, false);
    validate_params(
        &mut issues,
        "request.pathParams",
        &ep.request.path_params,
        false,
    );

    if let Some(body) = &ep.request.body
        && let Some(schema) = &body.schema
        && !schema.is_object()
        && !schema.is_boolean()
    {
        issues.push(Issue::new(
            "request.body.schema",
            "el JSON Schema debe ser un objeto",
        ));
    }

    let mut scenario_ids = HashSet::new();
    for (i, sc) in ep.scenarios.iter().enumerate() {
        let base = format!("scenarios[{i}]");
        check_id(&mut issues, &format!("{base}.id"), &sc.id);
        if !scenario_ids.insert(sc.id.as_str()) {
            issues.push(Issue::new(
                format!("{base}.id"),
                "identificador de escenario repetido",
            ));
        }
        if sc.name.trim().is_empty() {
            issues.push(Issue::new(
                format!("{base}.name"),
                "el nombre es obligatorio",
            ));
        }
        for (j, rule) in sc.rules.iter().enumerate() {
            validate_rule(&mut issues, &format!("{base}.rules[{j}]"), rule);
        }
        validate_response(&mut issues, &format!("{base}.response"), &sc.response);
    }
    if let Some(active) = &ep.active_scenario
        && !ep.scenarios.iter().any(|s| &s.id == active)
    {
        issues.push(Issue::new(
            "activeScenario",
            format!("no existe el escenario '{active}'"),
        ));
    }
    if let Some(r) = &ep.on_validation_error {
        validate_response(&mut issues, "onValidationError", r);
    }
    issues
}

fn validate_params(issues: &mut Vec<Issue>, base: &str, params: &[ParamSpec], header: bool) {
    let mut seen = HashSet::new();
    for (i, p) in params.iter().enumerate() {
        let key = if header {
            p.name.to_ascii_lowercase()
        } else {
            p.name.clone()
        };
        if p.name.trim().is_empty() {
            issues.push(Issue::new(
                format!("{base}[{i}].name"),
                "el nombre es obligatorio",
            ));
        } else if header && http_header_name_invalid(&p.name) {
            issues.push(Issue::new(
                format!("{base}[{i}].name"),
                "nombre de header inválido",
            ));
        }
        if !seen.insert(key) {
            issues.push(Issue::new(format!("{base}[{i}].name"), "nombre repetido"));
        }
        if let Some(pattern) = &p.pattern
            && let Err(e) = Regex::new(pattern)
        {
            issues.push(Issue::new(
                format!("{base}[{i}].pattern"),
                format!("expresión regular inválida: {e}"),
            ));
        }
    }
}

fn validate_rule(issues: &mut Vec<Issue>, base: &str, rule: &Rule) {
    let needs_key = !matches!(rule.source, RuleSource::Body | RuleSource::Method);
    if needs_key && rule.key.trim().is_empty() {
        issues.push(Issue::new(
            format!("{base}.key"),
            "indica el nombre del dato a evaluar",
        ));
    }
    let needs_value = !matches!(rule.operator, Operator::Exists | Operator::NotExists);
    if needs_value && rule.value.is_none() {
        issues.push(Issue::new(
            format!("{base}.value"),
            "el operador necesita un valor",
        ));
    }
    if rule.operator == Operator::Regex {
        match rule.value.as_ref().and_then(|v| v.as_str()) {
            Some(p) => {
                if let Err(e) = Regex::new(p) {
                    issues.push(Issue::new(
                        format!("{base}.value"),
                        format!("expresión regular inválida: {e}"),
                    ));
                }
            }
            None => issues.push(Issue::new(
                format!("{base}.value"),
                "la expresión regular debe ser texto",
            )),
        }
    }
    if matches!(rule.operator, Operator::GreaterThan | Operator::LessThan) {
        let numeric = rule.value.as_ref().is_some_and(|v| {
            v.is_number() || v.as_str().is_some_and(|s| s.trim().parse::<f64>().is_ok())
        });
        if !numeric {
            issues.push(Issue::new(
                format!("{base}.value"),
                "el valor debe ser numérico",
            ));
        }
    }
}

pub fn validate_response(issues: &mut Vec<Issue>, base: &str, r: &ResponseDef) {
    if !(100..=599).contains(&r.status) {
        issues.push(Issue::new(
            format!("{base}.status"),
            "código HTTP entre 100 y 599",
        ));
    }
    if r.delay_ms + r.jitter_ms > MAX_DELAY_MS {
        issues.push(Issue::new(
            format!("{base}.delayMs"),
            "el retraso total no puede superar 5 minutos",
        ));
    }
    for name in r.headers.keys() {
        if http_header_name_invalid(name) {
            issues.push(Issue::new(
                format!("{base}.headers.{name}"),
                "nombre de header inválido",
            ));
        }
    }
    match &r.body {
        ResponseBody::Binary { base64, .. } => {
            if base64
                .chars()
                .any(|c| !(c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '\n' | '\r')))
            {
                issues.push(Issue::new(format!("{base}.body.base64"), "base64 inválido"));
            }
        }
        ResponseBody::File { path, .. }
            if (path.is_empty()
                || path.starts_with(['/', '\\'])
                || path.contains("..")
                || path.contains(':')) =>
        {
            issues.push(Issue::new(
                format!("{base}.body.path"),
                "usa una ruta relativa dentro de la carpeta files/ del espacio",
            ));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ep(id: &str, method: HttpMethod, path: &str) -> Endpoint {
        Endpoint {
            schema_version: 1,
            id: id.into(),
            name: String::new(),
            description: String::new(),
            enabled: true,
            method,
            path: path.into(),
            request: RequestSpec::default(),
            scenarios: vec![Scenario {
                id: "ok".into(),
                name: "OK".into(),
                description: String::new(),
                enabled: true,
                rule_mode: RuleMode::All,
                rules: vec![],
                response: ResponseDef::json(200, json!({})),
            }],
            active_scenario: None,
            on_validation_error: None,
        }
    }

    #[test]
    fn valid_endpoint_has_no_issues() {
        let e = ep("sign", HttpMethod::Post, "/documents/{id}/sign");
        assert!(validate_endpoint(&e, &[]).is_empty());
    }

    #[test]
    fn detects_duplicates_and_bad_values() {
        let a = ep("a", HttpMethod::Get, "/x");
        let mut b = ep("b", HttpMethod::Any, "/x");
        b.scenarios[0].response.status = 99;
        b.scenarios[0].rules.push(Rule {
            source: RuleSource::Header,
            key: "".into(),
            operator: Operator::Regex,
            value: Some(json!("([")),
        });
        let issues = validate_endpoint(&b, &[&a]);
        let fields: Vec<_> = issues.iter().map(|i| i.field.as_str()).collect();
        assert!(fields.contains(&"path"));
        assert!(fields.contains(&"scenarios[0].response.status"));
        assert!(fields.contains(&"scenarios[0].rules[0].key"));
        assert!(fields.contains(&"scenarios[0].rules[0].value"));
    }

    #[test]
    fn rejects_file_traversal() {
        let mut issues = vec![];
        let mut r = ResponseDef::json(200, json!(null));
        r.body = ResponseBody::File {
            path: "../secret".into(),
            content_type: None,
        };
        validate_response(&mut issues, "r", &r);
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn service_prefix_conflicts() {
        let ws = Workspace::new("default", "Default");
        let mk = |id: &str, prefix: &str| Service {
            schema_version: 1,
            id: id.into(),
            name: id.into(),
            description: String::new(),
            upstream_url: None,
            prefix: prefix.into(),
            port: None,
            auto_start: true,
            variables: Default::default(),
            auth: AuthConfig::None,
            endpoints: vec![],
        };
        let a = mk("a", "/mifiel");
        let b = mk("b", "/mifiel");
        assert!(!validate_service(&b, &[&a], &ws).is_empty());
        let c = mk("c", "mifiel/");
        assert!(!validate_service(&c, &[], &ws).is_empty());
    }
}
