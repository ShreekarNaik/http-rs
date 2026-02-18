# 06 — Framing in practice: deciding when a request is "done"

We've talked about framing twice already (`00-orientation.md` §6,
`03-segments-vs-stream.md`). Now we actually implement it.

This chapter is the bridge between *theory* of byte streams and
*code* that handles them correctly.

---

## The problem, restated for clarity

TCP gives us a byte stream. The application must decide:

- when **one message ends**
- and **another may begin**

There are no message boundaries in TCP. The kernel will hand us
bytes in whatever chunks the network happens to deliver them. We
might call `read()` and get:

- 0 bytes (peer closed cleanly — EOF, not error)
- 1 byte (kernel had just one byte buffered when we asked)
- the entire request (kernel had buffered everything before we asked)
- half a request (we asked mid-arrival)
- one and a half requests (peer pipelined two requests on one conn)

A correct server must handle **all** of these.

---

## HTTP/1.1's framing rules

HTTP/1.1 uses a **hybrid framing scheme**:

```
GET /path HTTP/1.1\r\n           ← start line
Host: example.com\r\n            ← header lines
Content-Length: 13\r\n           ← (sometimes) tells body length
\r\n                             ← blank line — end of headers
Hello, world!                    ← body (exactly N bytes, per Content-Length)
```

Three independent framing decisions, layered:

1. **Lines are delimited by `\r\n`.** Each header is one line.
2. **The header section ends at the first empty line, i.e., `\r\n\r\n`.**
3. **The body's length is communicated *inside* the headers** — most
   commonly via `Content-Length: N`, sometimes via
   `Transfer-Encoding: chunked`, sometimes "until connection close."

Notice the recursion: to know the body length, you must finish
parsing the headers first. That's why parsing happens in **phases**:

```
phase 1: read until \r\n\r\n             (delimiter-based)
phase 2: parse headers                   (extract Content-Length)
phase 3: read exactly Content-Length more (length-based)
```

This chapter implements phase 1. Phase 2 and 3 come later.

---

## The naive approach (and why it's a bug)

```rust
let mut buf = [0u8; 1024];
let n = socket.read(&mut buf)?;
// "we have the request now, parse it"
```

This is **wrong** even though it works most of the time on localhost
with small requests. It assumes a single `read` returns a complete
message. We have demonstrated, by experiment, that this is false.

The fix is a **read loop**.

---

## The read loop pattern

Pseudocode:

```
accumulate = empty growable buffer
loop {
    n = socket.read(&mut small_fixed_buffer)
    match n {
        0       => peer closed before request was complete → error
        > 0     => append the n bytes to accumulate
                   if accumulate contains "\r\n\r\n" → break
                   else → continue
        Err(e)  => propagate or log+drop
    }
}
// at this point: accumulate holds the full headers (and possibly
// the first bytes of the body)
```

Three properties to notice:

1. We never assume any single `read` is "the request."
2. We append to a **growable** buffer (a `Vec<u8>` in Rust). The
   request could be 50 bytes or 8 KiB or more.
3. We stop the moment we see the framing signal — not based on time,
   not based on number of reads, not based on `Ok(0)`.

---

## Why `Vec<u8>` and not `[u8; N]`

A fixed-size array `[u8; N]` is allocated at compile time. Its size
cannot grow. If the request is larger than `N`, we have to either
truncate (wrong) or fail (acceptable, but inflexible).

`Vec<u8>` is a heap-allocated, dynamically-growing byte buffer. It's
the right tool here because HTTP request sizes are variable and we
don't know them in advance.

Both store the same kind of data (a sequence of `u8` bytes). They
differ in **where** the bytes live and **whether** the size can change:

|                | `[u8; N]`            | `Vec<u8>`              |
| -------------- | -------------------- | ---------------------- |
| Allocation     | stack, compile-time  | heap, runtime          |
| Growable?      | no                   | yes                    |
| Size known at  | compile time         | runtime                |
| Cost to create | free                 | one heap allocation    |
| Best for       | fixed scratch buffer | accumulating unknowns  |

Idiomatic Rust uses both, often together:

```rust
let mut chunk = [0u8; 512];      // small scratch buffer, fixed
let mut full  = Vec::new();      // growing accumulator
loop {
    let n = socket.read(&mut chunk)?;
    if n == 0 { break; }
    full.extend_from_slice(&chunk[..n]);
    if /* condition */ { break; }
}
```

