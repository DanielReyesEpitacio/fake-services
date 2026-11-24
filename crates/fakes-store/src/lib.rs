//! Persistencia de estado de ejecución en SQLite: historial HTTP y un
//! almacén clave-valor para estado de la aplicación. Más adelante aquí
//! vivirán las ejecuciones de workflows y los recursos simulados.
//!
//! La configuración declarativa NO vive aquí (ver `fakes-config`).

use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use base64::Engine;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize, Serializer};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

mod executions;
mod resources;
pub use executions::*;
pub use resources::*;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("base de datos: {0}")]
    Db(#[from] sqlx::Error),
    #[error("migración: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
}

pub type Result<T> = std::result::Result<T, StoreError>;

#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
}

/// Resultado de procesar una solicitud en el servidor de simulación.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Matched,
    NoScenario,
    NotFound,
    MethodNotAllowed,
    ValidationFailed,
    Unauthorized,
    ServiceStopped,
    Preflight,
    Error,
}

impl Outcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Outcome::Matched => "matched",
            Outcome::NoScenario => "no_scenario",
            Outcome::NotFound => "not_found",
            Outcome::MethodNotAllowed => "method_not_allowed",
            Outcome::ValidationFailed => "validation_failed",
            Outcome::Unauthorized => "unauthorized",
            Outcome::ServiceStopped => "service_stopped",
            Outcome::Preflight => "preflight",
            Outcome::Error => "error",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "matched" => Outcome::Matched,
            "no_scenario" => Outcome::NoScenario,
            "not_found" => Outcome::NotFound,
            "method_not_allowed" => Outcome::MethodNotAllowed,
            "validation_failed" => Outcome::ValidationFailed,
            "unauthorized" => Outcome::Unauthorized,
            "service_stopped" => Outcome::ServiceStopped,
            "preflight" => Outcome::Preflight,
            _ => Outcome::Error,
        }
    }

    pub fn is_error(&self) -> bool {
        !matches!(self, Outcome::Matched | Outcome::Preflight)
    }
}

/// Body capturado (posiblemente truncado).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, JsonSchema)]
pub struct CapturedBody {
    #[schemars(with = "String")]
    pub bytes: Vec<u8>,
    pub size: u64,
    pub truncated: bool,
}

impl CapturedBody {
    pub fn capture(bytes: &[u8], limit: u64) -> Self {
        let limit = usize::try_from(limit).unwrap_or(usize::MAX);
        Self {
            bytes: bytes[..bytes.len().min(limit)].to_vec(),
            size: bytes.len() as u64,
            truncated: bytes.len() > limit,
        }
    }
}

/// Se serializa como `{ size, truncated, encoding: "utf8"|"base64", content }`.
impl Serialize for CapturedBody {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("CapturedBody", 4)?;
        st.serialize_field("size", &self.size)?;
        st.serialize_field("truncated", &self.truncated)?;
        match std::str::from_utf8(&self.bytes) {
            Ok(text) => {
                st.serialize_field("encoding", "utf8")?;
                st.serialize_field("content", text)?;
            }
            Err(_) => {
                st.serialize_field("encoding", "base64")?;
                st.serialize_field(
                    "content",
                    &base64::engine::general_purpose::STANDARD.encode(&self.bytes),
                )?;
            }
        }
        st.end()
    }
}

/// Registro completo de una solicitud.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RequestRecord {
    pub id: String,
    /// Marca de tiempo en milisegundos Unix.
    pub ts_ms: i64,
    pub workspace_id: String,
    pub service_id: Option<String>,
    pub endpoint_id: Option<String>,
    pub scenario_id: Option<String>,
    pub method: String,
    pub path: String,
    pub query: String,
    pub remote_addr: Option<String>,
    pub request_headers: Vec<(String, String)>,
    pub request_body: CapturedBody,
    pub status: u16,
    pub response_headers: Vec<(String, String)>,
    pub response_body: CapturedBody,
    pub duration_ms: u64,
    pub outcome: Outcome,
    pub message: Option<String>,
}

