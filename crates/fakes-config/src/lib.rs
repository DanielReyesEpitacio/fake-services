//! Administración de configuraciones en disco.
//!
//! Estructura de un espacio de trabajo (pensada para Git):
//!
//! ```text
//! <carpeta>/
//!   fakes.workspace.json
//!   files/                       # archivos servidos por respuestas `file`
//!   services/<servicio>/service.json
//!   services/<servicio>/endpoints/<endpoint>.json
//!   services/<servicio>/workflows/<workflow>.json
//! ```
//!
//! Los archivos se escriben de forma atómica (temporal + rename) y con
//! formato estable para producir diffs limpios.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use fakes_domain::{Endpoint, Service, Workflow, Workspace};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub const WORKSPACE_FILE: &str = "fakes.workspace.json";
pub const SERVICES_DIR: &str = "services";
pub const SERVICE_FILE: &str = "service.json";
pub const ENDPOINTS_DIR: &str = "endpoints";
pub const WORKFLOWS_DIR: &str = "workflows";
pub const FILES_DIR: &str = "files";
pub const REGISTRY_FILE: &str = "workspaces.json";

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("error de E/S en {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("JSON inválido en {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, ConfigError>;

fn io_err(path: &Path) -> impl FnOnce(std::io::Error) -> ConfigError + '_ {
    move |source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Problema al cargar un archivo concreto (el resto del espacio sigue cargando).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LoadProblem {
    pub file: String,
    pub message: String,
}

/// Espacio de trabajo cargado desde disco.
#[derive(Debug, Clone)]
pub struct LoadedWorkspace {
    pub dir: PathBuf,
    pub workspace: Workspace,
    pub services: Vec<Service>,
    pub problems: Vec<LoadProblem>,
}

// ---------------------------------------------------------------------------
// Utilidades de lectura/escritura
// ---------------------------------------------------------------------------

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path).map_err(io_err(path))?;
    serde_json::from_str(&text).map_err(|source| ConfigError::Json {
        path: path.to_path_buf(),
        source,
    })
}

/// Escribe JSON con indentación de 2 espacios, salto final y de forma atómica.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut text = serde_json::to_string_pretty(value).map_err(|source| ConfigError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    text.push('\n');
    write_atomic(path, text.as_bytes())
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| ConfigError::Invalid(format!("ruta sin carpeta: {}", path.display())))?;
    fs::create_dir_all(dir).map_err(io_err(dir))?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("archivo");
    let tmp = dir.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        fastid()
    ));
    {
        let mut f = fs::File::create(&tmp).map_err(io_err(&tmp))?;
        f.write_all(bytes).map_err(io_err(&tmp))?;
        f.sync_all().map_err(io_err(&tmp))?;
    }
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        ConfigError::Io {
            path: path.to_path_buf(),
            source: e,
        }
    })
}

fn fastid() -> u128 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    nanos ^ u128::from(COUNTER.fetch_add(1, Ordering::Relaxed))
}

// ---------------------------------------------------------------------------
// Espacios de trabajo
// ---------------------------------------------------------------------------

pub fn service_dir(ws_dir: &Path, service_id: &str) -> PathBuf {
    ws_dir.join(SERVICES_DIR).join(service_id)
}

pub fn endpoint_file(ws_dir: &Path, service_id: &str, endpoint_id: &str) -> PathBuf {
    service_dir(ws_dir, service_id)
        .join(ENDPOINTS_DIR)
        .join(format!("{endpoint_id}.json"))
}

pub fn workflow_file(ws_dir: &Path, service_id: &str, workflow_id: &str) -> PathBuf {
    service_dir(ws_dir, service_id)
        .join(WORKFLOWS_DIR)
        .join(format!("{workflow_id}.json"))
}

pub fn is_workspace_dir(dir: &Path) -> bool {
    dir.join(WORKSPACE_FILE).is_file()
}

