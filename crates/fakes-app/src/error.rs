use fakes_domain::Issue;
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("la configuración tiene errores")]
    Validation(Vec<Issue>),
    #[error("{0}")]
    BadRequest(String),
    #[error(transparent)]
    Config(#[from] fakes_config::ConfigError),
    #[error(transparent)]
    Store(#[from] fakes_store::StoreError),
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::NotFound(_) => "not_found",
            AppError::Conflict(_) => "conflict",
            AppError::Validation(_) => "validation_failed",
            AppError::BadRequest(_) => "bad_request",
            AppError::Config(_) => "config_error",
            AppError::Store(_) => "store_error",
        }
    }

    pub fn issues(&self) -> &[Issue] {
        match self {
            AppError::Validation(i) => i,
            _ => &[],
        }
    }

    pub fn to_body(&self) -> ErrorBody {
        ErrorBody {
            error: self.code().to_string(),
            message: self.to_string(),
            issues: self.issues().to_vec(),
        }
    }
}

/// Forma JSON estándar de los errores de administración.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ErrorBody {
    pub error: String,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<Issue>,
}

pub(crate) fn not_found_service(id: &str) -> AppError {
    AppError::NotFound(format!("no existe el servicio '{id}'"))
}

pub(crate) fn not_found_endpoint(sid: &str, eid: &str) -> AppError {
    AppError::NotFound(format!("el servicio '{sid}' no tiene el endpoint '{eid}'"))
}
