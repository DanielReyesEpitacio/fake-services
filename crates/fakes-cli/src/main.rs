//! `fakes`: Fake Services en un solo binario (servidor, motor e interfaz web).
//!
//! - `fakes`: inicia Fake Services y abre la interfaz en el navegador; si ya
//!   está corriendo, solo abre la interfaz.
//! - `fakes serve`: inicia (`--sin-navegador` para servidores y autoinicio).
//! - `fakes abrir`: abre la interfaz de la instancia en ejecución.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::time::Duration;

use clap::{Args, Parser, Subcommand};
use fakes_api::{DEFAULT_ADMIN_PORT, LaunchError, LaunchOptions};

#[derive(Parser)]
#[command(
    name = "fakes",
    version,
    about = "Fake Services: simulador local de servicios externos",
    long_about = "Fake Services simula APIs externas (MiFiel, Belvo, S3, Stripe...) en tu máquina.\n\
                  Servicios falsos. Integraciones reales.\n\n\
                  Sin argumentos inicia Fake Services y abre la interfaz web en tu navegador."
)]
struct Cli {
    /// Carpeta de datos (registro de espacios, historial, token).
    #[arg(long, global = true, env = "FAKES_DATA_DIR")]
    data_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Args, Clone)]
struct AdminArgs {
    /// Dirección de la API de administración y la interfaz web.
    #[arg(long, default_value = "127.0.0.1", env = "FAKES_ADMIN_HOST")]
    admin_host: String,
    /// Puerto de la API de administración y la interfaz web.
    #[arg(long, default_value_t = DEFAULT_ADMIN_PORT, env = "FAKES_ADMIN_PORT")]
    admin_port: u16,
}