/// Versión resumida para listados.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RequestSummary {
    pub id: String,
    pub ts_ms: i64,
    pub service_id: Option<String>,
    pub endpoint_id: Option<String>,
    pub scenario_id: Option<String>,
    pub method: String,
    pub path: String,
    pub query: String,
    pub status: u16,
    pub duration_ms: u64,
    pub outcome: Outcome,
    pub message: Option<String>,
}

impl From<&RequestRecord> for RequestSummary {
    fn from(r: &RequestRecord) -> Self {
        Self {
            id: r.id.clone(),
            ts_ms: r.ts_ms,
            service_id: r.service_id.clone(),
            endpoint_id: r.endpoint_id.clone(),
            scenario_id: r.scenario_id.clone(),
            method: r.method.clone(),
            path: r.path.clone(),
            query: r.query.clone(),
            status: r.status,
            duration_ms: r.duration_ms,
            outcome: r.outcome,
            message: r.message.clone(),
        }
    }
}

/// Filtros para listar historial.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequestFilter {
    #[serde(default)]
    pub service_id: Option<String>,
    #[serde(default)]
    pub endpoint_id: Option<String>,
    /// Solo solicitudes con error (sin coincidencia, validación, etc.).
    #[serde(default)]
    pub errors_only: bool,
    /// Texto contenido en la ruta.
    #[serde(default)]
    pub search: Option<String>,
    /// Paginación: solicitudes anteriores a esta marca (ms).
    #[serde(default)]
    pub before_ms: Option<i64>,
    #[serde(default)]
    pub limit: Option<u32>,
}

pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

