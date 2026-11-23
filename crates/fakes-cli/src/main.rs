//! `fakes`: ejecuta Fake Services sin interfaz de escritorio.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use fakes_api::{DEFAULT_ADMIN_PORT, LaunchError, LaunchOptions};

#[derive(Parser)]
#[command(
    name = "fakes",
    version,
    about = "Fake Services: simulador local de servicios externos",
    long_about = "Fake Services simula APIs externas (MiFiel, Belvo, S3, Stripe...) en tu máquina.\n\
                  Servicios falsos. Integraciones reales."
)]
struct Cli {
    /// Carpeta de datos (registro de espacios, historial, token).
    #[arg(long, global = true, env = "FAKES_DATA_DIR")]
    data_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Inicia el servidor de simulación y la administración (por defecto).
    Serve {
        /// Dirección de la API de administración y la interfaz web.
        #[arg(long, default_value = "127.0.0.1", env = "FAKES_ADMIN_HOST")]
        admin_host: String,
        /// Puerto de la API de administración y la interfaz web.
        #[arg(long, default_value_t = DEFAULT_ADMIN_PORT, env = "FAKES_ADMIN_PORT")]
        admin_port: u16,
    },
    /// Muestra el token de la API de administración.
    Token,
    /// Muestra la carpeta de datos en uso.
    Paths,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn,tower_http=warn".into()),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();
    let data_dir = cli.data_dir.unwrap_or_else(fakes_config::default_data_dir);

    match cli.command.unwrap_or(Command::Serve {
        admin_host: "127.0.0.1".into(),
        admin_port: DEFAULT_ADMIN_PORT,
    }) {
        Command::Serve {
            admin_host,
            admin_port,
        } => serve(data_dir, admin_host, admin_port).await,
        Command::Token => {
            println!("{}", fakes_api::load_or_create_token(&data_dir)?);
            Ok(())
        }
        Command::Paths => {
            println!("Datos:  {}", data_dir.display());
            println!("Token:  {}", data_dir.join(fakes_api::TOKEN_FILE).display());
            println!("Base:   {}", data_dir.join("fakes.db").display());
            Ok(())
        }
    }
}

async fn serve(data_dir: PathBuf, admin_host: String, admin_port: u16) -> anyhow::Result<()> {
    let mut opts = LaunchOptions::new(data_dir.clone());
    opts.admin_host = admin_host;
    opts.admin_port = admin_port;
    let launched = match fakes_api::launch(opts).await {
        Ok(l) => l,
        Err(e @ LaunchError::AdminPortInUse(_)) => {
            eprintln!("✗ {e}");
            std::process::exit(2);
        }
        Err(e) => return Err(e.into()),
    };

    let status = launched.app.status().await?;
    println!();
    println!("  Fake Services {}", fakes_app::VERSION);
    println!("  Servicios falsos. Integraciones reales.");
    println!();
    println!(
        "  Espacio:        {} ({})",
        status.workspace_name, status.workspace_path
    );
    println!("  Simulaciones:   {}", status.central_url);
    for s in launched.app.list_services().await {
        let state = if s.running { "●" } else { "○" };
        println!("    {state} {:<20} {}", s.name, s.url);
    }
    for l in status.listeners.iter().filter(|l| l.error.is_some()) {
        println!("  ⚠ {}", l.error.as_deref().unwrap_or_default());
    }
    println!("  Administración: {}", launched.admin_url);
    println!(
        "  API:            {}/api/v1  (token: fakes token)",
        launched.admin_url
    );
    println!("  Datos:          {}", data_dir.display());
    println!();
    println!("  Ctrl+C para salir.");

    shutdown_signal().await;
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
