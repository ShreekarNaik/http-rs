# http — a multi-threaded HTTP server in Rust

A from-scratch HTTP/1.1 web server, built as a learning project while
working through [the Rust book's web server
chapter](https://doc.rust-lang.org/book/ch21-00-final-project-a-web-server.html).

No frameworks, no `hyper`, no `tokio` — just `std::net`, threads, and a
hand-rolled thread pool. The point is to *feel* how the pieces fit
together, not to be production-ready.

## What it does

- Binds a `TcpListener` on `127.0.0.1:7878`.
- Parses the HTTP request line and routes on it:
  - `GET /` → serves `hello.html` (200 OK)
  - `GET /sleep` → sleeps 5s first, then serves `hello.html` (to prove
    the pool is actually concurrent)
  - anything else → serves `404.html` (404 NOT FOUND)
- Uses a **custom thread pool** (not a thread-per-connection spawn) to
  handle requests concurrently with a fixed number of workers.
- **Shuts down gracefully** on `ctrl-c`: stops accepting connections and
  joins all the worker threads before exiting.

## How it's structured

```
http/
├── src/
│   ├── main.rs   # listener, routing, graceful shutdown
│   └── lib.rs    # the ThreadPool
├── hello.html    # served on GET /
├── 404.html      # served on anything else
├── Cargo.toml
└── notes/        # long-form notes written alongside the code
```

### The thread pool (`lib.rs`)

A `ThreadPool` holds a fixed number of `Worker` threads and a shared
job queue (an `mpsc::Receiver` behind an `Arc<Mutex<...>>`). `execute`
boxes a closure into a `Job` and pushes it onto the queue; workers pull
jobs off and run them. `Drop` on the pool closes the channel and joins
every worker, so nothing leaks.

This is the naive-but-educational version: it holds the mutex lock while
blocking on `recv`, and uses `unwrap` liberally. Those are exactly the
compromises you'd tighten up in a real server, but they'd obscure the
core ideas if I did them here.

## Running it

```sh
cd http
cargo run
# Listening on http://127.0.0.1:7878
```

Then, in another terminal:

```sh
curl http://127.0.0.1:7878/          # hello.html
curl http://127.0.0.1:7878/sleep     # waits 5s (proves concurrency)
curl http://127.0.0.1:7878/nope      # 404
```

Press `ctrl-c` to see the graceful shutdown log the workers being joined.

## The concurrency proof

`/sleep` exists to demonstrate *why* the thread pool matters. Fire a few
slow requests at once and watch other workers keep serving `/`:

```sh
# in three separate terminals, quickly:
curl http://127.0.0.1:7878/sleep
curl http://127.0.0.1:7878/sleep
curl http://127.0.0.1:7878/          # returns immediately
```

With a single-threaded server that third request would wait behind the
two 5-second sleeps. With the pool it returns instantly.

## What's intentionally missing

This is a teaching project, not a web framework. It deliberately skips:

- TLS / HTTPS
- keep-alive and pipelining
- chunked transfer encoding
- full header / body parsing (only the request line is read)
- request limits and timeouts
- any real error handling beyond `unwrap` + log-and-continue

## The `notes/` folder

The `notes/` directory is a long-form knowledge base written *alongside*
the code — it teaches the *why* behind each step (OSI, TCP lifecycle,
framing, error modes, concurrency). Read top-to-bottom on a first pass.