impl Store {
    pub async fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(5))
            .foreign_keys(true);
        Self::with_options(opts, 4).await
    }

    /// Base de datos en memoria (pruebas).
    pub async fn in_memory() -> Result<Self> {
        let opts = SqliteConnectOptions::from_str("sqlite::memory:")?;
        Self::with_options(opts, 1).await
    }

    async fn with_options(opts: SqliteConnectOptions, max: u32) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(max)
            .connect_with(opts)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    // -- historial ---------------------------------------------------------

    pub async fn insert_request(&self, r: &RequestRecord) -> Result<()> {
        sqlx::query(
            "INSERT INTO requests (id, ts_ms, workspace_id, service_id, endpoint_id, scenario_id,
                method, path, query, remote_addr, request_headers, request_body, request_body_size,
                request_body_truncated, status, response_headers, response_body, response_body_size,
                response_body_truncated, duration_ms, outcome, message)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&r.id)
        .bind(r.ts_ms)
        .bind(&r.workspace_id)
        .bind(&r.service_id)
        .bind(&r.endpoint_id)
        .bind(&r.scenario_id)
        .bind(&r.method)
        .bind(&r.path)
        .bind(&r.query)
        .bind(&r.remote_addr)
        .bind(serde_json::to_string(&r.request_headers).unwrap_or_default())
        .bind(&r.request_body.bytes)
        .bind(r.request_body.size as i64)
        .bind(r.request_body.truncated)
        .bind(i64::from(r.status))
        .bind(serde_json::to_string(&r.response_headers).unwrap_or_default())
        .bind(&r.response_body.bytes)
        .bind(r.response_body.size as i64)
        .bind(r.response_body.truncated)
        .bind(r.duration_ms as i64)
        .bind(r.outcome.as_str())
        .bind(&r.message)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_requests(
        &self,
        workspace_id: &str,
        f: &RequestFilter,
    ) -> Result<Vec<RequestSummary>> {
        let mut q: QueryBuilder<Sqlite> = QueryBuilder::new(
            "SELECT id, ts_ms, service_id, endpoint_id, scenario_id, method, path, query, status,
                    duration_ms, outcome, message
             FROM requests WHERE workspace_id = ",
        );
        q.push_bind(workspace_id);
        if let Some(s) = &f.service_id {
            q.push(" AND service_id = ").push_bind(s);
        }
        if let Some(e) = &f.endpoint_id {
            q.push(" AND endpoint_id = ").push_bind(e);
        }
        if f.errors_only {
            q.push(" AND outcome NOT IN ('matched', 'preflight')");
        }
        if let Some(s) = f.search.as_deref().filter(|s| !s.is_empty()) {
            q.push(" AND path LIKE ")
                .push_bind(format!("%{}%", escape_like(s)));
            q.push(" ESCAPE '\\'");
        }
        if let Some(b) = f.before_ms {
            q.push(" AND ts_ms < ").push_bind(b);
        }
        q.push(" ORDER BY ts_ms DESC, id DESC LIMIT ")
            .push_bind(i64::from(f.limit.unwrap_or(100).clamp(1, 1000)));
        let rows = q.build().fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|row| RequestSummary {
                id: row.get("id"),
                ts_ms: row.get("ts_ms"),
                service_id: row.get("service_id"),
                endpoint_id: row.get("endpoint_id"),
                scenario_id: row.get("scenario_id"),
                method: row.get("method"),
                path: row.get("path"),
                query: row.get("query"),
                status: row.get::<i64, _>("status") as u16,
                duration_ms: row.get::<i64, _>("duration_ms") as u64,
                outcome: Outcome::parse(row.get::<&str, _>("outcome")),
                message: row.get("message"),
            })
            .collect())
    }

    pub async fn get_request(&self, workspace_id: &str, id: &str) -> Result<Option<RequestRecord>> {
        let row = sqlx::query("SELECT * FROM requests WHERE workspace_id = ? AND id = ?")
            .bind(workspace_id)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|row| {
            let headers = |col: &str| -> Vec<(String, String)> {
                serde_json::from_str(row.get::<&str, _>(col)).unwrap_or_default()
            };
            RequestRecord {
                id: row.get("id"),
                ts_ms: row.get("ts_ms"),
                workspace_id: row.get("workspace_id"),
                service_id: row.get("service_id"),
                endpoint_id: row.get("endpoint_id"),
                scenario_id: row.get("scenario_id"),
                method: row.get("method"),
                path: row.get("path"),
                query: row.get("query"),
                remote_addr: row.get("remote_addr"),
                request_headers: headers("request_headers"),
                request_body: CapturedBody {
                    bytes: row.get("request_body"),
                    size: row.get::<i64, _>("request_body_size") as u64,
                    truncated: row.get("request_body_truncated"),
                },
                status: row.get::<i64, _>("status") as u16,
                response_headers: headers("response_headers"),
                response_body: CapturedBody {
                    bytes: row.get("response_body"),
                    size: row.get::<i64, _>("response_body_size") as u64,
                    truncated: row.get("response_body_truncated"),
                },
                duration_ms: row.get::<i64, _>("duration_ms") as u64,
                outcome: Outcome::parse(row.get::<&str, _>("outcome")),
                message: row.get("message"),
            }
        }))
    }

    /// Borra el historial del espacio (o solo de un servicio). Devuelve filas borradas.
    pub async fn clear_requests(
        &self,
        workspace_id: &str,
        service_id: Option<&str>,
    ) -> Result<u64> {
        let res = match service_id {
            Some(s) => {
                sqlx::query("DELETE FROM requests WHERE workspace_id = ? AND service_id = ?")
                    .bind(workspace_id)
                    .bind(s)
                    .execute(&self.pool)
                    .await?
            }
            None => {
                sqlx::query("DELETE FROM requests WHERE workspace_id = ?")
                    .bind(workspace_id)
                    .execute(&self.pool)
                    .await?
            }
        };
        Ok(res.rows_affected())
    }

    /// Conserva solo las `keep` solicitudes más recientes del espacio.
    pub async fn prune_requests(&self, workspace_id: &str, keep: u32) -> Result<u64> {
        let res = sqlx::query(
            "DELETE FROM requests WHERE workspace_id = ?1 AND ts_ms < (
                SELECT ts_ms FROM requests WHERE workspace_id = ?1
                ORDER BY ts_ms DESC LIMIT 1 OFFSET ?2)",
        )
        .bind(workspace_id)
        .bind(i64::from(keep.max(1)) - 1)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected())
    }

    pub async fn count_requests(&self, workspace_id: &str) -> Result<i64> {
        Ok(
            sqlx::query_scalar("SELECT COUNT(*) FROM requests WHERE workspace_id = ?")
                .bind(workspace_id)
                .fetch_one(&self.pool)
                .await?,
        )
    }

    // -- clave/valor -------------------------------------------------------

    pub async fn kv_get<T: serde::de::DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let v: Option<String> = sqlx::query_scalar("SELECT value FROM kv WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(v.and_then(|s| serde_json::from_str(&s).ok()))
    }

    pub async fn kv_set<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        sqlx::query(
            "INSERT INTO kv (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(serde_json::to_string(value).unwrap_or_else(|_| "null".into()))
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(ts: i64, service: &str, outcome: Outcome, path: &str) -> RequestRecord {
        RequestRecord {
            id: new_id(),
            ts_ms: ts,
            workspace_id: "ws".into(),
            service_id: Some(service.into()),
            endpoint_id: Some("e".into()),
            scenario_id: None,
            method: "GET".into(),
            path: path.into(),
            query: String::new(),
            remote_addr: Some("127.0.0.1".into()),
            request_headers: vec![("a".into(), "b".into())],
            request_body: CapturedBody::capture(b"hola mundo", 4),
            status: 200,
            response_headers: vec![],
            response_body: CapturedBody::capture(&[0xff, 0x00], 100),
            duration_ms: 3,
            outcome,
            message: None,
        }
    }

    #[tokio::test]
    async fn insert_list_get_prune() {
        let store = Store::in_memory().await.unwrap();
        for i in 0..10 {
            let svc = if i % 2 == 0 { "a" } else { "b" };
            let outcome = if i == 9 {
                Outcome::NotFound
            } else {
                Outcome::Matched
            };
            store
                .insert_request(&record(i, svc, outcome, &format!("/p_{i}")))
                .await
                .unwrap();
        }
        let all = store
            .list_requests("ws", &RequestFilter::default())
            .await
            .unwrap();
        assert_eq!(all.len(), 10);
        assert_eq!(all[0].ts_ms, 9);

        let f = RequestFilter {
            service_id: Some("a".into()),
            ..Default::default()
        };
        assert_eq!(store.list_requests("ws", &f).await.unwrap().len(), 5);

        let f = RequestFilter {
            errors_only: true,
            ..Default::default()
        };
        assert_eq!(store.list_requests("ws", &f).await.unwrap().len(), 1);

        // `_` es literal, no comodín
        let f = RequestFilter {
            search: Some("p_3".into()),
            ..Default::default()
        };
        assert_eq!(store.list_requests("ws", &f).await.unwrap().len(), 1);

        let f = RequestFilter {
            before_ms: Some(5),
            limit: Some(2),
            ..Default::default()
        };
        let page = store.list_requests("ws", &f).await.unwrap();
        assert_eq!(page.iter().map(|r| r.ts_ms).collect::<Vec<_>>(), vec![4, 3]);

        let full = store.get_request("ws", &all[0].id).await.unwrap().unwrap();
        assert_eq!(full.request_body.bytes, b"hola");
        assert!(full.request_body.truncated);
        assert_eq!(full.request_body.size, 10);
        assert_eq!(
            full.request_headers,
            vec![("a".to_string(), "b".to_string())]
        );
        let json = serde_json::to_value(&full).unwrap();
        assert_eq!(json["responseBody"]["encoding"], "base64");
        assert_eq!(json["requestBody"]["content"], "hola");

        assert_eq!(store.prune_requests("ws", 3).await.unwrap(), 7);
        assert_eq!(store.count_requests("ws").await.unwrap(), 3);
        assert_eq!(store.clear_requests("ws", None).await.unwrap(), 3);
    }

    #[tokio::test]
    async fn kv_roundtrip() {
        let store = Store::in_memory().await.unwrap();
        assert_eq!(store.kv_get::<Vec<String>>("x").await.unwrap(), None);
        store.kv_set("x", &vec!["a".to_string()]).await.unwrap();
        store.kv_set("x", &vec!["b".to_string()]).await.unwrap();
        assert_eq!(
            store.kv_get::<Vec<String>>("x").await.unwrap(),
            Some(vec!["b".to_string()])
        );
    }

    #[tokio::test]
    async fn opens_file_database() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("sub/fakes.db")).await.unwrap();
        store.kv_set("k", &1).await.unwrap();
        drop(store);
        let store = Store::open(&tmp.path().join("sub/fakes.db")).await.unwrap();
        assert_eq!(store.kv_get::<i32>("k").await.unwrap(), Some(1));
    }
}
