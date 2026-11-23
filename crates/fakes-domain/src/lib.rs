//! Dominio de Fake Services.
//!
//! Contiene únicamente tipos declarativos y reglas puras. Ningún otro
//! componente (HTTP, persistencia, UI, MCP) define su propio modelo: todos
//! consumen estos tipos.

pub mod exchange;
pub mod model;
pub mod path;
pub mod validate;
pub mod workflow;

pub use exchange::{IncomingRequest, lookup_path};
pub use model::*;
pub use path::{PathTemplate, Segment, normalize_prefix};
pub use validate::Issue;
pub use workflow::{
    Action, Condition, HttpAction, StartWhen, Step, StepBody, Workflow, WorkflowLink,
};

/// Convierte un texto en un identificador válido: `"AWS S3"` → `"aws-s3"`.
pub fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in input.chars() {
        let c = fold_accent(c);
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out.truncate(48);
    while out.ends_with('-') {
        out.pop();
    }
    out
}

fn fold_accent(c: char) -> char {
    match c {
        'á' | 'à' | 'ä' | 'â' | 'Á' | 'À' | 'Ä' | 'Â' => 'a',
        'é' | 'è' | 'ë' | 'ê' | 'É' | 'È' | 'Ë' | 'Ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' | 'Í' | 'Ì' | 'Ï' | 'Î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' | 'Ó' | 'Ò' | 'Ö' | 'Ô' => 'o',
        'ú' | 'ù' | 'ü' | 'û' | 'Ú' | 'Ù' | 'Ü' | 'Û' => 'u',
        'ñ' | 'Ñ' => 'n',
        other => other,
    }
}

/// Genera un identificador único a partir de una base y los existentes.
pub fn unique_id<'a>(base: &str, existing: impl Iterator<Item = &'a str> + Clone) -> String {
    let base = {
        let s = slugify(base);
        if s.is_empty() { "item".to_string() } else { s }
    };
    if !existing.clone().any(|e| e == base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|cand| !existing.clone().any(|e| e == cand))
        .expect("siempre hay un sufijo libre")
}

/// Identificador sugerido para un endpoint: `POST /documents/{id}/sign` → `post-documents-id-sign`.
pub fn endpoint_id_hint(method: HttpMethod, path: &str) -> String {
    let m = match method {
        HttpMethod::Any => "any",
        other => other.as_str(),
    };
    slugify(&format!("{m} {path}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_handles_accents_and_symbols() {
        assert_eq!(slugify("AWS S3"), "aws-s3");
        assert_eq!(slugify("  Firma rechazada!! "), "firma-rechazada");
        assert_eq!(slugify("Configuración Ñandú"), "configuracion-nandu");
    }

    #[test]
    fn unique_id_appends_suffix() {
        let existing = ["mifiel", "mifiel-2"];
        assert_eq!(unique_id("MiFiel", existing.iter().copied()), "mifiel-3");
        assert_eq!(unique_id("Belvo", existing.iter().copied()), "belvo");
    }

    #[test]
    fn endpoint_hint() {
        assert_eq!(
            endpoint_id_hint(HttpMethod::Post, "/documents/{id}/sign"),
            "post-documents-id-sign"
        );
    }
}
