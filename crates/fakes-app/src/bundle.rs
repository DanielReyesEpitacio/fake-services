//! Exportar e importar configuraciones como un solo archivo JSON.
//!
//! Un *bundle* contiene servicios completos (endpoints, escenarios y
//! workflows) y, opcionalmente, los ajustes del espacio y su carpeta `files/`.
//! La importación valida todo antes de escribir: o se importa completo o no
//! se toca nada.

use std::collections::{BTreeMap, HashSet};
use std::path::{Component, Path, PathBuf};

use base64::Engine;
use fakes_domain::{
    Endpoint, Issue, Service, Workflow, Workspace, normalize_prefix, slugify, unique_id,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    AppError, AppResult, FakesApp, VERSION, full_endpoint_issues, normalize_endpoint,
    sort_services, workflows,
};

pub const BUNDLE_FORMAT: &str = "fake-services";
pub const BUNDLE_VERSION: u32 = 1;
/// Tamaño máximo de los archivos incluidos en un bundle.
const MAX_FILES_BYTES: u64 = 25 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Bundle {
    /// Siempre `fake-services`.
    pub format: String,
    pub format_version: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub exported_at: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub app_version: String,
    /// Ajustes del espacio (servidor, variables, historial, webhooks).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<Workspace>,
    /// Servicios completos, con endpoints y workflows.
    #[serde(default)]
    pub services: Vec<Service>,
    /// Archivos de `files/` (ruta relativa → contenido en base64).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
    /// Servicios a exportar; vacío = todos.
    #[serde(default)]
    pub services: Vec<String>,
    /// Incluir los ajustes del espacio.
    #[serde(default)]
    pub include_workspace: bool,
    /// Incluir la carpeta `files/`.
    #[serde(default)]
    pub include_files: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub enum OnConflict {
    /// Importar con otro identificador y prefijo.
    #[default]
    Rename,
    /// Reemplazar el servicio existente.
    Replace,
    /// Conservar el existente y omitir el importado.
    Skip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub enum ImportMode {
    /// Agregar al espacio activo.
    #[default]
    Merge,
    /// Crear un espacio nuevo con el contenido y activarlo.
    NewWorkspace,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportRequest {
    pub bundle: Bundle,
    #[serde(default)]
    pub mode: ImportMode,
    #[serde(default)]
    pub on_conflict: OnConflict,
    /// Nombre del espacio nuevo (modo `newWorkspace`).
    #[serde(default)]
    pub workspace_name: Option<String>,
    /// Carpeta del espacio nuevo; vacía = carpeta de datos.
    #[serde(default)]
    pub path: Option<PathBuf>,
    /// Solo validar y mostrar qué pasaría.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Renamed {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub dry_run: bool,
    pub workspace_id: String,
    pub created: Vec<String>,
    pub replaced: Vec<String>,
    pub skipped: Vec<String>,
    pub renamed: Vec<Renamed>,
    pub files: usize,
    pub endpoints: usize,
    pub workflows: usize,
    pub warnings: Vec<String>,
}

/// Valida que una ruta de `files/` sea relativa y no salga de la carpeta.
fn safe_relative(path: &str) -> Option<PathBuf> {
    let p = Path::new(path);
    if path.is_empty() || p.is_absolute() || path.contains(':') {
        return None;
    }
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::Normal(s) => out.push(s),
            _ => return None,
        }
    }
    Some(out)
}

fn collect_files(
    root: &Path,
    dir: &Path,
    out: &mut BTreeMap<String, String>,
    total: &mut u64,
) -> AppResult<()> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            collect_files(root, &p, out, total)?;
        } else if p.is_file() {
            let bytes = std::fs::read(&p)
                .map_err(|e| AppError::BadRequest(format!("{}: {e}", p.display())))?;
            *total += bytes.len() as u64;
            if *total > MAX_FILES_BYTES {
                return Err(AppError::BadRequest(
                    "la carpeta files/ supera 25 MB; expórtala sin archivos y cópiala aparte"
                        .into(),
                ));
            }
            let rel = p
                .strip_prefix(root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            out.insert(rel, base64::engine::general_purpose::STANDARD.encode(bytes));
        }
    }
    Ok(())
}

