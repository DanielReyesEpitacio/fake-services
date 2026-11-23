# Configuración

La configuración declarativa es la fuente de verdad. La interfaz, la API REST y (más adelante)
el MCP leen y escriben estos mismos archivos.

## Estructura de un espacio de trabajo

```text
mi-proyecto/.fakes/
  fakes.workspace.json
  files/                                  # archivos para respuestas de tipo "file"
  services/
    mifiel/
      service.json
      endpoints/
        post-documents-id-sign.json
```

Los archivos usan JSON con indentación de 2 espacios y orden estable, así los diffs en Git son
legibles. Si un archivo tiene errores, se omite y se reporta en la pantalla de inicio; el resto
del espacio sigue funcionando. Tras editar archivos a mano (o hacer `git pull`), usa
**Recargar desde disco** o `POST /api/v1/reload`.

El esquema JSON de cada documento se puede consultar en
`GET /api/v1/schemas/{workspace|service|endpoint}`.

## `fakes.workspace.json`

```json
{
  "schemaVersion": 1,
  "id": "pagos",
  "name": "Proyecto Pagos",
  "server": { "host": "127.0.0.1", "port": 4000, "cors": true, "maxBodyBytes": 10485760 },
  "variables": { "CALLBACK_BASE": "http://localhost:3000" },
  "history": {
    "enabled": true,
    "maxEntries": 5000,
    "redactHeaders": ["authorization", "cookie", "x-api-key"],
    "maxBodyCaptureBytes": 262144
  }
}
```

## `service.json`

```json
{
  "schemaVersion": 1,
  "id": "mifiel",
  "name": "MiFiel",
  "description": "Firma electrónica",
  "upstreamUrl": "https://app.mifiel.com/api/v1",
  "prefix": "/mifiel",
  "port": 4101,
  "autoStart": true,
  "variables": { "API_VERSION": "v1" },
  "auth": { "type": "bearer", "token": null }
}
```

| Campo | Descripción |
|---|---|
| `prefix` | Ruta base en el servidor central (`http://127.0.0.1:4000/mifiel`). |
| `port` | Opcional. El servicio también responde en `http://127.0.0.1:4101/` sin prefijo. |
| `autoStart` | Si se inicia al abrir la aplicación (por defecto `true`). |
| `auth` | `none`, `bearer` (`token`), `apiKey` (`header`, `value`) o `basic` (`username`, `password`). Un valor nulo acepta cualquiera. Si falla, la simulación responde 401. |

## Endpoint

```json
{
  "schemaVersion": 1,
  "id": "post-documents-id-sign",
  "name": "Solicitar firma",
  "enabled": true,
  "method": "POST",
  "path": "/documents/{id}/sign",
  "request": {
    "headers": [{ "name": "Authorization", "required": true, "pattern": "Bearer .+" }],
    "query": [],
    "pathParams": [{ "name": "id", "required": true, "pattern": "doc_\\w+" }],
    "body": {
      "kind": "json",
      "required": true,
      "schema": {
        "type": "object",
        "required": ["callback_url", "signers"],
        "properties": { "callback_url": { "type": "string", "format": "uri" } }
      }
    }
  },
  "scenarios": [
    {
      "id": "rechazada",
      "name": "Firma rechazada",
      "rules": [{ "source": "body", "key": "signers.0.email", "operator": "contains", "value": "rechazo" }],
      "response": { "status": 409, "body": { "type": "json", "content": { "status": "rejected" } } }
    },
    {
      "id": "ok",
      "name": "Firma exitosa",
      "response": {
        "status": 201,
        "headers": { "x-request-id": "{{ uuid() }}" },
        "body": { "type": "json", "content": { "id": "{{ request.params.id }}", "status": "pending" } },
        "delayMs": 300
      }
    }
  ],
  "activeScenario": null,
  "onValidationError": null
}
```

### Rutas

