//! Representación neutral de una solicitud entrante, compartida por los
//! motores de coincidencia, respuestas y (en el futuro) workflows.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

#[derive(Debug, Clone, Default)]
pub struct IncomingRequest {
    pub method: String,
    /// Ruta completa recibida (con prefijo del servicio).
    pub path: String,
    /// Ruta relativa al servicio (sin prefijo). La llena el enrutador.
    pub route_path: String,
    pub query_string: String,
    pub query: Vec<(String, String)>,
    /// Headers con nombre en minúsculas, en orden de llegada.
    pub headers: Vec<(String, String)>,
    pub content_type: Option<String>,
    pub raw_body: Vec<u8>,
    /// Body interpretado: objeto/array para JSON, objeto para formularios y
    /// multipart, texto para XML/texto, `null` si está vacío o es binario.
    pub body: Value,
    /// Parámetros de ruta. Los llena el enrutador.
    pub params: BTreeMap<String, String>,
    pub remote_addr: Option<String>,
}

impl IncomingRequest {
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn query_param(&self, name: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// Body como texto UTF-8 (con reemplazo de bytes inválidos).
    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.raw_body).into_owned()
    }

    /// Mapa de headers (primer valor por nombre) para plantillas.
    pub fn headers_map(&self) -> Map<String, Value> {
        let mut m = Map::new();
        for (k, v) in &self.headers {
            m.entry(k.clone())
                .or_insert_with(|| Value::String(v.clone()));
        }
        m
    }

    /// Mapa de query (primer valor por nombre) para plantillas.
    pub fn query_map(&self) -> Map<String, Value> {
        let mut m = Map::new();
        for (k, v) in &self.query {
            m.entry(k.clone())
                .or_insert_with(|| Value::String(v.clone()));
        }
        m
    }

    /// Query con todos los valores por nombre.
    pub fn query_all(&self) -> Map<String, Value> {
        let mut m: Map<String, Value> = Map::new();
        for (k, v) in &self.query {
            let entry = m.entry(k.clone()).or_insert_with(|| Value::Array(vec![]));
            if let Value::Array(a) = entry {
                a.push(Value::String(v.clone()));
            }
        }
        m
    }
}

/// Busca un valor con ruta de puntos: `user.items.0.id`. Vacía = raíz.
pub fn lookup_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let path = path.trim();
    if path.is_empty() {
        return Some(root);
    }
    let mut cur = root;
    for part in path.split('.') {
        cur = match cur {
            Value::Object(m) => m.get(part)?,
            Value::Array(a) => a.get(part.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(cur)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lookup_nested() {
        let v = json!({"user": {"items": [{"id": 7}]}});
        assert_eq!(lookup_path(&v, "user.items.0.id"), Some(&json!(7)));
        assert_eq!(lookup_path(&v, "user.nope"), None);
        assert_eq!(lookup_path(&v, ""), Some(&v));
    }
}
