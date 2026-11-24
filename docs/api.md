# API REST de administración y servidor MCP

Base: `http://127.0.0.1:4010/api/v1`

La interfaz web usa esta misma API: un agente tiene exactamente las mismas capacidades.

## Autenticación

Todas las rutas exigen `Authorization: Bearer <token>`. Obtén el token con:

```bash
fakes token
```

El token se genera la primera vez y se guarda en la carpeta de datos (`fakes paths`) con permisos
solo para tu usuario. Para `EventSource` se acepta `?token=`. Por seguridad, el servidor rechaza
peticiones cuyo header `Host` no sea `127.0.0.1`, `localhost` o `[::1]` (protección contra DNS
rebinding).

## Errores

```json
{ "error": "validation_failed", "message": "la configuración tiene errores",
  "issues": [{ "field": "scenarios[0].response.status", "message": "código HTTP entre 100 y 599" }] }
```

| HTTP | `error` |
|---|---|
| 400 | `bad_request` (JSON mal formado o tipos incorrectos) |
| 404 | `not_found` |
| 409 | `conflict` |
| 422 | `validation_failed` (con `issues`) |
| 500 | `config_error`, `store_error` |

## Rutas

### Estado

| Método | Ruta | Descripción |
|---|---|---|
| GET | `/status` | Versión, espacio activo, URLs, puertos, contadores y problemas de carga. |
| POST | `/reload` | Relee el espacio activo desde disco. |
| GET | `/events` | Eventos en vivo (SSE): `request`, `config_changed`, `runtime_changed`, `execution`, `workflow_event`. |
| GET | `/schemas/{workspace\|service\|endpoint\|workflow\|bundle}` | JSON Schema de cada documento. |

### Espacios de trabajo

| Método | Ruta | Body |
|---|---|---|
| GET | `/workspaces` | — |
| POST | `/workspaces` | `{ "name", "id"?, "path"?, "activate"?, ...Workspace }` |
| POST | `/workspaces/register` | `{ "path", "activate"? }`: registra una carpeta existente. |
| POST | `/workspaces/{id}/activate` | — |
| DELETE | `/workspaces/{id}` | Quita del registro (no borra archivos). |
| GET / PUT / PATCH | `/workspace` | Espacio activo. `PATCH` usa JSON Merge Patch. |

### Servicios

| Método | Ruta | Body |
|---|---|---|
| GET | `/services` | — |
| POST | `/services` | `Service` (`id` y `prefix` se generan si faltan; puede incluir `endpoints`). |
| GET / PUT / PATCH / DELETE | `/services/{sid}` | `PUT` reemplaza la configuración (conserva endpoints si se omiten). |
| POST | `/services/{sid}/start`, `/services/{sid}/stop` | — |
| POST | `/services/start-all`, `/services/stop-all` | — |

### Endpoints

| Método | Ruta | Body |
|---|---|---|
| GET | `/services/{sid}/endpoints` | — |
| POST | `/services/{sid}/endpoints` | `Endpoint` (`id` y los `id` de escenarios se generan si faltan). |
| POST | `/services/{sid}/endpoints/validate` | `Endpoint`: valida sin guardar. |
| GET / PUT / PATCH / DELETE | `/services/{sid}/endpoints/{eid}` | — |
| POST | `/services/{sid}/endpoints/{eid}/enable`, `.../disable` | — |
| PUT | `/services/{sid}/endpoints/{eid}/active-scenario` | `{ "scenarioId": "rechazada" \| null }` |

### Workflows

| Método | Ruta | Body |
|---|---|---|
| GET | `/services/{sid}/workflows` | — |
| POST | `/services/{sid}/workflows` | `Workflow` (`id` y los `id` de pasos se generan si faltan). |
| POST | `/services/{sid}/workflows/validate` | `Workflow`: valida sin guardar. |
| GET / PUT / PATCH / DELETE | `/services/{sid}/workflows/{wid}` | `DELETE` responde 409 si un escenario u otro workflow lo usa. |
| POST | `/services/{sid}/workflows/{wid}/run` | `{ "input": {...}, "context": {"request": {...}, "response": {...}} }` → 202 con la ejecución. |