/// Crea un espacio de trabajo vacío. Falla si la carpeta ya contiene uno.
pub fn create_workspace(dir: &Path, ws: &Workspace) -> Result<()> {
    if is_workspace_dir(dir) {
        return Err(ConfigError::Invalid(format!(
            "la carpeta {} ya contiene un espacio de trabajo",
            dir.display()
        )));
    }
    fs::create_dir_all(dir.join(SERVICES_DIR)).map_err(io_err(dir))?;
    fs::create_dir_all(dir.join(FILES_DIR)).map_err(io_err(dir))?;
    save_workspace_meta(dir, ws)
}

pub fn save_workspace_meta(dir: &Path, ws: &Workspace) -> Result<()> {
    write_json(&dir.join(WORKSPACE_FILE), ws)
}

/// Carga un espacio completo. Los archivos con errores se reportan en
/// `problems` y se omiten, para que un archivo roto no tumbe todo.
pub fn load_workspace(dir: &Path) -> Result<LoadedWorkspace> {
    let workspace: Workspace = read_json(&dir.join(WORKSPACE_FILE))?;
    let mut services = Vec::new();
    let mut problems = Vec::new();
    let services_root = dir.join(SERVICES_DIR);

    let mut service_dirs: Vec<PathBuf> = match fs::read_dir(&services_root) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(io_err(&services_root)(e)),
    };
    service_dirs.sort();

    for sdir in service_dirs {
        let sfile = sdir.join(SERVICE_FILE);
        if !sfile.is_file() {
            continue;
        }
        let mut service: Service = match read_json(&sfile) {
            Ok(s) => s,
            Err(e) => {
                problems.push(problem(dir, &sfile, &e));
                continue;
            }
        };
        let folder = sdir.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if service.id != folder {
            problems.push(LoadProblem {
                file: rel(dir, &sfile),
                message: format!(
                    "el id '{}' no coincide con la carpeta '{folder}'; se usa el nombre de la carpeta",
                    service.id
                ),
            });
            service.id = folder.to_string();
        }
        service.endpoints =
            load_documents::<Endpoint>(dir, &sdir.join(ENDPOINTS_DIR), &mut problems, |e| {
                &mut e.id
            });
        sort_endpoints(&mut service.endpoints);
        service.workflows =
            load_documents::<Workflow>(dir, &sdir.join(WORKFLOWS_DIR), &mut problems, |w| {
                &mut w.id
            });
        service.workflows.sort_by_key(|a| a.name.to_lowercase());
        services.push(service);
    }
    services.sort_by_key(|a| a.name.to_lowercase());

    Ok(LoadedWorkspace {
        dir: dir.to_path_buf(),
        workspace,
        services,
        problems,
    })
}

/// Carga los JSON de una carpeta; el id de cada documento es el nombre del archivo.
fn load_documents<T: DeserializeOwned>(
    root: &Path,
    folder: &Path,
    problems: &mut Vec<LoadProblem>,
    id_of: impl Fn(&mut T) -> &mut String,
) -> Vec<T> {
    let mut files: Vec<PathBuf> = fs::read_dir(folder)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    let mut out = Vec::new();
    for file in files {
        match read_json::<T>(&file) {
            Ok(mut doc) => {
                let stem = file.file_stem().and_then(|n| n.to_str()).unwrap_or("");
                let id = id_of(&mut doc);
                if id != stem {
                    problems.push(LoadProblem {
                        file: rel(root, &file),
                        message: format!(
                            "el id '{id}' no coincide con el archivo; se usa '{stem}'"
                        ),
                    });
                    *id = stem.to_string();
                }
                out.push(doc);
            }
            Err(e) => problems.push(problem(root, &file, &e)),
        }
    }
    out
}

/// Orden estable para mostrar endpoints: por ruta y luego por método.
pub fn sort_endpoints(eps: &mut [Endpoint]) {
    eps.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then_with(|| a.method.as_str().cmp(b.method.as_str()))
            .then_with(|| a.id.cmp(&b.id))
    });
}

