//! Servidor HTTP de simulación.
//!
//! - Un servidor central (`127.0.0.1:4000` por defecto) publica cada
//!   servicio bajo su prefijo (`/mifiel`, `/aws-s3`...).
//! - Los servicios con puerto propio además escuchan en ese puerto sin prefijo.
//! - La configuración se lee de una instantánea compilada que se reemplaza
//!   atómicamente al editar: no hay que reiniciar nada para ver cambios.

mod body;
mod handler;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use std::time::Duration;

use arc_swap::ArcSwap;
use fakes_matching::CompiledWorkspace;
use fakes_store::RequestRecord;
use tokio::sync::oneshot;

pub use handler::{RESERVED_PREFIX, build_router};

/// Tiempo que se espera a que terminen las conexiones abiertas al cerrar un
/// puerto (p. ej. respuestas con retraso largo) antes de cortarlas.
const GRACEFUL_TIMEOUT: Duration = Duration::from_secs(2);

async fn stop_listener(mut l: RunningListener) {
    if let Some(tx) = l.shutdown.take() {
        let _ = tx.send(());
    }
    if tokio::time::timeout(GRACEFUL_TIMEOUT, &mut l.task)
        .await
        .is_err()
    {
        l.task.abort();
    }
}

/// Recibe cada solicitud procesada (historial, eventos en vivo...).
pub trait RequestSink: Send + Sync + 'static {
    fn record(&self, record: RequestRecord);
}

/// Sumidero que descarta todo (pruebas).
pub struct NullSink;
impl RequestSink for NullSink {
    fn record(&self, _: RequestRecord) {}
}

/// Configuración vigente que consultan los listeners en cada solicitud.
pub struct Snapshot {
    pub compiled: CompiledWorkspace,
    /// Servicios iniciados (los detenidos responden 503).
    pub running: HashSet<String>,
    /// Carpeta `files/` del espacio para respuestas de tipo archivo.
    pub files_root: Option<PathBuf>,
}

impl Snapshot {
    pub fn workspace_id(&self) -> &str {
        &self.compiled.workspace.id
    }

    /// URL pública del servidor central.
    pub fn central_url(&self) -> String {
        let s = &self.compiled.workspace.server;
        format!("http://{}:{}", display_host(&s.host), s.port)
    }
}

/// `0.0.0.0` no es navegable; para mostrar URLs se usa `127.0.0.1`.
pub fn display_host(host: &str) -> &str {
    match host {
        "0.0.0.0" => "127.0.0.1",
        "::" | "[::]" => "[::1]",
        h => h,
    }
}

pub struct ServerState {
    pub snapshot: ArcSwap<Snapshot>,
    pub sink: Arc<dyn RequestSink>,
}

impl ServerState {
    pub fn new(snapshot: Snapshot, sink: Arc<dyn RequestSink>) -> Arc<Self> {
        Arc::new(Self {
            snapshot: ArcSwap::from_pointee(snapshot),
            sink,
        })
    }
}