impl FakesApp {
    /// Exporta servicios (y opcionalmente el espacio) del espacio activo.
    pub async fn export_bundle(&self, opts: &ExportOptions) -> AppResult<Bundle> {
        let inner = self.inner.lock().await;
        let a = &inner.active;
        for id in &opts.services {
            if !a.services.iter().any(|s| &s.id == id) {
                return Err(AppError::NotFound(format!("no existe el servicio '{id}'")));
            }
        }
        let services: Vec<Service> = a
            .services
            .iter()
            .filter(|s| opts.services.is_empty() || opts.services.contains(&s.id))
            .cloned()
            .collect();
        let mut files = BTreeMap::new();
        if opts.include_files {
            let root = a.dir.join(fakes_config::FILES_DIR);
            let mut total = 0;
            collect_files(&root, &root, &mut files, &mut total)?;
        }
        Ok(Bundle {
            format: BUNDLE_FORMAT.into(),
            format_version: BUNDLE_VERSION,
            exported_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            app_version: VERSION.into(),
            // Ajustes tal como están en disco (sin ajustes temporales de la sesión).
            workspace: opts.include_workspace.then(|| {
                fakes_config::read_json::<Workspace>(&a.dir.join(fakes_config::WORKSPACE_FILE))
                    .unwrap_or_else(|_| a.workspace.clone())
            }),
            services,
            files,
        })
    }

    /// Importa un bundle. Valida todo antes de escribir.
    pub async fn import_bundle(&self, req: ImportRequest) -> AppResult<ImportReport> {
        check_format(&req.bundle)?;
        if req.mode == ImportMode::NewWorkspace {
            return self.import_as_new_workspace(req).await;
        }
        self.import_merge(req.bundle, req.on_conflict, req.dry_run)
            .await
    }

    async fn import_as_new_workspace(&self, req: ImportRequest) -> AppResult<ImportReport> {
        let mut ws = req
            .bundle
            .workspace
            .clone()
            .unwrap_or_else(|| Workspace::new("", "Importado"));
        if let Some(name) = req
            .workspace_name
            .as_deref()
            .filter(|n| !n.trim().is_empty())
        {
            ws.name = name.trim().to_string();
        }
        ws.id = String::new();
        if req.dry_run {
            // Validar contra un espacio vacío: no hay conflictos posibles.
            let mut report = ImportReport {
                dry_run: true,
                ..Default::default()
            };
            let issues = prepare_services(
                &req.bundle.services,
                &[],
                OnConflict::Rename,
                &mut report,
                &[],
            )
            .1;
            if !issues.is_empty() {
                return Err(AppError::Validation(issues));
            }
            report.files = req.bundle.files.len();
            return Ok(report);
        }
        self.create_workspace(ws, req.path, true).await?;
        self.import_merge(req.bundle, OnConflict::Rename, false)
            .await
    }