impl Default for AdminArgs {
    fn default() -> Self {
        Self {
            admin_host: std::env::var("FAKES_ADMIN_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
            admin_port: std::env::var("FAKES_ADMIN_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(DEFAULT_ADMIN_PORT),
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Inicia el servidor de simulación y la interfaz web.
    Serve {
        #[command(flatten)]
        admin: AdminArgs,
        /// No abrir el navegador al iniciar.
        #[arg(long, alias = "no-browser", env = "FAKES_SIN_NAVEGADOR")]
        sin_navegador: bool,
    },
    /// Abre la interfaz web de la instancia en ejecución.
    Abrir {
        #[command(flatten)]
        admin: AdminArgs,
    },
    /// Detiene la instancia en ejecución (también la que corre en segundo plano).
    Detener {
        #[command(flatten)]
        admin: AdminArgs,
    },
    /// Servidor MCP por stdio para agentes de IA (Claude Code, Cursor...).
    /// Usa la instancia en ejecución o inicia una si no hay.
    Mcp {
        #[command(flatten)]
        admin: AdminArgs,
    },
    /// Muestra el token de la API de administración (para agentes y scripts).
    Token,
    /// Muestra las carpetas y archivos que usa Fake Services.
    Paths,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    // En modo MCP, stdout es el canal del protocolo: los logs van a stderr.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn,tower_http=warn".into()),
        )
        .with_target(false)
        .with_writer(std::io::stderr)
        .with_ansi(!matches!(cli.command, Some(Command::Mcp { .. })))
        .init();

    let data_dir = cli.data_dir.unwrap_or_else(fakes_config::default_data_dir);

    match cli.command {
        // Sin argumentos: abrir si ya corre; si no, iniciar y abrir.
        None => {
            let admin = AdminArgs::default();
            let url = admin_url(&admin);
            if is_running(&admin) {
                println!("Fake Services ya está en ejecución: {url}");
                open_browser(&url);
                return Ok(());
            }
            serve(data_dir, admin, true).await
        }
        Some(Command::Serve {
            admin,
            sin_navegador,
        }) => serve(data_dir, admin, !sin_navegador).await,
        Some(Command::Abrir { admin }) => {
            let url = admin_url(&admin);
            if !is_running(&admin) {
                eprintln!("✗ Fake Services no está en ejecución en {url}. Inícialo con: fakes");
                std::process::exit(1);
            }
            open_browser(&url);
            Ok(())
        }
        Some(Command::Detener { admin }) => stop(&data_dir, &admin),
        Some(Command::Mcp { admin }) => mcp(data_dir, admin).await,
        Some(Command::Token) => {
            println!("{}", fakes_api::load_or_create_token(&data_dir)?);
            Ok(())
        }
        Some(Command::Paths) => {
            println!("Datos:      {}", data_dir.display());
            println!(
                "Token:      {}",
                data_dir.join(fakes_api::TOKEN_FILE).display()
            );
            println!("Base:       {}", data_dir.join("fakes.db").display());
            if let Ok(exe) = std::env::current_exe() {
                println!("Ejecutable: {}", exe.display());
            }
            Ok(())
        }
    }
}

/// MCP por stdio: un mensaje JSON-RPC por línea.
async fn mcp(data_dir: PathBuf, admin: AdminArgs) -> anyhow::Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let mut stdin = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();

    if is_running(&admin) {
        // Puente hacia la instancia que ya corre (misma configuración que la interfaz).
        let token = fakes_api::load_or_create_token(&data_dir)?;
        let url = format!("{}/mcp", admin_url(&admin));
        let client = reqwest::Client::builder().no_proxy().build()?;
        tracing::info!("MCP conectado a la instancia en ejecución ({url})");
        while let Some(line) = stdin.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }
            let resp = client
                .post(&url)
                .bearer_auth(&token)
                .header("content-type", "application/json")
                .body(line.clone())
                .send()
                .await;
            let out = match resp {
                Ok(r) if r.status().as_u16() == 202 => continue,
                Ok(r) => r.text().await.unwrap_or_default(),
                Err(e) => {
                    let id = serde_json::from_str::<serde_json::Value>(&line)
                        .ok()
                        .and_then(|v| v.get("id").cloned())
                        .unwrap_or(serde_json::Value::Null);
                    serde_json::json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32603, "message": format!("Fake Services no respondió: {e}")}}).to_string()
                }
            };
            stdout.write_all(out.trim_end().as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
        return Ok(());
    }

    // Sin instancia: se inicia aquí (con la interfaz web disponible) mientras dure la sesión.
    let mut opts = LaunchOptions::new(data_dir);
    opts.admin_host = admin.admin_host.clone();
    opts.admin_port = admin.admin_port;
    let launched = fakes_api::launch(opts).await?;
    tracing::info!(
        "Fake Services iniciado para MCP; interfaz en {}",
        launched.admin_url
    );
    let server = fakes_mcp::McpServer::new(launched.app.clone());
    while let Some(line) = stdin.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(msg) => server.handle_payload(msg).await,
            Err(e) => Some(
                serde_json::json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": format!("JSON inválido: {e}")}}),
            ),
        };
        if let Some(r) = reply {
            stdout.write_all(r.to_string().as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
    }
    launched.shutdown().await;
    Ok(())
}

/// Pide a la instancia en ejecución que se detenga y espera a que lo haga.
fn stop(data_dir: &std::path::Path, admin: &AdminArgs) -> anyhow::Result<()> {
    if !is_running(admin) {
        println!("Fake Services no está en ejecución.");
        return Ok(());
    }
    let token = fakes_api::load_or_create_token(data_dir)?;
    let host = if admin.admin_host == "0.0.0.0" {
        "127.0.0.1"
    } else {
        &admin.admin_host
    };
    let addr = (host, admin.admin_port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| anyhow::anyhow!("dirección inválida"))?;
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(2))?;
    s.set_read_timeout(Some(Duration::from_secs(5)))?;
    write!(
        s,
        "POST /api/v1/system/shutdown HTTP/1.0\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\nContent-Length: 0\r\n\r\n"
    )?;
    let mut resp = String::new();
    let _ = s.read_to_string(&mut resp);
    if !resp.contains(" 202") {
        let status = resp.lines().next().unwrap_or("sin respuesta");
        anyhow::bail!(
            "la instancia no aceptó la orden ({status}). ¿Es una versión anterior o usa otra carpeta de datos?"
        );
    }
    for _ in 0..100 {
        std::thread::sleep(Duration::from_millis(100));
        if !is_running(admin) {
            println!("Fake Services se detuvo.");
            return Ok(());
        }
    }
    anyhow::bail!("Fake Services no se detuvo a tiempo")
}

