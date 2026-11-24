//! Recursos simulados persistentes: consulta y edición desde la interfaz,
//! la API y el MCP. Las simulaciones los usan con `scenario.resource` y con
//! los pasos `resource` de los workflows.

use fakes_store::{CollectionInfo, ResourceRecord};
use serde_json::Value;

use crate::{AppError, AppResult, FakesApp, find_service};

fn check_collection(name: &str) -> AppResult<()> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
    if ok {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!(
            "colección inválida '{name}': usa letras, números, '-', '_' o '.' (máx. 64)"
        )))
    }
}

impl FakesApp {
    async fn resource_scope(&self, service_id: &str) -> AppResult<String> {
        let inner = self.inner.lock().await;
        find_service(&inner.active, service_id)?;
        Ok(inner.active.workspace.id.clone())
    }

    pub async fn list_resource_collections(
        &self,
        service_id: &str,
    ) -> AppResult<Vec<CollectionInfo>> {
        let ws = self.resource_scope(service_id).await?;
        Ok(self.store.resource_collections(&ws, service_id).await?)
    }

    pub async fn list_resources(
        &self,
        service_id: &str,
        collection: &str,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> AppResult<Vec<ResourceRecord>> {
        check_collection(collection)?;
        let ws = self.resource_scope(service_id).await?;
        Ok(self
            .store
            .resource_list(
                &ws,
                service_id,
                collection,
                limit.unwrap_or(fakes_store::LIST_LIMIT),
                offset.unwrap_or(0),
            )
            .await?)
    }

    pub async fn get_resource(
        &self,
        service_id: &str,
        collection: &str,
        id: &str,
    ) -> AppResult<ResourceRecord> {
        check_collection(collection)?;
        let ws = self.resource_scope(service_id).await?;
        self.store
            .resource_get(&ws, service_id, collection, id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("no existe el recurso {collection}/{id}")))
    }

    /// Crea o reemplaza un registro. Siempre conserva `id` dentro de los datos.
    pub async fn put_resource(
        &self,
        service_id: &str,
        collection: &str,
        id: &str,
        data: Value,
    ) -> AppResult<ResourceRecord> {
        check_collection(collection)?;
        let id = id.trim();
        if id.is_empty() {
            return Err(AppError::BadRequest(
                "el identificador del recurso es obligatorio".into(),
            ));
        }
        let ws = self.resource_scope(service_id).await?;
        let mut data = if data.is_object() {
            data
        } else {
            serde_json::json!({ "value": data })
        };
        data["id"] = Value::String(id.to_string());
        self.store
            .resource_put(&ws, service_id, collection, id, &data)
            .await?;
        self.get_resource(service_id, collection, id).await
    }

    pub async fn delete_resource(
        &self,
        service_id: &str,
        collection: &str,
        id: &str,
    ) -> AppResult<()> {
        check_collection(collection)?;
        let ws = self.resource_scope(service_id).await?;
        if self
            .store
            .resource_delete(&ws, service_id, collection, id)
            .await?
        {
            Ok(())
        } else {
            Err(AppError::NotFound(format!(
                "no existe el recurso {collection}/{id}"
            )))
        }
    }

    /// Borra una colección completa (o todas si `collection` es `None`).
    pub async fn clear_resources(
        &self,
        service_id: &str,
        collection: Option<&str>,
    ) -> AppResult<u64> {
        if let Some(c) = collection {
            check_collection(c)?;
        }
        let ws = self.resource_scope(service_id).await?;
        Ok(self
            .store
            .resource_clear(&ws, service_id, collection)
            .await?)
    }
}
