use std::{path::PathBuf, pin::Pin};

use receipts::handle_receipt_file;
use socket::listen_socket;
use state::State;
use watcher::monitor_dirs;

mod config;
mod notifications;
mod socket;
mod state;
mod watcher;
mod receipts;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().json().init();

    let state = State::initialize();


    // handle_receipt_file(&state, PathBuf::from("receips/WhatsApp Image 2025-04-13 at 17.11.44.jpeg")).await;


    let (monitor_dirs_task, listen_socket_task) = tokio::join!(monitor_dirs(state), listen_socket());

    tracing::info!("Shutting down");
}