    async fn import_merge(
        &self,
        bundle: Bundle,
        on_conflict: OnConflict,
        dry_run: bool,
    ) -> AppResult<ImportReport> {
        let mut inner = self.inner.lock().await;
        let mut report = ImportReport {
            dry_run,
            workspace_id: inner.active.workspace.id.clone(),
            ..Default::default()
        };
        if bundle.workspace.is_some() {
            report
                .warnings
                .push("Los ajustes del espacio del archivo no se aplican al combinar; solo los servicios.".into());
        }

        let mut reserved = self.opts.reserved_ports.clone();
        reserved.push(inner.active.workspace.server.port);
        let (prepared, mut issues) = prepare_services(
            &bundle.services,
            &inner.active.services,
            on_conflict,
            &mut report,
            &reserved,
        );

        // Archivos de files/
        let files_root = inner.active.dir.join(fakes_config::FILES_DIR);
        let mut files: Vec<(PathBuf, Vec<u8>)> = Vec::new();
        for (path, b64) in &bundle.files {
            let Some(rel) = safe_relative(path) else {
                issues.push(Issue::new(format!("files.{path}"), "ruta no permitida"));
                continue;
            };
            match base64::engine::general_purpose::STANDARD.decode(b64.trim()) {
                Ok(bytes) => {
                    let target = files_root.join(&rel);
                    if target.exists() && on_conflict != OnConflict::Replace {
                        report
                            .warnings
                            .push(format!("files/{path} ya existe; se conservó el actual"));
                    } else {
                        files.push((target, bytes));
                    }
                }
                Err(_) => issues.push(Issue::new(
                    format!("files.{path}"),
                    "contenido base64 inválido",
                )),
            }
        }

        if !issues.is_empty() {
            return Err(AppError::Validation(issues));
        }
        report.files = files.len();
        if dry_run {
            return Ok(report);
        }

        // Escritura.
        for (svc, replaces) in &prepared {
            if let Some(old) = replaces {
                fakes_config::delete_service(&inner.active.dir, old)?;
                inner.active.services.retain(|s| &s.id != old);
                inner.active.running.remove(old);
            }
            fakes_config::save_service_full(&inner.active.dir, svc)?;
            if svc.auto_start {
                inner.active.running.insert(svc.id.clone());
            }
            inner.active.services.push(svc.clone());
        }
        for (target, bytes) in files {
            fakes_config::write_atomic(&target, &bytes)?;
        }
        sort_services(&mut inner.active.services);
        self.publish(&inner, true).await;
        Ok(report)
    }
}

fn check_format(b: &Bundle) -> AppResult<()> {
    if b.format != BUNDLE_FORMAT {
        return Err(AppError::BadRequest(format!(
            "el archivo no es una exportación de Fake Services (format = '{}')",
            b.format
        )));
    }
    if b.format_version > BUNDLE_VERSION {
        return Err(AppError::BadRequest(format!(
            "el archivo es de una versión más nueva (formato {}); actualiza Fake Services",
            b.format_version
        )));
    }
    Ok(())
}