fn admin_url(admin: &AdminArgs) -> String {
    let host = if admin.admin_host == "0.0.0.0" {
        "127.0.0.1"
    } else {
        &admin.admin_host
    };
    format!("http://{host}:{}", admin.admin_port)
}

/// ¿Hay una instancia de Fake Services respondiendo en el puerto de administración?
fn is_running(admin: &AdminArgs) -> bool {
    let host = if admin.admin_host == "0.0.0.0" {
        "127.0.0.1"
    } else {
        &admin.admin_host
    };
    let Some(addr) = (host, admin.admin_port)
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.next())
    else {
        return false;
    };
    health(addr).unwrap_or(false)
}

fn health(addr: SocketAddr) -> std::io::Result<bool> {
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_millis(500))?;
    s.set_read_timeout(Some(Duration::from_secs(2)))?;
    s.write_all(b"GET /healthz HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n")?;
    let mut buf = String::new();
    s.read_to_string(&mut buf)?;
    Ok(buf.starts_with("HTTP/1") && buf.contains(" 200") && buf.trim_end().ends_with("ok"))
}

/// Abre una URL en el navegador del sistema (sin fallar si no hay escritorio).
fn open_browser(url: &str) {
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
            println!("  Abre {url} en tu navegador.");
            return;
        }
    }
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open")
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    if result.is_err() {
        println!("  Abre {url} en tu navegador.");
    }
}

async fn serve(data_dir: PathBuf, admin: AdminArgs, browser: bool) -> anyhow::Result<()> {
    let mut opts = LaunchOptions::new(data_dir.clone());
    opts.admin_host = admin.admin_host;
    opts.admin_port = admin.admin_port;
    let launched = match fakes_api::launch(opts).await {
        Ok(l) => l,
        Err(e @ LaunchError::AdminPortInUse(_)) => {
            eprintln!("✗ {e}");
            eprintln!("  Si es Fake Services, ábrelo con: fakes abrir");
            std::process::exit(2);
        }
        Err(e) => return Err(e.into()),
    };

    let status = launched.app.status().await?;
    println!();
    println!("  Fake Services {}", fakes_app::VERSION);
    println!("  Servicios falsos. Integraciones reales.");
    println!();
    println!("  Interfaz:       {}", launched.admin_url);
    println!("  Simulaciones:   {}", status.central_url);
    for s in launched.app.list_services().await {
        let state = if s.running { "●" } else { "○" };
        println!("    {state} {:<20} {}", s.name, s.url);
    }
    for l in status.listeners.iter().filter(|l| l.error.is_some()) {
        println!("  ⚠ {}", l.error.as_deref().unwrap_or_default());
    }
    println!(
        "  Espacio:        {} ({})",
        status.workspace_name, status.workspace_path
    );
    println!(
        "  API:            {}/api/v1  (token: fakes token)",
        launched.admin_url
    );
    println!();
    println!("  Ctrl+C para salir.");

    if browser {
        open_browser(&launched.admin_url);
    }

    tokio::select! {
        _ = shutdown_signal() => {}
        _ = launched.app.shutdown_requested() => {}
    }
    println!("\nDeteniendo…");
    launched.shutdown().await;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = term => {},
    }
}
