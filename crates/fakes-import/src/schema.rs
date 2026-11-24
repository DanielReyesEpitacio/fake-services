//! Utilidades sobre esquemas OpenAPI: resolver `$ref`, convertir a JSON
//! Schema válido y generar ejemplos.

use serde_json::{Map, Value, json};

const MAX_DEPTH: usize = 12;

/// Sigue un `$ref` local (`#/components/schemas/X`) hasta un valor concreto.
pub fn resolve<'a>(root: &'a Value, mut v: &'a Value) -> &'a Value {
    for _ in 0..20 {
        let Some(r) = v.get("$ref").and_then(Value::as_str) else {
            return v;
        };
        match r
            .strip_prefix('#')
            .and_then(|ptr| root.pointer(&unescape_pointer(ptr)))
        {
            Some(target) => v = target,
            None => return v,
        }
    }
    v
}

fn unescape_pointer(p: &str) -> String {
    // Los segmentos ya usan ~0/~1 si hace falta; algunos generadores dejan espacios codificados.
    p.replace("%20", " ")
}

/// Inlina referencias y normaliza un esquema OpenAPI a JSON Schema.
pub fn to_json_schema(root: &Value, schema: &Value) -> Value {
    inline(root, schema, 0, &mut Vec::new())
}

fn inline(root: &Value, schema: &Value, depth: usize, stack: &mut Vec<String>) -> Value {
    if depth > MAX_DEPTH {
        return json!({});
    }
    if let Some(r) = schema.get("$ref").and_then(Value::as_str) {
        if stack.iter().any(|s| s == r) {
            // Recursión: se corta con un esquema abierto.
            return json!({});
        }
        stack.push(r.to_string());
        let target = resolve(root, schema);
        let out = if std::ptr::eq(target, schema) {
            json!({})
        } else {
            inline(root, target, depth + 1, stack)
        };
        stack.pop();
        return out;
    }
    match schema {
        Value::Object(m) => {
            let mut out = Map::new();
            for (k, v) in m {
                match k.as_str() {
                    // Palabras de OpenAPI que no son JSON Schema.
                    "nullable" | "discriminator" | "xml" | "externalDocs" | "example"
                    | "deprecated" | "readOnly" | "writeOnly" => {}
                    "properties" | "patternProperties" | "definitions" | "$defs" => {
                        if let Value::Object(props) = v {
                            let mut p = Map::new();
                            for (pk, pv) in props {
                                p.insert(pk.clone(), inline(root, pv, depth + 1, stack));
                            }
                            out.insert(k.clone(), Value::Object(p));
                        }
                    }
                    "items" | "additionalProperties" | "not" => {
                        out.insert(k.clone(), inline(root, v, depth + 1, stack));
                    }
                    "allOf" | "anyOf" | "oneOf" => {
                        if let Value::Array(items) = v {
                            out.insert(
                                k.clone(),
                                Value::Array(
                                    items
                                        .iter()
                                        .map(|i| inline(root, i, depth + 1, stack))
                                        .collect(),
                                ),
                            );
                        }
                    }
                    _ => {
                        out.insert(k.clone(), v.clone());
                    }
                }
            }
            // `nullable: true` (OpenAPI 3.0) → el tipo también admite null.
            if m.get("nullable").and_then(Value::as_bool) == Some(true)
                && let Some(Value::String(t)) = out.get("type").cloned()
            {
                out.insert("type".into(), json!([t, "null"]));
            }
            // `format: binary` no es validable como JSON.
            if out.get("format").and_then(Value::as_str) == Some("binary") {
                out.remove("format");
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

/// Genera un ejemplo a partir de un esquema (ya inlinado o no).
/// Usa `example`, `default` o `enum` si existen; si no, valores representativos
/// con plantillas dinámicas (`{{ uuid() }}`, `{{ now() }}`...).
pub fn sample(root: &Value, schema: &Value) -> Value {
    sample_inner(root, schema, 0, "")
}

fn sample_inner(root: &Value, schema: &Value, depth: usize, name: &str) -> Value {
    if depth > 8 {
        return Value::Null;
    }
    let s = resolve(root, schema);
    if let Some(ex) = s.get("example") {
        return ex.clone();
    }
    if let Some(ex) = s
        .get("examples")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
    {
        return ex.clone();
    }
    if let Some(d) = s.get("default") {
        return d.clone();
    }
    if let Some(e) = s
        .get("enum")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
    {
        return e.clone();
    }
    if let Some(all) = s.get("allOf").and_then(Value::as_array) {
        let mut out = Map::new();
        for part in all {
            if let Value::Object(m) = sample_inner(root, part, depth + 1, name) {
                out.extend(m);
            }
        }
        return Value::Object(out);
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(first) = s.get(key).and_then(Value::as_array).and_then(|a| a.first()) {
            return sample_inner(root, first, depth + 1, name);
        }
    }
    let ty = match s.get("type") {
        Some(Value::String(t)) => t.as_str(),
        Some(Value::Array(ts)) => ts
            .iter()
            .filter_map(Value::as_str)
            .find(|t| *t != "null")
            .unwrap_or("null"),
        _ if s.get("properties").is_some() => "object",
        _ if s.get("items").is_some() => "array",
        _ => "",
    };
    match ty {
        "object" => {
            let mut out = Map::new();
            if let Some(Value::Object(props)) = s.get("properties") {
                for (k, v) in props {
                    out.insert(k.clone(), sample_inner(root, v, depth + 1, k));
                }
            }
            Value::Object(out)
        }
        "array" => match s.get("items") {
            Some(items) => Value::Array(vec![sample_inner(root, items, depth + 1, name)]),
            None => json!([]),
        },
        "integer" => json!(s.get("minimum").and_then(Value::as_i64).unwrap_or(1)),
        "number" => json!(s.get("minimum").and_then(Value::as_f64).unwrap_or(10.5)),
        "boolean" => json!(true),
        "string" => Value::String(sample_string(s, name)),
        _ => Value::Null,
    }
}

fn sample_string(s: &Value, name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    match s.get("format").and_then(Value::as_str) {
        Some("uuid") => return "{{ uuid() }}".into(),
        Some("date-time") => return "{{ now() }}".into(),
        Some("date") => return "{{ now('%Y-%m-%d') }}".into(),
        Some("email") => return "{{ fake('email') }}".into(),
        Some("uri" | "url") => return "https://example.com".into(),
        Some("byte") => return "ZXhhbXBsZQ==".into(),
        _ => {}
    }
    if lower == "id" || lower.ends_with("_id") || lower.ends_with("id") && lower.len() > 2 {
        return "{{ random_hex(12) }}".into();
    }
    if lower.contains("email") {
        return "{{ fake('email') }}".into();
    }
    if lower.contains("name") || lower.contains("nombre") {
        return "{{ fake('full_name') }}".into();
    }
    if lower.contains("phone") || lower.contains("telefono") {
        return "{{ fake('phone') }}".into();
    }
    if lower == "rfc" {
        return "{{ fake('rfc') }}".into();
    }
    if lower.contains("date") || lower.ends_with("_at") || lower.contains("fecha") {
        return "{{ now() }}".into();
    }
    if lower.contains("status") || lower.contains("estado") {
        return "active".into();
    }
    "string".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_and_inlines() {
        let root = json!({"components": {"schemas": {
            "Doc": {"type": "object", "required": ["id"], "properties": {
                "id": {"type": "string", "format": "uuid"},
                "owner": {"$ref": "#/components/schemas/User"},
                "parent": {"$ref": "#/components/schemas/Doc"},
                "note": {"type": "string", "nullable": true}
            }},
            "User": {"type": "object", "properties": {"email": {"type": "string", "format": "email"}}}
        }}});
        let s = to_json_schema(&root, &json!({"$ref": "#/components/schemas/Doc"}));
        assert_eq!(
            s["properties"]["owner"]["properties"]["email"]["type"],
            "string"
        );
        assert_eq!(s["properties"]["note"]["type"], json!(["string", "null"]));
        assert!(s["properties"]["parent"].is_object());

        let ex = sample(&root, &json!({"$ref": "#/components/schemas/Doc"}));
        assert_eq!(ex["id"], "{{ uuid() }}");
        assert_eq!(ex["owner"]["email"], "{{ fake('email') }}");
    }

    #[test]
    fn samples_prefer_examples_and_enums() {
        let root = json!({});
        assert_eq!(
            sample(&root, &json!({"type": "string", "example": "hola"})),
            json!("hola")
        );
        assert_eq!(
            sample(&root, &json!({"type": "string", "enum": ["a", "b"]})),
            json!("a")
        );
        assert_eq!(
            sample(
                &root,
                &json!({"type": "array", "items": {"type": "integer"}})
            ),
            json!([1])
        );
    }
}
