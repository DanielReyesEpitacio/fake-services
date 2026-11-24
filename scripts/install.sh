#!/usr/bin/env bash
# Instala o actualiza Fake Services para tu usuario en Linux. No usa sudo
# (salvo que aceptes quitar un paquete del sistema antiguo).
#
#   ./instalar.sh                 instala, actualiza o reinstala (siempre el mismo comando)
#   ./instalar.sh --desinstalar   quita lo instalado (tus datos se conservan)
#   ./instalar.sh -y              responde "sí" a todo
#
# Si hay una versión anterior: la detiene, la reemplaza, limpia restos de
# instalaciones viejas y, si estaba en ejecución, inicia la nueva.
# Funciona desde el paquete descargado (usa el binario incluido) o desde el
# repositorio (compila primero con scripts/build.sh).

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
BIN_DIR="${HOME}/.local/bin"
BIN="$BIN_DIR/fakes"
APPS_DIR="$DATA_HOME/applications"
ICON_DIR="$DATA_HOME/icons/hicolor/512x512/apps"
AUTOSTART="${XDG_CONFIG_HOME:-$HOME/.config}/autostart/fake-services.desktop"
LOG_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/fake-services"
# Restos de versiones anteriores (app de escritorio).
LEGACY_APP_DIR="$DATA_HOME/fake-services-app"
PKG_NAME="fake-services"
ADMIN_PORT="${FAKES_ADMIN_PORT:-4010}"

DESINSTALAR=0
SI=0
for arg in "$@"; do
  case "$arg" in
    --desinstalar|--uninstall) DESINSTALAR=1 ;;
    -y|--si|--yes) SI=1 ;;
    -h|--help|--ayuda) sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "Opción desconocida: $arg (usa --ayuda)" >&2; exit 2 ;;
  esac
done

if [ -t 1 ]; then B=$'\e[1m'; V=$'\e[32m'; A=$'\e[33m'; N=$'\e[0m'; else B=''; V=''; A=''; N=''; fi
paso() { echo; echo "${B}▸ $*${N}"; }
ok()   { echo "  ${V}✓${N} $*"; }
aviso(){ echo "  ${A}!${N} $*"; }

confirmar() {
  [ "$SI" = 1 ] && return 0
  [ -t 0 ] || return 1
  read -r -p "  $1 [s/N] " r
  [[ "$r" =~ ^[sSyY]$ ]]
}

version_of() { "$1" --version 2>/dev/null | awk '{print $2}'; }

