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
puede hacer desde el servidor MCP o la API de administración, sin escribir código. Un agente
puede analizar tu proyecto, detectar qué proveedores usa, instalar sus simulaciones y probarlas.

- Un solo ejecutable con servidor HTTP, motor de simulación, administración e interfaz web
  (se usa desde el navegador).
- Funciona sin Internet, sin Docker y sin dependencias, en Windows 10/11 y Linux.
- La configuración son archivos JSON que puedes versionar en Git y compartir entre sistemas.

## Estado

| Fase | Contenido | Estado |
|---|---|---|
| 1 | Espacios de trabajo, servicios, servidor central, endpoints, matching, respuestas dinámicas, historial, interfaz | ✅ Implementada |
| 2 | Motor de workflows: esperas, condiciones, webhooks con URLs dinámicas, reintentos, ejecuciones durables con línea de tiempo | ✅ Implementada |
| 3 | Servidor MCP (stdio y HTTP), importación de OpenAPI/Swagger y Postman, importar y exportar, detección de integraciones | ✅ Implementada |
| 4 | Editor visual de workflows, plantillas de proveedores, recursos persistentes, escenarios avanzados (probabilidad, número de llamada) | ✅ Implementada |

## Instalar

Fake Services es **un solo programa** (`fakes`) con el servidor, el motor y la interfaz web.
Una vez compilado no necesita Docker, Node, bases de datos ni conexión a Internet. Al ejecutarlo
se abre la interfaz en tu navegador.

No hay paquetes precompilados: se instala descargando el repositorio y compilándolo. Los
scripts hacen todo (compilar la interfaz y el binario, e instalar) con un solo comando.

### Requisitos para compilar

| Herramienta | Versión | Cómo obtenerla |
|---|---|---|
| Rust | estable | <https://rustup.rs> (en Windows, el toolchain MSVC con «Desarrollo para el escritorio con C++») |
| Node.js | 20 o superior | <https://nodejs.org> |
| pnpm | 9 o superior | `npm install -g pnpm` |
| Git | cualquiera | para descargar el repositorio |

### Linux

```bash
git clone <url-del-repositorio> fake-services
cd fake-services
./scripts/install.sh
```

Instala en tu carpeta personal (`~/.local/bin/fakes`), sin `sudo`, y crea el acceso
**Fake Services** en el menú de aplicaciones.

### Windows 10/11 (PowerShell)

```powershell
git clone <url-del-repositorio> fake-services
cd fake-services
powershell -ExecutionPolicy Bypass -File .\scripts\install.ps1
```

Instala para tu usuario (`%LOCALAPPDATA%\Programs\FakeServices`), agrega `fakes` al PATH y crea
el acceso **Fake Services** en el menú Inicio. No requiere permisos de administrador.

### Actualizar

```bash
git pull
./scripts/install.sh            # Windows: .\scripts\install.ps1
```

Es el mismo comando que para instalar. El instalador detecta la versión instalada y la reemplaza:

- Si Fake Services está corriendo, lo detiene de forma ordenada, instala y lo vuelve a iniciar en
  segundo plano.
- Limpia restos de instalaciones anteriores y actualiza el inicio automático.
- Tus espacios de trabajo, historial y ejecuciones se conservan; la base de datos se migra sola.

### Desinstalar

```bash
./scripts/install.sh --desinstalar      # Windows: .\scripts\install.ps1 -Desinstalar
```

### Generar paquetes para otras máquinas (opcional)

Si quieres distribuirlo a equipos sin herramientas de desarrollo, compílalo una vez y lleva el
paquete:

```bash
./scripts/build.sh --paquetes   # Linux: dist/*.deb, dist/*.rpm y dist/*.tar.gz (con instalar.sh)
.\scripts\build.ps1 -Paquete    # Windows: dist\*.zip (con instalar.cmd)
```

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
- En **Plantillas** instalas con un clic un proveedor listo para usar (ver abajo) o analizas la
  carpeta de tu proyecto para saber qué servicios externos usa.
- En **Importar y exportar** guardas tus servicios (con endpoints, escenarios, workflows y, si
  quieres, sus recursos) en un archivo JSON y los cargas en otra instalación. Ahí mismo importas
  un **OpenAPI/Swagger** (JSON o YAML) o una **colección de Postman**: cada respuesta documentada
  se convierte en un escenario.

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

## Conectar un agente (MCP)

