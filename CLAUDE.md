# Fake Services: guía para agentes que trabajan en este repositorio

- Producto: simulador local de servicios externos, AI-Agent-First. UI y documentación en español;
  identificadores técnicos en inglés.
- Núcleo en Rust (workspace de Cargo en `crates/`), interfaz Vue 3 en `ui/`, escritorio Tauri 2 en
  `apps/desktop/` (excluido de `default-members` porque requiere WebKitGTK).
- Toda funcionalidad nueva entra por `fakes-app` (casos de uso). La API REST (`fakes-api`), la UI y
  el futuro MCP solo traducen hacia esos métodos; no dupliques lógica en ellos.
- `fakes-domain` no puede depender de HTTP, disco ni plantillas.
- La configuración declarativa (JSON en disco) es la fuente de verdad; SQLite es solo estado de
  ejecución. No generes código ejecutable para representar endpoints o workflows.

## Comandos

- `cargo test`: pruebas del núcleo.
- `cargo clippy --all-targets -- -D warnings`: debe quedar limpio.
- `cargo run -p fakes-cli`: núcleo + API en :4010 y simulaciones en :4000.
- `cd ui && pnpm build`: compila la UI que se embebe en el binario (`ui/dist`).
- `cd ui && pnpm dev`: UI con recarga en :5173 (proxy a :4010 con el token local).
