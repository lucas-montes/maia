use std::pin::Pin;

use socket::listen_socket;
use state::State;
use watcher::monitor_dirs;

mod config;
mod notifications;
mod socket;
mod state;
mod watcher;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().json().init();

    let state = State::initialize();

    let monitor_task: Pin<Box<dyn Future<Output = ()> + Send>> = if state.config().needs_to_listen()
    {
        Box::pin(monitor_dirs(state.config().paths_to_watch()))
    } else {
        Box::pin(tokio::task::yield_now())
    };

    let (_monitor_dirs_task, _listen_socket_task) = tokio::join!(monitor_task, listen_socket());

    tracing::info!("Shutting down");
}