- Parámetros: `/documents/{id}`.
- Comodín final: `/bucket/{*key}` captura `a/b/c.txt`.
- Si varias rutas coinciden, gana la más específica (más segmentos literales). Un método concreto
  gana a `*`.
- Si la ruta existe con otro método, la respuesta es `405` con el header `Allow`.

### Orden de procesamiento

1. Se busca el servicio por prefijo (o por puerto propio). Si está detenido, responde `503`.
2. Se busca el endpoint (`404` o `405` si no hay).
3. Se verifica la autenticación del servicio (`401`).
4. Se valida la solicitud contra `request` (`422` u `onValidationError`).
5. Se elige el escenario: header `x-fakes-scenario` → `activeScenario` → primer escenario
   habilitado cuyas reglas se cumplen. Si ninguno aplica, responde `501`.
6. Se renderiza la respuesta, se aplica el retraso y se guarda en el historial.

Todas las respuestas de error generadas por el simulador tienen la forma
`{"error": "...", "message": "...", "source": "fake-services"}` para distinguirlas de las
respuestas configuradas.

### Reglas

| `source` | `key` |
|---|---|
| `header` | Nombre del header (sin distinguir mayúsculas). |
| `query` | Nombre del parámetro. |
| `path` | Nombre del parámetro de ruta. |
| `body` | Ruta con puntos (`user.items.0.id`). Vacía = body completo. |
| `method` | — |
| `variable` | Nombre de la variable del espacio/servicio. |

Operadores: `equals`, `notEquals`, `contains`, `notContains`, `exists`, `notExists`,
`greaterThan`, `lessThan`, `regex`. Las comparaciones son tolerantes (`"5"` es igual a `5`).
`ruleMode` puede ser `all` (por defecto) o `any`.

### Bodies de respuesta

| `type` | Campos |
|---|---|
| `json` | `content`: cualquier JSON; cada texto es plantilla. |
| `text` | `content` (plantilla), `contentType` opcional (se detecta XML/HTML). |
| `binary` | `base64`, `contentType`. |
| `file` | `path` relativo a `files/` (admite plantillas), `contentType` opcional. |
| `none` | Sin body. |

## Plantillas

Motor: [MiniJinja](https://docs.rs/minijinja) en modo seguro (sin acceso a disco ni red, con
límite de ejecución). Los datos faltantes se resuelven como vacíos.

### Contexto disponible

| Variable | Contenido |
|---|---|
| `request.method`, `request.path`, `request.routePath` | Método, ruta completa y ruta sin prefijo. |
| `request.params.X` | Parámetros de ruta. |
| `request.query.X` / `request.queryAll.X` | Primer valor / todos los valores. |
| `request.headers['x-name']` | Headers en minúsculas. |
| `request.body` | Body interpretado (JSON, formulario, multipart o texto). |
| `request.rawBody`, `request.ip`, `request.contentType` | Body original, IP y Content-Type. |
| `vars.X` | Variables del espacio y del servicio. |
| `service.id`, `service.name`, `service.url` | Datos del servicio. |
| `endpoint.id`, `scenario.id`, `workspace.id` | Identificadores. |
| `validation.errors` | Solo en `onValidationError`. |

### Funciones

`uuid()`, `now()` / `now('%Y-%m-%d')`, `timestamp()`, `timestamp_ms()`,
`random_int(min, max)`, `random_float(min, max)`, `random_string(n)`, `random_hex(n)`,
`pick([...])` y `fake(tipo)`.

Tipos de `fake`: `first_name`, `last_name`, `full_name`, `email`, `phone`, `company`, `city`,
`street`, `zip`, `rfc`, `curp`, `clabe`, `iban`, `word`, `sentence`.

### Filtros

Todos los de MiniJinja (`upper`, `default`, `length`, `tojson`…) más `b64encode`, `b64decode` y
`fromjson`.

```jinja
{{ request.query.nombre | default('anónimo') | upper }}
{{ request.body.items | length }}
{% if request.body.monto > 1000 %}revisión{% else %}aprobado{% endif %}
```
