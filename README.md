<p align="center">
  <img src="assets/brand/logo.svg" width="96" alt="Fake Services">
</p>

<h1 align="center">Fake Services</h1>
<p align="center"><strong>Servicios falsos. Integraciones reales.</strong></p>

Fake Services es un simulador de servicios externos **open source, gratuito y autoalojable**.
Te permite simular localmente las APIs de terceros que usa tu aplicación (MiFiel, Belvo,
COPOMEX, Stripe, AWS S3…) con endpoints, escenarios de error, retrasos y, pronto, workflows
asíncronos con webhooks.

Está pensado **para agentes de IA primero**: todo lo que se puede hacer desde la interfaz se
puede hacer desde la API de administración (y, en la fase 3, desde un servidor MCP), sin
escribir código.

- Un solo ejecutable con servidor HTTP, motor de simulación, administración e interfaz web.
- Funciona sin Internet y sin Docker, en Windows 10/11 y Linux.
- La configuración son archivos JSON que puedes versionar en Git y compartir entre sistemas.

## Estado

| Fase | Contenido | Estado |
|---|---|---|
| 1 | Espacios de trabajo, servicios, servidor central, endpoints, matching, respuestas dinámicas, historial, interfaz | ✅ Implementada |
| 2 | Motor de workflows: retrasos, condiciones, webhooks, reintentos, ejecuciones | Pendiente |
| 3 | Servidor MCP, importación OpenAPI/Postman, exportación | Pendiente (la API REST ya existe) |
| 4 | Editor visual de workflows, plantillas de proveedores, recursos persistentes | Pendiente |

## Inicio rápido

### Con el CLI (sin interfaz de escritorio)

```bash
cargo build --release -p fakes-cli
./target/release/fakes serve
```

```
  Fake Services 0.1.0
  Simulaciones:   http://127.0.0.1:4000
    ● Ejemplo              http://127.0.0.1:4000/ejemplo
  Administración: http://127.0.0.1:4010
```

Abre `http://127.0.0.1:4010` para usar la interfaz web y prueba el servicio de ejemplo:

```bash
curl http://127.0.0.1:4000/ejemplo/usuarios/42
curl -X POST http://127.0.0.1:4000/ejemplo/pagos -H 'content-type: application/json' -d '{"monto": 50000}'
```

### Aplicación de escritorio

La app de escritorio (Tauri) incluye el mismo núcleo, una ventana nativa, icono en la bandeja e
inicio automático con el sistema. Para compilarla en Linux necesitas las dependencias de WebKitGTK:

```bash
sudo apt install libwebkit2gtk-4.1-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev \
  librsvg2-dev libayatana-appindicator3-dev build-essential
cargo install tauri-cli --version '^2'
cd ui && pnpm install && pnpm build && cd ..
cd apps/desktop && cargo tauri build
```

Esto genera instaladores NSIS/MSI (Windows) y AppImage/DEB/RPM (Linux).

## Conceptos

- **Espacio de trabajo**: una carpeta con la configuración. Puede vivir dentro del repositorio
  de tu proyecto (`.fakes/`) para compartirla por Git.
- **Servicio**: un proveedor simulado. Tiene un prefijo (`/mifiel`) en el servidor central y,
  opcionalmente, su propio puerto.
- **Endpoint**: método + ruta (`POST /documents/{id}/sign`), lo que espera recibir (headers,
  query, body con JSON Schema) y una lista ordenada de **escenarios**.
- **Escenario**: reglas de coincidencia y la respuesta a devolver. Gana el primero cuyas reglas se
  cumplan. Puedes forzar uno desde la interfaz o con el header `x-fakes-scenario`.

## Plantillas en respuestas

Headers y bodies admiten plantillas seguras (sin ejecución de código):

```json
{
  "id": "{{ request.params.id }}",
  "callback": "{{ request.body.callback_url }}",
  "signers": "{{ request.body.signers }}",
  "created_at": "{{ now() }}",
  "folio": "{{ uuid() }}",
  "rfc": "{{ fake('rfc') }}"
}
```

Si un texto es solo `{{ expresión }}` se conserva el tipo (número, lista, objeto). Consulta
[docs/configuracion.md](docs/configuracion.md) para la referencia completa.

## Documentación

- [Configuración y plantillas](docs/configuracion.md)
- [API REST de administración](docs/api.md)
- [Arquitectura y decisiones técnicas](docs/arquitectura.md)
- [Marca](docs/marca.md)

## Desarrollo

```bash
cargo test                 # pruebas del núcleo
cargo run -p fakes-cli     # núcleo + API en :4010
cd ui && pnpm dev          # interfaz con recarga en caliente en :5173 (proxy a :4010)
```

## Licencia

MIT o Apache-2.0, a tu elección.
