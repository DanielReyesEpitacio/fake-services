#!/usr/bin/env bash
# Compila Fake Services en Linux.
#
#   ./scripts/build.sh              binario target/release/fakes (con la interfaz web incluida)
#   ./scripts/build.sh --paquetes   además genera en dist/: .deb, .rpm y .tar.gz
#
# Requisitos: Rust (rustup), Node.js 20+ y pnpm. El resultado no necesita nada de eso.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PAQUETES=0

for arg in "$@"; do
  case "$arg" in
    --paquetes|--packages) PAQUETES=1 ;;
    -h|--help|--ayuda) sed -n '2,7p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "Opción desconocida: $arg (usa --ayuda)" >&2; exit 2 ;;
  esac
done

if [ -t 1 ]; then B=$'\e[1m'; V=$'\e[32m'; A=$'\e[33m'; R=$'\e[31m'; N=$'\e[0m'; else B=''; V=''; A=''; R=''; N=''; fi
paso() { echo; echo "${B}▸ $*${N}"; }
ok()   { echo "  ${V}✓${N} $*"; }
aviso(){ echo "  ${A}!${N} $*"; }
falla(){ echo "  ${R}✗ $*${N}" >&2; exit 1; }

cd "$ROOT"

paso "Revisando herramientas"
command -v cargo >/dev/null || falla "No se encontró Rust. Instálalo desde https://rustup.rs y vuelve a ejecutar."
ok "Rust $(rustc --version | cut -d' ' -f2)"
command -v node >/dev/null || falla "No se encontró Node.js (20 o superior). Instálalo desde https://nodejs.org."
ok "Node $(node --version)"
if command -v pnpm >/dev/null; then PNPM=(pnpm)
elif command -v corepack >/dev/null; then PNPM=(corepack pnpm)
else falla "No se encontró pnpm. Instálalo con: npm install -g pnpm"; fi
ok "pnpm $("${PNPM[@]}" --version)"

paso "Compilando la interfaz web"
( cd ui && "${PNPM[@]}" install --frozen-lockfile && "${PNPM[@]}" build )
ok "Interfaz lista en ui/dist"

paso "Compilando fakes"
cargo build --release -p fakes-cli
ok "Binario: target/release/fakes ($(du -h target/release/fakes | cut -f1))"

if [ "$PAQUETES" = 1 ]; then
  VERSION="$(cargo metadata --no-deps --format-version 1 | sed -n 's/.*"name":"fakes-cli","version":"\([^"]*\)".*/\1/p')"
  ARCH="$(uname -m)"
  mkdir -p dist

  paso "Generando .deb"
  command -v cargo-deb >/dev/null || cargo install cargo-deb --locked
  cargo deb -p fakes-cli --no-build --output dist/
  ok "$(ls dist/*.deb | tail -1)"

  paso "Generando .rpm"
  command -v cargo-generate-rpm >/dev/null || cargo install cargo-generate-rpm --locked
  cargo generate-rpm -p crates/fakes-cli -o dist/
  ok "$(ls dist/*.rpm | tail -1)"

  paso "Generando .tar.gz"
  NAME="fake-services-${VERSION}-linux-${ARCH}"
  TMP="$(mktemp -d)"
  mkdir -p "$TMP/$NAME"
  install -m 0755 target/release/fakes "$TMP/$NAME/fakes"
  install -m 0755 scripts/install.sh "$TMP/$NAME/instalar.sh"
  install -m 0644 packaging/linux/fake-services.desktop "$TMP/$NAME/fake-services.desktop"
  install -m 0644 assets/brand/logo-512.png "$TMP/$NAME/fake-services.png"
  cat > "$TMP/$NAME/LEEME.txt" <<EOF
Fake Services ${VERSION}
Servicios falsos. Integraciones reales.

Instalar (sin sudo):   ./instalar.sh
Usar sin instalar:     ./fakes
Desinstalar:           ./instalar.sh --desinstalar

Al ejecutar "fakes" se abre la interfaz en tu navegador (http://127.0.0.1:4010).
Las simulaciones responden en http://127.0.0.1:4000.
EOF
  tar -C "$TMP" -czf "dist/${NAME}.tar.gz" "$NAME"
  rm -rf "$TMP"
  ok "dist/${NAME}.tar.gz"
fi

paso "Listo"
echo "  Ejecuta:   ./target/release/fakes"
echo "  Instala:   ./scripts/install.sh"
[ "$PAQUETES" = 1 ] && echo "  Paquetes:  $ROOT/dist/"
true
