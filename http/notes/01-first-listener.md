# 01 — First TCP listener: what we built, what we saw, what we learned

## The program

`src/main.rs` does five things:

1. Binds a `TcpListener` to `127.0.0.1:7878`.
2. Calls `accept()` once — blocks until a client connects.
3. Reads up to 1024 bytes from that connection into a stack buffer.
4. Prints the raw bytes and the UTF-8 interpretation.
5. Exits (which closes both sockets).

## What we observed

When `curl http://127.0.0.1:7878/` connected, we received:

```
GET / HTTP/1.1\r\n
Host: 127.0.0.1:7878\r\n
User-Agent: curl/8.7.1\r\n
Accept: */*\r\n
\r\n
```

Things to note:

- **It's plain ASCII text.** Byte 71 is `G`, 69 is `E`, 84 is `T`. The
  "byte stream" we read is just a sequence of characters.
- **Lines end with `\r\n`** (bytes 13, 10), not `\n` alone.
  This is the HTTP/1.1 spec. Why two characters? Historical — many
  internet protocols (SMTP, FTP, HTTP) use CRLF because early
  teleprinters needed to *return the carriage* (`\r`) AND
  *advance the line* (`\n`). Unix uses just `\n` for file newlines;
  internet protocols stuck with the older convention.
- **The request ends with a blank line: `\r\n\r\n`.** That's the
  header delimiter. There is no body (a GET typically has none),
  so the request is complete after the blank line.
- **No `Content-Length` was sent.** Because no body — there's nothing
  to measure. (When we POST, we'll see one.)

## Listener socket vs connection socket

The single most important conceptual point from this session.

- `TcpListener::bind(addr)` creates a **listening socket** that
  watches port 7878 for incoming connections.
  This socket does not carry data.
- `listener.accept()` creates a **new socket** — a `TcpStream` —
  representing one specific connection to one specific client.
  Reading and writing happens on this stream, not the listener.
- A single listener can accept many connections over its lifetime.
  Each connection is its own independent socket.

A TCP connection is uniquely identified by the 4-tuple:

```
(server-ip, server-port, client-ip, client-port)
```

Many clients can connect to the same server port simultaneously
because each will have a different (client-ip, client-port).

## The SYN / SYN-ACK / ACK handshake

The 3-way handshake happens in the **kernel**, not in our code.

```
Client                                          Server kernel
  │  ── SYN ───────────────────────────►            │
  │  ◄──────────────────── SYN-ACK ──               │
  │  ── ACK ───────────────────────────►            │
  │                                       (connection established,
  │                                        placed in accept queue)
                                                    │
                                       accept() returns a TcpStream
```

Key consequences:

- By the time `accept()` returns, the handshake is already done.
  Your code never sees SYN/ACK packets directly.
- The kernel keeps an **accept queue** (also called *backlog*).
  Connections that have completed the handshake but haven't been
  `accept()`ed yet sit here. The queue has a finite size; if it
  overflows, new SYNs are dropped — clients see "connection
  refused" or timeouts.
- This is why `accept()` is fast: most of the work (handshake,
  buffer setup) is already done. `accept()` just hands you a
  ready-to-use socket.

## Rust idioms picked up this session

- `fn main() -> io::Result<()>` lets you use `?` in main.
- `?` operator: propagates `Err` early, unwraps `Ok`.
  Only usable in functions returning `Result` (or `Option`).
- `&buff[..n]` slices a buffer to the meaningful prefix. Always
  slice after `read()` — the bytes past `n` are uninitialized
  from this read's perspective.
- Prefer `from_utf8` (returns `Result`) over `from_utf8_lossy`
  (silently substitutes replacement characters) for network input.
  Bytes are not text until proven.
- `let mut buff = [0u8; 1024];` — annotate the value type, not the
  binding, when both convey the same info. Less repetition.
- `_name` prefix means "intentionally unused, please don't warn."
  If you ARE using a variable, drop the underscore.

## Open questions surfaced (we will return to these)

- We only handled one client, then exited. How do we serve many?
- We read once with a 1024-byte buffer. What if the request is bigger?
- What if `read()` returns a *partial* request — only the headers, or
  only half a header line? When do we know we have the whole thing?
- We didn't reply at all. What does a real HTTP response look like
  on the wire, byte by byte?
