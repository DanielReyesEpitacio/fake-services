//! Persistencia de ejecuciones de workflows: estado durable para retomarlas
//! tras reiniciar la aplicación, y su línea de tiempo.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::sqlite::SqliteRow;
use sqlx::{QueryBuilder, Row, Sqlite};

use crate::{Result, Store};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Pending,
    Running,
    Waiting,
    Completed,
    Failed,
    Cancelled,
}

impl ExecutionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionStatus::Pending => "pending",
            ExecutionStatus::Running => "running",
            ExecutionStatus::Waiting => "waiting",
            ExecutionStatus::Completed => "completed",
            ExecutionStatus::Failed => "failed",
            ExecutionStatus::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "pending" => ExecutionStatus::Pending,
            "running" => ExecutionStatus::Running,
            "waiting" => ExecutionStatus::Waiting,
            "completed" => ExecutionStatus::Completed,
            "cancelled" => ExecutionStatus::Cancelled,
            _ => ExecutionStatus::Failed,
        }
    }

    pub fn is_finished(&self) -> bool {
        matches!(
            self,
            ExecutionStatus::Completed | ExecutionStatus::Failed | ExecutionStatus::Cancelled
        )
    }
}

/// Origen de una ejecución.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    Manual,
    RequestReceived,
    ResponseSent,
    Schedule,
    Workflow,
}

impl TriggerKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            TriggerKind::Manual => "manual",
            TriggerKind::RequestReceived => "request_received",
            TriggerKind::ResponseSent => "response_sent",
            TriggerKind::Schedule => "schedule",
            TriggerKind::Workflow => "workflow",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "request_received" => TriggerKind::RequestReceived,
            "response_sent" => TriggerKind::ResponseSent,
            "schedule" => TriggerKind::Schedule,
            "workflow" => TriggerKind::Workflow,
            _ => TriggerKind::Manual,
        }
    }
}

/// Estado completo y durable de una ejecución.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionRecord {
    pub id: String,
    pub workspace_id: String,
    pub service_id: String,
    pub workflow_id: String,
    pub workflow_name: String,
    pub status: ExecutionStatus,
    pub trigger: TriggerKind,
    pub parent_id: Option<String>,
    pub depth: u32,
    /// Índice del paso actual.
    pub pc: u32,
    /// Fase interna del paso actual (p. ej. retraso previo ya cumplido).
    pub phase: u32,
    /// Intento actual del paso (reintentos).
    pub attempt: u32,
    pub steps_executed: u32,
    pub current_step: Option<String>,
    /// Momento en que debe despertar si está esperando (ms Unix).
    pub wake_at_ms: Option<i64>,
    /// Ejecución hija que se está esperando.
    pub waiting_for: Option<String>,
    /// Contexto aislado: solicitud, respuesta, variables, resultados de pasos.
    pub context: Value,
    /// Copia del workflow al iniciar: editarlo no afecta ejecuciones en curso.
    pub definition: Value,
    pub error: Option<String>,
    pub created_ms: i64,
    pub updated_ms: i64,
    pub finished_ms: Option<i64>,
}

/// Resumen para listados y eventos en vivo.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionSummary {
    pub id: String,
    pub service_id: String,
    pub workflow_id: String,
    pub workflow_name: String,
    pub status: ExecutionStatus,
    pub trigger: TriggerKind,
    pub parent_id: Option<String>,
    pub current_step: Option<String>,
    pub steps_executed: u32,
    pub wake_at_ms: Option<i64>,
    pub error: Option<String>,
    pub created_ms: i64,
    pub updated_ms: i64,
    pub finished_ms: Option<i64>,
}

impl From<&ExecutionRecord> for ExecutionSummary {
    fn from(r: &ExecutionRecord) -> Self {
        Self {
            id: r.id.clone(),
            service_id: r.service_id.clone(),
            workflow_id: r.workflow_id.clone(),
            workflow_name: r.workflow_name.clone(),
            status: r.status,
            trigger: r.trigger,
            parent_id: r.parent_id.clone(),
            current_step: r.current_step.clone(),
            steps_executed: r.steps_executed,
            wake_at_ms: r.wake_at_ms,
            error: r.error.clone(),
            created_ms: r.created_ms,
            updated_ms: r.updated_ms,
            finished_ms: r.finished_ms,
        }
    }
}

