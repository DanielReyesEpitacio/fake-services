# Fake Services: guía para agentes que trabajan en este repositorio

- Producto: simulador local de servicios externos, AI-Agent-First. UI y documentación en español;
  identificadores técnicos en inglés.
- Núcleo en Rust (workspace de Cargo en `crates/`) e interfaz web Vue 3 en `ui/`, embebida en el
  binario `fakes` (`crates/fakes-cli`). No hay app de escritorio: la interfaz se usa en el navegador.
- Toda funcionalidad nueva entra por `fakes-app` (casos de uso). La API REST (`fakes-api`), la UI y
  el futuro MCP solo traducen hacia esos métodos; no dupliques lógica en ellos.
- `fakes-domain` no puede depender de HTTP, disco ni plantillas.
- La configuración declarativa (JSON en disco) es la fuente de verdad; SQLite es solo estado de
  ejecución. No generes código ejecutable para representar endpoints o workflows.

## Comandos

- `cargo test`: pruebas del núcleo.
- `cargo clippy --all-targets -- -D warnings`: debe quedar limpio.
- `cargo run -p fakes-cli -- serve --sin-navegador`: núcleo + API en :4010 y simulaciones en :4000.
- `./scripts/build.sh --paquetes`: binario de release + .deb, .rpm y .tar.gz en `dist/`.
- `cd ui && pnpm build`: compila la UI que se embebe en el binario (`ui/dist`).
- `cd ui && pnpm dev`: UI con recarga en :5173 (proxy a :4010 con el token local).
- `cd ui && pnpm test`: pruebas de la UI (incluye aislamiento de look and feels).

## Interfaz

- Vuetify 4 con look and feels de `ui/src/platform/adapters/vue/themes` (copiado del CRM; Cupertino
  Glass por defecto). No fijes `variant`, `density` ni `rounded` en las vistas salvo que sea semántico.
- Usa `FTextField`/`FSelect`/`FCombobox`/`FTextarea` (etiqueta externa) en lugar de `V*` para campos.
- Iconos con nombres `mdi-*`: Cupertino Glass los traduce a Phosphor.