### Ejecuciones

| Método | Ruta | Parámetros |
|---|---|---|
| GET | `/executions` | `serviceId`, `workflowId`, `status` (p. ej. `running,waiting`), `beforeMs`, `limit` |
| GET | `/executions/{id}` | Estado completo, contexto y línea de tiempo. |
| POST | `/executions/{id}/cancel` | — |
| DELETE | `/executions` | Borra las terminadas. |

Los eventos en vivo (`/events`) incluyen `execution` (cambio de estado) y `workflow_event`
(pasos `emit`).

### Importar y exportar

| Método | Ruta | Parámetros / body |
|---|---|---|
| GET | `/export` | `services` (ids separados por comas; vacío = todos), `workspace=true`, `files=true`, `resources=true`. Devuelve el archivo como descarga. |
| POST | `/import` | `{ "bundle": {...}, "mode": "merge"\|"newWorkspace", "onConflict": "rename"\|"replace"\|"skip", "workspaceName"?, "dryRun"? }` → reporte. Hasta 64 MB. |
| POST | `/import/spec` | `{ "content": "<OpenAPI o Postman, JSON o YAML>", "format"?: "openapi"\|"postman", "name"?, "prefix"?, "onConflict"?, "dryRun"? }` → reporte con `serviceId` y `warnings`. |
| GET | `/validate` | Valida toda la configuración del espacio activo (servicios, endpoints, workflows y archivos con errores). |
| POST | `/detect` | `{ "path": "/ruta/al/proyecto" }` → integraciones detectadas (dependencias, URLs y variables de entorno) con evidencia y plantilla sugerida. |

El bundle tiene `format: "fake-services"`, `formatVersion`, `services` (completos, con endpoints y
workflows), `workspace` opcional, `files` (ruta → base64) y `resources` (servicio → colección →
registros). Esquema: `/schemas/bundle`. La
importación valida todo antes de escribir; con `dryRun` solo informa qué pasaría. Los workflows
sueltos se importan con `POST /services/{sid}/workflows`.

Al importar un OpenAPI 3.x o Swagger 2.0 se crea un endpoint por operación, con sus parámetros,
JSON Schema del body y un escenario por respuesta documentada: la primera 2xx es la
predeterminada y las demás se fuerzan con `x-fakes-scenario: <código>` (p. ej. `404`). Los
ejemplos se usan tal cual; si no hay, se generan desde el esquema. En Postman v2, cada ejemplo
guardado es un escenario y `:id` / `{{var}}` en la ruta se convierten en parámetros.

### Plantillas de proveedores

| Método | Ruta | Body |
|---|---|---|
| GET | `/templates` | Catálogo (`id`, `name`, `category`, `tags`, `installed`...). |
| GET | `/templates/{id}` | Bundle completo de la plantilla. |
| POST | `/templates/{id}/install` | `{ "onConflict"?: "rename"\|"replace"\|"skip", "start"?: true, "dryRun"? }` → reporte con `serviceId`. |

### Recursos simulados

Colecciones persistentes por servicio (se guardan en SQLite y sobreviven reinicios).

| Método | Ruta | Body |
|---|---|---|
| GET | `/services/{sid}/resources` | Colecciones con su número de registros. |
| DELETE | `/services/{sid}/resources` | Borra todas las colecciones del servicio. |
| GET | `/services/{sid}/resources/{col}` | Registros (`limit`, `offset`; máx. 1000). |
| DELETE | `/services/{sid}/resources/{col}` | Vacía la colección. |
| GET / PUT / DELETE | `/services/{sid}/resources/{col}/{id}` | `PUT` crea o reemplaza con el body JSON. El `id` puede contener `/` (p. ej. llaves de S3). |

### Sistema

