//! Motor de respuestas.
//!
//! Convierte un [`ResponseDef`] declarativo en bytes listos para enviar.
//! Las plantillas usan MiniJinja en modo seguro: no hay acceso al sistema,
//! la ejecución tiene un límite de "combustible" y los datos faltantes se
//! resuelven como vacíos en lugar de fallar.
//!
//! Funciones disponibles en plantillas: `uuid()`, `now(fmt?)`, `timestamp()`,
//! `timestamp_ms()`, `random_int(min, max)`, `random_float(min, max)`,
//! `random_string(len)`, `random_hex(len)`, `pick(lista)`, `fake(tipo)`.
//! Filtros extra: `b64encode`, `b64decode`, `fromjson`.

mod fake;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use base64::Engine;
use fakes_domain::{Issue, ResponseBody, ResponseDef};
use minijinja::value::Value as MjValue;
use minijinja::{Environment, Error as MjError, ErrorKind, UndefinedBehavior};
use regex::Regex;
use serde_json::{Map, Value};

/// Límite de pasos por plantilla para evitar bucles costosos.
const TEMPLATE_FUEL: u64 = 200_000;
/// Tamaño máximo de un archivo servido como respuesta.
const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("error de plantilla en {field}: {message}")]
    Template { field: String, message: String },
    #[error("archivo de respuesta: {0}")]
    File(String),
    #[error("base64 inválido: {0}")]
    Base64(String),
}

#[derive(Debug, Clone)]
pub struct RenderedResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub delay_ms: u64,
}

impl RenderedResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Entorno de plantillas compartido (las funciones son puras y sin E/S).
pub fn environment() -> &'static Environment<'static> {
    static ENV: OnceLock<Environment<'static>> = OnceLock::new();
    ENV.get_or_init(build_env)
}

fn build_env() -> Environment<'static> {
    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::Chainable);
    env.set_fuel(Some(TEMPLATE_FUEL));
    env.set_keep_trailing_newline(true);

    env.add_function("uuid", || uuid::Uuid::new_v4().to_string());
    env.add_function("now", |fmt: Option<String>| {
        let now = chrono::Utc::now();
        match fmt {
            Some(f) => now.format(&f).to_string(),
            None => now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        }
    });
    env.add_function("timestamp", || chrono::Utc::now().timestamp());
    env.add_function("timestamp_ms", || chrono::Utc::now().timestamp_millis());
    env.add_function("random_int", |min: Option<i64>, max: Option<i64>| {
        let (min, max) = (min.unwrap_or(0), max.unwrap_or(1000));
        if min >= max {
            min
        } else {
            fastrand::i64(min..=max)
        }
    });
    env.add_function("random_float", |min: Option<f64>, max: Option<f64>| {
        let (min, max) = (min.unwrap_or(0.0), max.unwrap_or(1.0));
        let v = min + fastrand::f64() * (max - min);
        (v * 100.0).round() / 100.0
    });
    env.add_function("random_string", |len: Option<usize>| {
        let len = len.unwrap_or(16).min(4096);
        std::iter::repeat_with(fastrand::alphanumeric)
            .take(len)
            .collect::<String>()
    });
    env.add_function("random_hex", |len: Option<usize>| {
        let len = len.unwrap_or(16).min(4096);
        std::iter::repeat_with(|| char::from_digit(fastrand::u32(0..16), 16).unwrap_or('0'))
            .take(len)
            .collect::<String>()
    });
    env.add_function("pick", |items: MjValue| -> Result<MjValue, MjError> {
        let len = items.len().unwrap_or(0);
        if len == 0 {
            return Ok(MjValue::UNDEFINED);
        }
        items.get_item(&MjValue::from(fastrand::usize(0..len)))
    });
    env.add_function("fake", |kind: String| -> Result<String, MjError> {
        fake::generate(&kind).ok_or_else(|| {
            MjError::new(
                ErrorKind::InvalidOperation,
                format!(
                    "fake('{kind}') no existe; opciones: {}",
                    fake::KINDS.join(", ")
                ),
            )
        })
    });
    env.add_filter("b64encode", |s: String| {
        base64::engine::general_purpose::STANDARD.encode(s)
    });
    env.add_filter("b64decode", |s: String| -> Result<String, MjError> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(s.trim())
            .map_err(|e| MjError::new(ErrorKind::InvalidOperation, e.to_string()))?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    });
    env.add_filter("fromjson", |s: String| -> Result<MjValue, MjError> {
        let v: Value = serde_json::from_str(&s)
            .map_err(|e| MjError::new(ErrorKind::InvalidOperation, e.to_string()))?;
        Ok(MjValue::from_serialize(&v))
    });
    env
}

