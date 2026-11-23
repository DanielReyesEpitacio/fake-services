//! Motor de coincidencia de solicitudes.
//!
//! Compila la configuración declarativa en una estructura de búsqueda
//! (rutas ordenadas por especificidad, regex y JSON Schemas precompilados)
//! y responde tres preguntas para cada solicitud:
//!
//! 1. ¿Qué servicio y endpoint la atienden? ([`CompiledWorkspace::route`])
//! 2. ¿Cumple lo que el endpoint espera? ([`CompiledEndpoint::validate`])
//! 3. ¿Qué escenario de respuesta aplica? ([`CompiledEndpoint::select_scenario`])

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use fakes_domain::{
    AuthConfig, BodyKind, Endpoint, HttpMethod, IncomingRequest, Issue, Operator, ParamSpec,
    PathTemplate, Rule, RuleMode, RuleSource, Scenario, Service, Workspace, lookup_path,
    normalize_prefix,
};
use regex::Regex;
use serde_json::Value;

/// Header para forzar un escenario por solicitud: `x-fakes-scenario: firma-rechazada`.
pub const SCENARIO_HEADER: &str = "x-fakes-scenario";

// ---------------------------------------------------------------------------
// Compilación
// ---------------------------------------------------------------------------

pub struct CompiledWorkspace {
    pub workspace: Arc<Workspace>,
    pub services: Vec<CompiledService>,
}

pub struct CompiledService {
    pub service: Arc<Service>,
    pub prefix: String,
    /// Variables combinadas (espacio + servicio).
    pub vars: BTreeMap<String, String>,
    pub endpoints: Vec<CompiledEndpoint>,
}

pub struct CompiledEndpoint {
    pub endpoint: Endpoint,
    template: PathTemplate,
    header_patterns: Vec<Option<Regex>>,
    query_patterns: Vec<Option<Regex>>,
    path_patterns: Vec<Option<Regex>>,
    body_schema: Option<jsonschema::Validator>,
    scenarios: Vec<CompiledScenario>,
}

struct CompiledScenario {
    rule_regex: Vec<Option<Regex>>,
}

fn anchored(pattern: &str) -> Option<Regex> {
    Regex::new(&format!("^(?:{pattern})$")).ok()
}

fn compile_patterns(params: &[ParamSpec]) -> Vec<Option<Regex>> {
    params
        .iter()
        .map(|p| p.pattern.as_deref().and_then(anchored))
        .collect()
}

impl CompiledEndpoint {
    pub fn compile(endpoint: Endpoint) -> Option<Self> {
        let template = PathTemplate::parse(&endpoint.path).ok()?;
        let body_schema = endpoint
            .request
            .body
            .as_ref()
            .and_then(|b| b.schema.as_ref())
            .and_then(|s| match jsonschema::validator_for(s) {
                Ok(v) => Some(v),
                Err(e) => {
                    tracing::warn!(endpoint = %endpoint.id, "JSON Schema inválido: {e}");
                    None
                }
            });
        let scenarios = endpoint
            .scenarios
            .iter()
            .map(|s| CompiledScenario {
                rule_regex: s
                    .rules
                    .iter()
                    .map(|r| match (r.operator, &r.value) {
                        (Operator::Regex, Some(Value::String(p))) => Regex::new(p).ok(),
                        _ => None,
                    })
                    .collect(),
            })
            .collect();
        Some(Self {
            header_patterns: compile_patterns(&endpoint.request.headers),
            query_patterns: compile_patterns(&endpoint.request.query),
            path_patterns: compile_patterns(&endpoint.request.path_params),
            body_schema,
            scenarios,
            template,
            endpoint,
        })
    }

    pub fn template(&self) -> &PathTemplate {
        &self.template
    }
}

impl CompiledWorkspace {
    pub fn compile(workspace: Workspace, services: Vec<Service>) -> Self {
        let workspace = Arc::new(workspace);
        let mut compiled: Vec<CompiledService> = services
            .into_iter()
            .map(|svc| {
                let mut vars = workspace.variables.clone();
                vars.extend(svc.variables.clone());
                let mut endpoints: Vec<CompiledEndpoint> = svc
                    .endpoints
                    .iter()
                    .filter(|e| e.enabled)
                    .cloned()
                    .filter_map(CompiledEndpoint::compile)
                    .collect();
                // Más específicos primero; a igualdad, método concreto antes que `*`.
                endpoints.sort_by(|a, b| {
                    b.template
                        .specificity()
                        .cmp(&a.template.specificity())
                        .then_with(|| {
                            (a.endpoint.method == HttpMethod::Any)
                                .cmp(&(b.endpoint.method == HttpMethod::Any))
                        })
                });
                CompiledService {
                    prefix: normalize_prefix(&svc.prefix),
                    vars,
                    endpoints,
                    service: Arc::new(svc),
                }
            })
            .collect();
        // Prefijos más largos primero para que `/aws/s3` gane a `/aws`.
        compiled.sort_by_key(|s| std::cmp::Reverse(s.prefix.len()));
        Self {
            workspace,
            services: compiled,
        }
    }

