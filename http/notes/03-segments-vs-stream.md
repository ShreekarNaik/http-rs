# 03 — TCP segments vs the byte-stream abstraction

A confusion that catches almost everyone the first time they look at
TCP closely: **TCP is packetized on the wire, but not at the API**.

This file resolves that contradiction.

---

## The contradiction

If you read about TCP, you learn:

- TCP transmits data in **segments**, each with a header.
- Headers carry **sequence numbers**, **lengths**, **ACK numbers**,
  flags like `SYN`, `FIN`, `PSH`, `RST`, a checksum, a window size,
  and so on.
- Segments are discrete, well-defined packets on the wire.

But the API your program sees offers:

- A **byte stream**. No segment boundaries. No headers. No flags.
- `read(buf)` returns "some bytes," not "one segment."
- `write(buf)` puts bytes in a buffer; you can't tell how they'll be
  packetized.

Both descriptions are correct. They describe **different layers**.

---

## The layering

```
   ┌─────────────────────────────────────────────────────────────┐
   │ Application                                                 │
   │   sees: ordered, reliable bytes; NO message boundaries.     │
   │   API: read(), write() on a TcpStream                       │
   └─────────────────────────────────────────────────────────────┘
   ┌─────────────────────────────────────────────────────────────┐
   │ Kernel TCP                                                  │
   │   sees: segments with headers, seq numbers, ACKs, flags.    │
   │   does: reorder, retransmit, deduplicate, congestion control│
   │   strips all of this before handing bytes upward.           │
   └─────────────────────────────────────────────────────────────┘
   ┌─────────────────────────────────────────────────────────────┐
   │ Kernel IP                                                   │
   │   sees: IP packets being routed across hops.                │
   │   does: route packets, handle fragmentation.                │
   └─────────────────────────────────────────────────────────────┘
   ┌─────────────────────────────────────────────────────────────┐
   │ Hardware / link layer                                       │
   │   sees: Ethernet frames, MAC addresses, voltage / radio.    │
   └─────────────────────────────────────────────────────────────┘
```

**Each layer hides the one below it.** The application sees a clean
stream; the kernel deals with the mess; the hardware deals with the
physics. This is the entire point of the OSI / TCP-IP layering model.

---

## Why TCP hides segment boundaries from applications

1. **The boundaries are not yours.**
   Segment size is decided by network conditions — MTU, MSS,
   congestion window, Nagle's algorithm — none of which correspond
   to anything in your application's structure.
2. **Reassembly is hard; the kernel does it once for everyone.**
   Segments arrive out of order, can be duplicated, can be lost.
   Sequence numbers let the kernel rebuild the stream. Doing this
   in every application would be wasteful and error-prone.
3. **TCP's contract IS the byte stream.**
   The whole point of TCP-as-an-abstraction is to spare you from
   thinking about packets. If you want packet semantics, use UDP.

---

## TCP vs UDP — message boundaries

|                              | TCP         | UDP               |
| ---------------------------- | ----------- | ----------------- |
| Wire unit                    | segment     | datagram          |
| Application sees             | byte stream | discrete messages |
| Reliable                     | yes         | no                |
| Ordered                      | yes         | no                |
| Message boundaries preserved | **no**      | **yes**           |
| API                          | `read(buf)` | `recvfrom(buf)`   |

This is the cleanest way to see why HTTP needs framing rules: it sits
on TCP, which deliberately gives up message boundaries. UDP-based
protocols (DNS, QUIC at the transport level) don't have this
particular problem.

---

## The postal-service analogy (worth memorizing)

You mail a 500-page novel. The post office splits it across N
envelopes of varying sizes, possibly sends them via different routes,
re-sends any that get lost, and discards duplicates.

The recipient never sees envelopes. They get a single ordered stack
of 500 pages. To figure out where chapter 1 ends and chapter 2 begins,
they must read the *content of the pages* — the envelopes are gone.

In this analogy:

- **Envelopes** = TCP segments
- **Postal service** = kernel TCP
- **Pages in order** = byte stream presented to the application
- **Chapter boundaries** = application-layer message boundaries
- **Reading content to find chapter breaks** = HTTP parsing for
  `\r\n\r\n` and `Content-Length`

---

## What about the `PSH` flag?

A common red herring. The `PSH` (push) flag in a TCP segment header
hints to the receiver's kernel "deliver this promptly to the
application, don't sit on it in the buffer." It is **not a message
boundary marker.** Applications cannot rely on it, cannot see it
through the standard socket API, and should not use it for framing.

Treat `PSH` as a performance hint between kernels, invisible to you.

---

## Working rule

> Every layer hides the one below it. You trade visibility for
> abstraction. If you find yourself wishing TCP gave you segment
> info, you're either (a) reaching for the wrong tool, and want UDP
> or a custom protocol, or (b) trying to solve a framing problem
> that belongs at the *application* layer, where HTTP lives.

---

## Test your understanding

Predict the outcomes (no peeking at code or docs):

1. The client calls `write(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")` once.
   The server calls `read(buf)` once with a 1024-byte buffer.
   How many bytes can the server's `read` return?

   - 0?  any number from 1 to 38?  exactly 38?  more than 38?
2. The client calls `write(...)` twice in quick succession with two
   complete HTTP requests. The server calls `read(buf)` once.
   What can the server's `read` return?
3. The server has read 20 bytes so far. The peer crashes (hard).
   What does the next `read()` return?

(Answers can be reasoned out from the rules above. Try, then ask.)
