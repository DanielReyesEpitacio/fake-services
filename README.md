<p align="center">
  <img src="assets/brand/logo.svg" width="96" alt="Fake Services">
</p>

<h1 align="center">Fake Services</h1>
<p align="center"><strong>Servicios falsos. Integraciones reales.</strong></p>

Fake Services es un simulador de servicios externos **open source, gratuito y autoalojable**.
Te permite simular localmente las APIs de terceros que usa tu aplicación (MiFiel, Belvo,
COPOMEX, Stripe, AWS S3…) con endpoints, escenarios de error, retrasos y workflows
asíncronos con webhooks.

Está pensado **para agentes de IA primero**: todo lo que se puede hacer desde la interfaz se
puede hacer desde la API de administración (y, en la fase 3, desde un servidor MCP), sin
escribir código.

- Un solo ejecutable con servidor HTTP, motor de simulación, administración e interfaz web
  (se usa desde el navegador).
- Funciona sin Internet, sin Docker y sin dependencias, en Windows 10/11 y Linux.
- La configuración son archivos JSON que puedes versionar en Git y compartir entre sistemas.

## Estado

| Fase | Contenido | Estado |
|---|---|---|
| 1 | Espacios de trabajo, servicios, servidor central, endpoints, matching, respuestas dinámicas, historial, interfaz | ✅ Implementada |
| 2 | Motor de workflows: esperas, condiciones, webhooks con URLs dinámicas, reintentos, ejecuciones durables con línea de tiempo | ✅ Implementada |
| 3 | Importar y exportar servicios, espacios y workflows (JSON) | ✅ Implementada |
| 3 | Servidor MCP, importación de OpenAPI y Postman | Pendiente (la API REST ya existe) |
| 4 | Editor visual de workflows, plantillas de proveedores, recursos persistentes | Pendiente |

## Instalar

Fake Services es **un solo programa** (`fakes`) con el servidor, el motor y la interfaz web.
No necesita Docker, Node, bases de datos ni conexión a Internet. Al ejecutarlo se abre la
interfaz en tu navegador.

### Desde un paquete (sin compilar)

Descarga el paquete de tu sistema desde *Releases*:

| Sistema | Paquete | Instalar |
|---|---|---|
| Ubuntu, Debian y derivadas | `fake-services_<versión>_amd64.deb` | `sudo apt install ./fake-services_*.deb` |
| Fedora, RHEL y derivadas | `fake-services-<versión>.x86_64.rpm` | `sudo dnf install ./fake-services-*.rpm` |
| Cualquier Linux (sin sudo) | `fake-services-<versión>-linux-x86_64.tar.gz` | descomprime y ejecuta `./instalar.sh` |
| Windows 10/11 | `fake-services-<versión>-windows-x86_64.zip` | descomprime y haz doble clic en `instalar.cmd` |

Después abre **Fake Services** desde el menú de aplicaciones (o ejecuta `fakes`).

### Desde el código

Requiere Rust, Node.js 20+ y pnpm:

```bash
./scripts/install.sh          # Linux: compila e instala en ~/.local (sin sudo)
./scripts/build.sh --paquetes # genera .deb, .rpm y .tar.gz en dist/
```

```powershell
.\scripts\install.ps1         # Windows: compila e instala para tu usuario
.\scripts\build.ps1 -Paquete  # genera el .zip en dist\
```

**Actualizar es usar el mismo instalador.** `instalar.sh` / `instalar.cmd` detecta la versión
instalada y la reemplaza:

- Si Fake Services está corriendo, lo detiene de forma ordenada, instala y lo vuelve a iniciar en
  segundo plano.
- Limpia restos de instalaciones anteriores (app de escritorio, rutas viejas) y actualiza el
  inicio automático.
- Si encuentra además un paquete del sistema (`.deb`/`.rpm`), ofrece quitarlo para que quede una
  sola instalación.
- Tus espacios de trabajo, historial y ejecuciones se conservan; la base de datos se migra sola.

Con los paquetes `.deb`/`.rpm`, `apt install`/`dnf install` de la versión nueva también actualiza.

Para desinstalar: `./instalar.sh --desinstalar` o `instalar.ps1 -Desinstalar`.

## Uso

```bash
fakes              # inicia y abre la interfaz; si ya está corriendo, solo la abre
fakes serve --sin-navegador   # para servidores o terminales sin escritorio
fakes detener      # detiene la instancia en ejecución (también la de segundo plano)
fakes token        # token de la API para agentes y scripts
```

- Interfaz: `http://127.0.0.1:4010` · Simulaciones: `http://127.0.0.1:4000/<servicio>`
- En **Ajustes → Iniciar con el sistema**, Fake Services arranca en segundo plano al iniciar
  sesión.
- En **Importar y exportar** guardas tus servicios (con endpoints, escenarios y workflows) en un
  archivo JSON y los cargas en otra instalación. También desde la API: `GET /api/v1/export` y
  `POST /api/v1/import`.

Prueba el servicio de ejemplo:

```bash
curl http://127.0.0.1:4000/ejemplo/usuarios/42
curl -X POST http://127.0.0.1:4000/ejemplo/pagos -H 'content-type: application/json' -d '{"monto": 50000}'
```

Y un flujo asíncrono tipo MiFiel: responde `201` al instante y después envía dos webhooks
(«pendiente» a los 5 s y «firmado» a los 15 s) al `callback_url`:

```bash
curl -i -X POST http://127.0.0.1:4000/ejemplo/documentos/contrato-1/firmar \
  -H 'content-type: application/json' \
  -d '{"callback_url": "http://127.0.0.1:4000/ejemplo/webhooks", "firmante": "ana@example.com"}'
```

El header `x-fakes-executions` trae el id de la ejecución; síguela en *Ejecuciones*.

## Conceptos

- **Espacio de trabajo**: una carpeta con la configuración. Puede vivir dentro del repositorio
  de tu proyecto (`.fakes/`) para compartirla por Git.
- **Servicio**: un proveedor simulado. Tiene un prefijo (`/mifiel`) en el servidor central y,
  opcionalmente, su propio puerto.
- **Endpoint**: método + ruta (`POST /documents/{id}/sign`), lo que espera recibir (headers,
  query, body con JSON Schema) y una lista ordenada de **escenarios**.
- **Escenario**: reglas de coincidencia y la respuesta a devolver. Gana el primero cuyas reglas se
  cumplan. Puedes forzar uno desde la interfaz o con el header `x-fakes-scenario`. Un escenario
  puede iniciar **workflows**.
- **Workflow**: pasos asíncronos sin código: esperar, solicitud HTTP (webhook) con reintentos,
  condiciones con saltos, variables, emitir eventos, ejecutar otro workflow, simular un error o
  finalizar. Se inicia al recibir la solicitud, al completar la respuesta, manualmente o cada
  cierto tiempo.
- **Ejecución**: cada corrida de un workflow, con contexto aislado y línea de tiempo. Se guarda en
  SQLite tras cada paso: si cierras la app, al volver a abrirla continúa donde quedó.

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
cargo run -p fakes-cli -- serve --sin-navegador   # núcleo + API en :4010
cd ui && pnpm dev          # interfaz con recarga en caliente en :5173 (proxy a :4010)
```

## Licencia

MIT o Apache-2.0, a tu elección.