    pub fn service(&self, id: &str) -> Option<&CompiledService> {
        self.services.iter().find(|s| s.service.id == id)
    }

    /// Servicio que atiende una ruta del servidor central, y la ruta restante.
    pub fn service_for_path(&self, path: &str) -> Option<(&CompiledService, String)> {
        self.services.iter().find_map(|s| {
            if s.prefix.is_empty() {
                return None;
            }
            strip_prefix(path, &s.prefix).map(|rest| (s, rest))
        })
    }
}

/// Quita el prefijo respetando límites de segmento: `/mifiel` no coincide con `/mifielx`.
fn strip_prefix(path: &str, prefix: &str) -> Option<String> {
    let rest = path.strip_prefix(prefix)?;
    if rest.is_empty() {
        Some("/".to_string())
    } else if rest.starts_with('/') {
        Some(rest.to_string())
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Enrutamiento
// ---------------------------------------------------------------------------

pub enum EndpointMatch<'a> {
    Found {
        endpoint: &'a CompiledEndpoint,
        params: BTreeMap<String, String>,
    },
    /// La ruta existe pero no con ese método.
    MethodNotAllowed {
        allowed: Vec<&'static str>,
    },
    NotFound,
}

impl CompiledService {
    pub fn find_endpoint(&self, method: &str, route_path: &str) -> EndpointMatch<'_> {
        let mut allowed = Vec::new();
        for ep in &self.endpoints {
            if let Some(params) = ep.template.match_path(route_path) {
                if ep.endpoint.method.matches(method)
                    || (method.eq_ignore_ascii_case("HEAD")
                        && ep.endpoint.method == HttpMethod::Get)
                {
                    let params = params
                        .into_iter()
                        .map(|(k, v)| (k, percent_decode(&v)))
                        .collect();
                    return EndpointMatch::Found {
                        endpoint: ep,
                        params,
                    };
                }
                let m = ep.endpoint.method.as_str();
                if !allowed.contains(&m) {
                    allowed.push(m);
                }
            }
        }
        if allowed.is_empty() {
            EndpointMatch::NotFound
        } else {
            EndpointMatch::MethodNotAllowed { allowed }
        }
    }

    /// Verifica la autenticación configurada. `Err` trae el motivo.
    pub fn check_auth(&self, req: &IncomingRequest) -> Result<(), String> {
        match &self.service.auth {
            AuthConfig::None => Ok(()),
            AuthConfig::Bearer { token } => {
                let header = req
                    .header("authorization")
                    .ok_or("falta el header Authorization")?;
                let given = header
                    .strip_prefix("Bearer ")
                    .or_else(|| header.strip_prefix("bearer "))
                    .ok_or("se esperaba 'Authorization: Bearer <token>'")?
                    .trim();
                if given.is_empty() {
                    return Err("el token está vacío".into());
                }
                match token {
                    Some(t) if t != given => Err("token inválido".into()),
                    _ => Ok(()),
                }
            }
            AuthConfig::ApiKey { header, value } => {
                let given = req
                    .header(header)
                    .ok_or_else(|| format!("falta el header {header}"))?;
                match value {
                    Some(v) if v != given => Err(format!("valor inválido en {header}")),
                    _ => Ok(()),
                }
            }
            AuthConfig::Basic { username, password } => {
                use base64::Engine;
                let header = req
                    .header("authorization")
                    .ok_or("falta el header Authorization")?;
                let encoded = header
                    .strip_prefix("Basic ")
                    .or_else(|| header.strip_prefix("basic "))
                    .ok_or("se esperaba 'Authorization: Basic ...'")?;
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(encoded.trim())
                    .map_err(|_| "credenciales Basic mal codificadas")?;
                let decoded = String::from_utf8_lossy(&decoded);
                let (u, p) = decoded
                    .split_once(':')
                    .ok_or("credenciales Basic inválidas")?;
                if username.as_deref().is_some_and(|x| x != u)
                    || password.as_deref().is_some_and(|x| x != p)
                {
                    return Err("usuario o contraseña inválidos".into());
                }
                Ok(())
            }
        }
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push(h * 16 + l);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Validación de la solicitud
// ---------------------------------------------------------------------------

impl CompiledEndpoint {
    /// Comprueba headers, query, parámetros de ruta y body.
    pub fn validate(&self, req: &IncomingRequest) -> Vec<Issue> {
        let mut issues = Vec::new();
        let spec = &self.endpoint.request;

        for (p, re) in spec.headers.iter().zip(&self.header_patterns) {
            check_param(&mut issues, "header", p, re.as_ref(), req.header(&p.name));
        }
        for (p, re) in spec.query.iter().zip(&self.query_patterns) {
            check_param(
                &mut issues,
                "query",
                p,
                re.as_ref(),
                req.query_param(&p.name),
            );
        }
        for (p, re) in spec.path_params.iter().zip(&self.path_patterns) {
            let v = req.params.get(&p.name).map(String::as_str);
            check_param(&mut issues, "path", p, re.as_ref(), v);
        }

        if let Some(body) = &spec.body {
            let empty = req.raw_body.is_empty();
            if empty {
                if body.required {
                    issues.push(Issue::new("body", "el body es obligatorio"));
                }
                return issues;
            }
            let ct = req
                .content_type
                .as_deref()
                .unwrap_or("")
                .to_ascii_lowercase();
            let kind_ok = match body.kind {
                BodyKind::Any | BodyKind::Binary => true,
                BodyKind::Json => ct.contains("json"),
                BodyKind::Xml => ct.contains("xml"),
                BodyKind::Text => ct.starts_with("text/") || ct.is_empty(),
                BodyKind::Form => ct.starts_with("application/x-www-form-urlencoded"),
                BodyKind::Multipart => ct.starts_with("multipart/form-data"),
            };
            if !kind_ok {
                issues.push(Issue::new(
                    "header.content-type",
                    format!(
                        "Content-Type no compatible con el body esperado ({:?})",
                        body.kind
                    )
                    .to_lowercase(),
                ));
            }
            if body.kind == BodyKind::Json && kind_ok && req.body.is_null() {
                issues.push(Issue::new("body", "el body no es JSON válido"));
            }
            if let Some(validator) = &self.body_schema {
                for err in validator.iter_errors(&req.body) {
                    let pointer = err.instance_path().to_string();
                    let field = if pointer.is_empty() {
                        "body".to_string()
                    } else {
                        format!("body{}", pointer.replace('/', "."))
                    };
                    issues.push(Issue::new(field, err.to_string()));
                }
            }
        }
        issues
    }
}

fn check_param(
    issues: &mut Vec<Issue>,
    kind: &str,
    p: &ParamSpec,
    re: Option<&Regex>,
    value: Option<&str>,
) {
    let field = format!("{kind}.{}", p.name);
    match value {
        None if p.required => issues.push(Issue::new(field, "es obligatorio")),
        None => {}
        Some(v) => {
            if let Some(re) = re
                && !re.is_match(v)
            {
                issues.push(Issue::new(
                    field,
                    format!("no cumple el patrón {}", p.pattern.as_deref().unwrap_or("")),
                ));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Selección de escenario
// ---------------------------------------------------------------------------

/// Motivo por el que se eligió un escenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionReason {
    /// Forzado con el header `x-fakes-scenario`.
    Header,
    /// Forzado con `activeScenario` en la configuración.
    Active,
    /// Sus reglas coincidieron (o no tenía reglas).
    Rules,
}

impl CompiledEndpoint {
    pub fn select_scenario(
        &self,
        req: &IncomingRequest,
        vars: &BTreeMap<String, String>,
    ) -> Option<(&Scenario, SelectionReason)> {
        let scenarios = &self.endpoint.scenarios;
        if let Some(forced) = req.header(SCENARIO_HEADER)
            && let Some(s) = scenarios
                .iter()
                .find(|s| s.enabled && (s.id == forced || s.name.eq_ignore_ascii_case(forced)))
        {
            return Some((s, SelectionReason::Header));
        }
        if let Some(active) = &self.endpoint.active_scenario
            && let Some(s) = scenarios.iter().find(|s| &s.id == active && s.enabled)
        {
            return Some((s, SelectionReason::Active));
        }
        scenarios
            .iter()
            .zip(&self.scenarios)
            .filter(|(s, _)| s.enabled)
            .find(|(s, c)| scenario_matches(s, c, req, vars))
            .map(|(s, _)| (s, SelectionReason::Rules))
    }
}

fn scenario_matches(
    s: &Scenario,
    c: &CompiledScenario,
    req: &IncomingRequest,
    vars: &BTreeMap<String, String>,
) -> bool {
    if s.rules.is_empty() {
        return true;
    }
    let mut results = s
        .rules
        .iter()
        .zip(&c.rule_regex)
        .map(|(r, re)| eval_rule(r, re.as_ref(), req, vars));
    match s.rule_mode {
        RuleMode::All => results.all(|b| b),
        RuleMode::Any => results.any(|b| b),
    }
}

/// Obtiene el dato de la solicitud al que se refiere una regla.
pub fn rule_actual(
    rule: &Rule,
    req: &IncomingRequest,
    vars: &BTreeMap<String, String>,
) -> Option<Value> {
    let s = |v: &str| Some(Value::String(v.to_string()));
    match rule.source {
        RuleSource::Header => req.header(&rule.key).and_then(s),
        RuleSource::Query => req.query_param(&rule.key).and_then(s),
        RuleSource::Path => req.params.get(&rule.key).and_then(|v| s(v)),
        RuleSource::Method => s(&req.method),
        RuleSource::Variable => vars.get(&rule.key).and_then(|v| s(v)),
        RuleSource::Body => {
            if rule.key.trim().is_empty() {
                if req.raw_body.is_empty() {
                    None
                } else if req.body.is_null() || req.body.is_string() {
                    Some(Value::String(req.body_text()))
                } else {
                    Some(req.body.clone())
                }
            } else {
                lookup_path(&req.body, &rule.key).cloned()
            }
        }
    }
}

pub fn eval_rule(
    rule: &Rule,
    compiled_regex: Option<&Regex>,
    req: &IncomingRequest,
    vars: &BTreeMap<String, String>,
) -> bool {
    let actual = rule_actual(rule, req, vars);
    eval_operator(
        rule.operator,
        actual.as_ref(),
        rule.value.as_ref(),
        compiled_regex,
    )
}

/// Evalúa un operador sobre un valor real y uno esperado. Reutilizable por
/// el motor de workflows.
pub fn eval_operator(
    op: Operator,
    actual: Option<&Value>,
    expected: Option<&Value>,
    compiled_regex: Option<&Regex>,
) -> bool {
    let present = actual.is_some_and(|v| !v.is_null());
    match op {
        Operator::Exists => present,
        Operator::NotExists => !present,
        Operator::Equals => match (actual, expected) {
            (Some(a), Some(e)) => loose_eq(a, e),
            (None, Some(Value::Null)) => true,
            _ => false,
        },
        Operator::NotEquals => !eval_operator(Operator::Equals, actual, expected, None),
        Operator::Contains => match (actual, expected) {
            (Some(a), Some(e)) => contains(a, e),
            _ => false,
        },
        Operator::NotContains => !eval_operator(Operator::Contains, actual, expected, None),
        Operator::GreaterThan | Operator::LessThan => {
            match (actual.and_then(as_f64), expected.and_then(as_f64)) {
                (Some(a), Some(e)) => {
                    if op == Operator::GreaterThan {
                        a > e
                    } else {
                        a < e
                    }
                }
                _ => false,
            }
        }
        Operator::Regex => {
            let Some(a) = actual else { return false };
            let text = as_text(a);
            match compiled_regex {
                Some(re) => re.is_match(&text),
                None => expected
                    .and_then(Value::as_str)
                    .and_then(|p| Regex::new(p).ok())
                    .is_some_and(|re| re.is_match(&text)),
            }
        }
    }
}

fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

fn as_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Igualdad tolerante: `"5" == 5`, `"true" == true`; objetos y arrays en profundidad.
pub fn loose_eq(a: &Value, e: &Value) -> bool {
    match (a, e) {
        (Value::Number(_), _) | (_, Value::Number(_))
            if !(a.is_array() || a.is_object() || e.is_array() || e.is_object()) =>
        {
            match (as_f64(a), as_f64(e)) {
                (Some(x), Some(y)) => (x - y).abs() < f64::EPSILON,
                _ => as_text(a) == as_text(e),
            }
        }
        (Value::Bool(x), Value::String(s)) | (Value::String(s), Value::Bool(x)) => {
            s.eq_ignore_ascii_case(if *x { "true" } else { "false" })
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| loose_eq(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| loose_eq(v, w)))
        }
        _ => as_text(a) == as_text(e),
    }
}

fn contains(a: &Value, e: &Value) -> bool {
    match a {
        Value::Array(items) => items.iter().any(|i| loose_eq(i, e)),
        Value::Object(m) => e.as_str().is_some_and(|k| m.contains_key(k)),
        other => as_text(other).contains(&as_text(e)),
    }
}

/// Mapa de servicios por puerto propio.
pub fn services_by_port(ws: &CompiledWorkspace) -> HashMap<u16, String> {
    ws.services
        .iter()
        .filter_map(|s| s.service.port.map(|p| (p, s.service.id.clone())))
        .collect()
}

#[cfg(test)]
mod tests;
