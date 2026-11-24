//! Integración con el sistema operativo: iniciar Fake Services al iniciar sesión.
//!
//! - Linux: entrada XDG en `~/.config/autostart/fake-services.desktop`
//!   (GNOME, KDE, XFCE, Cinnamon, MATE...).
//! - Windows: valor `FakeServices` en `HKCU\...\CurrentVersion\Run`, que inicia
//!   el ejecutable oculto (sin ventana de consola).
//!
//! Siempre se usa el ejecutable que está corriendo, con `serve --sin-navegador`.

use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::Serialize;

use crate::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AutostartStatus {
    /// Si el sistema operativo actual lo admite.
    pub supported: bool,
    pub enabled: bool,
    /// Mecanismo usado (para mostrarlo en la interfaz).
    pub method: String,
    /// Comando que se ejecutará al iniciar sesión.
    pub command: Option<String>,
}

fn current_exe() -> AppResult<PathBuf> {
    std::env::current_exe()
        .map_err(|e| AppError::BadRequest(format!("no se pudo ubicar el ejecutable: {e}")))
}

pub fn autostart_status() -> AppResult<AutostartStatus> {
    imp::status(&current_exe()?)
}

pub fn set_autostart(enabled: bool) -> AppResult<AutostartStatus> {
    imp::set(&current_exe()?, enabled)
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use super::*;

    fn autostart_file(config_dir: &Path) -> PathBuf {
        config_dir.join("autostart").join("fake-services.desktop")
    }

    fn config_dir() -> AppResult<PathBuf> {
        directories::BaseDirs::new()
            .map(|d| d.config_dir().to_path_buf())
            .ok_or_else(|| {
                AppError::BadRequest("no se encontró la carpeta de configuración".into())
            })
    }

    fn quote(p: &Path) -> String {
        format!("\"{}\"", p.display().to_string().replace('"', "\\\""))
    }

    pub(super) fn desktop_entry(exe: &Path) -> String {
        format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=Fake Services\n\
             Comment=Simulador local de servicios externos\n\
             Exec={} serve --sin-navegador\n\
             Icon=fake-services\n\
             Terminal=false\n\
             X-GNOME-Autostart-enabled=true\n",
            quote(exe)
        )
    }

    pub(super) fn status_in(config: &Path, exe: &Path) -> AutostartStatus {
        let file = autostart_file(config);
        AutostartStatus {
            supported: true,
            enabled: std::fs::read_to_string(&file)
                .is_ok_and(|c| c.contains("fake-services") || c.contains("Fake Services")),
            method: format!("Inicio automático del escritorio ({})", file.display()),
            command: Some(format!("{} serve --sin-navegador", exe.display())),
        }
    }

    pub(super) fn set_in(config: &Path, exe: &Path, enabled: bool) -> AppResult<AutostartStatus> {
        let file = autostart_file(config);
        if enabled {
            fakes_config::write_atomic(&file, desktop_entry(exe).as_bytes())?;
        } else {
            match std::fs::remove_file(&file) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(AppError::BadRequest(format!(
                        "no se pudo quitar {}: {e}",
                        file.display()
                    )));
                }
            }
        }
        Ok(status_in(config, exe))
    }

    pub(super) fn status(exe: &Path) -> AppResult<AutostartStatus> {
        Ok(status_in(&config_dir()?, exe))
    }

    pub(super) fn set(exe: &Path, enabled: bool) -> AppResult<AutostartStatus> {
        set_in(&config_dir()?, exe, enabled)
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::process::Command;

    const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE: &str = "FakeServices";

    fn command(exe: &Path) -> String {
        format!(
            "powershell.exe -NoProfile -WindowStyle Hidden -Command \"& '{}' serve --sin-navegador\"",
            exe.display().to_string().replace('\'', "''")
        )
    }

    fn reg(args: &[&str]) -> std::io::Result<std::process::Output> {
        Command::new("reg").args(args).output()
    }

    pub(super) fn status(exe: &Path) -> AppResult<AutostartStatus> {
        let enabled = reg(&["query", KEY, "/v", VALUE]).is_ok_and(|o| o.status.success());
        Ok(AutostartStatus {
            supported: true,
            enabled,
            method: format!("Registro de Windows ({KEY}\\{VALUE})"),
            command: Some(command(exe)),
        })
    }

    pub(super) fn set(exe: &Path, enabled: bool) -> AppResult<AutostartStatus> {
        let out = if enabled {
            reg(&[
                "add",
                KEY,
                "/v",
                VALUE,
                "/t",
                "REG_SZ",
                "/d",
                &command(exe),
                "/f",
            ])
        } else {
            reg(&["delete", KEY, "/v", VALUE, "/f"])
        }
        .map_err(|e| AppError::BadRequest(format!("no se pudo ejecutar reg.exe: {e}")))?;
        if !out.status.success() && enabled {
            return Err(AppError::BadRequest(format!(
                "reg.exe falló: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        status(exe)
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;

    pub(super) fn status(_exe: &Path) -> AppResult<AutostartStatus> {
        Ok(AutostartStatus {
            supported: false,
            enabled: false,
            method: "No disponible en macOS todavía".into(),
            command: None,
        })
    }

    pub(super) fn set(_exe: &Path, _enabled: bool) -> AppResult<AutostartStatus> {
        Err(AppError::BadRequest(
            "el inicio automático aún no está disponible en macOS".into(),
        ))
    }
}

#[cfg(all(test, unix, not(target_os = "macos")))]
mod tests {
    use super::imp::*;
    use std::path::Path;

    #[test]
    fn xdg_autostart_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = Path::new("/opt/fake services/fakes");
        assert!(!status_in(tmp.path(), exe).enabled);
        let s = set_in(tmp.path(), exe, true).unwrap();
        assert!(s.enabled);
        let content =
            std::fs::read_to_string(tmp.path().join("autostart/fake-services.desktop")).unwrap();
        assert!(content.contains("Exec=\"/opt/fake services/fakes\" serve --sin-navegador"));
        let s = set_in(tmp.path(), exe, false).unwrap();
        assert!(!s.enabled);
        // Desactivar dos veces no falla.
        assert!(set_in(tmp.path(), exe, false).is_ok());
        assert!(desktop_entry(exe).starts_with("[Desktop Entry]"));
    }
}