fn single_expression() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\s*\{\{\s*(.+?)\s*\}\}\s*$").unwrap())
}

fn mj_err(field: &str, e: MjError) -> RenderError {
    let mut message = e.to_string();
    if let Some(detail) = e.detail()
        && !message.contains(detail)
    {
        message = format!("{message}: {detail}");
    }
    RenderError::Template {
        field: field.to_string(),
        message,
    }
}

/// Renderiza un texto como plantilla.
pub fn render_text(src: &str, ctx: &MjValue, field: &str) -> Result<String, RenderError> {
    if !src.contains("{{") && !src.contains("{%") {
        return Ok(src.to_string());
    }
    environment()
        .render_str(src, ctx)
        .map_err(|e| mj_err(field, e))
}

/// Renderiza un valor JSON: cada texto es plantilla; un texto que es solo
/// `{{ expr }}` conserva el tipo del resultado.
pub fn render_json(v: &Value, ctx: &MjValue, field: &str) -> Result<Value, RenderError> {
    Ok(match v {
        Value::String(s) => {
            if let Some(caps) = single_expression().captures(s) {
                let expr_src = &caps[1];
                if !expr_src.contains("}}") && !expr_src.contains("{{") {
                    let expr = environment()
                        .compile_expression(expr_src)
                        .map_err(|e| mj_err(field, e))?;
                    let out = expr.eval(ctx).map_err(|e| mj_err(field, e))?;
                    return Ok(if out.is_undefined() || out.is_none() {
                        Value::Null
                    } else {
                        serde_json::to_value(&out).unwrap_or(Value::Null)
                    });
                }
            }
            Value::String(render_text(s, ctx, field)?)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .enumerate()
                .map(|(i, x)| render_json(x, ctx, &format!("{field}[{i}]")))
                .collect::<Result<_, _>>()?,
        ),
        Value::Object(m) => {
            let mut out = Map::with_capacity(m.len());
            for (k, x) in m {
                let key = render_text(k, ctx, field)?;
                out.insert(key, render_json(x, ctx, &format!("{field}.{k}"))?);
            }
            Value::Object(out)
        }
        other => other.clone(),
    })
}

/// Construye la respuesta final.
pub fn render_response(
    def: &ResponseDef,
    ctx: &Value,
    files_root: Option<&Path>,
) -> Result<RenderedResponse, RenderError> {
    let ctx = MjValue::from_serialize(ctx);
    let mut headers: Vec<(String, String)> = Vec::new();
    for (k, v) in &def.headers {
        headers.push((k.clone(), render_text(v, &ctx, &format!("headers.{k}"))?));
    }
    let (body, default_ct): (Vec<u8>, Option<String>) = match &def.body {
        ResponseBody::None => (Vec::new(), None),
        ResponseBody::Json { content } => {
            let v = render_json(content, &ctx, "body")?;
            let bytes = serde_json::to_vec_pretty(&v).unwrap_or_default();
            (bytes, Some("application/json".into()))
        }
        ResponseBody::Text {
            content,
            content_type,
        } => {
            let text = render_text(content, &ctx, "body")?;
            let ct = content_type.clone().unwrap_or_else(|| {
                let t = text.trim_start();
                if t.starts_with("<?xml") {
                    "application/xml; charset=utf-8".into()
                } else if t.starts_with("<!DOCTYPE html") || t.starts_with("<html") {
                    "text/html; charset=utf-8".into()
                } else {
                    "text/plain; charset=utf-8".into()
                }
            });
            (text.into_bytes(), Some(ct))
        }
        ResponseBody::Binary {
            base64: b64,
            content_type,
        } => {
            // El base64 admite plantilla (p. ej. contenido guardado en un recurso).
            let b64 = render_text(b64, &ctx, "body.base64")?;
            let cleaned: String = b64.chars().filter(|c| !c.is_whitespace()).collect();
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(cleaned)
                .map_err(|e| RenderError::Base64(e.to_string()))?;
            (
                bytes,
                Some(
                    content_type
                        .clone()
                        .unwrap_or_else(|| "application/octet-stream".into()),
                ),
            )
        }
        ResponseBody::File { path, content_type } => {
            let root = files_root.ok_or_else(|| {
                RenderError::File("el espacio de trabajo no tiene carpeta de archivos".into())
            })?;
            let rendered = render_text(path, &ctx, "body.path")?;
            let full = safe_join(root, &rendered)?;
            let meta = std::fs::metadata(&full)
                .map_err(|e| RenderError::File(format!("{rendered}: {e}")))?;
            if meta.len() > MAX_FILE_BYTES {
                return Err(RenderError::File(format!(
                    "{rendered}: archivo demasiado grande"
                )));
            }
            let bytes =
                std::fs::read(&full).map_err(|e| RenderError::File(format!("{rendered}: {e}")))?;
            let ct = content_type.clone().unwrap_or_else(|| {
                mime_guess::from_path(&full)
                    .first_or_octet_stream()
                    .essence_str()
                    .to_string()
            });
            (bytes, Some(ct))
        }
    };
    if let Some(ct) = default_ct
        && !headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
    {
        headers.push(("content-type".into(), ct));
    }
    let jitter = if def.jitter_ms > 0 {
        fastrand::u64(0..=def.jitter_ms)
    } else {
        0
    };
    Ok(RenderedResponse {
        status: def.status,
        headers,
        body,
        delay_ms: def.delay_ms + jitter,
    })
}

