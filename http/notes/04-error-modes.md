# 04 — Error modes: when "success" lies to you

A network call returning `Ok(())` does **not** mean what you think it
means. This chapter is about the gap between "my syscall succeeded"
and "the network operation succeeded" — and the policy decisions that
gap forces on you.

---

## The central claim

> Most TCP errors are **asynchronous** — discovered after the syscall
> that caused them has already returned successfully.

If you internalize one thing from this chapter, internalize that.
Every other lesson here is a corollary.

---

## Why is this true?

Because of the **kernel send buffer** (see `02-tcp-timing-and-lifecycle.md`).

When you call `socket.write_all(bytes)?`, here is what actually happens:

```
your code            kernel send buffer        the network
   │                       │                        │
   │  write(bytes)         │                        │
   ├──────────────────────►│                        │
   │   "got it, returning" │                        │
   │◄──────────────────────┤  Ok(())                │
   │                       │   later... eventually  │
   │                       ├───────────────────────►│
   │                       │     ... maybe          │
   │                       │     the peer is gone   │
   │                       │◄─── RST ◄──────────────┤
   │                       │                        │
   │      (you have already moved on)               │
```

Your `Ok(())` proves bytes are queued in **your** kernel. It does NOT
prove:

- the peer received them
- the peer is even alive
- the network ever carried them

The kernel is essentially saying: "I accept this responsibility,"
not: "this has been delivered." Real delivery confirmation in TCP
takes the form of ACKs, which the kernel handles internally and
never surfaces to your application via `write`.

---

## How errors *do* surface — eventually

If the connection breaks, you learn about it on **the next syscall**
that requires interaction with that broken connection:

| What you do next   | What you might see                  |
| ------------------ | ----------------------------------- |
| Another `write()`  | `Err(BrokenPipe)` or `ConnectionReset` |
| A `read()`         | `Err(ConnectionReset)` or `Ok(0)` (EOF) |
| `flush()`          | error from queued failed writes     |
| `shutdown()`       | error if state already broken       |
| nothing            | error is silently absorbed at drop  |

This is why production servers do "health-check writes" — small
periodic writes whose only purpose is to surface a dead connection
before they have important data to send.

---

## The error taxonomy you actually meet

We met these by running deliberate experiments against the server:

### `ConnectionReset` — peer sent RST

The peer rudely terminated the connection (not a graceful FIN). You
see this when you `read` or `write` after a peer crash, abrupt close,
or explicit `SO_LINGER 0`-style reset.

- **Cause:** peer died, peer code chose RST, intermediate firewall
  decided to RST you.
- **What you should do:** log it, drop the connection, move on. There
  is **no retry** for an inbound server-side connection — the connection
  is the unit of work, and it's gone.

### `BrokenPipe` — you wrote to a peer-gone socket

The peer closed (sent FIN), but the connection's reverse direction
(client → server) is no longer reachable. Subsequent writes raise
`BrokenPipe`.

- **Cause:** peer closed early, peer crashed, peer's NAT timed out.
- **What you should do:** log it, drop the connection, move on.

### `Ok(0)` from `read` — peer closed cleanly

**Not an error.** This is the well-defined "end of stream" signal.
The peer sent FIN; no more bytes will ever arrive on this socket.

- **Cause:** orderly shutdown from peer.
- **What you should do:** if you have a complete request → respond.
  If you don't → it's a truncated request, which IS an error at the
  *application* layer, even though TCP's behavior was correct.

### `Err(UTF-8)` or your-own-parser-error

Not a TCP error at all — your application decided the bytes the peer
sent were malformed. TCP delivered them faithfully; the meaning is
wrong.

- **Cause:** malicious or buggy client.
- **What you should do:** log, drop, possibly rate-limit the source.
  Never crash the server.

### `AddrInUse` — bind failed

Different category entirely. This is a **startup error**, not a
per-client error. The port is held by another process or by your
own previous instance in `TIME_WAIT`.

- **Cause:** Ctrl-C'd server, immediate restart; kernel still
  mourning the dead 4-tuples.
- **Quick fix:** wait ~60s, or change the port.
- **Real fix:** set `SO_REUSEADDR` on the listener before `bind`.
  We will do this when it starts hurting in development.

---

## TIME_WAIT — the most-misunderstood TCP state

After your server's `close()` on a connection, that connection's
4-tuple `(local_ip, local_port, peer_ip, peer_port)` enters
**TIME_WAIT** for typically 30–120 seconds.

Why? To absorb any stray retransmits from the dead connection,
so they don't get misinterpreted as belonging to a new connection
that happens to reuse the same 4-tuple.

For **clients**, TIME_WAIT is mostly invisible — the OS chooses an
ephemeral local port and avoids collisions.

For **servers** binding a fixed port (like 7878), TIME_WAIT can cause
`AddrInUse` on quick restart, because the kernel is still holding
half-dead 4-tuples on that port.

```
server crash / Ctrl-C
   │
   ▼
4-tuple in TIME_WAIT for ~60s     ← "address already in use" lives here
   │
   ▼
4-tuple released, port reusable
```

`SO_REUSEADDR` is the kernel option that says: "I know TIME_WAIT
4-tuples exist on this port; let me bind anyway." It does not waive
the safety property — new connections still get fresh state — it
just lets a *listener* coexist with leftover dying connections.

---

## A policy table you can actually use

For every error category, decide three things in advance: **log
level**, **what to do with this connection**, **what to do with this
peer in the future**.

| Error                       | Log     | This conn          | This peer            |
| --------------------------- | ------- | ------------------ | -------------------- |
| `ConnectionReset` on read   | warn    | drop               | nothing              |
| `BrokenPipe` on write       | debug   | drop               | nothing              |
| `Ok(0)` mid-request         | warn    | drop               | nothing              |
| Malformed input (parse err) | warn    | send 4xx, drop     | maybe rate-limit IP  |
| Connection idle too long    | info    | drop               | nothing              |
| `AddrInUse` at startup      | fatal   | n/a                | crash + alert        |
| Any other I/O error         | error   | drop               | investigate          |

The shape of this table matters more than its exact contents. **The
server's top-level loop must categorize and continue, never bubble
up and die.**

---

## Working rule

> A successful syscall is a promise from your kernel, not a promise
> from the network. Build every server with that humility — assume
> errors will arrive late, and that the only sane response to most
> of them is to drop the connection and accept the next one.

---

## Test your understanding

Predict, without running:

1. You `write_all(huge_buffer)` (e.g., 100 MB) to a client that just
   crashed. Is the syscall fast or slow? Does it return `Ok` or `Err`?
   Why might that depend on the size of the kernel send buffer?

2. Your server `write_all`s a tiny response, returns from
   `handle_client`, drops the `TcpStream`. The peer (curl) was already
   gone when you wrote. Do you ever learn that the peer was gone?

3. Two servers binding to port 7878 simultaneously. Both call `bind`.
   What does each `bind` return? Does the kernel race? Or is `bind`
   atomic?
