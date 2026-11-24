//! Importar especificaciones (OpenAPI, Postman), validar la configuración
//! completa y detectar integraciones en proyectos.

use std::path::PathBuf;

use fakes_domain::Issue;
use fakes_domain::validate::validate_service;
use fakes_import::{ImportOptions, SpecFormat};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::bundle::{BUNDLE_FORMAT, BUNDLE_VERSION, Bundle, ImportReport, OnConflict};
use crate::views::ValidationReport;
use crate::{AppError, AppResult, FakesApp, full_endpoint_issues, workflows};

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpecImportRequest {
    /// Contenido del documento (JSON o YAML).
    pub content: String,
    /// `openapi` o `postman`; si se omite, se detecta.
    #[serde(default)]
    pub format: Option<SpecFormatInput>,
    /// Nombre del servicio (por defecto, el del documento).
    #[serde(default)]
    pub name: Option<String>,
    /// Prefijo de rutas (por defecto, `/<id>`).
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub on_conflict: OnConflict,
    /// Solo validar y mostrar qué se crearía.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SpecFormatInput {
    Openapi,
    Postman,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SpecImportReport {
    pub format: SpecFormatInput,
    /// Servicio generado (identificador final tras resolver conflictos).
    pub service_id: String,
    #[serde(flatten)]
    pub report: ImportReport,
}

impl FakesApp {
    /// Convierte un OpenAPI o una colección de Postman en un servicio y lo importa.
    pub async fn import_spec(&self, req: SpecImportRequest) -> AppResult<SpecImportReport> {
        let format = match req.format {
            Some(f) => f,
            None => match fakes_import::detect_format(&req.content) {
                Some(SpecFormat::Openapi) => SpecFormatInput::Openapi,
                Some(SpecFormat::Postman) => SpecFormatInput::Postman,
                Some(SpecFormat::Bundle) => {
                    return Err(AppError::BadRequest(
                        "es una exportación de Fake Services; impórtala con /import".into(),
                    ));
                }
                None => {
                    return Err(AppError::BadRequest(
                        "no se reconoce el formato: se espera OpenAPI 3.x, Swagger 2.0 o una colección de Postman v2".into(),
                    ));
                }
            },
        };
        let opts = ImportOptions {
            name: req.name.filter(|n| !n.trim().is_empty()),
            prefix: req.prefix.filter(|p| !p.trim().is_empty()),
        };
        let imported = match format {
            SpecFormatInput::Openapi => fakes_import::from_openapi(&req.content, &opts),
            SpecFormatInput::Postman => fakes_import::from_postman(&req.content, &opts),
        }
        .map_err(AppError::BadRequest)?;
        let original_id = imported.service.id.clone();
        let bundle = Bundle {
            format: BUNDLE_FORMAT.into(),
            format_version: BUNDLE_VERSION,
            exported_at: String::new(),
            app_version: String::new(),
            workspace: None,
            services: vec![imported.service],
            files: Default::default(),
            resources: Default::default(),
        };
        let mut report = self
            .import_merge(bundle, req.on_conflict, req.dry_run)
            .await?;
        report.warnings.extend(imported.warnings);
        let service_id = report
            .renamed
            .iter()
            .find(|r| r.from == original_id)
            .map(|r| r.to.clone())
            .unwrap_or(original_id);
        Ok(SpecImportReport {
            format,
            service_id,
            report,
        })
    }

    /// Valida toda la configuración del espacio activo (servicios, endpoints,
    /// workflows y archivos que no se pudieron cargar).
    pub async fn validate_all(&self) -> AppResult<ValidationReport> {
        let inner = self.inner.lock().await;
        let a = &inner.active;
        let mut issues: Vec<Issue> = a
            .problems
            .iter()
            .map(|p| Issue::new(format!("archivo:{}", p.file), p.message.clone()))
            .collect();
        let all: Vec<&fakes_domain::Service> = a.services.iter().collect();
        for svc in &a.services {
            let base = format!("services[{}]", svc.id);
            issues.extend(
                validate_service(svc, &all, &a.workspace)
                    .into_iter()
                    .map(|i| Issue::new(format!("{base}.{}", i.field), i.message)),
            );
            let wf_ids: Vec<&str> = svc.workflows.iter().map(|w| w.id.as_str()).collect();
            for wf in &svc.workflows {
                let siblings: Vec<&fakes_domain::Workflow> =
                    svc.workflows.iter().filter(|w| w.id != wf.id).collect();
                issues.extend(
                    workflows::full_workflow_issues(wf, &siblings, &wf_ids)
                        .into_iter()
                        .map(|i| {
                            Issue::new(
                                format!("{base}.workflows[{}].{}", wf.id, i.field),
                                i.message,
                            )
                        }),
                );
            }
            for ep in &svc.endpoints {
                let siblings: Vec<&fakes_domain::Endpoint> =
                    svc.endpoints.iter().filter(|e| e.id != ep.id).collect();
                issues.extend(
                    full_endpoint_issues(ep, &siblings, &wf_ids)
                        .into_iter()
                        .map(|i| {
                            Issue::new(
                                format!("{base}.endpoints[{}].{}", ep.id, i.field),
                                i.message,
                            )
                        }),
                );
            }
        }
        Ok(ValidationReport {
            valid: issues.is_empty(),
            issues,
        })
    }

    /// Analiza un proyecto en disco y devuelve las integraciones que usa.
    pub async fn detect_integrations(
        &self,
        path: PathBuf,
    ) -> AppResult<fakes_import::detect::DetectionReport> {
        let path = if path.is_relative() {
            std::env::current_dir()
                .map(|d| d.join(&path))
                .unwrap_or(path)
        } else {
            path
        };
        tokio::task::spawn_blocking(move || fakes_import::detect::detect_integrations(&path))
            .await
            .map_err(|e| AppError::BadRequest(e.to_string()))?
            .map_err(AppError::BadRequest)
    }
}
