use std::{fs, path::Path};

use serde::{Deserialize, Serialize};
use shared::protocol::MessageProtocol;
use tokio::{
    io::{self, AsyncWriteExt},
    net::UnixListener,
};

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
enum Message {
    //TODO: join those two together
    Goal(todo::goals::Commands),
    Task(todo::tasks::Commands),
    //TODO: maybe combine this one with AI to make it more general
    Knowledge(knowledge::cli::Commands),
    Chrome
}
impl MessageProtocol for Message {}



async fn handle_message(message: Message)-> String {

    todo!()
}

pub async fn listen_socket() {
    tracing::info!("Maia daemon started");
    let socket_path = "/tmp/maia.sock";
    let path = Path::new(socket_path);

    if path.exists() {
        fs::remove_file(path).expect("Failed to remove existing socket");
    }

    let listener = UnixListener::bind(socket_path).expect("Failed to open socket");
    tracing::info!("Socket bound to {}", socket_path);
    loop {
        tracing::info!("ready ");
        match listener.accept().await {
            Ok((mut stream, _addr)) => {
                loop {
                    //Wait for the socket to be readable
                    if let Err(err) = stream.readable().await {
                        tracing::error!(err=%err, "Failed to wait for readable: ");
                        continue;
                    };

                    let mut buf = Vec::with_capacity(4096);
                    match stream.try_read_buf(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            let response = match Message::from_bytes(&buf) {
                                Ok(message) => handle_message(message).await,
                                Err(err) => {
                                    tracing::error!(err=%err, "Failed to parse message");
                                    "error".into()
                                }
                            };

                            if let Err(err) = stream.write_all(response.as_bytes()).await {
                                tracing::error!(err=%err, "Stream is not ready");
                            };
                            if let Err(err) = stream.flush().await {
                                tracing::error!(err=%err, "Stream is not ready");
                            };
                        }
                        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                            continue;
                        }
                        Err(err) => {
                            tracing::error!(err=%err, "Stream is not ready");
                        }
                    }
                }
            }
            Err(err) => tracing::error!(err=%err, "Connection failed "),
        }
        tracing::info!("ready ");
    }
}
