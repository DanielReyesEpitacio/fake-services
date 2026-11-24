//! Colecciones de Postman v2.0/v2.1 → servicio de Fake Services.
//!
//! Cada petición se vuelve un endpoint; los ejemplos guardados (`response`)
//! se vuelven escenarios. Peticiones repetidas (mismo método y ruta) se
//! combinan en un solo endpoint.

use std::collections::BTreeMap;

use fakes_domain::*;
use serde_json::{Value, json};

use crate::{ImportOptions, Imported, forced_rule, parse_document, scenario_id};

pub fn from_postman(text: &str, opts: &ImportOptions) -> Result<Imported, String> {
    let root = parse_document(text)?;
    let info = root
        .get("info")
        .ok_or("no es una colección de Postman (falta `info`)")?;
    let schema = info.get("schema").and_then(Value::as_str).unwrap_or("");
    if !schema.contains("getpostman.com") && !schema.contains("postman") {
        return Err("no es una colección de Postman v2 (el `info.schema` no es de Postman)".into());
    }
    let items = root
        .get("item")
        .and_then(Value::as_array)
        .ok_or("la colección no tiene `item`")?;

    let name = opts
        .name
        .clone()
        .or_else(|| info.get("name").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| "Colección importada".into());
    let description = description_text(info.get("description"))
        .lines()
        .next()
        .unwrap_or("")
        .to_string();

    let mut variables = BTreeMap::new();
    if let Some(vars) = root.get("variable").and_then(Value::as_array) {
        for v in vars {
            if let (Some(k), Some(val)) = (v.get("key").and_then(Value::as_str), v.get("value")) {
                let clean: String = k
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                    .collect();
                if !clean.is_empty() && !clean.starts_with(|c: char| c.is_ascii_digit()) {
                    variables.insert(
                        clean,
                        match val {
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        },
                    );
                }
            }
        }
    }

    let mut warnings = Vec::new();
    let mut endpoints: Vec<Endpoint> = Vec::new();
    let mut upstream = None;
    walk(items, &mut endpoints, &mut warnings, &mut upstream, "");

    if endpoints.is_empty() {
        return Err("la colección no tiene peticiones".into());
    }
    // Escenario por defecto: el primer 2xx va al final sin reglas; el resto se fuerza.
    for ep in endpoints.iter_mut() {
        if ep.scenarios.is_empty() {
            ep.scenarios.push(Scenario {
                id: "ok".into(),
                name: "Respuesta exitosa".into(),
                description: String::new(),
                enabled: true,
                rule_mode: RuleMode::All,
                rules: vec![],
                response: ResponseDef::json(
                    if ep.method == HttpMethod::Post {
                        201
                    } else {
                        200
                    },
                    json!({ "ok": true }),
                ),
                workflows: vec![],
                probability: None,
                resource: None,
            });
            continue;
        }
        let idx = ep
            .scenarios
            .iter()
            .position(|s| (200..300).contains(&s.response.status))
            .unwrap_or(0);
        let default = ep.scenarios.remove(idx);
        for s in ep.scenarios.iter_mut() {
            s.rules = vec![forced_rule(&s.id)];
        }
        ep.scenarios.push(default);
    }

    let id = slugify(&name);
    Ok(Imported {
        service: Service {
            schema_version: SCHEMA_VERSION,
            id: id.clone(),
            name,
            description,
            upstream_url: upstream,
            prefix: opts.prefix.clone().unwrap_or_else(|| format!("/{id}")),
            port: None,
            auto_start: true,
            variables,
            auth: collection_auth(root.get("auth")),
            endpoints,
            workflows: vec![],
        },
        warnings,
    })
}

fn description_text(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Object(m)) => m
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    }
}

fn collection_auth(auth: Option<&Value>) -> AuthConfig {
    let Some(auth) = auth else {
        return AuthConfig::None;
    };
    match auth.get("type").and_then(Value::as_str) {
        Some("bearer" | "oauth2") => AuthConfig::Bearer { token: None },
        Some("basic") => AuthConfig::Basic {
            username: None,
            password: None,
        },
        Some("apikey") => {
            let entries = auth
                .get("apikey")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let get = |k: &str| {
                entries
                    .iter()
                    .find(|e| e.get("key").and_then(Value::as_str) == Some(k))
                    .and_then(|e| e.get("value").and_then(Value::as_str))
                    .map(str::to_string)
            };
            if get("in").as_deref() == Some("query") {
                AuthConfig::None
            } else {
                AuthConfig::ApiKey {
                    header: get("key").unwrap_or_else(|| "X-Api-Key".into()),
                    value: None,
                }
            }
        }
        _ => AuthConfig::None,
    }
}

