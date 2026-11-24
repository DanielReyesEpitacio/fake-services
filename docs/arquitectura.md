# Arquitectura

## Stack

| Capa | Tecnología | Por qué |
|---|---|---|
| Núcleo | Rust 2024, Tokio, Axum | Binario único, sin runtime externo, bajo consumo de memoria (~15 MB en reposo) y concurrencia segura para muchas ejecuciones simultáneas. |
| Distribución | Binario único + paquetes `.deb`, `.rpm`, `.tar.gz` y `.zip` | Sin dependencias del sistema ni WebView: la interfaz se usa en el navegador. El CLI abre el navegador, y el autoinicio se integra con el sistema (XDG autostart en Linux, clave `Run` en Windows). |
| Interfaz | Vue 3, TypeScript, Vite, Vuetify 4, CodeMirror 6 | Componentes maduros y temas nativos. Los *look and feels* (Cupertino Glass por defecto, Fluent, JetBrains, Atlas…) vienen del módulo de temas de la plataforma (`ui/src/platform/adapters/vue/themes`). La UI se compila y se embebe en el binario (`rust-embed`): funciona sin Internet. |
| Plantillas | MiniJinja | Sintaxis `{{ }}` conocida, sandbox, límite de ejecución (fuel) y sin acceso a E/S. |
| Validación | `jsonschema` | JSON Schema completo para validar bodies. |
| Persistencia | Archivos JSON + SQLite (`sqlx`, incluido en el binario) | JSON para la configuración (Git); SQLite en modo WAL para el historial y las ejecuciones de workflows (estado durable y línea de tiempo). |

Alternativas descartadas: una app de escritorio (Tauri, Electron), que exige WebView o un
Chromium empaquetado y dependencias del sistema, sin aportar nada que el navegador no dé, y
Docker, porque es requisito no depender de él.

## Componentes

```text
crates/
  fakes-domain    1. Dominio: tipos declarativos, rutas, validación estructural.
  fakes-config    2. Administración de configuraciones: JSON en disco, registro de espacios.
  fakes-server    3. Servidor HTTP: servidor central + puertos propios, CORS, historial.
  fakes-matching  4. Motor de coincidencia: rutas, autenticación, validación, escenarios.
  fakes-response  5. Motor de respuestas: plantillas y construcción de respuestas.
  fakes-workflow  6. Motor de workflows: ejecuciones durables, webhooks con protección SSRF.
  fakes-store     7. Persistencia de ejecución: SQLite (historial, ejecuciones, recursos).
  fakes-api       8. API REST + entrega de la interfaz web + transporte HTTP del MCP.
  fakes-mcp       9. Servidor MCP (JSON-RPC): herramientas sobre los casos de uso de fakes-app.
  fakes-import       Conversión de OpenAPI/Swagger y Postman a servicios; detección de integraciones.
  ui/            10. Interfaz gráfica (Vue + Vuetify).
  fakes-app       Servicios de aplicación: los casos de uso que comparten UI, REST y MCP.
  fakes-cli       Binario `fakes`: arranque, apertura del navegador, empaquetado.
packaging/        Acceso del menú (Linux) e instalador de doble clic (Windows).
scripts/          Compilar, empaquetar e instalar.
```

Dependencias (sin ciclos, de abajo hacia arriba):

```text
domain ← config
domain ← matching ← server
domain ← response ← server
store  ← server
domain, matching, response, store ← workflow
import ← app
config, matching, response, store, server, workflow ← app ← api ← cli
app ← mcp ← api, cli
```

`fakes-domain` no depende de nada del sistema (ni HTTP, ni disco, ni plantillas).

## Flujo de una modificación

```text
UI / agente → API REST → FakesApp.update_endpoint()
  1. normaliza (ids generados) y valida (estructura + sintaxis de plantillas)
  2. escribe el archivo JSON de forma atómica (temporal + rename)
  3. recompila la configuración (rutas ordenadas, regex y JSON Schemas precompilados)
  4. la publica con un intercambio atómico (ArcSwap): las solicitudes en curso
     terminan con la versión anterior y las nuevas usan la nueva, sin reiniciar
  5. reconcilia los puertos abiertos (abre/cierra puertos propios de servicios)
  6. emite un evento `config_changed` para las interfaces abiertas
```

## Flujo de una solicitud simulada

```text
cliente → listener (central o puerto propio)
  → servicio por prefijo → ¿iniciado? → endpoint → auth → validación → escenario
  → plantilla → retraso → respuesta
  → historial (canal asíncrono → SQLite → evento `request`)
```

El historial nunca bloquea la respuesta: se escribe en una tarea aparte y, si se satura, se
descarta el registro con una advertencia.

## Seguridad