// ---------------------------------------------------------------------------
// Listeners
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ListenerKey {
    pub host: String,
    pub port: u16,
    /// `None` = servidor central; `Some(id)` = puerto propio de un servicio.
    pub service_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenerStatus {
    pub host: String,
    pub port: u16,
    pub service_id: Option<String>,
    pub url: String,
    pub listening: bool,
    pub error: Option<String>,
}

struct RunningListener {
    shutdown: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
    local_addr: SocketAddr,
}

/// Administra los sockets abiertos según la configuración.
pub struct MockServer {
    state: Arc<ServerState>,
    listeners: tokio::sync::Mutex<HashMap<ListenerKey, RunningListener>>,
    errors: Mutex<BTreeMap<ListenerKey, String>>,
}

impl MockServer {
    pub fn new(state: Arc<ServerState>) -> Arc<Self> {
        Arc::new(Self {
            state,
            listeners: Default::default(),
            errors: Default::default(),
        })
    }

    pub fn state(&self) -> &Arc<ServerState> {
        &self.state
    }

    /// Reemplaza la configuración y ajusta los sockets abiertos.
    pub async fn apply(&self, snapshot: Snapshot) -> Vec<ListenerStatus> {
        self.state.snapshot.store(Arc::new(snapshot));
        self.reconcile().await
    }

    fn desired(&self) -> Vec<ListenerKey> {
        let snap = self.state.snapshot.load();
        let server = &snap.compiled.workspace.server;
        let mut keys = vec![ListenerKey {
            host: server.host.clone(),
            port: server.port,
            service_id: None,
        }];
        for s in &snap.compiled.services {
            if let Some(port) = s.service.port {
                keys.push(ListenerKey {
                    host: server.host.clone(),
                    port,
                    service_id: Some(s.service.id.clone()),
                });
            }
        }
        keys
    }

    pub async fn reconcile(&self) -> Vec<ListenerStatus> {
        let desired = self.desired();
        let mut listeners = self.listeners.lock().await;

        // Cerrar los que sobran.
        let stale: Vec<ListenerKey> = listeners
            .keys()
            .filter(|k| !desired.contains(k))
            .cloned()
            .collect();
        for key in stale {
            if let Some(l) = listeners.remove(&key) {
                stop_listener(l).await;
                tracing::info!(port = key.port, "listener detenido");
            }
        }
        self.errors
            .lock()
            .unwrap()
            .retain(|k, _| desired.contains(k));

        // Abrir los nuevos (o reintentar los que fallaron).
        for key in &desired {
            let alive = listeners.get(key).is_some_and(|l| !l.task.is_finished());
            if alive {
                continue;
            }
            listeners.remove(key);
            match self.start_listener(key).await {
                Ok(l) => {
                    tracing::info!(addr = %l.local_addr, service = ?key.service_id, "escuchando");
                    self.errors.lock().unwrap().remove(key);
                    listeners.insert(key.clone(), l);
                }
                Err(e) => {
                    tracing::warn!(port = key.port, "no se pudo abrir el puerto: {e}");
                    self.errors.lock().unwrap().insert(key.clone(), e);
                }
            }
        }
        drop(listeners);
        self.status().await
    }

    async fn start_listener(&self, key: &ListenerKey) -> Result<RunningListener, String> {
        let listener = tokio::net::TcpListener::bind((key.host.as_str(), key.port))
            .await
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::AddrInUse => {
                    format!("el puerto {} ya está en uso por otro programa", key.port)
                }
                std::io::ErrorKind::PermissionDenied => {
                    format!("sin permiso para usar el puerto {}", key.port)
                }
                _ => format!("{}:{}: {e}", key.host, key.port),
            })?;
        let local_addr = listener.local_addr().map_err(|e| e.to_string())?;
        let app = build_router(self.state.clone(), key.service_id.clone());
        let (tx, rx) = oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            let res = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async {
                let _ = rx.await;
            })
            .await;
            if let Err(e) = res {
                tracing::error!("servidor de simulación terminó con error: {e}");
            }
        });
        Ok(RunningListener {
            shutdown: Some(tx),
            task,
            local_addr,
        })
    }

    pub async fn status(&self) -> Vec<ListenerStatus> {
        let listeners = self.listeners.lock().await;
        let errors = self.errors.lock().unwrap().clone();
        let mut out = Vec::new();
        for key in self.desired() {
            let running = listeners.get(&key);
            let port = running.map(|l| l.local_addr.port()).unwrap_or(key.port);
            out.push(ListenerStatus {
                url: format!("http://{}:{}", display_host(&key.host), port),
                host: key.host.clone(),
                port,
                service_id: key.service_id.clone(),
                listening: running.is_some_and(|l| !l.task.is_finished()),
                error: errors.get(&key).cloned(),
            });
        }
        out
    }

    /// Dirección real del servidor central (útil con puerto 0 en pruebas).
    pub async fn central_addr(&self) -> Option<SocketAddr> {
        let listeners = self.listeners.lock().await;
        listeners
            .iter()
            .find(|(k, _)| k.service_id.is_none())
            .map(|(_, l)| l.local_addr)
    }

    pub async fn shutdown(&self) {
        let mut listeners = self.listeners.lock().await;
        for (_, l) in listeners.drain() {
            stop_listener(l).await;
        }
    }
}

#[cfg(test)]
mod tests;
