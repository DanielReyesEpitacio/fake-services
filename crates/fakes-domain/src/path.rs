//! Plantillas de ruta: `/documents/{id}/sign`, `/files/{*key}`.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Literal(String),
    Param(String),
    /// Comodín final que captura el resto de la ruta (puede contener `/`).
    CatchAll(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathTemplate {
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathError(pub String);

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PathError {}

fn valid_param_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

impl PathTemplate {
    pub fn parse(path: &str) -> Result<Self, PathError> {
        if !path.starts_with('/') {
            return Err(PathError("la ruta debe comenzar con '/'".into()));
        }
        if path.contains(['?', '#']) {
            return Err(PathError(
                "la ruta no puede incluir query string ni fragmento".into(),
            ));
        }
        let mut segments = Vec::new();
        let parts: Vec<&str> = split_path(path).collect();
        let mut seen = std::collections::HashSet::new();
        for (i, part) in parts.iter().enumerate() {
            if let Some(inner) = part.strip_prefix('{').and_then(|p| p.strip_suffix('}')) {
                let (catch_all, name) = match inner.strip_prefix('*') {
                    Some(n) => (true, n),
                    None => (false, inner),
                };
                if !valid_param_name(name) {
                    return Err(PathError(format!(
                        "nombre de parámetro inválido: '{inner}'"
                    )));
                }
                if !seen.insert(name.to_string()) {
                    return Err(PathError(format!("parámetro repetido: '{name}'")));
                }
                if catch_all {
                    if i != parts.len() - 1 {
                        return Err(PathError(
                            "el comodín {*nombre} solo puede ir al final".into(),
                        ));
                    }
                    segments.push(Segment::CatchAll(name.to_string()));
                } else {
                    segments.push(Segment::Param(name.to_string()));
                }
            } else if part.contains(['{', '}']) {
                return Err(PathError(format!(
                    "segmento inválido '{part}': los parámetros ocupan el segmento completo"
                )));
            } else {
                segments.push(Segment::Literal(part.to_string()));
            }
        }
        Ok(Self { segments })
    }

    /// Nombres de los parámetros en orden.
    pub fn param_names(&self) -> Vec<&str> {
        self.segments
            .iter()
            .filter_map(|s| match s {
                Segment::Param(n) | Segment::CatchAll(n) => Some(n.as_str()),
                Segment::Literal(_) => None,
            })
            .collect()
    }

    /// Prioridad para ordenar coincidencias: más literales primero,
    /// luego más segmentos, y los comodines al final.
    pub fn specificity(&self) -> (u8, usize, usize) {
        let literals = self
            .segments
            .iter()
            .filter(|s| matches!(s, Segment::Literal(_)))
            .count();
        let has_catch_all = self
            .segments
            .iter()
            .any(|s| matches!(s, Segment::CatchAll(_)));
        (u8::from(!has_catch_all), literals, self.segments.len())
    }

    /// Intenta coincidir con una ruta concreta (ya sin prefijo).
    /// Devuelve los parámetros capturados (sin decodificar).
    pub fn match_path(&self, path: &str) -> Option<Vec<(String, String)>> {
        let parts: Vec<&str> = split_path(path).collect();
        let mut params = Vec::new();
        let mut idx = 0;
        for seg in &self.segments {
            match seg {
                Segment::Literal(lit) => {
                    let part = parts.get(idx)?;
                    if part != lit {
                        return None;
                    }
                    idx += 1;
                }
                Segment::Param(name) => {
                    let part = parts.get(idx)?;
                    params.push((name.clone(), (*part).to_string()));
                    idx += 1;
                }
                Segment::CatchAll(name) => {
                    let rest = parts[idx.min(parts.len())..].join("/");
                    params.push((name.clone(), rest));
                    idx = parts.len();
                }
            }
        }
        (idx == parts.len()).then_some(params)
    }
}

/// Divide una ruta en segmentos no vacíos (ignora `/` repetidas y la final).
pub fn split_path(path: &str) -> impl Iterator<Item = &str> {
    path.split('/').filter(|p| !p.is_empty())
}

/// Normaliza un prefijo: `mifiel/` → `/mifiel`; `/` → `` (raíz).
pub fn normalize_prefix(prefix: &str) -> String {
    let parts: Vec<&str> = split_path(prefix.trim()).collect();
    if parts.is_empty() {
        String::new()
    } else {
        format!("/{}", parts.join("/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_matches_params() {
        let t = PathTemplate::parse("/documents/{id}/sign").unwrap();
        assert_eq!(t.param_names(), vec!["id"]);
        let p = t.match_path("/documents/doc_1/sign").unwrap();
        assert_eq!(p, vec![("id".into(), "doc_1".into())]);
        assert!(t.match_path("/documents/doc_1").is_none());
        assert!(t.match_path("/documents/doc_1/sign/x").is_none());
        assert!(t.match_path("/documents/doc_1/sign/").is_some());
    }

    #[test]
    fn catch_all_captures_rest() {
        let t = PathTemplate::parse("/bucket/{*key}").unwrap();
        let p = t.match_path("/bucket/a/b/c.txt").unwrap();
        assert_eq!(p, vec![("key".into(), "a/b/c.txt".into())]);
        let p = t.match_path("/bucket").unwrap();
        assert_eq!(p, vec![("key".into(), "".into())]);
    }

    #[test]
    fn root_path() {
        let t = PathTemplate::parse("/").unwrap();
        assert!(t.match_path("/").is_some());
        assert!(t.match_path("").is_some());
        assert!(t.match_path("/x").is_none());
    }

    #[test]
    fn rejects_invalid() {
        assert!(PathTemplate::parse("documents").is_err());
        assert!(PathTemplate::parse("/a/{*rest}/b").is_err());
        assert!(PathTemplate::parse("/a/{id}/{id}").is_err());
        assert!(PathTemplate::parse("/a/doc-{id}").is_err());
        assert!(PathTemplate::parse("/a?x=1").is_err());
    }

    #[test]
    fn specificity_orders_literals_first() {
        let a = PathTemplate::parse("/documents/latest").unwrap();
        let b = PathTemplate::parse("/documents/{id}").unwrap();
        let c = PathTemplate::parse("/{*all}").unwrap();
        assert!(a.specificity() > b.specificity());
        assert!(b.specificity() > c.specificity());
    }

    #[test]
    fn normalizes_prefix() {
        assert_eq!(normalize_prefix("mifiel/"), "/mifiel");
        assert_eq!(normalize_prefix("//aws//s3"), "/aws/s3");
        assert_eq!(normalize_prefix("/"), "");
    }
}