Fake Services incluye un servidor [MCP](https://modelcontextprotocol.io) con más de 45
herramientas: crear servicios, endpoints y workflows, instalar plantillas, importar OpenAPI,
probar endpoints, leer el historial y las ejecuciones, y manejar recursos. Las mismas reglas de
validación que la interfaz: nada de código generado.

**Por stdio** (recomendado). Si Fake Services ya está corriendo, `fakes mcp` se conecta a esa
instancia; si no, arranca una dentro del mismo proceso.

```bash
claude mcp add fake-services -- fakes mcp
```

Para otros clientes (Claude Desktop, Cursor, VS Code…), el comando es `fakes` con el
argumento `mcp`:

```json
{ "mcpServers": { "fake-services": { "command": "fakes", "args": ["mcp"] } } }
```

**Por HTTP**: `http://127.0.0.1:4010/mcp` con el header `Authorization: Bearer <token>`
(el token se obtiene con `fakes token`).

```bash
claude mcp add --transport http fake-services http://127.0.0.1:4010/mcp \
  --header "Authorization: Bearer $(fakes token)"
```

Un flujo típico para el agente: `detectar_integraciones_proyecto` → `instalar_plantilla` o
`importar_openapi` → `obtener_estado` (URLs) → `probar_endpoint` → `listar_solicitudes` /
`listar_ejecuciones`.

## Plantillas de proveedores

| Plantilla | Qué simula |
|---|---|
| `mifiel` | Firma electrónica: documentos persistentes, firma asíncrona con webhook al `callback_url`, rechazo forzable, PDF firmado |
| `finkok` | PAC CFDI 4.0 (SOAP): timbrado, cancelación, estatus y utilidades, con sus WSDL y códigos de error |
| `stripe` | Clientes e intents de pago (form-urlencoded), tarjetas rechazadas y webhook `payment_intent.succeeded` |
| `sendgrid` | `POST /v3/mail/send`: guarda cada correo para revisarlo y envía el evento `delivered` |
| `aws-s3` | Objetos reales (binarios) con PUT, GET, HEAD, DELETE y ListObjectsV2 en estilo path |
| `belvo` | Links persistentes, cuentas y transacciones; credenciales inválidas con el usuario `bnk_error` |
| `copomex` | Códigos postales, estados y municipios de México |

Se instalan desde **Plantillas**, con `POST /api/v1/templates/<id>/install` o con la herramienta
MCP `instalar_plantilla`. Después puedes editarlas como cualquier otro servicio.

## Conceptos

- **Espacio de trabajo**: una carpeta con la configuración. Puede vivir dentro del repositorio
  de tu proyecto (`.fakes/`) para compartirla por Git.
- **Servicio**: un proveedor simulado. Tiene un prefijo (`/mifiel`) en el servidor central y,
  opcionalmente, su propio puerto.
- **Endpoint**: método + ruta (`POST /documents/{id}/sign`), lo que espera recibir (headers,
  query, body con JSON Schema) y una lista ordenada de **escenarios**.
- **Escenario**: reglas de coincidencia y la respuesta a devolver. Gana el primero cuyas reglas se
  cumplan. Puedes forzar uno desde la interfaz o con el header `x-fakes-scenario`. Un escenario
  puede iniciar **workflows**, aplicar solo un porcentaje de las veces (fallas intermitentes) o
  depender del **número de llamada** (por ejemplo, fallar las dos primeras y responder bien a la
  tercera).
- **Recurso**: datos persistentes de un servicio, en colecciones (`documents`, `objects`,
  `customers`…). Un escenario o un paso de workflow puede crearlos, leerlos, actualizarlos o
  borrarlos: lo que un `POST` crea, un `GET` posterior lo devuelve. Se ven y editan en la pestaña
  **Recursos** del servicio.
- **Workflow**: pasos asíncronos sin código: esperar, solicitud HTTP (webhook) con reintentos,
  condiciones (sí/no), «dividir según» con varios casos, variables, recursos, emitir eventos,
  ejecutar otro workflow, simular un error o finalizar. Se programa en un **editor de nodos**:
  arrastras pasos desde la paleta y unes sus salidas con nombre («Éxito», «Fallo», «Sí», «No», cada
  caso) con el siguiente paso; soltar una salida en un espacio vacío crea el paso conectado. Sobre el
  diagrama puedes ver el recorrido de cualquier ejecución (pasos visitados, fallos y el paso en curso).
  También se puede editar como lista. Se inicia al recibir la solicitud, al completar la respuesta,
  manualmente o cada cierto tiempo.
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
  "rfc": "{{ fake('rfc') }}",
  "intento": "{{ request.callNumber }}",
  "documento": "{{ resource }}"
}
```

Si un texto es solo `{{ expresión }}` se conserva el tipo (número, lista, objeto). Consulta
[docs/configuracion.md](docs/configuracion.md) para la referencia completa.

## Documentación

- [Configuración y plantillas](docs/configuracion.md)
- [API REST de administración y MCP](docs/api.md)
- [Arquitectura y decisiones técnicas](docs/arquitectura.md)
- [Marca](docs/marca.md)

## Desarrollo

```bash
cargo test                 # pruebas del núcleo
cargo run -p fakes-cli -- serve --sin-navegador   # núcleo + API en :4010
cargo run -p fakes-cli -- mcp                      # servidor MCP por stdio
cd ui && pnpm dev          # interfaz con recarga en caliente en :5173 (proxy a :4010)
```

## Licencia

MIT o Apache-2.0, a tu elección.
