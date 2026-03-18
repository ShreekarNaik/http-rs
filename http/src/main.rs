//! A simple multi-threaded HTTP server, built as a learning project
//! following the Rust book's "Final Project: Building a Multithreaded
//! Web Server" chapter.
//!
//! What I built, layer by layer:
//!
//!   1. `TcpListener` to accept TCP connections.
//!   2. A tiny hand-rolled HTTP/1.1 request *parser* (enough to find
//!      the request line and route on it).
//!   3. Routing: `/` serves `hello.html`, `/sleep` simulates a slow
//!      request, anything else is a 404.
//!   4. A custom `ThreadPool` (in `lib.rs`) so I handle clients
//!      concurrently without spawning an unbounded number of threads.
//!   5. Graceful shutdown: `Drop` on the pool joins workers, and I
//!      install a ctrl-c handler so the process exits cleanly.
//!
//! This is deliberately NOT production-grade — it exists to make the
//! ideas concrete. No TLS, no keep-alive, no chunked encoding, no
//! proper header parsing. Just enough to feel how the pieces fit.

use std::{
    fs,
    io::{prelude::*, BufReader},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use http::ThreadPool;

fn main() {
    // A shared "am I shutting down?" flag. The ctrl-c handler flips
    // it; the accept loop checks it each iteration. Using an atomic
    // means I can safely share it between threads without a lock.
    let running = Arc::new(AtomicBool::new(true));

    // A pool of 4 worker threads. Requests get handed to the pool
    // instead of spawning a brand-new thread per connection.
    let pool = ThreadPool::new(4);

    // Install a ctrl-c handler so I can break out of the accept loop
    // and shut down cleanly (drop the pool → join the workers).
    let r = Arc::clone(&running);
    ctrlc::set_handler(move || {
        println!("\nReceived ctrl-c, shutting down...");
        r.store(false, Ordering::SeqCst);
    })
    .expect("failed to set ctrl-c handler");

    let listener = TcpListener::bind("127.0.0.1:7878").unwrap();
    println!("Listening on http://127.0.0.1:7878");

    // The dispatcher loop. Each accepted connection gets pushed onto
    // the thread pool as a "job".
    for stream in listener.incoming() {
        // Respect the shutdown flag — stop accepting new connections
        // once I've been told to stop.
        if !running.load(Ordering::SeqCst) {
            break;
        }

        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                // A single bad connection shouldn't kill the server.
                eprintln!("Error accepting connection: {e}");
                continue;
            }
        };

        pool.execute(move || {
            handle_connection(stream);
        });
    }

    println!("Server stopped. Dropping thread pool...");
    // `pool` is dropped here, joining all the workers.
}

/// Read a single HTTP request from `stream`, figure out what the
/// client wants, and write back a response.
fn handle_connection(mut stream: TcpStream) {
    // `BufReader` lets me read line-by-line (via `lines()`) instead of
    // messing with raw byte buffers.
    let buf_reader = BufReader::new(&stream);

    // Grab just the request line, e.g. `GET / HTTP/1.1`.
    // I use `.next()` because I only care about the first line for
    // routing. A real parser would read all the headers too.
    let request_line = match buf_reader.lines().next() {
        Some(Ok(line)) => line,
        _ => {
            // Couldn't even read a request line — just give up.
            return;
        }
    };

    // Route based on the request line.
    let (status_line, filename) = match &request_line[..] {
        "GET / HTTP/1.1" => ("HTTP/1.1 200 OK", "hello.html"),
        "GET /sleep HTTP/1.1" => {
            // Simulate a slow request. With a single-threaded server
            // this would block *everyone* for 5s. With the pool, other
            // workers can still serve requests.
            thread::sleep(Duration::from_secs(5));
            ("HTTP/1.1 200 OK", "hello.html")
        }
        _ => ("HTTP/1.1 404 NOT FOUND", "404.html"),
    };

    let contents = fs::read_to_string(filename).unwrap_or_else(|_| {
        // If the file is missing (shouldn't happen, but be safe),
        // fall back to a hardcoded 404 body.
        String::from("<h1>404</h1><p>Not Found</p>")
    });
    let length = contents.len();

    let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{contents}");

    // I need `&stream` back (it was borrowed by `buf_reader`), so
    // rebuild the writer. `stream` is still valid — the borrow ended
    // when `buf_reader` dropped at the end of the request-line read.
    stream.write_all(response.as_bytes()).unwrap();
}