# PIDs de Fake Services instalado (no toca compilaciones de desarrollo).
installed_pids() {
  local pid exe
  for pid in $(pgrep -x fakes 2>/dev/null; pgrep -x fake-services 2>/dev/null); do
    exe="$(readlink "/proc/$pid/exe" 2>/dev/null || true)"
    exe="${exe% (deleted)}"
    case "$exe" in
      "$BIN"|/usr/bin/fakes|/usr/bin/fake-services|"$LEGACY_APP_DIR"/*|/tmp/.mount_*) echo "$pid" ;;
    esac
  done
}

health() {
  { exec 3<>"/dev/tcp/127.0.0.1/$ADMIN_PORT"; } 2>/dev/null || return 1
  printf 'GET /healthz HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n' >&3
  local resp; resp="$(timeout 2 cat <&3 || true)"
  exec 3>&- 3<&-
  [[ "$resp" == *" 200"* ]]
}

# Detiene las instancias instaladas: primero de forma ordenada, luego por señal.
stop_running() {
  local pids; pids="$(installed_pids | sort -u | tr '\n' ' ')"
  [ -z "${pids// /}" ] && return 1
  aviso "Fake Services está en ejecución; se detendrá para continuar."
  local tool="${1:-}"
  if [ -n "$tool" ] && [ -x "$tool" ]; then
    "$tool" detener >/dev/null 2>&1 || true
  fi
  for i in $(seq 1 40); do
    [ -z "$(installed_pids)" ] && break
    # Versiones sin `fakes detener`: señal de término (también es un cierre ordenado).
    [ "$i" -eq 10 ] && kill -TERM $pids 2>/dev/null || true
    sleep 0.3
  done
  if [ -n "$(installed_pids)" ]; then
    kill -KILL $pids 2>/dev/null || true
    sleep 0.3
  fi
  ok "Instancia anterior detenida"
  return 0
}

refresh_menus() {
  command -v update-desktop-database >/dev/null && update-desktop-database "$APPS_DIR" 2>/dev/null || true
  command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "${ICON_DIR%/512x512/apps}" 2>/dev/null || true
}

system_package() {
  if command -v dpkg >/dev/null && dpkg -s "$PKG_NAME" >/dev/null 2>&1; then echo apt
  elif command -v rpm >/dev/null && rpm -q "$PKG_NAME" >/dev/null 2>&1; then echo dnf
  fi
}

remove_legacy() {
  if [ -d "$LEGACY_APP_DIR" ]; then
    rm -rf "$LEGACY_APP_DIR"
    ok "Se quitó la app de escritorio anterior (AppImage)"
  fi
}

# ---------------------------------------------------------------------------
if [ "$DESINSTALAR" = 1 ]; then
  paso "Desinstalando Fake Services"
  stop_running "$BIN" || true
  rm -f "$BIN" "$APPS_DIR/fake-services.desktop" "$ICON_DIR/fake-services.png" "$AUTOSTART"
  remove_legacy
  refresh_menus
  ok "Se quitaron el programa, el acceso del menú y el inicio automático"
  pkg="$(system_package)"
  if [ -n "$pkg" ]; then
    aviso "También está instalado el paquete del sistema «$PKG_NAME»."
    if confirmar "¿Quitarlo? (pedirá tu contraseña de sudo)"; then
      sudo "$pkg" remove -y "$PKG_NAME" && ok "Paquete del sistema eliminado"
    fi
  fi
  echo
  echo "  Tus espacios de trabajo e historial siguen en ~/.local/share/fake-services."
  echo "  Bórralos a mano si ya no los necesitas."
  exit 0
fi

# ---------------------------------------------------------------------------
# ¿Paquete descargado (binario junto al script) o repositorio?
if [ -x "$HERE/fakes" ]; then
  SRC_BIN="$HERE/fakes"
  SRC_DESKTOP="$HERE/fake-services.desktop"
  SRC_ICON="$HERE/fake-services.png"
else
  ROOT="$(cd "$HERE/.." && pwd)"
  "$ROOT/scripts/build.sh"
  SRC_BIN="$ROOT/target/release/fakes"
  SRC_DESKTOP="$ROOT/packaging/linux/fake-services.desktop"
  SRC_ICON="$ROOT/assets/brand/logo-512.png"
fi

NEW_VERSION="$(version_of "$SRC_BIN")"
OLD_VERSION=""
[ -x "$BIN" ] && OLD_VERSION="$(version_of "$BIN")"

if [ -z "$OLD_VERSION" ]; then
  paso "Instalando Fake Services $NEW_VERSION"
elif [ "$OLD_VERSION" = "$NEW_VERSION" ]; then
  paso "Reinstalando Fake Services $NEW_VERSION"
else
  paso "Actualizando Fake Services de $OLD_VERSION a $NEW_VERSION"
fi

WAS_RUNNING=0
if stop_running "$SRC_BIN"; then WAS_RUNNING=1; fi
remove_legacy

pkg="$(system_package)"
if [ -n "$pkg" ]; then
  aviso "Hay un paquete del sistema «$PKG_NAME» de una instalación anterior."
  if confirmar "¿Quitarlo para dejar una sola instalación? (pedirá tu contraseña de sudo)"; then
    sudo "$pkg" remove -y "$PKG_NAME" && ok "Paquete del sistema eliminado"
  else
    aviso "Se conserva; la versión de ~/.local/bin tiene prioridad si está antes en tu PATH."
  fi
fi

mkdir -p "$BIN_DIR" "$APPS_DIR" "$ICON_DIR"
# Copia a un temporal y renombra: reemplazo atómico aunque el archivo esté en uso.
install -m 0755 "$SRC_BIN" "$BIN.nuevo"
mv -f "$BIN.nuevo" "$BIN"
ok "Programa: $BIN"

if [ -f "$SRC_ICON" ]; then
  install -m 0644 "$SRC_ICON" "$ICON_DIR/fake-services.png"
fi
if [ -f "$SRC_DESKTOP" ]; then
  # Ruta completa: el menú del escritorio no siempre incluye ~/.local/bin en el PATH.
  sed "s|^Exec=fakes|Exec=\"$BIN\"|" "$SRC_DESKTOP" > "$APPS_DIR/fake-services.desktop"
  chmod 0644 "$APPS_DIR/fake-services.desktop"
  ok "Acceso en el menú de aplicaciones: «Fake Services»"
fi
refresh_menus

# El inicio automático siempre apunta al binario instalado.
if [ -f "$AUTOSTART" ]; then
  sed -i "s|^Exec=.*|Exec=\"$BIN\" serve --sin-navegador|" "$AUTOSTART"
  ok "Inicio automático actualizado"
fi

case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *)
    aviso "$BIN_DIR no está en tu PATH. Para usar «fakes» desde la terminal agrega a ~/.bashrc o ~/.zshrc:"
    echo "      export PATH=\"\$HOME/.local/bin:\$PATH\""
    ;;
esac

if [ "$WAS_RUNNING" = 1 ]; then
  mkdir -p "$LOG_DIR"
  nohup setsid "$BIN" serve --sin-navegador >"$LOG_DIR/fakes.log" 2>&1 < /dev/null &
  for _ in $(seq 1 50); do health && break; sleep 0.2; done
  if health; then
    ok "Fake Services $NEW_VERSION se reinició en segundo plano (detenlo con: fakes detener)"
  else
    aviso "No se pudo confirmar el reinicio; revisa $LOG_DIR/fakes.log"
  fi
fi

paso "Listo"
echo "  Abre «Fake Services» desde el menú de aplicaciones o ejecuta:  fakes"
echo "  La interfaz se abre en tu navegador: http://127.0.0.1:$ADMIN_PORT"
echo "  Tus espacios de trabajo y configuración se conservan entre versiones."
