# 00 — Orientation: where is our code, and what is it doing?

Before we write a single line, we need a mental picture of *where*
our code lives relative to everything else, and *what work* it is
(and isn't) doing.

This file is the map. Every later file refers back to it.

---

## 1. The big picture: who is talking to whom?

When you load a webpage, this is what's happening in slow motion:

```
        YOUR LAPTOP                                SOME SERVER
   ┌──────────────────┐                       ┌──────────────────┐
   │  browser (app)   │                       │  web server (app)│
   ├──────────────────┤                       ├──────────────────┤
   │  OS kernel       │   ◄── network ──►     │  OS kernel       │
   ├──────────────────┤                       ├──────────────────┤
   │  network card    │                       │  network card    │
   └────────┬─────────┘                       └────────┬─────────┘
            │                                          │
            └──── wires, fiber, radio waves ───────────┘
                   (switches, routers in between)
```

Two key observations:

1. **Your application code does not touch the wire directly.**
   You don't send electrons; you ask the operating system kernel to
   send bytes, and the kernel handles everything below it.

2. **The "network" between two computers is not a single thing.**
   It's a stack of cooperating systems, each solving a different
   problem. The OSI model is how we describe those layers.

---

## 2. The OSI model (and the more honest TCP/IP model)

The OSI model is a *conceptual* 7-layer model. Real networks use
roughly 4 layers (the TCP/IP model). Both are useful — OSI for
talking about ideas, TCP/IP for matching reality.

```
  OSI layer        | TCP/IP layer | Example          | Who implements it?
  -----------------+--------------+------------------+----------------------
  7. Application   | Application  | HTTP, SMTP, SSH  | YOU (this project!)
  6. Presentation  |     "        | TLS, encoding    | libraries / app
  5. Session       |     "        | (rarely separate)| libraries / app
  -----------------+--------------+------------------+----------------------
  4. Transport     | Transport    | TCP, UDP         | OS kernel
  -----------------+--------------+------------------+----------------------
  3. Network       | Internet     | IP, routing      | OS kernel
  -----------------+--------------+------------------+----------------------
  2. Data link     | Link         | Ethernet, Wi-Fi  | OS + network card
  1. Physical      |     "        | wires, radio     | hardware
```

Things to internalise:

- **We are building HTTP — layer 7.** Everything below is provided.
- **TCP is provided by the kernel.** When we call into Rust's
  `std::net::TcpStream`, the standard library is making a system call
  (a request to the kernel) to do TCP work on our behalf.
- **Each layer talks only to the one directly above/below it.**
  HTTP doesn't know about IP addresses or packet routing. It just
  asks TCP to "deliver these bytes reliably to that peer."

---

## 3. What does TCP actually give us?

TCP (Transmission Control Protocol) gives our application:

- a **reliable, ordered, byte stream** between two endpoints.
- "reliable" = bytes you sent will arrive, or you'll get an error.
- "ordered" = bytes arrive in the order you sent them.
- "byte stream" = there are NO messages, NO requests, NO records —
  just a sequence of bytes flowing in each direction.

That last point is the most important one for us:

> TCP does **not** know about "HTTP requests" or "messages."
> If you write `"hello world"` then `"goodbye"`, the receiver might
> read it as `"hello worldgoodbye"`, `"hel"` + `"lo worldgoodbye"`,
> or any other chunking. The byte order is preserved, but the
> *grouping* is not.

This is the seed of an enormous amount of HTTP's design.
HTTP exists *partly* to impose structure (request/response, headers,
body boundaries) on top of TCP's structureless byte stream.

---

## 4. What does UDP give us, by contrast?

UDP (User Datagram Protocol) gives:

- **discrete datagrams** (messages), not a stream.
- **no reliability** — packets can be lost, duplicated, or arrive
  out of order, and you won't be told.
- **no connection** — just send a packet at an address and hope.

We are using TCP because HTTP requires reliability and ordering.
But it's worth knowing UDP exists — it's what DNS, video calls,
and game servers use, because losing one packet matters less than
adding latency to wait for retransmission.

---

## 5. Sockets — the application's handle on the network

Across nearly every operating system, the API the application uses
to do networking is called the **socket** API. Originally from BSD
Unix in the 1980s, now everywhere.

A socket is a kernel-owned object that represents one endpoint of a
network connection. Your program holds a *handle* to it (a small
integer on Unix called a "file descriptor"; Rust hides this behind
the `TcpStream` / `TcpListener` types). You read and write bytes
through that handle; the kernel does the actual sending.

Key mental model:

```
   Rust code:  let mut stream = TcpStream::connect("1.2.3.4:80")?;
                                            │
                                            │  (system call)
                                            ▼
   Kernel:     opens a TCP socket
               performs the TCP handshake (SYN, SYN-ACK, ACK)
               returns a file descriptor (e.g. fd 5)
                                            │
                                            ▼
   Rust:       wraps fd 5 in a TcpStream struct, hands it back to you
```

When you later call `stream.write(b"GET / HTTP/1.1\r\n\r\n")`,
the bytes go into a kernel-managed buffer; the kernel will
package them into TCP segments and ship them out.

---

## 6. What does HTTP actually look like on the wire?

A minimal HTTP/1.1 request is just text, ending lines with `\r\n`:

```
GET / HTTP/1.1\r\n
Host: example.com\r\n
\r\n
```

The corresponding response is also just text, then maybe a body:

```
HTTP/1.1 200 OK\r\n
Content-Type: text/plain\r\n
Content-Length: 13\r\n
\r\n
Hello, world!
```

That's it. No magic. HTTP is, at its core, a *text framing convention*
agreed on by both ends, layered on top of a TCP byte stream.

The complexity of real HTTP comes from:
- How do you know where headers end and the body begins?
  (Answer: a blank line, `\r\n\r\n`.)
- How do you know how long the body is?
  (Several answers, all painful: `Content-Length`,
  `Transfer-Encoding: chunked`, or "until the connection closes".)
- What about persistent connections, pipelining, compression,
  caching, ranges, partial content, websocket upgrades...?

We will encounter each of those by hitting the problem they solve,
not by reading a 200-page spec up front.

---

## 7. Where we go from here

Next: open a TCP listener in Rust, connect to it with a tool we
already have (`nc` / netcat or `curl`), and **observe what bytes
actually arrive** when a real HTTP client talks to us.

That experiment will surface a pile of questions:
- Why is the data arriving in chunks?
- How do I know when the request is "done"?
- What happens if two clients connect at once?
- What if the client sends something malformed?

Those questions will drive the next several files in this directory.
