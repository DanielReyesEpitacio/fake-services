//! Herramientas de plantillas de proveedores y recursos simulados.

use std::sync::Arc;

use fakes_app::bundle::OnConflict;
use fakes_app::{FakesApp, TemplateInstallRequest};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tools::done;
use crate::{Tool, tool};

#[derive(Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct Empty {}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallArgs {
    /// Identificador de la plantilla (ver listar_plantillas), p. ej. `mifiel`.
    pub template_id: String,
    /// Qué hacer si ya existe un servicio con ese id: `rename` (por defecto), `replace` o `skip`.
    #[serde(default)]
    pub on_conflict: OnConflict,
    /// Iniciar el servicio al instalarlo (por defecto, sí).
    #[serde(default)]
    pub start: Option<bool>,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TemplateRef {
    pub template_id: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CollectionArgs {
    pub service_id: String,
    /// Colección (p. ej. `documents`). Si se omite, lista las colecciones del servicio.
    #[serde(default)]
    pub collection: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub offset: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceRef {
    pub service_id: String,
    pub collection: String,
    pub record_id: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveResource {
    pub service_id: String,
    pub collection: String,
    pub record_id: String,
    /// Datos completos del registro (objeto JSON); reemplaza el existente.
    pub data: Value,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClearArgs {
    pub service_id: String,
    /// Colección a vaciar; si se omite, se borran todas las del servicio.
    #[serde(default)]
    pub collection: Option<String>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResourceListing {
    /// Colecciones del servicio (cuando no se pidió una colección).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collections: Option<Vec<fakes_app::CollectionInfo>>,
    /// Registros de la colección pedida.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub records: Option<Vec<fakes_app::ResourceRecord>>,
}

#[derive(Serialize, JsonSchema)]
pub struct Cleared {
    pub deleted: u64,
}

pub fn all() -> Vec<Tool> {
    vec![
        tool(
            "listar_plantillas",
            "Listar plantillas de proveedores",
            "Servicios listos para instalar (MiFiel, Finkok, Stripe, SendGrid, AWS S3, Belvo, COPOMEX) con su categoría y si ya están instalados.",
            true,
            |app: Arc<FakesApp>, _a: Empty| async move {
                Ok::<_, fakes_app::AppError>(app.list_templates().await)
            },
        ),
        tool(
            "obtener_plantilla",
            "Obtener plantilla",
            "Bundle completo de una plantilla (servicio, endpoints, escenarios y workflows) para revisarlo o adaptarlo antes de instalarlo.",
            true,
            |_app: Arc<FakesApp>, a: TemplateRef| async move {
                fakes_app::templates::template_bundle(&a.template_id)
            },
        ),
        tool(
            "instalar_plantilla",
            "Instalar plantilla",
            "Crea el servicio de una plantilla en el espacio activo y lo inicia. Después usa obtener_estado para conocer su URL.",
            false,
            |app: Arc<FakesApp>, a: InstallArgs| async move {
                app.install_template(
                    &a.template_id,
                    TemplateInstallRequest {
                        on_conflict: a.on_conflict,
                        start: a.start,
                        dry_run: a.dry_run,
                    },
                )
                .await
            },
        ),
        tool(
            "listar_recursos",
            "Listar recursos simulados",
            "Sin `collection`, lista las colecciones persistentes del servicio con su número de registros; con `collection`, lista sus registros.",
            true,
            |app: Arc<FakesApp>, a: CollectionArgs| async move {
                Ok(match a.collection {
                    Some(c) => ResourceListing {
                        collections: None,
                        records: Some(
                            app.list_resources(&a.service_id, &c, a.limit, a.offset)
                                .await?,
                        ),
                    },
                    None => ResourceListing {
                        collections: Some(app.list_resource_collections(&a.service_id).await?),
                        records: None,
                    },
                })
            },
        ),
        tool(
            "obtener_recurso",
            "Obtener recurso",
            "Un registro de una colección persistente (p. ej. el documento que creó la simulación).",
            true,
            |app: Arc<FakesApp>, a: ResourceRef| async move {
                app.get_resource(&a.service_id, &a.collection, &a.record_id)
                    .await
            },
        ),
        tool(
            "guardar_recurso",
            "Guardar recurso",
            "Crea o reemplaza un registro (útil para preparar datos antes de una prueba: saldo, estado de un documento...).",
            false,
            |app: Arc<FakesApp>, a: SaveResource| async move {
                app.put_resource(&a.service_id, &a.collection, &a.record_id, a.data)
                    .await
            },
        ),
        tool(
            "eliminar_recurso",
            "Eliminar recurso",
            "Borra un registro de una colección.",
            false,
            |app: Arc<FakesApp>, a: ResourceRef| async move {
                app.delete_resource(&a.service_id, &a.collection, &a.record_id)
                    .await
                    .map(|_| done("recurso eliminado"))
            },
        ),
        tool(
            "vaciar_recursos",
            "Vaciar recursos",
            "Borra una colección completa o todas las del servicio.",
            false,
            |app: Arc<FakesApp>, a: ClearArgs| async move {
                Ok(Cleared {
                    deleted: app
                        .clear_resources(&a.service_id, a.collection.as_deref())
                        .await?,
                })
            },
        ),
    ]
}