fn rel(root: &Path, file: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

fn problem(root: &Path, file: &Path, e: &ConfigError) -> LoadProblem {
    let message = match e {
        ConfigError::Json { source, .. } => source.to_string(),
        other => other.to_string(),
    };
    LoadProblem {
        file: rel(root, file),
        message,
    }
}

/// Guarda `service.json` (sin endpoints ni workflows; van en archivos propios).
pub fn save_service(ws_dir: &Path, service: &Service) -> Result<()> {
    let mut meta = service.clone();
    meta.endpoints.clear();
    meta.workflows.clear();
    write_json(&service_dir(ws_dir, &service.id).join(SERVICE_FILE), &meta)
}

/// Guarda el servicio con sus endpoints y workflows, eliminando archivos sobrantes.
pub fn save_service_full(ws_dir: &Path, service: &Service) -> Result<()> {
    save_service(ws_dir, service)?;
    for ep in &service.endpoints {
        save_endpoint(ws_dir, &service.id, ep)?;
    }
    for wf in &service.workflows {
        save_workflow(ws_dir, &service.id, wf)?;
    }
    let sdir = service_dir(ws_dir, &service.id);
    let ep_ids: Vec<&str> = service.endpoints.iter().map(|e| e.id.as_str()).collect();
    let wf_ids: Vec<&str> = service.workflows.iter().map(|w| w.id.as_str()).collect();
    remove_stale(&sdir.join(ENDPOINTS_DIR), &ep_ids)?;
    remove_stale(&sdir.join(WORKFLOWS_DIR), &wf_ids)?;
    Ok(())
}

fn remove_stale(folder: &Path, keep: &[&str]) -> Result<()> {
    if let Ok(rd) = fs::read_dir(folder) {
        for entry in rd.filter_map(|e| e.ok()) {
            let p = entry.path();
            let stem = p.file_stem().and_then(|n| n.to_str()).unwrap_or("");
            let is_json = p.extension().is_some_and(|x| x == "json");
            if is_json && !keep.contains(&stem) {
                fs::remove_file(&p).map_err(io_err(&p))?;
            }
        }
    }
    Ok(())
}

pub fn save_workflow(ws_dir: &Path, service_id: &str, wf: &Workflow) -> Result<()> {
    write_json(&workflow_file(ws_dir, service_id, &wf.id), wf)
}

pub fn delete_workflow(ws_dir: &Path, service_id: &str, workflow_id: &str) -> Result<()> {
    let f = workflow_file(ws_dir, service_id, workflow_id);
    match fs::remove_file(&f) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_err(&f)(e)),
    }
}

pub fn delete_service(ws_dir: &Path, service_id: &str) -> Result<()> {
    let dir = service_dir(ws_dir, service_id);
    match fs::remove_dir_all(&dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_err(&dir)(e)),
    }
}

pub fn save_endpoint(ws_dir: &Path, service_id: &str, ep: &Endpoint) -> Result<()> {
    write_json(&endpoint_file(ws_dir, service_id, &ep.id), ep)
}

pub fn delete_endpoint(ws_dir: &Path, service_id: &str, endpoint_id: &str) -> Result<()> {
    let f = endpoint_file(ws_dir, service_id, endpoint_id);
    match fs::remove_file(&f) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_err(&f)(e)),
    }
}

// ---------------------------------------------------------------------------
// Registro de espacios y carpeta de datos
// ---------------------------------------------------------------------------

/// Lista de espacios conocidos y cuál está activo. Vive en la carpeta de datos.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Registry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
    #[serde(default)]
    pub workspaces: Vec<RegistryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryEntry {
    pub id: String,
    pub path: PathBuf,
}

impl Registry {
    pub fn load(data_dir: &Path) -> Result<Self> {
        let f = data_dir.join(REGISTRY_FILE);
        if !f.exists() {
            return Ok(Self::default());
        }
        read_json(&f)
    }

    pub fn save(&self, data_dir: &Path) -> Result<()> {
        write_json(&data_dir.join(REGISTRY_FILE), self)
    }

    pub fn find(&self, id: &str) -> Option<&RegistryEntry> {
        self.workspaces.iter().find(|w| w.id == id)
    }
}

