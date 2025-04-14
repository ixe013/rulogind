use clap::Parser;

use std::{
    env,
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    process::{Command, Stdio},
    str,
    sync::Arc,
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value_t = 513)]
    port: u16,
}

fn handle_client(expected_username: &str, allow_root: bool, mut stream: TcpStream) -> io::Result<()> {
    let mut buffer = [0; 256];
    let bytes_read = stream.read(&mut buffer)?;

    if bytes_read > 0 {
        let received_data = &buffer[..bytes_read];
        let parts: Vec<&str> = received_data
            .split(|&b| b == 0)
            .filter_map(|s| str::from_utf8(s).ok())
            .collect();

        let _ = parts.get(0).unwrap_or(&"").trim();
        let server_username_received = parts.get(1).unwrap_or(&"").trim();
        let _ = parts.get(2).unwrap_or(&"").trim();

        if server_username_received == expected_username
        || (allow_root && server_username_received == "root") {
            println!("Authenticated as {}", server_username_received);

            let greeting = "\0⛳Coucou\r\n";
            if !stream.write_all(greeting.as_bytes()).is_ok() {
                eprintln!("Impossible to greet.");
            }
            stream.flush()?;

            use std::os::unix::io::{AsRawFd, FromRawFd};

            let raw_fd = stream.as_raw_fd();
            let stdin_fd = unsafe { libc::dup(raw_fd) };
            let stdout_fd = unsafe { libc::dup(raw_fd) };
            let stderr_fd = unsafe { libc::dup(raw_fd) };

            let stdin = unsafe { Stdio::from_raw_fd(stdin_fd) };
            let stdout = unsafe { Stdio::from_raw_fd(stdout_fd) };
            let stderr = unsafe { Stdio::from_raw_fd(stderr_fd) };

            let child = Command::new("/bin/sh")
                .stdin(stdin)
                .stdout(stdout)
                .stderr(stderr)
                .spawn();

            match child {
                Ok(mut process) => {
                    if let Err(status) = process.wait() {
                        eprintln!("An error occured while waiting for the shell exit : {}", status);
                    }
                }
                Err(e) => {
                    eprintln!("Cannot start shell : {}", e);
                }
            }
        } else {
            println!("Authentification failed for {server_username_received}");
            stream.write_all(b"Login incorrect.\r\n")?;
        }
    } else if bytes_read == 0 {
        println!("Client disconnected.");
    } else {
        eprintln!("Error reading from client.");
    }

    println!("Goodbye.");

    Ok(())
}

fn main() -> io::Result<()> {
    let args = Args::parse();
    let mut handles = vec![];
    let bind_addr = format!("0.0.0.0:{}", args.port);

    println!("Listening on {}", bind_addr);

    let listener = TcpListener::bind(bind_addr)?;
    println!("Waiting for connection...");

    let expected_username = Arc::new(env::var("USER").unwrap_or_else(|_| String::from("unknown")));

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let addr = stream.peer_addr()?;
                println!("Connection accepted from {addr}");
                let expected_username_clone = Arc::clone(&expected_username);
                let handle = std::thread::spawn(move || {
                    if let Err(e) = handle_client(&expected_username_clone, true, stream) {
                        eprintln!("An error occured while handling the client connection: {e}");
                    }
                });
                handles.push(handle);
            }
            Err(e) => {
                eprintln!("An error occured trying to accept the connection: {e}");
            }
        }
    }

    for handle in handles {
        let _ = handle.join(); // On ignore le Result retourné par join()
    }

    Ok(())
}