- Simulaciones y administración escuchan en `127.0.0.1` por defecto.
- La API de administración exige un token aleatorio de 48 caracteres guardado con permisos
  `0600`, compara en tiempo constante y valida el header `Host` contra DNS rebinding.
- Las plantillas no ejecutan código, no acceden al disco y tienen un límite de pasos.
- Las respuestas de tipo archivo solo pueden leer dentro de `files/` (se resuelven enlaces
  simbólicos y se rechaza `..`).
- Las expresiones regulares (crate `regex`) se evalúan en tiempo lineal: no hay ReDoS.
- Los headers sensibles se ocultan parcialmente en el historial (`Bearer abcd••••`).
- Límite de tamaño de body (10 MB por defecto) y retraso máximo de 5 minutos.
- Webhooks con protección SSRF: se verifica la IP *resuelta* (también en el resolvedor DNS del
  cliente, contra DNS rebinding); metadatos de nube, link-local y multicast bloqueados siempre;
  loopback y redes privadas configurables; sin redirecciones ni proxies del entorno.
- Límites del motor: pasos por ejecución (contra bucles infinitos), 5 niveles de workflows
  anidados, 256 ejecuciones simultáneas, esperas de hasta 24 h, tiempo límite HTTP de 120 s y
  hasta 10 reintentos.

## Motor de workflows

- Cada ejecución corre en su propia tarea con un contexto JSON aislado.
- Tras cada transición se guarda en SQLite el estado completo: paso actual, fase, intento,
  `wake_at` de las esperas y el contexto. La definición del workflow se copia al iniciar, así que
  editarlo no afecta ejecuciones en curso.
- Al arrancar (o al volver a un espacio), las ejecuciones sin terminar se retoman; las esperas
  respetan la hora original. Un paso que quedó a la mitad se repite (al menos una vez).
- Las esperas cancelables usan `tokio::select!`; la cancelación del usuario marca la ejecución
  como cancelada, mientras que un apagado la deja pendiente para retomarla.
- El servidor de simulación no depende del motor: recibe un `WorkflowStarter` (inversión de
  dependencias) que la capa de aplicación conecta con `fakes-workflow`.

## MCP, plantillas y recursos

- **MCP**: implementado a mano sobre JSON-RPC 2.0 (`initialize`, `tools/list`, `tools/call`,
  `ping`). Los tipos del dominio y las vistas derivan `JsonSchema`, así los esquemas de entrada y
  salida de cada herramienta se generan de los mismos tipos que usa REST. `fakes mcp` habla por
  stdio y, si ya hay una instancia corriendo, reenvía cada mensaje a su `POST /mcp`: no hay dos
  procesos escribiendo la misma configuración.
- **Importación de especificaciones**: `fakes-import` convierte OpenAPI/Postman en un bundle y la
  importación reutiliza el mismo camino que un archivo exportado (validación completa, conflictos,
  `dryRun`).
- **Plantillas de proveedores**: bundles JSON en `templates/`, incluidos en el binario con
  `include_str!`. Instalar una plantilla es importar su bundle; no hay código por proveedor.
- **Recursos persistentes**: tabla `resources` en SQLite (espacio, servicio, colección, id → JSON).
  Son estado de ejecución, no configuración; se borran con su servicio y pueden ir en un bundle.
- **Escenarios avanzados**: el servidor cuenta las llamadas por endpoint en memoria (se reinician
  al cambiar la configuración) y la probabilidad se evalúa después de las reglas.

## Interfaz y apariencia

```text
ui/src/
  app/
    configuration/   router y apariencia (catálogo de look and feels, preferencias)
    modules/         dashboard, services, endpoints, workflows (lista y diagrama con Vue Flow),
                     executions, resources, templates, transfer, history, workspaces, settings
    shared/          layout, componentes, estado en vivo, estilos
  infrastructure/    cliente de la API REST de administración
  platform/          módulo de temas (look and feels de Vuetify)
```

- El usuario elige **look and feel**, **modo** (claro, oscuro o sistema) y **color de acento**
  en *Apariencia*. Se guarda en `localStorage`.
- Cada look and feel se carga con `import()` dinámico antes de crear Vuetify, así solo se
  descarga su CSS. Cambiarlo recarga la interfaz (sus defaults se aplican al crear Vuetify);
  el modo cambia en caliente con `theme.change()`.
- Los look and feels usan campos `singleLine`. Para que las etiquetas sigan visibles con valor,
  la app usa `FTextField`, `FSelect`, `FCombobox` y `FTextarea` (etiqueta externa asociada al
  input por `id`) en lugar de los componentes `V*` directamente.
- No se fijan variantes, densidades ni radios en las vistas: los define el look and feel.
