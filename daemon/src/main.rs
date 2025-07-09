use std::{
    fs::{self, File, Permissions},
    os::unix::fs::PermissionsExt,
    path::Path,
};

use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
};

mod model;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    tracing_subscriber::fmt::init();

    tracing::info!("Maia daemon started");
    let socket_path = "/tmp/maia.sock";
    let path = Path::new(socket_path);


    if path.exists() {
        fs::remove_file(path).expect("Failed to remove existing socket");
    }

    tracing::info!("Creating socket at {}", socket_path);
    let listener = UnixListener::bind(socket_path).expect("Failed to open socket");
    tracing::info!("Socket bound to {}", socket_path);
    loop {
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

                            if let Ok(message) = String::from_utf8(buf) {

                                let response = format!("Daemon got: {}", message);
                                if let Err(err) = stream.write_all(response.as_bytes()).await {
                                    tracing::error!(err=%err, "Stream is not ready");
                                };
                                if let Err(err) = stream.flush().await {
                                    tracing::error!(err=%err, "Stream is not ready");
                                };
                            } else {
                                tracing::error!("Received invalid UTF-8 data");
                            }
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
    }
}
