# Arquitectura

## Stack

| Capa | Tecnología | Por qué |
|---|---|---|
| Núcleo | Rust 2024, Tokio, Axum | Binario único, sin runtime externo, bajo consumo de memoria (~15 MB en reposo) y concurrencia segura para muchas ejecuciones simultáneas. |
| Escritorio | Tauri 2 | WebView del sistema (WebView2 / WebKitGTK) en lugar de un Chromium empaquetado; genera NSIS/MSI, AppImage, DEB y RPM; incluye bandeja, inicio automático e instancia única. |
| Interfaz | Vue 3, TypeScript, Vite, Vuetify 4, CodeMirror 6 | Componentes maduros y temas nativos. Los *look and feels* (Cupertino Glass por defecto, Fluent, JetBrains, Atlas…) vienen del módulo de temas de la plataforma (`ui/src/platform/adapters/vue/themes`). La UI se compila y se embebe en el binario (`rust-embed`): funciona sin Internet. |
| Plantillas | MiniJinja | Sintaxis `{{ }}` conocida, sandbox, límite de ejecución (fuel) y sin acceso a E/S. |
| Validación | `jsonschema` | JSON Schema completo para validar bodies. |
| Persistencia | Archivos JSON + SQLite (`sqlx`, incluido en el binario) | JSON para la configuración (Git); SQLite en modo WAL para el historial y, en la fase 2, las ejecuciones y timers durables. |

Alternativas descartadas: Electron (más de 200 MB de RAM y sin modo sin interfaz natural), Go +
Wails (su empaquetado AppImage/DEB está menos maduro) y Docker (es requisito no depender de él).

## Componentes

```text
crates/
  fakes-domain    1. Dominio: tipos declarativos, rutas, validación estructural.
  fakes-config    2. Administración de configuraciones: JSON en disco, registro de espacios.
  fakes-server    3. Servidor HTTP: servidor central + puertos propios, CORS, historial.
  fakes-matching  4. Motor de coincidencia: rutas, autenticación, validación, escenarios.
  fakes-response  5. Motor de respuestas: plantillas y construcción de respuestas.
  (fase 2)        6. Motor de workflows.
  fakes-store     7. Persistencia de ejecución: SQLite (historial, clave-valor).
  fakes-api       8. API REST + entrega de la interfaz web.
  (fase 3)        9. Servidor MCP (usará fakes-app igual que la API REST).
  ui/            10. Interfaz gráfica (Vue + Vuetify).
  fakes-app       Servicios de aplicación: los casos de uso que comparten UI, REST y MCP.
  fakes-cli       Binario `fakes` (sin interfaz de escritorio).
apps/desktop      Binario de escritorio (Tauri).
```

Dependencias (sin ciclos, de abajo hacia arriba):

```text
domain ← config
domain ← matching ← server
domain ← response ← server
store  ← server
config, matching, response, store, server ← app ← api ← cli / desktop
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
- Fase 2: protección SSRF en webhooks verificando la IP *resuelta* (bloqueo de metadatos
  `169.254.169.254`, link-local y redes privadas configurables), límites de pasos por ejecución
  y de profundidad de sub-workflows.

## Preparado para las siguientes fases

- **Workflows** (fase 2): `fakes-matching::eval_operator` ya es reutilizable para condiciones;
  `IncomingRequest` está en el dominio para usarse como contexto de ejecución; SQLite alojará
  ejecuciones, línea de tiempo y timers durables para retomar tras reiniciar.
- **MCP** (fase 3): los tipos del dominio y las vistas derivan `JsonSchema`, de modo que los
  esquemas de entrada y salida de cada herramienta se generan de los mismos tipos que usa REST.
- **Recursos simulados persistentes** (fase 4): colecciones genéricas en SQLite por espacio.

## Interfaz y apariencia

```text
ui/src/
  app/
    configuration/   router y apariencia (catálogo de look and feels, preferencias)
    modules/         dashboard, services, endpoints, history, workspaces, settings
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
