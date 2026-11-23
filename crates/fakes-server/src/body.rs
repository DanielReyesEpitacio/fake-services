//! Interpretación del body según su Content-Type.

use bytes::Bytes;
use serde_json::{Map, Value};

/// Convierte el body en un valor utilizable por reglas y plantillas.
pub async fn parse_body(content_type: Option<&str>, raw: &Bytes) -> Value {
    if raw.is_empty() {
        return Value::Null;
    }
    let ct = content_type.unwrap_or("").to_ascii_lowercase();
    if ct.contains("json") {
        return serde_json::from_slice(raw).unwrap_or(Value::Null);
    }
    if ct.starts_with("application/x-www-form-urlencoded") {
        return form_to_value(form_urlencoded::parse(raw).into_owned());
    }
    if ct.starts_with("multipart/form-data") {
        if let Some(v) = parse_multipart(content_type.unwrap_or(""), raw.clone()).await {
            return v;
        }
        return Value::Null;
    }
    match std::str::from_utf8(raw) {
        Ok(text) => {
            // Sin Content-Type: intentar JSON por comodidad.
            if ct.is_empty()
                && let Ok(v) = serde_json::from_str::<Value>(text)
                && (v.is_object() || v.is_array())
            {
                return v;
            }
            Value::String(text.to_string())
        }
        Err(_) => Value::Null,
    }
}

/// `a=1&a=2&b=3` → `{"a": ["1","2"], "b": "3"}`.
fn form_to_value(pairs: impl IntoIterator<Item = (String, String)>) -> Value {
    let mut m = Map::new();
    for (k, v) in pairs {
        insert_multi(&mut m, k, Value::String(v));
    }
    Value::Object(m)
}

fn insert_multi(m: &mut Map<String, Value>, k: String, v: Value) {
    match m.get_mut(&k) {
        None => {
            m.insert(k, v);
        }
        Some(Value::Array(a)) => a.push(v),
        Some(existing) => {
            let prev = existing.take();
            *existing = Value::Array(vec![prev, v]);
        }
    }
}

/// Campos de texto como texto; archivos como `{filename, contentType, size}`.
async fn parse_multipart(content_type: &str, raw: Bytes) -> Option<Value> {
    let boundary = multer::parse_boundary(content_type).ok()?;
    let stream = futures_util::stream::once(async move { Ok::<_, std::io::Error>(raw) });
    let mut mp = multer::Multipart::new(stream, boundary);
    let mut m = Map::new();
    while let Ok(Some(field)) = mp.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        let filename = field.file_name().map(str::to_string);
        let ct = field.content_type().map(|c| c.to_string());
        let data = field.bytes().await.ok()?;
        let value = match filename {
            Some(filename) => serde_json::json!({
                "filename": filename,
                "contentType": ct,
                "size": data.len(),
            }),
            None => Value::String(String::from_utf8_lossy(&data).into_owned()),
        };
        insert_multi(&mut m, name, value);
    }
    Some(Value::Object(m))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn parses_common_types() {
        let b = |s: &str| Bytes::from(s.to_string());
        assert_eq!(
            parse_body(Some("application/json"), &b(r#"{"a":1}"#)).await,
            json!({"a": 1})
        );
        assert_eq!(
            parse_body(Some("application/json"), &b("{roto")).await,
            Value::Null
        );
        assert_eq!(
            parse_body(
                Some("application/x-www-form-urlencoded"),
                &b("a=1&a=2&b=x+y")
            )
            .await,
            json!({"a": ["1", "2"], "b": "x y"})
        );
        assert_eq!(
            parse_body(Some("text/xml"), &b("<a/>")).await,
            json!("<a/>")
        );
        assert_eq!(parse_body(None, &b(r#"[1]"#)).await, json!([1]));
        assert_eq!(
            parse_body(None, &Bytes::from_static(&[0xff, 0xfe])).await,
            Value::Null
        );
    }

    #[tokio::test]
    async fn parses_multipart() {
        let body = "--X\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\nContrato\r\n\
--X\r\nContent-Disposition: form-data; name=\"file\"; filename=\"c.pdf\"\r\nContent-Type: application/pdf\r\n\r\n%PDF-1\r\n\
--X--\r\n";
        let v = parse_body(Some("multipart/form-data; boundary=X"), &Bytes::from(body)).await;
        assert_eq!(
            v,
            json!({"title": "Contrato", "file": {"filename": "c.pdf", "contentType": "application/pdf", "size": 6}})
        );
    }
}
