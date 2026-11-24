//! Plantillas de proveedores: servicios listos para instalar (MiFiel,
//! Finkok, Stripe...). Son bundles JSON incluidos en el binario.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::bundle::{Bundle, ImportReport, OnConflict};
use crate::{AppError, AppResult, FakesApp};

struct Entry {
    id: &'static str,
    category: &'static str,
    tags: &'static [&'static str],
    json: &'static str,
}

const TEMPLATES: &[Entry] = &[
    Entry {
        id: "mifiel",
        category: "Firma electrónica",
        tags: &["webhooks", "recursos", "pdf"],
        json: include_str!("../../../templates/mifiel.json"),
    },
    Entry {
        id: "finkok",
        category: "Facturación (CFDI)",
        tags: &["soap", "xml", "cfdi"],
        json: include_str!("../../../templates/finkok.json"),
    },
    Entry {
        id: "stripe",
        category: "Pagos",
        tags: &["webhooks", "recursos", "form"],
        json: include_str!("../../../templates/stripe.json"),
    },
    Entry {
        id: "sendgrid",
        category: "Correo",
        tags: &["webhooks", "recursos"],
        json: include_str!("../../../templates/sendgrid.json"),
    },
    Entry {
        id: "aws-s3",
        category: "Almacenamiento",
        tags: &["binario", "recursos", "xml"],
        json: include_str!("../../../templates/aws-s3.json"),
    },
    Entry {
        id: "belvo",
        category: "Open finance",
        tags: &["recursos"],
        json: include_str!("../../../templates/belvo.json"),
    },
    Entry {
        id: "copomex",
        category: "Códigos postales",
        tags: &["consulta"],
        json: include_str!("../../../templates/copomex.json"),
    },
];

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub tags: Vec<String>,
    /// URL real del proveedor (a la que apunta tu código en producción).
    pub upstream_url: Option<String>,
    pub prefix: String,
    pub endpoints: usize,
    pub workflows: usize,
    /// Ya hay un servicio con este identificador en el espacio activo.
    pub installed: bool,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TemplateInstallRequest {
    #[serde(default)]
    pub on_conflict: OnConflict,
    /// Iniciar el servicio después de instalarlo (por defecto, sí).
    #[serde(default)]
    pub start: Option<bool>,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInstallReport {
    pub template_id: String,
    /// Identificador final del servicio instalado.
    pub service_id: String,
    #[serde(flatten)]
    pub report: ImportReport,
}

fn parse(e: &Entry) -> Bundle {
    serde_json::from_str(e.json).expect("plantilla válida")
}

fn find(id: &str) -> AppResult<&'static Entry> {
    TEMPLATES
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| AppError::NotFound(format!("no existe la plantilla '{id}'")))
}

/// Bundle de una plantilla (para exportarlo o inspeccionarlo).
pub fn template_bundle(id: &str) -> AppResult<Bundle> {
    find(id).map(parse)
}

impl FakesApp {
    pub async fn list_templates(&self) -> Vec<TemplateInfo> {
        let installed: Vec<String> = self
            .list_services()
            .await
            .into_iter()
            .map(|s| s.id)
            .collect();
        TEMPLATES
            .iter()
            .map(|e| {
                let b = parse(e);
                let s = &b.services[0];
                TemplateInfo {
                    id: e.id.into(),
                    name: s.name.clone(),
                    description: s.description.clone(),
                    category: e.category.into(),
                    tags: e.tags.iter().map(|t| t.to_string()).collect(),
                    upstream_url: s.upstream_url.clone(),
                    prefix: s.prefix.clone(),
                    endpoints: s.endpoints.len(),
                    workflows: s.workflows.len(),
                    installed: installed.iter().any(|i| i == &s.id),
                }
            })
            .collect()
    }

    pub async fn install_template(
        &self,
        id: &str,
        req: TemplateInstallRequest,
    ) -> AppResult<TemplateInstallReport> {
        let bundle = template_bundle(id)?;
        let original = bundle.services[0].id.clone();
        let report = self
            .import_merge(bundle, req.on_conflict, req.dry_run)
            .await?;
        let service_id = report
            .renamed
            .iter()
            .find(|r| r.from == original)
            .map(|r| r.to.clone())
            .unwrap_or(original);
        if !req.dry_run && req.start.unwrap_or(true) && !report.skipped.contains(&service_id) {
            self.start_service(&service_id).await?;
        }
        Ok(TemplateInstallReport {
            template_id: id.into(),
            service_id,
            report,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_are_valid() {
        for e in TEMPLATES {
            let b = parse(e);
            assert_eq!(b.services.len(), 1, "{}", e.id);
            let s = &b.services[0];
            assert_eq!(s.id, e.id);
            let issues = fakes_domain::validate::validate_service(
                s,
                &[],
                &fakes_domain::Workspace::new("ws", "ws"),
            );
            assert!(issues.is_empty(), "{}: {issues:?}", e.id);
            let wf_ids: Vec<&str> = s.workflows.iter().map(|w| w.id.as_str()).collect();
            for ep in &s.endpoints {
                let sib: Vec<&fakes_domain::Endpoint> =
                    s.endpoints.iter().filter(|x| x.id != ep.id).collect();
                let issues = crate::full_endpoint_issues(ep, &sib, &wf_ids);
                assert!(issues.is_empty(), "{}/{}: {issues:?}", e.id, ep.id);
            }
            for wf in &s.workflows {
                let sib: Vec<&fakes_domain::Workflow> =
                    s.workflows.iter().filter(|x| x.id != wf.id).collect();
                let issues = crate::workflows::full_workflow_issues(wf, &sib, &wf_ids);
                assert!(issues.is_empty(), "{}/{}: {issues:?}", e.id, wf.id);
            }
        }
    }
}