We `read` into the fixed buffer (because `read` needs a fixed slice)
and immediately append the read bytes to the `Vec` (because the
total size is unknown).

---

## Searching for `\r\n\r\n` in `Vec<u8>`

The needle is 4 bytes. The haystack is a `Vec<u8>`. There are several
ways to detect the needle; only some are correct.

### Wrong: check only the most recent read

```rust
if chunk[..n].windows(4).any(|w| w == b"\r\n\r\n") { ... }
```

What if `\r\n` arrives at the end of one read and the next `\r\n`
arrives at the start of the next? The boundary spans two reads. You
have to search **the accumulated buffer**, not just the latest chunk.

### Wrong: check only the last 4 bytes

```rust
if full.ends_with(b"\r\n\r\n") { ... }
```

This is wrong because **a single `read` can return both the end of
the headers AND some body bytes**. The `\r\n\r\n` would be somewhere
*in the middle* of `full`, not at the end. Stopping when `\r\n\r\n`
is at the end means you wait forever (or until timeout) for bytes
that already arrived.

### Right: search the whole accumulator

```rust
if full.windows(4).any(|w| w == b"\r\n\r\n") { ... }
```

`windows(4)` returns an iterator over every overlapping 4-byte
sliding window into the slice. `.any(|w| w == b"\r\n\r\n")` returns
true if any window matches. This finds `\r\n\r\n` regardless of
position, so it is correct.

**Performance footnote:** searching `full.windows(4)` from scratch on
every iteration is `O(n)` per loop, so the whole accumulator costs
`O(n^2)` if the request arrives one byte at a time. For real HTTP
servers this matters; for ours, it does not yet. We will optimize
when it actually hurts.

---

## The body problem (preview)

When `\r\n\r\n` is found, it might not be at the end of `full`. The
bytes *after* `\r\n\r\n` are the first bytes of the body.

```
full = [G E T   / ... \r\n\r\n  H e l l o]
                                ^         ^
                                |         |
                            end of headers, beginning of body
```

You must **not discard** those bytes when transitioning from
"reading headers" to "reading body." A naive parser that does
`socket.read(...)` again to get the body will lose data and either
hang or misframe.

We handle this carefully in the body-reading phase.

---

## The security side: unbounded reads are a DoS

Our read loop currently has no upper bound. A malicious client can:

```
client: G        (sleep 60s)
client: E        (sleep 60s)
client: T        (sleep 60s)
... forever
```

The connection stays open. The thread stays parked. Memory grows by
one byte per minute. Multiply by N clients and the server is dead
without a single byte of attack-traffic-per-second.

This attack has a name: **Slowloris** (2009). Real fixes:

- **header size limit** (cap `full.len()`, e.g., 8 KiB)
- **read timeout** (drop a connection that takes too long)
- **idle timeout** (drop if no bytes arrive for K seconds)

We'll add at least one of these soon. For now, know that the
unbounded loop is a known vulnerability, and we're shipping it
on purpose so we can feel the gap.

---

## Working rules

> Every byte that arrives is appended to an accumulator until the
> framing condition is met. The accumulator, not any individual
> `read`, is the source of truth.

> `Ok(0)` mid-frame is always an application-layer error: the peer
> closed before sending a complete message. Treat it as such, even
> though TCP itself behaved correctly.

> Every read loop needs a bound. Without one, slow clients become a
> denial-of-service vector.

---

## Test your understanding

1. The client sends `"GET / HTTP/1.1\r\n\r\nGET / HTTP/1.1\r\n\r\n"`
   in one TCP write (two complete requests, pipelined). Your server
   does the read-loop and stops at the first `\r\n\r\n`. What's in
   `full` after the loop? What problem will the next request face?

2. You set a header limit of 8 KiB. A client sends 8 KiB of headers
   followed by `\r\n\r\n`. Should this succeed or fail? What about
   8 KiB - 1 byte of headers followed by `\r\n\r\n`?

3. If `chunk` is `[u8; 512]` and `full` is `Vec<u8>`, what happens
   to the contents of `chunk` between iterations? Does
   `extend_from_slice` *copy* the bytes, or just record a reference
   to them? Why does the answer matter?

4. The HTTP spec allows `\n` alone as a line terminator (without
   `\r`) in some legacy contexts. Does your `\r\n\r\n` search handle
   that? Should it? What's the security implication of being too
   lenient here? (Search the term "HTTP request smuggling" later.)
