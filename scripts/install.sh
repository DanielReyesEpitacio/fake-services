#!/usr/bin/env bash
# Instala Fake Services en Linux (compila primero si hace falta).
#
#   ./scripts/install.sh               CLI `fakes` + app de escritorio
#   ./scripts/install.sh --solo-cli    solo el CLI `fakes` (sin sudo)
#   ./scripts/install.sh --desinstalar quita lo instalado por este script
#
# El CLI se instala en ~/.local/bin. La app de escritorio se instala con el
# paquete de tu distribución (DEB o RPM) o, si no hay, como AppImage en tu
# carpeta personal.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DIR="${HOME}/.local/bin"
APP_DIR="${HOME}/.local/share/fake-services-app"
DESKTOP_FILE="${HOME}/.local/share/applications/fake-services.desktop"
PKG_NAME="fake-services"
SOLO_CLI=0
DESINSTALAR=0
EXTRA=()

for arg in "$@"; do
  case "$arg" in
    --solo-cli) SOLO_CLI=1 ;;
    --desinstalar|--uninstall) DESINSTALAR=1 ;;
    -y|--si) EXTRA+=("--si") ;;
    -h|--help|--ayuda) sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "Opción desconocida: $arg (usa --ayuda)" >&2; exit 2 ;;
  esac
done

if [ -t 1 ]; then B=$'\e[1m'; V=$'\e[32m'; A=$'\e[33m'; N=$'\e[0m'; else B=''; V=''; A=''; N=''; fi
paso() { echo; echo "${B}▸ $*${N}"; }
ok()   { echo "  ${V}✓${N} $*"; }
aviso(){ echo "  ${A}!${N} $*"; }

cd "$ROOT"

# ---------------------------------------------------------------------------
if [ "$DESINSTALAR" = 1 ]; then
  paso "Desinstalando Fake Services"
  rm -f "$BIN_DIR/fakes" && ok "CLI eliminado de $BIN_DIR"
  if [ -d "$APP_DIR" ] || [ -f "$DESKTOP_FILE" ]; then
    rm -rf "$APP_DIR" "$DESKTOP_FILE"
    ok "AppImage y acceso directo eliminados"
  fi
  if command -v dpkg >/dev/null && dpkg -s "$PKG_NAME" >/dev/null 2>&1; then
    sudo apt-get remove -y "$PKG_NAME" && ok "Paquete DEB eliminado"
  elif command -v rpm >/dev/null && rpm -q "$PKG_NAME" >/dev/null 2>&1; then
    sudo dnf remove -y "$PKG_NAME" && ok "Paquete RPM eliminado"
  fi
  echo
  echo "  Tus espacios de trabajo e historial se conservan en ~/.local/share/fake-services."
  echo "  Bórralos a mano si ya no los necesitas."
  exit 0
fi

# ---------------------------------------------------------------------------
shopt -s nullglob

# Siempre se compila: cargo y pnpm solo rehacen lo que cambió, y así nunca
# se instala un binario viejo.
if [ "$SOLO_CLI" = 1 ]; then
  "$ROOT/scripts/build.sh" --solo-cli "${EXTRA[@]}"
else
  "$ROOT/scripts/build.sh" "${EXTRA[@]}"
fi

# ---------------------------------------------------------------------------
paso "Instalando el CLI"
mkdir -p "$BIN_DIR"
install -m 0755 target/release/fakes "$BIN_DIR/fakes"
ok "fakes → $BIN_DIR/fakes"
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) aviso "$BIN_DIR no está en tu PATH. Agrega a ~/.bashrc o ~/.zshrc:  export PATH=\"\$HOME/.local/bin:\$PATH\"" ;;
esac

# ---------------------------------------------------------------------------
if [ "$SOLO_CLI" = 0 ]; then
  paso "Instalando la app de escritorio"
  debs=(target/release/bundle/deb/*.deb)
  rpms=(target/release/bundle/rpm/*.rpm)
  images=(target/release/bundle/appimage/*.AppImage)

  if command -v apt-get >/dev/null && [ ${#debs[@]} -gt 0 ]; then
    echo "  Se instalará ${debs[-1]##*/} con apt (pedirá tu contraseña de sudo)."
    sudo apt-get install -y "./${debs[-1]}"
    ok "App instalada desde el paquete DEB"
  elif command -v dnf >/dev/null && [ ${#rpms[@]} -gt 0 ]; then
    echo "  Se instalará ${rpms[-1]##*/} con dnf (pedirá tu contraseña de sudo)."
    sudo dnf install -y "./${rpms[-1]}"
    ok "App instalada desde el paquete RPM"
  elif [ ${#images[@]} -gt 0 ]; then
    mkdir -p "$APP_DIR" "$(dirname "$DESKTOP_FILE")"
    install -m 0755 "${images[-1]}" "$APP_DIR/FakeServices.AppImage"
    install -m 0644 apps/desktop/icons/512x512.png "$APP_DIR/icon.png"
    cat > "$DESKTOP_FILE" <<EOF
[Desktop Entry]
Type=Application
Name=Fake Services
Comment=Simulador local de servicios externos
Exec=$APP_DIR/FakeServices.AppImage
Icon=$APP_DIR/icon.png
Categories=Development;
Terminal=false
EOF
    command -v update-desktop-database >/dev/null && update-desktop-database "$(dirname "$DESKTOP_FILE")" 2>/dev/null || true
    ok "AppImage instalada en $APP_DIR (aparece en el menú de aplicaciones)"
    aviso "Si no abre, instala libfuse2 (Ubuntu/Debian: sudo apt install libfuse2)."
  else
    aviso "No se encontraron instaladores de escritorio; solo quedó instalado el CLI."
  fi
fi

# ---------------------------------------------------------------------------
paso "Listo"
echo "  Ejecuta:  fakes serve"
echo "  Interfaz: http://127.0.0.1:4010   Simulaciones: http://127.0.0.1:4000"
[ "$SOLO_CLI" = 0 ] && echo "  O abre «Fake Services» desde el menú de aplicaciones."
true
