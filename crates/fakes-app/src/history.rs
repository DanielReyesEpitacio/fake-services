//! Escritura asíncrona del historial: el servidor de simulación nunca
//! espera a la base de datos.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use fakes_server::RequestSink;
use fakes_store::{RequestRecord, RequestSummary, Store};
use tokio::sync::{broadcast, mpsc};

use crate::AppEvent;

/// Cada cuántas inserciones se recorta el historial.
const PRUNE_EVERY: u32 = 200;
pub struct HistorySink {
    tx: mpsc::Sender<RequestRecord>,
}

impl HistorySink {
    /// `keep` es el máximo de solicitudes a conservar (configurable en caliente).
    pub fn spawn(
        store: Store,
        events: broadcast::Sender<AppEvent>,
        keep: Arc<AtomicU32>,
    ) -> Arc<Self> {
        let (tx, mut rx) = mpsc::channel::<RequestRecord>(4096);
        tokio::spawn(async move {
            let mut since_prune = 0u32;
            while let Some(rec) = rx.recv().await {
                if let Err(e) = store.insert_request(&rec).await {
                    tracing::warn!("no se pudo guardar el historial: {e}");
                    continue;
                }
                let _ = events.send(AppEvent::Request(RequestSummary::from(&rec)));
                since_prune += 1;
                if since_prune >= PRUNE_EVERY {
                    since_prune = 0;
                    if let Err(e) = store
                        .prune_requests(&rec.workspace_id, keep.load(Ordering::Relaxed))
                        .await
                    {
                        tracing::warn!("no se pudo recortar el historial: {e}");
                    }
                }
            }
        });
        Arc::new(Self { tx })
    }
}

impl RequestSink for HistorySink {
    fn record(&self, record: RequestRecord) {
        if self.tx.try_send(record).is_err() {
            tracing::warn!("historial saturado: se descartó una solicitud");
        }
    }
}
