use std::net::{TcpListener, TcpStream, SocketAddr};
use std::io;
use std::io::{Read, Write};
use std::str::from_utf8;
use std::thread;
use std::time::Duration;

fn handle_client(mut socket: TcpStream, addr: SocketAddr) -> io::Result<()> {
    println!("new client connected from {addr:?}");
    // thread::sleep(Duration::from_secs(5));
    let mut buff:[u8; 1024] = [0; 1024];
    let n: usize = socket.read(&mut buff)?;
    println!("Received Raw data: {:?}", &buff[..n]);
    println!("Received String data: {:?}", from_utf8(&buff[..n]));
    let message: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 13\r\n\r\nHello, world!";
    socket.write_all(message)?;
    println!("Sent response: {:?}", from_utf8(message));
    Ok(())
}

fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:7878")?;

    loop {
        match listener.accept() {
            Ok((socket, addr) ) => {
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
