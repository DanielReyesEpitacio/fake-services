//! JSON Merge Patch (RFC 7396) sobre tipos del dominio.

use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::AppError;

pub fn merge(target: &mut Value, patch: Value) {
    match patch {
        Value::Object(p) => {
            if !target.is_object() {
                *target = Value::Object(Default::default());
            }
            let t = target.as_object_mut().expect("objeto");
            for (k, v) in p {
                if v.is_null() {
                    t.remove(&k);
                } else {
                    merge(t.entry(k).or_insert(Value::Null), v);
                }
            }
        }
        other => *target = other,
    }
}

pub fn apply<T: Serialize + DeserializeOwned>(current: &T, patch: Value) -> Result<T, AppError> {
    if !patch.is_object() {
        return Err(AppError::BadRequest(
            "el parche debe ser un objeto JSON".into(),
        ));
    }
    let mut v = serde_json::to_value(current)
        .map_err(|e| AppError::BadRequest(format!("no se pudo serializar: {e}")))?;
    merge(&mut v, patch);
    serde_json::from_value(v).map_err(|e| AppError::BadRequest(format!("parche inválido: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rfc7396() {
        let mut t = json!({"a": "b", "c": {"d": "e", "f": "g"}});
        merge(&mut t, json!({"a": "z", "c": {"f": null}}));
        assert_eq!(t, json!({"a": "z", "c": {"d": "e"}}));
        let mut t = json!({"a": [1, 2]});
        merge(&mut t, json!({"a": [3]}));
        assert_eq!(t, json!({"a": [3]}));
    }
}