/// Prepara los servicios del bundle frente a los existentes: resuelve
/// conflictos, normaliza identificadores y valida. Devuelve cada servicio con
/// el id del servicio que reemplaza (si aplica).
fn prepare_services(
    incoming: &[Service],
    existing: &[Service],
    on_conflict: OnConflict,
    report: &mut ImportReport,
    reserved_ports: &[u16],
) -> (Vec<(Service, Option<String>)>, Vec<Issue>) {
    let mut issues = Vec::new();
    let mut out: Vec<(Service, Option<String>)> = Vec::new();
    // Ids/prefijos/puertos ocupados tras la importación.
    let mut taken_ids: HashSet<String> = existing.iter().map(|s| s.id.clone()).collect();
    let mut taken_prefixes: HashSet<String> = existing.iter().map(|s| s.prefix.clone()).collect();
    let mut taken_ports: HashSet<u16> = existing.iter().filter_map(|s| s.port).collect();
    taken_ports.extend(reserved_ports);

    for (i, original) in incoming.iter().enumerate() {
        let mut svc = original.clone();
        svc.schema_version = fakes_domain::SCHEMA_VERSION;
        if svc.id.trim().is_empty() {
            svc.id = slugify(&svc.name);
        }
        svc.prefix = if svc.prefix.trim().is_empty() {
            format!("/{}", svc.id)
        } else {
            normalize_prefix(&svc.prefix)
        };

        let mut replaces = None;
        if let Some(current) = existing.iter().find(|s| s.id == svc.id) {
            match on_conflict {
                OnConflict::Skip => {
                    report.skipped.push(svc.id.clone());
                    continue;
                }
                OnConflict::Replace => {
                    replaces = Some(current.id.clone());
                    taken_prefixes.remove(&current.prefix);
                    if let Some(p) = current.port {
                        taken_ports.remove(&p);
                    }
                }
                OnConflict::Rename => {
                    let new_id = unique_id(&svc.id, taken_ids.iter().map(String::as_str));
                    report.renamed.push(Renamed {
                        from: svc.id.clone(),
                        to: new_id.clone(),
                    });
                    svc.id = new_id;
                }
            }
        } else if taken_ids.contains(&svc.id) {
            // Repetido dentro del mismo archivo.
            let new_id = unique_id(&svc.id, taken_ids.iter().map(String::as_str));
            report.renamed.push(Renamed {
                from: svc.id.clone(),
                to: new_id.clone(),
            });
            svc.id = new_id;
        }

        if !svc.prefix.is_empty() && taken_prefixes.contains(&svc.prefix) {
            let base = slugify(&svc.id);
            let mut candidate = format!("/{base}");
            let mut n = 2;
            while taken_prefixes.contains(&candidate) {
                candidate = format!("/{base}-{n}");
                n += 1;
            }
            report.warnings.push(format!(
                "«{}»: el prefijo {} ya se usa; se cambió a {candidate}",
                svc.name, svc.prefix
            ));
            svc.prefix = candidate;
        }
        if let Some(p) = svc.port
            && taken_ports.contains(&p)
        {
            report.warnings.push(format!(
                "«{}»: el puerto {p} ya se usa; se quitó el puerto propio",
                svc.name
            ));
            svc.port = None;
        }

        // Normalizar y validar workflows y endpoints.
        let base = format!("services[{i}]");
        let mut wfs: Vec<Workflow> = Vec::new();
        for mut wf in std::mem::take(&mut svc.workflows) {
            workflows::normalize_workflow(&mut wf, &wfs);
            wfs.push(wf);
        }
        let wf_ids: Vec<&str> = wfs.iter().map(|w| w.id.as_str()).collect();
        for (j, wf) in wfs.iter().enumerate() {
            let siblings: Vec<&Workflow> = wfs[..j].iter().collect();
            issues.extend(
                workflows::full_workflow_issues(wf, &siblings, &wf_ids)
                    .into_iter()
                    .map(|is| {
                        Issue::new(format!("{base}.workflows[{j}].{}", is.field), is.message)
                    }),
            );
        }
        let mut eps: Vec<Endpoint> = Vec::new();
        for (j, mut ep) in std::mem::take(&mut svc.endpoints).into_iter().enumerate() {
            normalize_endpoint(&mut ep, &eps);
            let siblings: Vec<&Endpoint> = eps.iter().collect();
            issues.extend(
                full_endpoint_issues(&ep, &siblings, &wf_ids)
                    .into_iter()
                    .map(|is| {
                        Issue::new(format!("{base}.endpoints[{j}].{}", is.field), is.message)
                    }),
            );
            eps.push(ep);
        }
        svc.endpoints = eps;
        svc.workflows = wfs;

        // Validación del servicio con un espacio vacío (los conflictos ya se resolvieron).
        let ws = Workspace::new("import", "import");
        issues.extend(
            fakes_domain::validate::validate_service(&svc, &[], &ws)
                .into_iter()
                .filter(|is| is.field != "port")
                .map(|is| Issue::new(format!("{base}.{}", is.field), is.message)),
        );

        report.endpoints += svc.endpoints.len();
        report.workflows += svc.workflows.len();
        if replaces.is_some() {
            report.replaced.push(svc.id.clone());
        } else {
            report.created.push(svc.id.clone());
        }
        taken_ids.insert(svc.id.clone());
        taken_prefixes.insert(svc.prefix.clone());
        if let Some(p) = svc.port {
            taken_ports.insert(p);
        }
        out.push((svc, replaces));
    }
    (out, issues)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_only() {
        assert_eq!(safe_relative("a/b.pdf"), Some(PathBuf::from("a/b.pdf")));
        assert!(safe_relative("../x").is_none());
        assert!(safe_relative("/etc/passwd").is_none());
        assert!(safe_relative("a/../../x").is_none());
        assert!(safe_relative("C:/x").is_none());
        assert!(safe_relative("").is_none());
    }
}