/// Entrada de la línea de tiempo de una ejecución.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEntry {
    pub seq: i64,
    pub ts_ms: i64,
    /// `started`, `step_started`, `step_completed`, `step_failed`, `waiting`,
    /// `retry_scheduled`, `http_request`, `http_response`, `event_emitted`,
    /// `variables_set`, `condition_evaluated`, `child_started`, `completed`,
    /// `failed`, `cancelled`, `recovered`.
    pub kind: String,
    pub step_id: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Value::is_null")]
    pub data: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionFilter {
    #[serde(default)]
    pub service_id: Option<String>,
    #[serde(default)]
    pub workflow_id: Option<String>,
    /// Estados separados por comas: `running,waiting`.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub before_ms: Option<i64>,
    #[serde(default)]
    pub limit: Option<u32>,
}

const SUMMARY_COLS: &str = "id, service_id, workflow_id, workflow_name, status, trigger_kind, parent_id, \
    current_step, steps_executed, wake_at_ms, error, created_ms, updated_ms, finished_ms";

fn summary_from_row(row: &SqliteRow) -> ExecutionSummary {
    ExecutionSummary {
        id: row.get("id"),
        service_id: row.get("service_id"),
        workflow_id: row.get("workflow_id"),
        workflow_name: row.get("workflow_name"),
        status: ExecutionStatus::parse(row.get::<&str, _>("status")),
        trigger: TriggerKind::parse(row.get::<&str, _>("trigger_kind")),
        parent_id: row.get("parent_id"),
        current_step: row.get("current_step"),
        steps_executed: row.get::<i64, _>("steps_executed") as u32,
        wake_at_ms: row.get("wake_at_ms"),
        error: row.get("error"),
        created_ms: row.get("created_ms"),
        updated_ms: row.get("updated_ms"),
        finished_ms: row.get("finished_ms"),
    }
}

fn record_from_row(row: &SqliteRow) -> ExecutionRecord {
    ExecutionRecord {
        id: row.get("id"),
        workspace_id: row.get("workspace_id"),
        service_id: row.get("service_id"),
        workflow_id: row.get("workflow_id"),
        workflow_name: row.get("workflow_name"),
        status: ExecutionStatus::parse(row.get::<&str, _>("status")),
        trigger: TriggerKind::parse(row.get::<&str, _>("trigger_kind")),
        parent_id: row.get("parent_id"),
        depth: row.get::<i64, _>("depth") as u32,
        pc: row.get::<i64, _>("pc") as u32,
        phase: row.get::<i64, _>("phase") as u32,
        attempt: row.get::<i64, _>("attempt") as u32,
        steps_executed: row.get::<i64, _>("steps_executed") as u32,
        current_step: row.get("current_step"),
        wake_at_ms: row.get("wake_at_ms"),
        waiting_for: row.get("waiting_for"),
        context: serde_json::from_str(row.get::<&str, _>("context")).unwrap_or(Value::Null),
        definition: serde_json::from_str(row.get::<&str, _>("definition")).unwrap_or(Value::Null),
        error: row.get("error"),
        created_ms: row.get("created_ms"),
        updated_ms: row.get("updated_ms"),
        finished_ms: row.get("finished_ms"),
    }
}

