use std::{
    io::{self, Read, Write},
    os::unix::net::UnixStream,
};

mod cli;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = cli::Cli::handle();

    let mut stream = UnixStream::connect("/tmp/maia.sock").expect("Failed to open socket");

    loop {
        // Try to write data, this may still fail with `WouldBlock`
        // if the readiness event is a false positive.
        match stream.write(cli.message()) {
            Ok(n) => {
                println!("Wrote {} bytes", n);
                break;
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                continue;
            }
            Err(err) => {
                eprintln!("Stream is not ready {}", err);
            }
        }
    }

    stream.flush().expect("msg");
    println!("Sent");

    // // Shutdown the write half to signal end of input (optional, helps daemon detect EOF)
    // stream.shutdown(std::net::Shutdown::Write)?;

    // Wait for and read the response
    let mut response = [0; 1024]; // Buffer for response
    match stream.read(&mut response) {
        Ok(_) => {
            if response.is_empty() {
                println!("No response received from daemon");
            } else {
                println!("Received: {}", String::from_utf8_lossy(&response));
            }
        }
        Err(e) => {
            eprintln!("Error reading response: {}", e);
        }
    }

    Ok(())
}
