//! Aplicación de escritorio de Fake Services.
//!
//! El núcleo (servidor de simulación + API de administración) corre dentro
//! de este mismo proceso. La ventana es solo un visor de la interfaz web
//! servida por la API local, así que cerrar la ventana no detiene las
//! simulaciones: la app queda en la bandeja del sistema.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;

use fakes_api::{LaunchError, LaunchOptions, Launched};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

/// Argumento con el que el sistema inicia la app al encender el equipo.
const HIDDEN_ARG: &str = "--oculto";

struct Core(Mutex<Option<Launched>>);
struct AdminUrl(String);

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn".into()),
        )
        .with_target(false)
        .init();

    let start_hidden = std::env::args().any(|a| a == HIDDEN_ARG);

    let app = tauri::Builder::default()
        // Una sola instancia: abrir la app de nuevo enfoca la ventana existente.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![HIDDEN_ARG]),
        ))
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let handle = app.handle().clone();
            let opts = LaunchOptions::new(fakes_config::default_data_dir());

            // Arrancar el núcleo antes de mostrar la ventana.
            let launched = tauri::async_runtime::block_on(fakes_api::launch(opts));
            let admin_url = match launched {
                Ok(l) => {
                    let url = l.admin_url.clone();
                    app.manage(Core(Mutex::new(Some(l))));
                    url
                }
                // Otra instancia (por ejemplo `fakes serve`) ya ocupa el puerto:
                // mostramos su interfaz en lugar de fallar.
                Err(LaunchError::AdminPortInUse(port)) => {
                    tracing::warn!("usando la instancia existente en el puerto {port}");
                    app.manage(Core(Mutex::new(None)));
                    format!("http://127.0.0.1:{port}")
                }
                Err(e) => return Err(Box::new(e)),
            };
            app.manage(AdminUrl(admin_url));

            build_tray(&handle)?;
            if !start_hidden {
                show_main_window(&handle);
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("no se pudo iniciar Fake Services");

    app.run(|app, event| match event {
        // Cerrar la ventana la oculta; la app sigue en la bandeja.
        RunEvent::WindowEvent {
            label,
            event: WindowEvent::CloseRequested { api, .. },
            ..
        } if label == "main" => {
            api.prevent_close();
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.hide();
            }
        }
        RunEvent::ExitRequested { code, api, .. } if code.is_none() => {
            // Salida implícita (sin ventanas): seguir en la bandeja.
            api.prevent_exit();
        }
        RunEvent::Exit => {
            let core = app.state::<Core>();
            let launched = core.0.lock().ok().and_then(|mut g| g.take());
            if let Some(l) = launched {
                tauri::async_runtime::block_on(l.shutdown());
            }
        }
        _ => {}
    });
}

fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        return;
    }
    let url = app.state::<AdminUrl>().0.clone();
    let Ok(parsed) = url.parse() else {
        tracing::error!("URL de administración inválida: {url}");
        return;
    };
    let built = WebviewWindowBuilder::new(app, "main", WebviewUrl::External(parsed))
        .title("Fake Services")
        .inner_size(1360.0, 860.0)
        .min_inner_size(960.0, 600.0)
        .build();
    if let Err(e) = built {
        tracing::error!("no se pudo abrir la ventana: {e}");
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Abrir Fake Services", true, None::<&str>)?;
    let browser = MenuItem::with_id(app, "browser", "Abrir en el navegador", true, None::<&str>)?;
    let autostart_enabled = app.autolaunch().is_enabled().unwrap_or(false);
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "Iniciar con el sistema",
        true,
        autostart_enabled,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &browser, &sep, &autostart, &sep, &quit])?;

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Fake Services")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "open" => show_main_window(app),
            "browser" => {
                use tauri_plugin_opener::OpenerExt;
                let url = app.state::<AdminUrl>().0.clone();
                let _ = app.opener().open_url(url, None::<&str>);
            }
            "autostart" => {
                let launcher = app.autolaunch();
                let enabled = launcher.is_enabled().unwrap_or(false);
                let res = if enabled {
                    launcher.disable()
                } else {
                    launcher.enable()
                };
                if let Err(e) = res {
                    tracing::warn!("no se pudo cambiar el inicio automático: {e}");
                }
                let _ = autostart.set_checked(launcher.is_enabled().unwrap_or(false));
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}