/// Carpeta de datos local según el sistema operativo:
/// Linux `~/.local/share/fake-services`, Windows `%APPDATA%\FakeServices\fake-services\data`.
pub fn default_data_dir() -> PathBuf {
    directories::ProjectDirs::from("dev", "FakeServices", "fake-services")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".fake-services"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fakes_domain::*;
    use serde_json::json;

    fn sample_service() -> Service {
        Service {
            schema_version: 1,
            id: "mifiel".into(),
            name: "MiFiel".into(),
            description: "Firma electrónica".into(),
            upstream_url: Some("https://app.mifiel.com/api/v1".into()),
            prefix: "/mifiel".into(),
            port: None,
            auto_start: true,
            variables: Default::default(),
            auth: AuthConfig::Bearer { token: None },
            workflows: vec![serde_json::from_value(json!({"id": "notificar", "name": "Notificar", "steps": [{"id": "espera", "type": "wait", "durationMs": 10}]})).unwrap()],
            endpoints: vec![Endpoint {
                schema_version: 1,
                id: "post-documents-id-sign".into(),
                name: "Firmar".into(),
                description: String::new(),
                enabled: true,
                method: HttpMethod::Post,
                path: "/documents/{id}/sign".into(),
                request: RequestSpec::default(),
                scenarios: vec![Scenario {
                    id: "ok".into(),
                    name: "Firma exitosa".into(),
                    description: String::new(),
                    enabled: true,
                    rule_mode: RuleMode::All,
                    workflows: vec![],
                    rules: vec![],
                    response: ResponseDef::json(201, json!({"status": "pending"})),
                }],
                active_scenario: None,
                on_validation_error: None,
            }],
        }
    }

    #[test]
    fn roundtrip_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("ws");
        let ws = Workspace::new("demo", "Demo");
        create_workspace(&dir, &ws).unwrap();
        assert!(create_workspace(&dir, &ws).is_err());

        let svc = sample_service();
        save_service_full(&dir, &svc).unwrap();

        let loaded = load_workspace(&dir).unwrap();
        assert!(loaded.problems.is_empty(), "{:?}", loaded.problems);
        assert_eq!(loaded.workspace, ws);
        assert_eq!(loaded.services, vec![svc.clone()]);

        // service.json no debe contener endpoints ni workflows
        let raw = fs::read_to_string(dir.join("services/mifiel/service.json")).unwrap();
        assert!(!raw.contains("endpoints"));
        assert!(!raw.contains("workflows"));
        assert!(
            dir.join("services/mifiel/workflows/notificar.json")
                .is_file()
        );
        assert!(raw.ends_with('\n'));

        // eliminar endpoint sobrante al guardar completo
        let mut svc2 = svc.clone();
        svc2.endpoints.clear();
        svc2.workflows.clear();
        save_service_full(&dir, &svc2).unwrap();
        let loaded = load_workspace(&dir).unwrap();
        assert!(loaded.services[0].endpoints.is_empty());
        assert!(loaded.services[0].workflows.is_empty());

        delete_service(&dir, "mifiel").unwrap();
        assert!(load_workspace(&dir).unwrap().services.is_empty());
    }

    #[test]
    fn broken_file_reports_problem() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        create_workspace(dir, &Workspace::new("demo", "Demo")).unwrap();
        save_service_full(dir, &sample_service()).unwrap();
        fs::write(
            dir.join("services/mifiel/endpoints/roto.json"),
            "{ esto no es json",
        )
        .unwrap();
        let loaded = load_workspace(dir).unwrap();
        assert_eq!(loaded.services[0].endpoints.len(), 1);
        assert_eq!(loaded.problems.len(), 1);
        assert_eq!(
            loaded.problems[0].file,
            "services/mifiel/endpoints/roto.json"
        );
    }

    #[test]
    fn registry_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let mut r = Registry::load(tmp.path()).unwrap();
        assert!(r.workspaces.is_empty());
        r.workspaces.push(RegistryEntry {
            id: "demo".into(),
            path: tmp.path().join("demo"),
        });
        r.active = Some("demo".into());
        r.save(tmp.path()).unwrap();
        assert_eq!(Registry::load(tmp.path()).unwrap(), r);
    }
}
