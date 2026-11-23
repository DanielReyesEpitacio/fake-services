# API REST de administración

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
| GET | `/events` | Eventos en vivo (SSE): `request`, `config_changed`, `runtime_changed`. |
| GET | `/schemas/{workspace\|service\|endpoint}` | JSON Schema de cada documento. |

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