/// Une `root` + `rel` impidiendo salir de `root` (incluye enlaces simbólicos).
pub fn safe_join(root: &Path, rel: &str) -> Result<PathBuf, RenderError> {
    if rel.contains("..") || rel.starts_with(['/', '\\']) || rel.contains(':') {
        return Err(RenderError::File(format!("ruta no permitida: {rel}")));
    }
    let root_c = root
        .canonicalize()
        .map_err(|e| RenderError::File(format!("carpeta files/: {e}")))?;
    let full = root_c
        .join(rel)
        .canonicalize()
        .map_err(|e| RenderError::File(format!("{rel}: {e}")))?;
    if !full.starts_with(&root_c) {
        return Err(RenderError::File(format!("ruta fuera de files/: {rel}")));
    }
    Ok(full)
}

// ---------------------------------------------------------------------------
// Validación de sintaxis de plantillas
// ---------------------------------------------------------------------------

fn check_syntax(issues: &mut Vec<Issue>, field: &str, src: &str) {
    if !src.contains("{{") && !src.contains("{%") {
        return;
    }
    if let Some(caps) = single_expression().captures(src)
        && !caps[1].contains("{{")
        && !caps[1].contains("}}")
    {
        if let Err(e) = environment().compile_expression(&caps[1]) {
            issues.push(Issue::new(field, format!("plantilla inválida: {e}")));
        }
        return;
    }
    if let Err(e) = environment().template_from_str(src) {
        issues.push(Issue::new(field, format!("plantilla inválida: {e}")));
    }
}

fn check_json(issues: &mut Vec<Issue>, field: &str, v: &Value) {
    match v {
        Value::String(s) => check_syntax(issues, field, s),
        Value::Array(a) => a
            .iter()
            .enumerate()
            .for_each(|(i, x)| check_json(issues, &format!("{field}[{i}]"), x)),
        Value::Object(m) => m.iter().for_each(|(k, x)| {
            check_syntax(issues, field, k);
            check_json(issues, &format!("{field}.{k}"), x)
        }),
        _ => {}
    }
}

/// Valida la sintaxis de todas las plantillas de una respuesta.
pub fn validate_templates(base: &str, def: &ResponseDef) -> Vec<Issue> {
    let mut issues = Vec::new();
    for (k, v) in &def.headers {
        check_syntax(&mut issues, &format!("{base}.headers.{k}"), v);
    }
    match &def.body {
        ResponseBody::Json { content } => {
            check_json(&mut issues, &format!("{base}.body.content"), content)
        }
        ResponseBody::Text { content, .. } => {
            check_syntax(&mut issues, &format!("{base}.body.content"), content)
        }
        ResponseBody::Binary { base64, .. } => {
            check_syntax(&mut issues, &format!("{base}.body.base64"), base64)
        }
        ResponseBody::File { path, .. } => {
            check_syntax(&mut issues, &format!("{base}.body.path"), path)
        }
        _ => {}
    }
    issues
}

#[cfg(test)]
mod tests;

/// Valida la sintaxis de una plantilla de texto.
pub fn validate_template_text(field: &str, src: &str) -> Vec<Issue> {
    let mut issues = Vec::new();
    check_syntax(&mut issues, field, src);
    issues
}

/// Valida la sintaxis de todas las plantillas dentro de un valor JSON.
pub fn validate_template_json(field: &str, v: &Value) -> Vec<Issue> {
    let mut issues = Vec::new();
    check_json(&mut issues, field, v);
    issues
}
