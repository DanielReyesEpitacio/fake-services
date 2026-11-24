//! OpenAPI 3.x y Swagger 2.0 → servicio de Fake Services.

use std::collections::BTreeMap;

use fakes_domain::*;
use serde_json::{Value, json};

use crate::schema::{resolve, sample, to_json_schema};
use crate::{ImportOptions, Imported, forced_rule, parse_document, scenario_id};

const METHODS: &[(&str, HttpMethod)] = &[
    ("get", HttpMethod::Get),
    ("post", HttpMethod::Post),
    ("put", HttpMethod::Put),
    ("patch", HttpMethod::Patch),
    ("delete", HttpMethod::Delete),
    ("head", HttpMethod::Head),
    ("options", HttpMethod::Options),
];

pub fn from_openapi(text: &str, opts: &ImportOptions) -> Result<Imported, String> {
    let root = parse_document(text)?;
    let swagger2 = root
        .get("swagger")
        .and_then(Value::as_str)
        .is_some_and(|v| v.starts_with('2'));
    let openapi3 = root
        .get("openapi")
        .and_then(Value::as_str)
        .is_some_and(|v| v.starts_with('3'));
    if !swagger2 && !openapi3 {
        return Err(
            "el documento no es OpenAPI 3.x ni Swagger 2.0 (falta `openapi` o `swagger`)".into(),
        );
    }
    let paths = root
        .get("paths")
        .and_then(Value::as_object)
        .ok_or("el documento no tiene `paths`")?;

    let mut warnings = Vec::new();
    let info = root.get("info").cloned().unwrap_or(json!({}));
    let title = opts
        .name
        .clone()
        .or_else(|| {
            info.get("title")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "API importada".into());
    let description = info
        .get("description")
        .and_then(Value::as_str)
        .map(|d| truncate(d.lines().next().unwrap_or(""), 300))
        .unwrap_or_default();

    let upstream = if openapi3 {
        root.pointer("/servers/0/url")
            .and_then(Value::as_str)
            .map(str::to_string)
    } else {
        root.get("host").and_then(Value::as_str).map(|h| {
            let scheme = root
                .pointer("/schemes/0")
                .and_then(Value::as_str)
                .unwrap_or("https");
            let base = root.get("basePath").and_then(Value::as_str).unwrap_or("");
            format!("{scheme}://{h}{base}")
        })
    };

    let auth = detect_auth(&root, openapi3);

    let mut endpoints = Vec::new();
    for (raw_path, item) in paths {
        let item = resolve(&root, item);
        let path_params: Vec<Value> = item
            .get("parameters")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let path = normalize_path(raw_path);
        if PathTemplate::parse(&path).is_err() {
            warnings.push(format!(
                "se omitió la ruta {raw_path}: formato no soportado"
            ));
            continue;
        }
        for (key, method) in METHODS {
            let Some(op) = item.get(*key) else { continue };
            match build_endpoint(&root, openapi3, &path, *method, op, &path_params) {
                Ok((ep, mut w)) => {
                    warnings.append(&mut w);
                    endpoints.push(ep);
                }
                Err(e) => warnings.push(format!("{} {raw_path}: {e}", method.as_str())),
            }
        }
    }
    if endpoints.is_empty() {
        return Err("no se encontraron operaciones en `paths`".into());
    }

    let id = slugify(&title);
    let service = Service {
        schema_version: SCHEMA_VERSION,
        id: id.clone(),
        name: title,
        description,
        upstream_url: upstream,
        prefix: opts.prefix.clone().unwrap_or_else(|| format!("/{id}")),
        port: None,
        auto_start: true,
        variables: BTreeMap::new(),
        auth,
        endpoints,
        workflows: vec![],
    };
    Ok(Imported { service, warnings })
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}

/// `/users/{user-id}` y `/files/{path}` son válidos tal cual; se limpian nombres raros.
fn normalize_path(p: &str) -> String {
    let p = if p.starts_with('/') {
        p.to_string()
    } else {
        format!("/{p}")
    };
    p.split('/')
        .map(|seg| {
            if let Some(inner) = seg.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                let clean: String = inner
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                            c
                        } else {
                            '_'
                        }
                    })
                    .collect();
                format!("{{{clean}}}")
            } else {
                seg.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn detect_auth(root: &Value, openapi3: bool) -> AuthConfig {
    let schemes = if openapi3 {
        root.pointer("/components/securitySchemes")
    } else {
        root.get("securityDefinitions")
    };
    let Some(schemes) = schemes.and_then(Value::as_object) else {
        return AuthConfig::None;
    };
    // Esquema exigido globalmente; si no hay, el primero declarado.
    let name = root
        .pointer("/security/0")
        .and_then(Value::as_object)
        .and_then(|m| m.keys().next().cloned())
        .or_else(|| schemes.keys().next().cloned());
    let Some(scheme) = name.and_then(|n| schemes.get(&n)).map(|s| resolve(root, s)) else {
        return AuthConfig::None;
    };
    match (
        scheme.get("type").and_then(Value::as_str),
        scheme.get("scheme").and_then(Value::as_str),
    ) {
        (Some("http"), Some(s)) if s.eq_ignore_ascii_case("bearer") => {
            AuthConfig::Bearer { token: None }
        }
        (Some("http"), Some(s)) if s.eq_ignore_ascii_case("basic") => AuthConfig::Basic {
            username: None,
            password: None,
        },
        (Some("basic"), _) => AuthConfig::Basic {
            username: None,
            password: None,
        },
        (Some("oauth2" | "openIdConnect"), _) => AuthConfig::Bearer { token: None },
        (Some("apiKey"), _) if scheme.get("in").and_then(Value::as_str) == Some("header") => {
            AuthConfig::ApiKey {
                header: scheme
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("X-Api-Key")
                    .to_string(),
                value: None,
            }
        }
        _ => AuthConfig::None,
    }
}

fn param_spec(root: &Value, p: &Value) -> Option<(String, ParamSpec)> {
    let p = resolve(root, p);
    let location = p.get("in")?.as_str()?.to_string();
    let name = p.get("name")?.as_str()?.to_string();
    let schema = p.get("schema").map(|s| resolve(root, s)).unwrap_or(p);
    let example = p
        .get("example")
        .or_else(|| schema.get("example"))
        .map(|e| match e {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        });
    Some((
        location.clone(),
        ParamSpec {
            name,
            required: p
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(location == "path"),
            description: p
                .get("description")
                .and_then(Value::as_str)
                .map(|d| truncate(d, 200))
                .unwrap_or_default(),
            pattern: schema
                .get("pattern")
                .and_then(Value::as_str)
                .map(str::to_string),
            example,
        },
    ))
}

fn build_endpoint(
    root: &Value,
    openapi3: bool,
    path: &str,
    method: HttpMethod,
    op: &Value,
    path_level_params: &[Value],
) -> Result<(Endpoint, Vec<String>), String> {
    let warnings = Vec::new();
    let mut request = RequestSpec::default();

    // Parámetros (los de la operación sustituyen a los de la ruta con el mismo nombre).
    let mut params: BTreeMap<(String, String), ParamSpec> = BTreeMap::new();
    let op_params = op
        .get("parameters")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut body_param = None;
    for p in path_level_params.iter().chain(op_params.iter()) {
        let resolved = resolve(root, p);
        match resolved.get("in").and_then(Value::as_str) {
            Some("body") => body_param = Some(resolved.clone()),
            Some("formData") => {
                request.body.get_or_insert(BodySpec {
                    kind: BodyKind::Form,
                    required: false,
                    schema: None,
                    example: None,
                });
            }
            _ => {
                if let Some((loc, spec)) = param_spec(root, p) {
                    params.insert((loc, spec.name.to_ascii_lowercase()), spec);
                }
            }
        }
    }
    for ((loc, _), spec) in params {
        match loc.as_str() {
            "query" => request.query.push(spec),
            "path" => request.path_params.push(spec),
            "header" => {
                let lower = spec.name.to_ascii_lowercase();
                if lower != "authorization" && lower != "content-type" && lower != "accept" {
                    request.headers.push(spec);
                }
            }
            _ => {}
        }
    }
    // Solo parámetros de ruta que existen en la plantilla.
    let names = PathTemplate::parse(path)
        .map(|t| {
            t.param_names()
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    request.path_params.retain(|p| names.contains(&p.name));

    // Body esperado.
    if openapi3 {
        if let Some(rb) = op.get("requestBody").map(|r| resolve(root, r)) {
            let required = rb.get("required").and_then(Value::as_bool).unwrap_or(false);
            if let Some(content) = rb.get("content").and_then(Value::as_object) {
                let (ct, media) = pick_media(content);
                let kind = body_kind(&ct);
                let schema = media
                    .get("schema")
                    .filter(|_| {
                        matches!(kind, BodyKind::Json | BodyKind::Form | BodyKind::Multipart)
                    })
                    .map(|s| to_json_schema(root, s));
                let example = media_example(root, media);
                request.body = Some(BodySpec {
                    kind,
                    required,
                    schema,
                    example,
                });
            }
        }
    } else if let Some(bp) = body_param {
        let schema = bp.get("schema");
        request.body = Some(BodySpec {
            kind: BodyKind::Json,
            required: bp.get("required").and_then(Value::as_bool).unwrap_or(false),
            schema: schema.map(|s| to_json_schema(root, s)),
            example: schema.map(|s| sample(root, s)),
        });
    }

    // Respuestas → escenarios.
    let mut scenarios: Vec<Scenario> = Vec::new();
    let mut default_idx = None;
    if let Some(responses) = op.get("responses").and_then(Value::as_object) {
        let mut entries: Vec<(&String, &Value)> = responses.iter().collect();
        entries.sort_by_key(|(code, _)| status_of(code));
        for (code, resp) in entries {
            let resp = resolve(root, resp);
            let status = status_of(code);
            let desc = resp
                .get("description")
                .and_then(Value::as_str)
                .map(|d| truncate(d.lines().next().unwrap_or(""), 80))
                .unwrap_or_default();
            let name = if desc.is_empty() {
                format!("HTTP {status}")
            } else {
                format!("{status} · {desc}")
            };
            let body = response_body(root, openapi3, resp);
            let mut headers = BTreeMap::new();
            if let Some(hs) = resp.get("headers").and_then(Value::as_object) {
                for (h, hv) in hs.iter().take(10) {
                    let hv = resolve(root, hv);
                    let schema = hv.get("schema").unwrap_or(hv);
                    let v = sample(root, schema);
                    let text = match v {
                        Value::String(s) => s,
                        Value::Null => continue,
                        other => other.to_string(),
                    };
                    headers.insert(h.to_ascii_lowercase(), text);
                }
            }
            let id = scenario_id(&name, &scenarios);
            if default_idx.is_none() && (200..300).contains(&status) {
                default_idx = Some(scenarios.len());
            }
            scenarios.push(Scenario {
                id,
                name,
                description: String::new(),
                enabled: true,
                rule_mode: RuleMode::All,
                rules: vec![],
                response: ResponseDef {
                    status,
                    headers,
                    body,
                    delay_ms: 0,
                    jitter_ms: 0,
                },
                workflows: vec![],
                probability: None,
                resource: None,
            });
        }
    }
    if scenarios.is_empty() {
        scenarios.push(Scenario {
            id: "ok".into(),
            name: "Respuesta exitosa".into(),
            description: String::new(),
            enabled: true,
            rule_mode: RuleMode::All,
            rules: vec![],
            response: ResponseDef::json(200, json!({})),
            workflows: vec![],
            probability: None,
            resource: None,
        });
        default_idx = Some(0);
    }
    // La respuesta exitosa responde por defecto (al final); las demás se fuerzan.
    let default_idx = default_idx.unwrap_or(0);
    let default = scenarios.remove(default_idx);
    for s in scenarios.iter_mut() {
        s.rules = vec![forced_rule(&s.id)];
    }
    scenarios.push(default);

    let id = endpoint_id_hint(method, path);
    let summary = op
        .get("summary")
        .and_then(Value::as_str)
        .or_else(|| op.get("operationId").and_then(Value::as_str))
        .map(|s| truncate(s, 120))
        .unwrap_or_default();
    let description = op
        .get("description")
        .and_then(Value::as_str)
        .map(|d| truncate(d, 400))
        .unwrap_or_default();

    Ok((
        Endpoint {
            schema_version: SCHEMA_VERSION,
            id,
            name: summary,
            description,
            enabled: true,
            method,
            path: path.to_string(),
            request,
            scenarios,
            active_scenario: None,
            on_validation_error: None,
        },
        warnings,
    ))
}

fn status_of(code: &str) -> u16 {
    let c = code.to_ascii_uppercase();
    if let Ok(n) = c.parse::<u16>() {
        return n;
    }
    match c.chars().next() {
        Some(d @ '1'..='5') if c.ends_with("XX") => (d as u16 - '0' as u16) * 100,
        _ => 500, // `default`
    }
}

fn pick_media(content: &serde_json::Map<String, Value>) -> (String, &Value) {
    let preferred = [
        "application/json",
        "application/problem+json",
        "application/x-www-form-urlencoded",
        "multipart/form-data",
    ];
    for p in preferred {
        if let Some((k, v)) = content.iter().find(|(k, _)| k.starts_with(p)) {
            return (k.clone(), v);
        }
    }
    if let Some((k, v)) = content.iter().find(|(k, _)| k.contains("json")) {
        return (k.clone(), v);
    }
    content
        .iter()
        .next()
        .map(|(k, v)| (k.clone(), v))
        .unwrap_or((String::new(), &Value::Null))
}

fn body_kind(ct: &str) -> BodyKind {
    let ct = ct.to_ascii_lowercase();
    if ct.contains("json") {
        BodyKind::Json
    } else if ct.contains("xml") {
        BodyKind::Xml
    } else if ct.starts_with("application/x-www-form-urlencoded") {
        BodyKind::Form
    } else if ct.starts_with("multipart/") {
        BodyKind::Multipart
    } else if ct.starts_with("text/") {
        BodyKind::Text
    } else if ct.is_empty() {
        BodyKind::Any
    } else {
        BodyKind::Binary
    }
}

fn media_example(root: &Value, media: &Value) -> Option<Value> {
    if let Some(e) = media.get("example") {
        return Some(e.clone());
    }
    if let Some(first) = media
        .get("examples")
        .and_then(Value::as_object)
        .and_then(|m| m.values().next())
    {
        let first = resolve(root, first);
        if let Some(v) = first.get("value") {
            return Some(v.clone());
        }
    }
    media.get("schema").map(|s| sample(root, s))
}

fn response_body(root: &Value, openapi3: bool, resp: &Value) -> ResponseBody {
    if openapi3 {
        let Some(content) = resp.get("content").and_then(Value::as_object) else {
            return ResponseBody::None;
        };
        if content.is_empty() {
            return ResponseBody::None;
        }
        let (ct, media) = pick_media(content);
        let example = media_example(root, media).unwrap_or(Value::Null);
        if ct.contains("json") || ct.is_empty() {
            ResponseBody::Json { content: example }
        } else {
            ResponseBody::Text {
                content: match example {
                    Value::String(s) => s,
                    Value::Null => String::new(),
                    other => other.to_string(),
                },
                content_type: Some(ct),
            }
        }
    } else {
        if let Some(ex) = resp.get("examples").and_then(Value::as_object)
            && let Some(v) = ex.get("application/json").or_else(|| ex.values().next())
        {
            return ResponseBody::Json { content: v.clone() };
        }
        match resp.get("schema") {
            Some(s) => ResponseBody::Json {
                content: sample(root, s),
            },
            None => ResponseBody::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PETSTORE: &str = r##"
openapi: 3.0.3
info: { title: Firma Digital, version: "1.0", description: "API de firma.\nMás detalle." }
servers: [ { url: "https://api.firma.mx/v1" } ]
components:
  securitySchemes:
    token: { type: http, scheme: bearer }
  schemas:
    Documento:
      type: object
      required: [id, status]
      properties:
        id: { type: string, format: uuid }
        status: { type: string, enum: [pending, signed] }
        created_at: { type: string, format: date-time }
security: [ { token: [] } ]
paths:
  /documents:
    get:
      summary: Listar documentos
      parameters: [ { name: page, in: query, schema: { type: integer } } ]
      responses:
        "200":
          description: OK
          content: { application/json: { schema: { type: array, items: { $ref: "#/components/schemas/Documento" } } } }
    post:
      operationId: createDocument
      requestBody:
        required: true
        content:
          application/json:
            schema: { type: object, required: [name], properties: { name: { type: string } } }
            example: { name: contrato.pdf }
      responses:
        "201": { description: Creado, content: { application/json: { schema: { $ref: "#/components/schemas/Documento" } } } }
        "422": { description: Datos inválidos, content: { application/json: { example: { error: invalid } } } }
  /documents/{id}/sign:
    parameters: [ { name: id, in: path, required: true, schema: { type: string, pattern: "^doc_" } } ]
    post:
      summary: Firmar
      responses:
        "202": { description: Aceptado }
        "404": { description: No existe }
"##;

    #[test]
    fn converts_openapi3_yaml() {
        let imp = from_openapi(PETSTORE, &ImportOptions::default()).unwrap();
        let s = &imp.service;
        assert_eq!(s.name, "Firma Digital");
        assert_eq!(s.id, "firma-digital");
        assert_eq!(s.description, "API de firma.");
        assert_eq!(s.upstream_url.as_deref(), Some("https://api.firma.mx/v1"));
        assert!(matches!(s.auth, AuthConfig::Bearer { .. }));
        assert_eq!(s.endpoints.len(), 3);

        let list = s
            .endpoints
            .iter()
            .find(|e| e.method == HttpMethod::Get)
            .unwrap();
        assert_eq!(list.request.query[0].name, "page");
        let body = match &list.scenarios.last().unwrap().response.body {
            ResponseBody::Json { content } => content.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(body[0]["status"], "pending");
        assert_eq!(body[0]["id"], "{{ uuid() }}");

        let create = s
            .endpoints
            .iter()
            .find(|e| e.path == "/documents" && e.method == HttpMethod::Post)
            .unwrap();
        assert_eq!(create.name, "createDocument");
        let rb = create.request.body.as_ref().unwrap();
        assert!(rb.required);
        assert_eq!(rb.example, Some(json!({"name": "contrato.pdf"})));
        // 201 por defecto (último, sin reglas); 422 forzable.
        assert_eq!(create.scenarios.len(), 2);
        assert_eq!(create.scenarios[1].response.status, 201);
        assert!(create.scenarios[1].rules.is_empty());
        assert_eq!(create.scenarios[0].response.status, 422);
        assert_eq!(create.scenarios[0].rules[0].source, RuleSource::Header);

        let sign = s
            .endpoints
            .iter()
            .find(|e| e.path == "/documents/{id}/sign")
            .unwrap();
        assert_eq!(
            sign.request.path_params[0].pattern.as_deref(),
            Some("^doc_")
        );
        assert_eq!(sign.scenarios.last().unwrap().response.status, 202);
        // Todo lo generado pasa la validación del dominio.
        for e in &s.endpoints {
            assert!(
                fakes_domain::validate::validate_endpoint(e, &[]).is_empty(),
                "{}",
                e.id
            );
        }
    }

    #[test]
    fn converts_swagger2_json() {
        let doc = json!({
            "swagger": "2.0", "info": {"title": "Pagos"}, "host": "api.pagos.mx", "basePath": "/v2", "schemes": ["https"],
            "securityDefinitions": {"key": {"type": "apiKey", "in": "header", "name": "X-Api-Key"}},
            "definitions": {"Pago": {"type": "object", "properties": {"monto": {"type": "number"}}}},
            "paths": {"/pagos": {"post": {
                "parameters": [{"in": "body", "name": "body", "required": true, "schema": {"$ref": "#/definitions/Pago"}}],
                "responses": {"200": {"description": "ok", "schema": {"$ref": "#/definitions/Pago"}}}
            }}}
        });
        let imp = from_openapi(&doc.to_string(), &ImportOptions::default()).unwrap();
        assert_eq!(
            imp.service.upstream_url.as_deref(),
            Some("https://api.pagos.mx/v2")
        );
        assert!(
            matches!(&imp.service.auth, AuthConfig::ApiKey { header, .. } if header == "X-Api-Key")
        );
        let ep = &imp.service.endpoints[0];
        assert_eq!(
            ep.request.body.as_ref().unwrap().schema.as_ref().unwrap()["properties"]["monto"]["type"],
            "number"
        );
    }

    #[test]
    fn rejects_non_openapi() {
        assert!(from_openapi("{\"hola\": 1}", &ImportOptions::default()).is_err());
        assert!(from_openapi("no es json ni yaml: [", &ImportOptions::default()).is_err());
    }
}
