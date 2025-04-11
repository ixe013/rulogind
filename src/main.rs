use clap::Parser;

use std::{
    env,
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    process::{Command, Stdio},
    str,
    sync::Arc,
    os::unix::io::{FromRawFd, RawFd},
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value_t = 513)]
    port: u16,
}

// Définition des états
enum State {
    Initial,
    Authenticating,
    Established,
    Closing,
}

fn handle_client(expected_username: &str, allow_root: bool, mut stream: TcpStream) -> io::Result<()> {
    let mut state = State::Initial;
    let mut child: Option<std::process::Child> = None;
    let mut child_stdin: Option<std::fs::File> = None;
    let mut child_stdout: Option<std::fs::File> = None;
    let mut child_stderr: Option<std::fs::File> = None;

    let mut buffer = [0; 1024];

    loop {
        match state {
            State::Initial => {
                println!("Initial state...");
                state = State::Authenticating;
            }
            State::Authenticating => {
                println!("Authenticating...");
                let bytes_read = stream.read(&mut buffer)?;
                if bytes_read > 0 && buffer[0] == 0 {
                    let received_data = &buffer[1..bytes_read];
                    let parts: Vec<&str> = received_data
                        .split(|&b| b == 0)
                        .filter_map(|s| str::from_utf8(s).ok())
                        .collect();

                    let _ = parts.get(0).unwrap_or(&"").trim();
                    let server_username_received = parts.get(1).unwrap_or(&"").trim();
                    let _ = parts.get(2).unwrap_or(&"").trim();

                    if server_username_received == expected_username
                        || (allow_root && server_username_received == "root")
                    {
                        println!("Authenticated as {}", server_username_received);

                        let greeting = "\0Coucou\r\n";
                        stream.write_all(greeting.as_bytes())?;
                        stream.flush()?;

                        // Créer le processus enfant et les pipes
                        let stdin_child = create_pipe()?;
                        let stdout_child = create_pipe()?;
                        let stderr_child = create_pipe()?;

                        child = Some(Command::new("/bin/sh")
                            .stdin(unsafe { Stdio::from_raw_fd(stdin_child.0) })
                            .stdout(unsafe { Stdio::from_raw_fd(stdout_child.1) })
                            .stderr(unsafe { Stdio::from_raw_fd(stderr_child.1) })
                            .spawn()?);

                        unsafe {
                            libc::close(stdin_child.0);
                            libc::close(stdout_child.1);
                            libc::close(stderr_child.1);
                        }

                        child_stdin = Some(unsafe { std::fs::File::from_raw_fd(stdin_child.1) });
                        child_stdout = Some(unsafe { std::fs::File::from_raw_fd(stdout_child.0) });
                        child_stderr = Some(unsafe { std::fs::File::from_raw_fd(stderr_child.0) });

                        state = State::Established;
                    } else {
                        println!("Authentification failed for {server_username_received}");
                        stream.write_all(b"Login incorrect.\r\n")?;
                        state = State::Closing;
                    }
                } else if bytes_read == 0 {
                    println!("Client disconnected.");
                    state = State::Closing;
                } else {
                    eprintln!("Error reading from client.");
                    state = State::Closing;
                }
            }
            State::Established => {
                println!("Established");
                // Lire les données du client, les analyser, les écrire dans le shell
                // Lire les données du shell et les écrire dans le client
                // Utiliser des opérations non bloquantes et `select` (ou équivalent) pour attendre les événements
                if let Some(ref mut child_stdin) = child_stdin {
                    let client_data_available = stream.read(&mut buffer).unwrap_or(0);
                    if client_data_available > 0 {
                        let data = &buffer[..client_data_available];
                        // Ici, analyser les caractères de contrôle
                        child_stdin.write_all(data)?;
                    }
                }

                if let Some(ref mut child_stdout) = child_stdout {
                    let shell_data_available = child_stdout.read(&mut buffer).unwrap_or(0);
                    if shell_data_available > 0 {
                        let data = &buffer[..shell_data_available];
                        stream.write_all(data)?;
                    }
                }

                if let Some(ref mut child_stderr) = child_stderr {
                    let shell_err_available = child_stderr.read(&mut buffer).unwrap_or(0);
                    if shell_err_available > 0 {
                        let data = &buffer[..shell_err_available];
                        stream.write_all(data)?;
                    }
                }

                // Vérifier si le processus enfant s'est terminé
                if let Some(ref mut child) = child {
                    if let Some(status) = child.try_wait()? {
                        println!("Shell exited with status: {}", status);
                        state = State::Closing;
                    }
                }

                //TODO: Add a timeout to avoid infinite loop
            }
            State::Closing => {
                // Fermer les connexions et les processus
                println!("Closing connection");
                if let Some(mut child) = child {
                    child.kill()?;
                    child.wait()?;
                }
                break;
            }
        }
    }

    Ok(())
}

// Fonction utilitaire pour créer une paire de pipes
fn create_pipe() -> io::Result<(RawFd, RawFd)> {
    let mut pipe_fds = [0; 2];
    let ret = unsafe { libc::pipe(pipe_fds.as_mut_ptr()) };
    if ret == 0 {
        Ok((pipe_fds[0], pipe_fds[1]))
    } else {
        Err(io::Error::last_os_error())
    }
}

fn main() -> io::Result<()> {
    let args = Args::parse();
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
                if let Err(e) = handle_client(&expected_username, true, stream) {
                    eprintln!("An error occured while handling the client connection: {e}");
                }
            }
            Err(e) => {
                eprintln!("An error occured trying to accept the connection: {e}");
            }
        }
    }

    Ok(())
}

