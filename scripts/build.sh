#!/usr/bin/env bash
# Compila Fake Services en Linux.
#
#   ./scripts/build.sh              CLI + instaladores de escritorio (AppImage, DEB, RPM)
#   ./scripts/build.sh --solo-cli   solo el binario `fakes` (no necesita WebKitGTK)
#
# Si faltan dependencias del sistema para la app de escritorio, el script
# muestra el comando para instalarlas y pregunta antes de usar sudo.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOLO_CLI=0
SI_A_TODO=0

for arg in "$@"; do
  case "$arg" in
    --solo-cli) SOLO_CLI=1 ;;
    -y|--si) SI_A_TODO=1 ;;
    -h|--help|--ayuda)
      sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'
      echo "  -y, --si       acepta instalar dependencias sin preguntar"
      exit 0
      ;;
    *) echo "Opción desconocida: $arg (usa --ayuda)" >&2; exit 2 ;;
  esac
done

if [ -t 1 ]; then
  B=$'\e[1m'; V=$'\e[32m'; A=$'\e[33m'; R=$'\e[31m'; N=$'\e[0m'
else
  B=''; V=''; A=''; R=''; N=''
fi
paso() { echo; echo "${B}▸ $*${N}"; }
ok()   { echo "  ${V}✓${N} $*"; }
aviso(){ echo "  ${A}!${N} $*"; }
falla(){ echo "  ${R}✗ $*${N}" >&2; exit 1; }

confirmar() {
  [ "$SI_A_TODO" = 1 ] && return 0
  [ -t 0 ] || return 1
  read -r -p "  $1 [s/N] " resp
  [[ "$resp" =~ ^[sSyY]$ ]]
}

cd "$ROOT"

# ---------------------------------------------------------------------------
paso "Revisando herramientas"

command -v cargo >/dev/null || falla "No se encontró Rust. Instálalo desde https://rustup.rs y vuelve a ejecutar."
ok "Rust $(rustc --version | cut -d' ' -f2)"

command -v node >/dev/null || falla "No se encontró Node.js (20 o superior). Instálalo desde https://nodejs.org."
ok "Node $(node --version)"

if command -v pnpm >/dev/null; then
  PNPM=(pnpm)
elif command -v corepack >/dev/null; then
  PNPM=(corepack pnpm)
else
  falla "No se encontró pnpm. Instálalo con: npm install -g pnpm"
fi
ok "pnpm $("${PNPM[@]}" --version)"

# ---------------------------------------------------------------------------
if [ "$SOLO_CLI" = 0 ]; then
  paso "Revisando dependencias de la app de escritorio"

  faltan=()
  for pc in webkit2gtk-4.1 javascriptcoregtk-4.1 libsoup-3.0; do
    pkg-config --exists "$pc" 2>/dev/null || faltan+=("$pc")
  done

  if [ ${#faltan[@]} -gt 0 ]; then
    aviso "Faltan bibliotecas del sistema: ${faltan[*]}"
    if command -v apt-get >/dev/null; then
      CMD="sudo apt-get update && sudo apt-get install -y libwebkit2gtk-4.1-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev librsvg2-dev libayatana-appindicator3-dev libxdo-dev libssl-dev build-essential curl wget file patchelf pkg-config"
    elif command -v dnf >/dev/null; then
      CMD="sudo dnf install -y webkit2gtk4.1-devel libsoup3-devel javascriptcoregtk4.1-devel librsvg2-devel libappindicator-gtk3-devel libxdo-devel openssl-devel curl wget file patchelf pkgconf-pkg-config gcc gcc-c++ make rpm-build"
    elif command -v pacman >/dev/null; then
      CMD="sudo pacman -S --needed --noconfirm webkit2gtk-4.1 base-devel curl wget file openssl libappindicator-gtk3 librsvg xdotool patchelf"
    else
      falla "Instala WebKitGTK 4.1 y libsoup 3 con el gestor de paquetes de tu distribución, o usa --solo-cli."
    fi
    echo "  Comando: $CMD"
    if confirmar "¿Instalarlas ahora? (pedirá tu contraseña de sudo)"; then
      bash -c "$CMD"
      ok "Dependencias instaladas"
    else
      falla "Sin esas bibliotecas no se puede compilar la app de escritorio. Ejecuta el comando de arriba o usa --solo-cli."
    fi
  else
    ok "WebKitGTK y libsoup disponibles"
  fi

  if ! cargo tauri --version >/dev/null 2>&1; then
    aviso "Falta tauri-cli (se instala una sola vez, tarda unos minutos)"
    cargo install tauri-cli --version '^2' --locked
  fi
  ok "$(cargo tauri --version)"
fi

# ---------------------------------------------------------------------------
paso "Compilando la interfaz"
( cd ui && "${PNPM[@]}" install --frozen-lockfile && "${PNPM[@]}" build )
ok "Interfaz lista en ui/dist"

paso "Compilando el CLI (fakes)"
cargo build --release -p fakes-cli
ok "Binario: target/release/fakes"

if [ "$SOLO_CLI" = 0 ]; then
  paso "Generando instaladores de escritorio"
  aviso "AppImage descarga herramientas de empaquetado la primera vez: necesita Internet."
  ( cd apps/desktop && cargo tauri build )
fi

# ---------------------------------------------------------------------------
paso "Listo"
echo "  CLI:        $ROOT/target/release/fakes"
if [ "$SOLO_CLI" = 0 ]; then
  shopt -s nullglob
  for f in target/release/bundle/appimage/*.AppImage target/release/bundle/deb/*.deb target/release/bundle/rpm/*.rpm; do
    echo "  Instalador: $ROOT/$f"
  done
fi
echo
echo "  Para instalar: ./scripts/install.sh"
