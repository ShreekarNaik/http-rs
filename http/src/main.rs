use std::net::{TcpListener, TcpStream, SocketAddr};
use std::io;
use std::io::{Read, Write};
use std::fs;
use std::thread;

fn handle_client(mut socket: TcpStream, addr: SocketAddr) -> io::Result<()> {
    println!("new client connected from {addr:?}");

    let mut buff: [u8; 1024] = [0; 1024];
    let n = socket.read(&mut buff)?;

    // parse just the request line, e.g. "GET / HTTP/1.1"
    let request = String::from_utf8_lossy(&buff[..n]);
    let request_line = request.lines().next().unwrap_or("");

    let (status_line, filename) = if request_line == "GET / HTTP/1.1" {
        ("HTTP/1.1 200 OK", "hello.html")
    } else {
        ("HTTP/1.1 404 NOT FOUND", "404.html")
    };

    let contents = fs::read_to_string(filename)?;
    let response = format!(
        "{status_line}\r\nContent-Length: {}\r\n\r\n{contents}",
        contents.len()
    );

    socket.write_all(response.as_bytes())?;
    Ok(())
}

fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:7878")?;
    println!("Listening on http://127.0.0.1:7878");

    loop {
        match listener.accept() {
            Ok((socket, addr)) => {
                thread::spawn(move || {
                    if let Err(e) = handle_client(socket, addr) {
                        eprintln!("Error handling client {addr:?}. : {e:?}");
                    };
                });
            }
            Err(e) => {
                eprintln!("Error couldn't accept connection. : {e:?}")
            }
        }
    }
}
