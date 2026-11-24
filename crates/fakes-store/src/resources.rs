//! Recursos simulados persistentes: colecciones de documentos JSON por
//! servicio (objetos de S3, documentos firmados, cuentas, transacciones...).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::Row;

use crate::{Result, Store};

/// Registros máximos por colección que devuelve `list`.
pub const LIST_LIMIT: u32 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ResourceOperation {
    Create,
    Upsert,
    Update,
    Get,
    Delete,
    List,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResourceOutcome {
    /// Registro resultante (o borrado, en `delete`).
    Record(Value),
    List(Vec<Value>),
    /// El registro no existe (get/update/delete).
    Missing,
}

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CollectionInfo {
    pub collection: String,
    pub count: i64,
    pub updated_ms: i64,
}

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResourceRecord {
    pub id: String,
    pub data: Value,
    pub created_ms: i64,
    pub updated_ms: i64,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Combina `patch` sobre `base` (nivel superior; `null` borra la clave).
fn merge(base: &mut Value, patch: Value) {
    if !base.is_object() {
        *base = Value::Object(Map::new());
    }
    match patch {
        Value::Object(p) => {
            let b = base.as_object_mut().expect("objeto");
            for (k, v) in p {
                if v.is_null() {
                    b.remove(&k);
                } else {
                    b.insert(k, v);
                }
            }
        }
        other => *base = other,
    }
}

fn with_id(mut data: Value, id: &str) -> Value {
    if !data.is_object() {
        data = serde_json::json!({ "value": data });
    }
    data["id"] = Value::String(id.to_string());
    data
}

impl Store {
    pub async fn resource_get(
        &self,
        ws: &str,
        svc: &str,
        col: &str,
        id: &str,
    ) -> Result<Option<ResourceRecord>> {
        let row = sqlx::query(
            "SELECT id, data, created_ms, updated_ms FROM resources
             WHERE workspace_id = ? AND service_id = ? AND collection = ? AND id = ?",
        )
        .bind(ws)
        .bind(svc)
        .bind(col)
        .bind(id)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|r| ResourceRecord {
            id: r.get("id"),
            data: serde_json::from_str(r.get::<&str, _>("data")).unwrap_or(Value::Null),
            created_ms: r.get("created_ms"),
            updated_ms: r.get("updated_ms"),
        }))
    }

    pub async fn resource_put(
        &self,
        ws: &str,
        svc: &str,
        col: &str,
        id: &str,
        data: &Value,
    ) -> Result<()> {
        let now = now_ms();
        sqlx::query(
            "INSERT INTO resources (workspace_id, service_id, collection, id, data, created_ms, updated_ms)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(workspace_id, service_id, collection, id)
             DO UPDATE SET data = excluded.data, updated_ms = excluded.updated_ms",
        )
        .bind(ws)
        .bind(svc)
        .bind(col)
        .bind(id)
        .bind(serde_json::to_string(data).unwrap_or_default())
        .bind(now)
        .bind(now)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn resource_delete(&self, ws: &str, svc: &str, col: &str, id: &str) -> Result<bool> {
        let r = sqlx::query(
            "DELETE FROM resources WHERE workspace_id = ? AND service_id = ? AND collection = ? AND id = ?",
        )
        .bind(ws)
        .bind(svc)
        .bind(col)
        .bind(id)
        .execute(self.pool())
        .await?;
        Ok(r.rows_affected() > 0)
    }

    pub async fn resource_list(
        &self,
        ws: &str,
        svc: &str,
        col: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ResourceRecord>> {
        let rows = sqlx::query(
            "SELECT id, data, created_ms, updated_ms FROM resources
             WHERE workspace_id = ? AND service_id = ? AND collection = ?
             ORDER BY created_ms, id LIMIT ? OFFSET ?",
        )
        .bind(ws)
        .bind(svc)
        .bind(col)
        .bind(i64::from(limit.clamp(1, LIST_LIMIT)))
        .bind(i64::from(offset))
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| ResourceRecord {
                id: r.get("id"),
                data: serde_json::from_str(r.get::<&str, _>("data")).unwrap_or(Value::Null),
                created_ms: r.get("created_ms"),
                updated_ms: r.get("updated_ms"),
            })
            .collect())
    }

    pub async fn resource_collections(&self, ws: &str, svc: &str) -> Result<Vec<CollectionInfo>> {
        let rows = sqlx::query(
            "SELECT collection, COUNT(*) AS n, MAX(updated_ms) AS u FROM resources
             WHERE workspace_id = ? AND service_id = ? GROUP BY collection ORDER BY collection",
        )
        .bind(ws)
        .bind(svc)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| CollectionInfo {
                collection: r.get("collection"),
                count: r.get("n"),
                updated_ms: r.get("u"),
            })
            .collect())
    }

    /// Borra una colección (o todas las del servicio). Devuelve registros borrados.
    pub async fn resource_clear(&self, ws: &str, svc: &str, col: Option<&str>) -> Result<u64> {
        let r = match col {
            Some(c) => {
                sqlx::query("DELETE FROM resources WHERE workspace_id = ? AND service_id = ? AND collection = ?")
                    .bind(ws)
                    .bind(svc)
                    .bind(c)
                    .execute(self.pool())
                    .await?
            }
            None => {
                sqlx::query("DELETE FROM resources WHERE workspace_id = ? AND service_id = ?")
                    .bind(ws)
                    .bind(svc)
                    .execute(self.pool())
                    .await?
            }
        };
        Ok(r.rows_affected())
    }

    /// Aplica una operación con la semántica de los escenarios y workflows.
    pub async fn resource_apply(
        &self,
        ws: &str,
        svc: &str,
        col: &str,
        op: ResourceOperation,
        id: Option<&str>,
        data: Option<Value>,
    ) -> Result<ResourceOutcome> {
        let id = id.map(str::trim).filter(|s| !s.is_empty());
        match op {
            ResourceOperation::List => Ok(ResourceOutcome::List(
                self.resource_list(ws, svc, col, LIST_LIMIT, 0)
                    .await?
                    .into_iter()
                    .map(|r| with_id(r.data, &r.id))
                    .collect(),
            )),
            ResourceOperation::Create => {
                let id = id
                    .map(str::to_string)
                    .or_else(|| {
                        data.as_ref()
                            .and_then(|d| d.get("id"))
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
                let record = with_id(data.unwrap_or(Value::Object(Map::new())), &id);
                self.resource_put(ws, svc, col, &id, &record).await?;
                Ok(ResourceOutcome::Record(record))
            }
            ResourceOperation::Get => {
                let Some(id) = id else {
                    return Ok(ResourceOutcome::Missing);
                };
                Ok(match self.resource_get(ws, svc, col, id).await? {
                    Some(r) => ResourceOutcome::Record(with_id(r.data, &r.id)),
                    None => ResourceOutcome::Missing,
                })
            }
            ResourceOperation::Update | ResourceOperation::Upsert => {
                let Some(id) = id else {
                    return Ok(ResourceOutcome::Missing);
                };
                let existing = self.resource_get(ws, svc, col, id).await?;
                if existing.is_none() && op == ResourceOperation::Update {
                    return Ok(ResourceOutcome::Missing);
                }
                let mut record = existing
                    .map(|r| r.data)
                    .unwrap_or(Value::Object(Map::new()));
                if let Some(d) = data {
                    merge(&mut record, d);
                }
                let record = with_id(record, id);
                self.resource_put(ws, svc, col, id, &record).await?;
                Ok(ResourceOutcome::Record(record))
            }
            ResourceOperation::Delete => {
                let Some(id) = id else {
                    return Ok(ResourceOutcome::Missing);
                };
                match self.resource_get(ws, svc, col, id).await? {
                    Some(r) => {
                        self.resource_delete(ws, svc, col, id).await?;
                        Ok(ResourceOutcome::Record(with_id(r.data, &r.id)))
                    }
                    None => Ok(ResourceOutcome::Missing),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn resource_semantics() {
        let s = Store::in_memory().await.unwrap();
        let ap = |op, id: Option<&'static str>, data: Option<Value>| {
            let s = s.clone();
            async move {
                s.resource_apply("ws", "mifiel", "documents", op, id, data)
                    .await
                    .unwrap()
            }
        };
        // create con id generado
        let ResourceOutcome::Record(r) = ap(
            ResourceOperation::Create,
            None,
            Some(json!({"status": "pending"})),
        )
        .await
        else {
            panic!()
        };
        let gen_id = r["id"].as_str().unwrap().to_string();
        assert_eq!(gen_id.len(), 32);

        let ResourceOutcome::Record(r) = ap(
            ResourceOperation::Create,
            Some("d1"),
            Some(json!({"status": "pending", "name": "x"})),
        )
        .await
        else {
            panic!()
        };
        assert_eq!(r, json!({"status": "pending", "name": "x", "id": "d1"}));
        // update combina
        let ResourceOutcome::Record(r) = ap(
            ResourceOperation::Update,
            Some("d1"),
            Some(json!({"status": "signed", "name": null})),
        )
        .await
        else {
            panic!()
        };
        assert_eq!(r, json!({"status": "signed", "id": "d1"}));
        assert_eq!(
            ap(ResourceOperation::Update, Some("nope"), Some(json!({}))).await,
            ResourceOutcome::Missing
        );
        // upsert crea
        assert!(matches!(
            ap(ResourceOperation::Upsert, Some("d2"), Some(json!({"a": 1}))).await,
            ResourceOutcome::Record(_)
        ));
        // list
        let ResourceOutcome::List(l) = ap(ResourceOperation::List, None, None).await else {
            panic!()
        };
        assert_eq!(l.len(), 3);
        // get / delete
        assert!(matches!(
            ap(ResourceOperation::Get, Some("d1"), None).await,
            ResourceOutcome::Record(_)
        ));
        assert!(matches!(
            ap(ResourceOperation::Delete, Some("d1"), None).await,
            ResourceOutcome::Record(_)
        ));
        assert_eq!(
            ap(ResourceOperation::Get, Some("d1"), None).await,
            ResourceOutcome::Missing
        );
        assert_eq!(
            ap(ResourceOperation::Delete, Some("d1"), None).await,
            ResourceOutcome::Missing
        );

        let cols = s.resource_collections("ws", "mifiel").await.unwrap();
        assert_eq!(
            (cols[0].collection.as_str(), cols[0].count),
            ("documents", 2)
        );
        // aislamiento por servicio
        assert!(
            s.resource_collections("ws", "otro")
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(s.resource_clear("ws", "mifiel", None).await.unwrap(), 2);
    }
}