fn walk(
    items: &[Value],
    out: &mut Vec<Endpoint>,
    warnings: &mut Vec<String>,
    upstream: &mut Option<String>,
    folder: &str,
) {
    for item in items {
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if let Some(children) = item.get("item").and_then(Value::as_array) {
            let f = if folder.is_empty() {
                name.clone()
            } else {
                format!("{folder} / {name}")
            };
            walk(children, out, warnings, upstream, &f);
            continue;
        }
        let Some(req) = item.get("request") else {
            continue;
        };
        match convert(
            &name,
            req,
            item.get("response").and_then(Value::as_array),
            upstream,
        ) {
            Ok(ep) => merge(out, ep),
            Err(e) => warnings.push(format!("«{name}»: {e}")),
        }
    }
}

fn merge(out: &mut Vec<Endpoint>, ep: Endpoint) {
    if let Some(existing) = out
        .iter_mut()
        .find(|e| e.method == ep.method && e.path == ep.path)
    {
        for mut s in ep.scenarios {
            s.id = scenario_id(&s.name, &existing.scenarios);
            existing.scenarios.push(s);
        }
    } else {
        let mut ep = ep;
        ep.id = unique_id(&ep.id, out.iter().map(|e| e.id.as_str()));
        out.push(ep);
    }
}

/// Convierte segmentos de Postman (`:id`, `{{userId}}`) en parámetros `{id}`.
fn segment(seg: &str) -> String {
    if let Some(name) = seg.strip_prefix(':') {
        return format!("{{{}}}", clean_param(name));
    }
    if let Some(inner) = seg.strip_prefix("{{").and_then(|s| s.strip_suffix("}}")) {
        return format!("{{{}}}", clean_param(inner));
    }
    seg.to_string()
}

fn clean_param(s: &str) -> String {
    let c: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if c.is_empty() { "param".into() } else { c }
}