impl Store {
    pub async fn insert_execution(&self, r: &ExecutionRecord) -> Result<()> {
        sqlx::query(
            "INSERT INTO executions (id, workspace_id, service_id, workflow_id, workflow_name, status,
                trigger_kind, parent_id, depth, pc, phase, attempt, steps_executed, current_step,
                wake_at_ms, waiting_for, context, definition, error, created_ms, updated_ms, finished_ms)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&r.id)
        .bind(&r.workspace_id)
        .bind(&r.service_id)
        .bind(&r.workflow_id)
        .bind(&r.workflow_name)
        .bind(r.status.as_str())
        .bind(r.trigger.as_str())
        .bind(&r.parent_id)
        .bind(i64::from(r.depth))
        .bind(i64::from(r.pc))
        .bind(i64::from(r.phase))
        .bind(i64::from(r.attempt))
        .bind(i64::from(r.steps_executed))
        .bind(&r.current_step)
        .bind(r.wake_at_ms)
        .bind(&r.waiting_for)
        .bind(serde_json::to_string(&r.context).unwrap_or_default())
        .bind(serde_json::to_string(&r.definition).unwrap_or_default())
        .bind(&r.error)
        .bind(r.created_ms)
        .bind(r.updated_ms)
        .bind(r.finished_ms)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Guarda el estado mutable de una ejecución.
    pub async fn save_execution(&self, r: &ExecutionRecord) -> Result<()> {
        sqlx::query(
            "UPDATE executions SET status = ?, pc = ?, phase = ?, attempt = ?, steps_executed = ?,
                current_step = ?, wake_at_ms = ?, waiting_for = ?, context = ?, error = ?,
                updated_ms = ?, finished_ms = ?
             WHERE id = ?",
        )
        .bind(r.status.as_str())
        .bind(i64::from(r.pc))
        .bind(i64::from(r.phase))
        .bind(i64::from(r.attempt))
        .bind(i64::from(r.steps_executed))
        .bind(&r.current_step)
        .bind(r.wake_at_ms)
        .bind(&r.waiting_for)
        .bind(serde_json::to_string(&r.context).unwrap_or_default())
        .bind(&r.error)
        .bind(r.updated_ms)
        .bind(r.finished_ms)
        .bind(&r.id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn get_execution(
        &self,
        workspace_id: &str,
        id: &str,
    ) -> Result<Option<ExecutionRecord>> {
        let row = sqlx::query("SELECT * FROM executions WHERE workspace_id = ? AND id = ?")
            .bind(workspace_id)
            .bind(id)
            .fetch_optional(self.pool())
            .await?;
        Ok(row.as_ref().map(record_from_row))
    }

    /// Estado de una ejecución sin importar el espacio (uso interno del motor).
    pub async fn execution_status(&self, id: &str) -> Result<Option<ExecutionRecord>> {
        let row = sqlx::query("SELECT * FROM executions WHERE id = ?")
            .bind(id)
            .fetch_optional(self.pool())
            .await?;
        Ok(row.as_ref().map(record_from_row))
    }

    pub async fn list_executions(
        &self,
        workspace_id: &str,
        f: &ExecutionFilter,
    ) -> Result<Vec<ExecutionSummary>> {
        let mut q: QueryBuilder<Sqlite> = QueryBuilder::new(format!(
            "SELECT {SUMMARY_COLS} FROM executions WHERE workspace_id = "
        ));
        q.push_bind(workspace_id);
        if let Some(s) = &f.service_id {
            q.push(" AND service_id = ").push_bind(s);
        }
        if let Some(w) = &f.workflow_id {
            q.push(" AND workflow_id = ").push_bind(w);
        }
        if let Some(st) = f.status.as_deref().filter(|s| !s.trim().is_empty()) {
            q.push(" AND status IN (");
            let mut sep = q.separated(", ");
            for s in st.split(',') {
                sep.push_bind(ExecutionStatus::parse(s.trim()).as_str());
            }
            q.push(")");
        }
        if let Some(b) = f.before_ms {
            q.push(" AND created_ms < ").push_bind(b);
        }
        q.push(" ORDER BY created_ms DESC, id DESC LIMIT ")
            .push_bind(i64::from(f.limit.unwrap_or(100).clamp(1, 1000)));
        let rows = q.build().fetch_all(self.pool()).await?;
        Ok(rows.iter().map(summary_from_row).collect())
    }

    /// Ejecuciones sin terminar de un espacio (para retomarlas al arrancar).
    pub async fn unfinished_executions(&self, workspace_id: &str) -> Result<Vec<ExecutionRecord>> {
        let rows = sqlx::query(
            "SELECT * FROM executions WHERE workspace_id = ?
             AND status IN ('pending', 'running', 'waiting') ORDER BY created_ms",
        )
        .bind(workspace_id)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.iter().map(record_from_row).collect())
    }

    pub async fn count_active_executions(&self, workspace_id: &str) -> Result<i64> {
        Ok(sqlx::query_scalar(
            "SELECT COUNT(*) FROM executions WHERE workspace_id = ?
             AND status IN ('pending', 'running', 'waiting')",
        )
        .bind(workspace_id)
        .fetch_one(self.pool())
        .await?)
    }

    pub async fn append_timeline(
        &self,
        execution_id: &str,
        ts_ms: i64,
        kind: &str,
        step_id: Option<&str>,
        message: &str,
        data: &Value,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO execution_events (execution_id, ts_ms, kind, step_id, message, data)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(execution_id)
        .bind(ts_ms)
        .bind(kind)
        .bind(step_id)
        .bind(message)
        .bind(if data.is_null() {
            None
        } else {
            Some(serde_json::to_string(data).unwrap_or_default())
        })
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn timeline(&self, execution_id: &str) -> Result<Vec<TimelineEntry>> {
        let rows = sqlx::query(
            "SELECT seq, ts_ms, kind, step_id, message, data FROM execution_events
             WHERE execution_id = ? ORDER BY seq",
        )
        .bind(execution_id)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .iter()
            .map(|row| TimelineEntry {
                seq: row.get("seq"),
                ts_ms: row.get("ts_ms"),
                kind: row.get("kind"),
                step_id: row.get("step_id"),
                message: row.get("message"),
                data: row
                    .get::<Option<&str>, _>("data")
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or(Value::Null),
            })
            .collect())
    }

    /// Conserva solo las `keep` ejecuciones terminadas más recientes.
    pub async fn prune_executions(&self, workspace_id: &str, keep: u32) -> Result<u64> {
        let res = sqlx::query(
            "DELETE FROM executions WHERE workspace_id = ?1
               AND status IN ('completed', 'failed', 'cancelled')
               AND created_ms < (
                 SELECT created_ms FROM executions WHERE workspace_id = ?1
                   AND status IN ('completed', 'failed', 'cancelled')
                 ORDER BY created_ms DESC LIMIT 1 OFFSET ?2)",
        )
        .bind(workspace_id)
        .bind(i64::from(keep.max(1)) - 1)
        .execute(self.pool())
        .await?;
        Ok(res.rows_affected())
    }

    /// Borra las ejecuciones terminadas del espacio.
    pub async fn clear_finished_executions(&self, workspace_id: &str) -> Result<u64> {
        let res = sqlx::query(
            "DELETE FROM executions WHERE workspace_id = ?
             AND status IN ('completed', 'failed', 'cancelled')",
        )
        .bind(workspace_id)
        .execute(self.pool())
        .await?;
        Ok(res.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rec(id: &str, created: i64, status: ExecutionStatus) -> ExecutionRecord {
        ExecutionRecord {
            id: id.into(),
            workspace_id: "ws".into(),
            service_id: "svc".into(),
            workflow_id: "wf".into(),
            workflow_name: "Wf".into(),
            status,
            trigger: TriggerKind::Manual,
            parent_id: None,
            depth: 0,
            pc: 0,
            phase: 0,
            attempt: 0,
            steps_executed: 0,
            current_step: None,
            wake_at_ms: None,
            waiting_for: None,
            context: json!({"vars": {}}),
            definition: json!({"name": "Wf"}),
            error: None,
            created_ms: created,
            updated_ms: created,
            finished_ms: None,
        }
    }

    #[tokio::test]
    async fn execution_lifecycle() {
        let store = Store::in_memory().await.unwrap();
        let mut r = rec("e1", 1, ExecutionStatus::Pending);
        store.insert_execution(&r).await.unwrap();
        store
            .insert_execution(&rec("e2", 2, ExecutionStatus::Completed))
            .await
            .unwrap();
        store
            .insert_execution(&rec("e3", 3, ExecutionStatus::Failed))
            .await
            .unwrap();

        r.status = ExecutionStatus::Waiting;
        r.pc = 2;
        r.wake_at_ms = Some(99);
        r.context = json!({"vars": {"n": 1}});
        store.save_execution(&r).await.unwrap();
        let got = store.get_execution("ws", "e1").await.unwrap().unwrap();
        assert_eq!(got, r);

        let unfinished = store.unfinished_executions("ws").await.unwrap();
        assert_eq!(unfinished.len(), 1);
        assert_eq!(store.count_active_executions("ws").await.unwrap(), 1);

        let f = ExecutionFilter {
            status: Some("completed, failed".into()),
            ..Default::default()
        };
        let list = store.list_executions("ws", &f).await.unwrap();
        assert_eq!(
            list.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            vec!["e3", "e2"]
        );

        store
            .append_timeline("e1", 5, "started", None, "Inició", &Value::Null)
            .await
            .unwrap();
        store
            .append_timeline("e1", 6, "step_completed", Some("a"), "ok", &json!({"x": 1}))
            .await
            .unwrap();
        let tl = store.timeline("e1").await.unwrap();
        assert_eq!(tl.len(), 2);
        assert_eq!(tl[1].step_id.as_deref(), Some("a"));
        assert_eq!(tl[1].data, json!({"x": 1}));

        assert_eq!(store.prune_executions("ws", 1).await.unwrap(), 1);
        assert_eq!(store.clear_finished_executions("ws").await.unwrap(), 1);
        assert!(store.get_execution("ws", "e1").await.unwrap().is_some());
    }
}
