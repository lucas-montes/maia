//! Headless entrypoint for the Maia sync server.
//!
//! The server used to exist only as a library that the Tauri desktop app
//! started in-process, which meant the HTTP API was alive exactly as long as the
//! window was. This binary serves the same API without a GUI, so it can run as a
//! long-lived service.
//!
//! Configuration comes from the environment; see [`sync_server::config`] for the
//! variables and their defaults.

use anyhow::Context;
use clap::{Parser, Subcommand};
use sync_server::config::Config;
use sync_server::db;

#[derive(Parser)]
#[command(name = "sync-server", about = "Maia sync API server")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Serve the HTTP API until SIGTERM or ctrl-c.
    Serve,
    /// Apply pending schema migrations, then exit.
    Migrate,
    /// Report the database's schema version against this binary's.
    Status,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::from_env();

    match cli.command {
        Command::Serve => serve(config).await,
        Command::Migrate => migrate(config),
        Command::Status => status(config),
    }
}

async fn serve(config: Config) -> anyhow::Result<()> {
    // An unparseable RUST_LOG falls back to `info` rather than panicking, since
    // this runs unattended under systemd.
    let filter = std::env::var("RUST_LOG")
        .ok()
        .and_then(|value| value.parse::<tracing_subscriber::EnvFilter>().ok())
        .unwrap_or_else(|| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let db_path = config.db_path.clone();
    let addr = config.bind_addr.clone();
    let (port, handle) = sync_server::start_server(config).await?;
    tracing::info!(%addr, port, db = %db_path.display(), "sync-server listening");

    // Propagates the serve error instead of unwrapping inside the task. By the
    // time this resolves the router, the pool and its `Connection` have been
    // dropped, so SQLite has checkpointed the WAL.
    handle.await.context("server task failed")?;

    tracing::info!("shutdown complete");
    Ok(())
}

fn migrate(config: Config) -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_writer(std::io::stderr).init();
    let conn = open(&config.db_path)?;
    let from = db::user_version(&conn)?;
    db::migrate(&conn)?;
    tracing::info!(
        "schema at v{}",
        db::user_version(&conn)?.max(from)
    );
    Ok(())
}

fn status(config: Config) -> anyhow::Result<()> {
    let conn = open(&config.db_path)?;
    let version = db::user_version(&conn)?;
    println!("database:   {}", config.db_path.display());
    println!("schema:     v{version}");
    println!("binary:     v{}", db::LATEST_VERSION);
    if version > db::LATEST_VERSION {
        // Worth a non-zero exit: `ExecStartPre = migrate` would otherwise treat a
        // downgraded database as a success and start the server on it.
        eprintln!(
            "database is newer than this binary; restore a snapshot or rebuild forward"
        );
        std::process::exit(1);
    }
    if version < db::LATEST_VERSION {
        println!("pending:    {} migration(s) to apply", db::LATEST_VERSION - version);
    }
    Ok(())
}

fn open(path: &std::path::Path) -> anyhow::Result<rusqlite::Connection> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let conn = rusqlite::Connection::open(path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
    Ok(conn)
}