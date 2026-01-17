# 02 — TCP timing and connection lifecycle

The single most important file to internalize before writing more code.
Refer back to this whenever you're confused about *when* something
happens in a TCP connection.

---

## The two big claims

1. Every TCP connection has **two kernel buffers**, one for each direction.
2. **Blocking `read()` parks your thread; it does not poll.**

Most timing confusion comes from forgetting one of these.

---

## Per-connection buffers

For every accepted TCP connection, the kernel maintains:

```
       send buffer          receive buffer
            │                     ▲
   write() │                     │ read()
            ▼                     │
        (kernel decides when to send)
            │                     │
            └─── network ─────────┘
```

- `write(buf)` copies bytes into the kernel **send buffer** and returns.
  The kernel will transmit them as TCP segments when convenient.
  `write()` returning does NOT mean the peer has received the bytes.
- `read(buf)` copies bytes *out of* the kernel **receive buffer** into
  your program. If the receive buffer is empty, behaviour depends on
  blocking mode (see below).
- Both buffers have finite capacity. If the receive buffer fills (because
  your program isn't reading), TCP's flow control mechanism tells the
  sender to slow down. If the send buffer fills (because the peer or
  network can't keep up), `write()` will block until space frees up.

---

## What `read()` does in blocking mode (the default)

```
read(buf):
    if receive buffer has data:
        copy up to buf.len() bytes
        return Ok(n) with n ≥ 1
    else if peer sent FIN (clean close):
        return Ok(0)            ← EOF signal
    else if error:
        return Err(e)
    else:
        park the thread; wake when one of the above changes
```

**Important consequences:**

- `read()` cannot return `Ok(0)` "because nothing was ready yet."
  `Ok(0)` means EOF — the other side has *explicitly* closed.
- `read()` is allowed to return *fewer bytes than you asked for*,
  even if more were on the way. It returns whatever is buffered right
  now. **One read does not equal one message.** This is the core
  framing problem HTTP must solve.
- A parked thread uses zero CPU. The kernel handles wake-up.

---

## The 4-way connection close (FIN)

TCP is **full-duplex**: two independent byte streams in opposite
directions. Closing one direction does not close the other.

```
   A → FIN → B           A says "I'm done sending."
   A ← ACK ← B           B acknowledges. A→B is now half-closed.

   A ← FIN ← B           B says "I'm done sending too."
   A → ACK → B           A acknowledges. Connection fully closed.
```

In our code, we don't write FIN ourselves. The kernel emits FIN
when our `TcpStream` value is dropped (its destructor runs `close()`
on the file descriptor). Rust's RAII guarantees this happens
deterministically when the value goes out of scope.

Practical signals:

- We *send* FIN by dropping the socket (or calling `shutdown`).
- We *receive* FIN by `read()` returning `Ok(0)`.

---

## Why "blocking" is not the same as "polling"

Beginners think "blocking" implies a busy loop checking for data.
It doesn't. The kernel maintains a list of threads waiting on each
resource. When a TCP segment arrives, the kernel:

1. Receives the packet (driver interrupt).
2. Parses the TCP header to find which connection it belongs to.
3. Copies the payload into that connection's receive buffer.
4. Looks up any threads parked on that socket and marks them
   *runnable*.
5. The scheduler later puts them back on a CPU.

This is fundamentally why a Rust web server *can* serve many clients
with a thread-per-connection design without burning CPU — most threads
are parked most of the time. The cost is memory per thread, not CPU.
That cost is what motivates async I/O for high-concurrency servers,
but it's not because blocking I/O wastes CPU — it doesn't.

---

## Timeline of our single-client server (recap of the discussion)

```
your code                   server kernel              network      client kernel    curl
─────────                   ─────────────              ───────      ─────────────    ────
bind(7878)                 creates listening fd
accept() — blocks          waits
                            ◄─ SYN ──────────────                  sends SYN        connect()
                            ── SYN-ACK ─────────►
                            ◄── ACK ──
                            handshake done, new fd
accept() returns
read(buf) — blocks
                            ◄── PSH+ACK ────────                   sends bytes      write()
                            data in recv buffer,
                            wakes thread
read returns Ok(n)
println!(...)
end of match arm:
  socket drops → close()    flushes send buf (empty)
                            ── FIN ────────────►                                   read returns 0
                                                                                    "empty reply"
main returns                cleanup of listener fd
```

---

## Mental model checklist

You should now be able to answer these without looking back:

1. After `accept()` returns, has the 3-way handshake happened?
   ☐ yes, fully completed in the kernel before accept returned.
2. If `read()` returns `Ok(0)`, what does it mean?
   ☐ peer sent FIN; no more bytes will ever arrive.
3. If you call `write()` and it returns `Ok(13)`, has the peer
   received 13 bytes?
   ☐ no — 13 bytes are in the kernel send buffer; transmission is async.
4. Who calls `close()` on the socket fd in our Rust code?
   ☐ nobody by name; Rust's `Drop` impl on `TcpStream` does it.
5. Can `read()` return less data than you asked for?
   ☐ yes, always; one read is not one message.
