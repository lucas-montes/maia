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
    handle_receipt_file(&state, PathBuf::from("receips/WhatsApp Image 2025-04-13 at 17.20.18.jpeg")).await;



    // let monitor_task: Pin<Box<dyn Future<Output = ()> + Send>> = if state.config().needs_to_listen()
    // {
    //     Box::pin(monitor_dirs(state.clone()))
    // } else {
    //     Box::pin(tokio::task::yield_now())
    // };

    // let (_monitor_dirs_task, _listen_socket_task) = tokio::join!(monitor_task, listen_socket());

    tracing::info!("Shutting down");
}
