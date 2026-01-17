use std::net::TcpListener;
use std::io;
use std::io::Read;
use std::str::from_utf8;

fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:7878")?;
    // saw multiple incoming connections possibility, how would syn-ack work then for tcp??? i am confused at this rn.
    
    match listener.accept() {
        Ok((mut _socket, addr) )=> {
            println!("new client connected from {addr:?}");
            let mut buff:[u8; 1024] = [0; 1024];
            let n = _socket.read(&mut buff)?;
            println!("Received Raw data: {:?}", &buff[..n]);
            println!("Received String data: {:?}", from_utf8(&buff[..n]));
        },
        Err(e) => {
            println!("Error couldn't accept connection. : {e:?}")
        }
    }
    

    Ok(())
}