| Método | Ruta | Body |
|---|---|---|
| GET | `/system/autostart` | — |
| PUT | `/system/autostart` | `{ "enabled": true }`: iniciar Fake Services al iniciar sesión. |
| POST | `/system/shutdown` | Detiene Fake Services de forma ordenada (202). Lo usan `fakes detener` y los instaladores. |

### Historial

| Método | Ruta | Parámetros |
|---|---|---|
| GET | `/requests` | `serviceId`, `endpointId`, `errorsOnly`, `search`, `beforeMs`, `limit` (máx. 1000) |
| GET | `/requests/{id}` | Detalle con headers y bodies. |
| DELETE | `/requests` | `serviceId` opcional. |

## Ejemplo: simular MiFiel en tres llamadas

```bash
T=$(fakes token); API=http://127.0.0.1:4010/api/v1

curl -s -X POST $API/services -H "Authorization: Bearer $T" -H 'content-type: application/json' \
  -d '{"name": "MiFiel", "auth": {"type": "bearer"}}'

curl -s -X POST $API/services/mifiel/endpoints -H "Authorization: Bearer $T" -H 'content-type: application/json' -d '{
  "method": "POST", "path": "/documents/{id}/sign",
  "scenarios": [{"name": "Firma exitosa", "response": {"status": 201,
    "body": {"type": "json", "content": {"id": "{{ request.params.id }}", "status": "pending"}}}}]
}'

curl -s -X POST http://127.0.0.1:4000/mifiel/documents/doc_1/sign -H 'Authorization: Bearer x'
```

## Servidor MCP

Fake Services implementa el [Model Context Protocol](https://modelcontextprotocol.io) (JSON-RPC
2.0, versiones `2025-06-18`, `2025-03-26` y `2024-11-05`) con dos transportes:

- **stdio**: `fakes mcp`. Si hay una instancia corriendo, la usa a través de `POST /mcp`; si no,
  arranca una dentro del proceso (con `--admin`, además expone la interfaz y la API).
- **HTTP**: `POST http://127.0.0.1:4010/mcp` con `Authorization: Bearer <token>`. Las
  notificaciones responden `202`; `initialize` devuelve el header `mcp-session-id`.

```bash
claude mcp add fake-services -- fakes mcp
```

Cada herramienta llama al mismo caso de uso que la interfaz y la API REST, con esquemas de
entrada estrictos (sin campos desconocidos) y de salida (`structuredContent`). Los errores de
validación vuelven con `isError: true` y la lista de campos a corregir.

| Grupo | Herramientas |
|---|---|
| Estado y espacios | `obtener_estado`, `listar_espacios_trabajo`, `crear_espacio_trabajo`, `obtener_espacio_trabajo`, `activar_espacio_trabajo` |
| Servicios | `listar_servicios`, `obtener_servicio`, `crear_servicio`, `actualizar_servicio`, `eliminar_servicio`, `iniciar_servicio`, `detener_servicio` |
| Endpoints | `listar_endpoints`, `obtener_endpoint`, `crear_endpoint`, `actualizar_endpoint`, `eliminar_endpoint`, `habilitar_endpoint`, `deshabilitar_endpoint`, `forzar_escenario`, `probar_endpoint` |
| Workflows | `listar_workflows`, `obtener_workflow`, `crear_workflow`, `actualizar_workflow`, `eliminar_workflow`, `validar_workflow`, `ejecutar_workflow`, `cancelar_workflow` |
| Ejecuciones e historial | `listar_ejecuciones`, `obtener_ejecucion`, `obtener_linea_tiempo`, `listar_solicitudes`, `obtener_solicitud` |
| Importar y exportar | `importar_openapi`, `importar_coleccion_postman`, `importar_paquete`, `exportar`, `validar_configuracion`, `detectar_integraciones_proyecto`, `obtener_esquema` |
| Plantillas | `listar_plantillas`, `obtener_plantilla`, `instalar_plantilla` |
| Recursos | `listar_recursos`, `obtener_recurso`, `guardar_recurso`, `eliminar_recurso`, `vaciar_recursos` |

Las actualizaciones (`actualizar_*`) reciben un JSON Merge Patch en `changes`.