fn url_parts(url: &Value) -> (Vec<String>, Vec<(String, String)>, Option<String>) {
    match url {
        Value::String(raw) => parse_raw_url(raw),
        Value::Object(m) => {
            let path: Vec<String> = match m.get("path") {
                Some(Value::Array(segs)) => segs
                    .iter()
                    .filter_map(|s| match s {
                        Value::String(s) => Some(s.clone()),
                        Value::Object(o) => {
                            o.get("value").and_then(Value::as_str).map(str::to_string)
                        }
                        _ => None,
                    })
                    .collect(),
                Some(Value::String(p)) => p.split('/').map(str::to_string).collect(),
                _ => m
                    .get("raw")
                    .and_then(Value::as_str)
                    .map(|r| parse_raw_url(r).0)
                    .unwrap_or_default(),
            };
            let query = m
                .get("query")
                .and_then(Value::as_array)
                .map(|q| {
                    q.iter()
                        .filter(|e| e.get("disabled").and_then(Value::as_bool) != Some(true))
                        .filter_map(|e| {
                            Some((
                                e.get("key")?.as_str()?.to_string(),
                                e.get("value")
                                    .and_then(Value::as_str)
                                    .unwrap_or("")
                                    .to_string(),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let host = match m.get("host") {
                Some(Value::Array(h)) => Some(
                    h.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("."),
                ),
                Some(Value::String(h)) => Some(h.clone()),
                _ => None,
            };
            let base = host.filter(|h| !h.starts_with("{{")).map(|h| {
                let proto = m.get("protocol").and_then(Value::as_str).unwrap_or("https");
                format!("{proto}://{h}")
            });
            (path, query, base)
        }
        _ => (vec![], vec![], None),
    }
}

fn parse_raw_url(raw: &str) -> (Vec<String>, Vec<(String, String)>, Option<String>) {
    let (no_query, query) = raw.split_once('?').unwrap_or((raw, ""));
    let mut base = None;
    let rest = if let Some(i) = no_query.find("://") {
        let after = &no_query[i + 3..];
        let (host, path) = after.split_once('/').unwrap_or((after, ""));
        base = Some(format!("{}{}", &no_query[..i + 3], host));
        path.to_string()
    } else if no_query.starts_with("{{") {
        // {{baseUrl}}/ruta
        no_query
            .split_once("}}")
            .map(|(_, p)| p.trim_start_matches('/').to_string())
            .unwrap_or_default()
    } else {
        no_query.trim_start_matches('/').to_string()
    };
    let q = query
        .split('&')
        .filter(|s| !s.is_empty())
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (k.to_string(), v.to_string())
        })
        .collect();
    (rest.split('/').map(str::to_string).collect(), q, base)
}

fn convert(
    name: &str,
    req: &Value,
    responses: Option<&Vec<Value>>,
    upstream: &mut Option<String>,
) -> Result<Endpoint, String> {
    let (method_str, url) = match req {
        Value::String(u) => ("GET".to_string(), Value::String(u.clone())),
        _ => (
            req.get("method")
                .and_then(Value::as_str)
                .unwrap_or("GET")
                .to_ascii_uppercase(),
            req.get("url").cloned().unwrap_or(Value::Null),
        ),
    };
    let method: HttpMethod = serde_json::from_value(json!(method_str))
        .map_err(|_| format!("método no soportado: {method_str}"))?;
    let (segments, query, base) = url_parts(&url);
    if upstream.is_none() {
        *upstream = base;
    }
    let mut seen = std::collections::HashSet::new();
    let segs: Vec<String> = segments
        .iter()
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut seg = segment(s);
            // Parámetros repetidos: se renombran.
            if seg.starts_with('{') {
                let mut n = 2;
                let base = seg.clone();
                while !seen.insert(seg.clone()) {
                    seg = format!("{}_{n}}}", base.trim_end_matches('}'));
                    n += 1;
                }
            }
            seg
        })
        .collect();
    let path = format!("/{}", segs.join("/"));
    PathTemplate::parse(&path).map_err(|e| format!("ruta {path}: {}", e.0))?;

    let mut request = RequestSpec {
        query: query
            .into_iter()
            .map(|(k, v)| ParamSpec {
                name: k,
                required: false,
                description: String::new(),
                pattern: None,
                example: (!v.is_empty()).then_some(v),
            })
            .collect(),
        ..Default::default()
    };
    // Quitar duplicados de query.
    let mut qseen = std::collections::HashSet::new();
    request.query.retain(|p| qseen.insert(p.name.clone()));

    if let Some(body) = req.get("body") {
        match body.get("mode").and_then(Value::as_str) {
            Some("raw") => {
                let raw = body.get("raw").and_then(Value::as_str).unwrap_or("");
                let lang = body
                    .pointer("/options/raw/language")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let parsed = serde_json::from_str::<Value>(raw).ok();
                request.body = Some(BodySpec {
                    kind: if lang == "json" || parsed.is_some() {
                        BodyKind::Json
                    } else if lang == "xml" {
                        BodyKind::Xml
                    } else {
                        BodyKind::Any
                    },
                    required: false,
                    schema: None,
                    example: parsed,
                });
            }
            Some("urlencoded") => {
                request.body = Some(BodySpec {
                    kind: BodyKind::Form,
                    required: false,
                    schema: None,
                    example: None,
                })
            }
            Some("formdata") => {
                request.body = Some(BodySpec {
                    kind: BodyKind::Multipart,
                    required: false,
                    schema: None,
                    example: None,
                })
            }
            _ => {}
        }
    }

    let mut scenarios: Vec<Scenario> = Vec::new();
    for r in responses.map(Vec::as_slice).unwrap_or(&[]) {
        let status = r.get("code").and_then(Value::as_u64).unwrap_or(200) as u16;
        let rname = r.get("name").and_then(Value::as_str).unwrap_or("").trim();
        let label = if rname.is_empty() {
            format!("HTTP {status}")
        } else {
            format!("{status} · {rname}")
        };
        let mut headers = BTreeMap::new();
        let mut content_type = None;
        if let Some(hs) = r.get("header").and_then(Value::as_array) {
            for h in hs {
                if let (Some(k), Some(v)) = (
                    h.get("key").and_then(Value::as_str),
                    h.get("value").and_then(Value::as_str),
                ) {
                    let lk = k.to_ascii_lowercase();
                    if lk == "content-type" {
                        content_type = Some(v.to_string());
                    } else if !matches!(
                        lk.as_str(),
                        "content-length"
                            | "date"
                            | "connection"
                            | "server"
                            | "transfer-encoding"
                            | "set-cookie"
                    ) {
                        headers.insert(lk, v.to_string());
                    }
                }
            }
        }
        let raw = r.get("body").and_then(Value::as_str).unwrap_or("");
        let body = if raw.trim().is_empty() {
            ResponseBody::None
        } else if let Ok(v) = serde_json::from_str::<Value>(raw) {
            ResponseBody::Json { content: v }
        } else {
            ResponseBody::Text {
                content: raw.to_string(),
                content_type,
            }
        };
        let id = scenario_id(&label, &scenarios);
        scenarios.push(Scenario {
            id,
            name: label,
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

    Ok(Endpoint {
        schema_version: SCHEMA_VERSION,
        id: endpoint_id_hint(method, &path),
        name: name.to_string(),
        description: description_text(req.get("description"))
            .chars()
            .take(400)
            .collect(),
        enabled: true,
        method,
        path,
        request,
        scenarios,
        active_scenario: None,
        on_validation_error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collection() -> Value {
        json!({
            "info": {"name": "Belvo", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"},
            "auth": {"type": "basic"},
            "variable": [{"key": "baseUrl", "value": "https://sandbox.belvo.com"}],
            "item": [
                {"name": "Links", "item": [
                    {"name": "Crear link", "request": {
                        "method": "POST",
                        "url": {"raw": "{{baseUrl}}/api/links/", "host": ["{{baseUrl}}"], "path": ["api", "links", ""]},
                        "body": {"mode": "raw", "raw": "{\"institution\": \"erebor_mx_retail\"}", "options": {"raw": {"language": "json"}}}
                    }, "response": [
                        {"name": "Creado", "code": 201, "header": [{"key": "Content-Type", "value": "application/json"}], "body": "{\"id\": \"abc\", \"status\": \"valid\"}"},
                        {"name": "Credenciales inválidas", "code": 400, "body": "[{\"code\": \"invalid\"}]"}
                    ]},
                    {"name": "Detalle", "request": {"method": "GET", "url": "https://sandbox.belvo.com/api/links/:id/?expand=all"},
                     "response": [{"name": "ok", "code": 200, "body": "{\"id\": \"abc\"}"}]},
                    {"name": "Detalle (no existe)", "request": {"method": "GET", "url": "https://sandbox.belvo.com/api/links/:id/"},
                     "response": [{"name": "no existe", "code": 404, "body": "{\"detail\": \"Not found\"}"}]}
                ]},
                {"name": "Salud", "request": "https://sandbox.belvo.com/health"}
            ]
        })
    }

    #[test]
    fn converts_collection() {
        let imp = from_postman(&collection().to_string(), &ImportOptions::default()).unwrap();
        let s = &imp.service;
        assert_eq!(s.id, "belvo");
        assert!(matches!(s.auth, AuthConfig::Basic { .. }));
        assert_eq!(s.variables["baseUrl"], "https://sandbox.belvo.com");
        assert_eq!(s.upstream_url.as_deref(), Some("https://sandbox.belvo.com"));
        assert_eq!(
            s.endpoints.len(),
            3,
            "{:?}",
            s.endpoints.iter().map(|e| &e.path).collect::<Vec<_>>()
        );

        let create = s
            .endpoints
            .iter()
            .find(|e| e.method == HttpMethod::Post)
            .unwrap();
        assert_eq!(create.path, "/api/links");
        assert_eq!(
            create.request.body.as_ref().unwrap().example,
            Some(json!({"institution": "erebor_mx_retail"}))
        );
        assert_eq!(create.scenarios.last().unwrap().response.status, 201);
        assert_eq!(create.scenarios[0].response.status, 400);

        // Dos peticiones a la misma ruta se combinan.
        let detail = s
            .endpoints
            .iter()
            .find(|e| e.path == "/api/links/{id}")
            .unwrap();
        assert_eq!(detail.scenarios.len(), 2);
        assert_eq!(detail.request.query[0].name, "expand");
        assert!(
            s.endpoints
                .iter()
                .any(|e| e.path == "/health" && e.scenarios[0].response.status == 200)
        );
        for e in &s.endpoints {
            assert!(
                fakes_domain::validate::validate_endpoint(e, &[]).is_empty(),
                "{}",
                e.id
            );
        }
    }

    #[test]
    fn rejects_non_postman() {
        assert!(
            from_postman(
                r#"{"info": {"name": "x"}, "item": []}"#,
                &ImportOptions::default()
            )
            .is_err()
        );
    }
}
