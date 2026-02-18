# 05 — Concurrency: thread-per-connection

The first concurrency model in our journey. Simple, correct, doesn't
scale — and feeling exactly *why* it doesn't scale is the prerequisite
for understanding async later.

---

## The pain it solves

After Challenge #3 our server was single-threaded:

```rust
loop {
    let (socket, addr) = listener.accept()?;
    handle_client(socket, addr); // serial — blocks until done
}
```

Two clients, each taking 5s of handler time:

```
   t=0         t=5         t=10
   ├──A holds──┤
                ├──B waits in accept queue──┤
```

Client B's curl returned at t≈10. Why? Not because TCP was slow.
Not because the network was slow. Because **the single user-space
thread was busy with A**, and `accept()` cannot return for B until
A's handler is done.

This is the moment we needed concurrency in user space.

---

## What "user-space concurrency" means here

Remember: while the server thread is busy with A, the **kernel is
still doing TCP work for B**. The kernel:

- replied to B's SYN
- completed B's handshake
- received B's bytes into B's receive buffer
- placed the established connection on the **accept queue**

The kernel does not need user-space cooperation for any of this.
What it needs cooperation for is **handing the established connection
back to your program** via `accept()`. And that's the bottleneck.

So "concurrency" here means: process the handed-back connections in
parallel, so the dispatch loop doesn't wait on per-client work.

---

## The thread-per-connection model

```
        ┌───────────────────────────────────────────┐
        │           main thread (dispatcher)        │
        │                                           │
        │   loop {                                  │
        │     accept() ──► (socket, addr)           │
        │     spawn thread { handle_client(...) }   │
        │     immediately loop again                │
        │   }                                       │
        └───────────────────────────────────────────┘
                       │       │       │
                       ▼       ▼       ▼
                   worker  worker  worker
                   (A)     (B)     (C)
                   reads,  reads,  reads,
                   writes, writes, writes,
                   exits.  exits.  exits.
```

Each worker thread owns:

- the `TcpStream` (kernel file descriptor, RAII-closed on drop)
- its own stack
- whatever it allocates locally

Threads do **not** share state — there's nothing to synchronize. This
is the simplest form of concurrency: independent, isolated workers.

---

## Why this is "free correctness"

Three properties fall out of the design without any extra effort:

1. **No shared mutable state.** Each connection has its own socket,
   its own buffer, its own everything. The borrow checker doesn't
   need to enforce anything — there's nothing to race over.

2. **RAII cleans up everything.** When a thread's closure returns:
   - the `TcpStream` is dropped → `close(fd)` runs → kernel frees socket
   - the buffer is dropped → memory released
   - the thread's stack is reclaimed by the OS
   No leaks, no manual cleanup, no resource pools.

3. **Errors stay local.** A panic or error in one thread doesn't
   touch the others. The dispatcher thread is *only* dispatching,
   so it has almost no surface area to fail.

This is why thread-per-connection was the dominant server model from
~1995 to ~2005. It's simple, robust, and correct.

---

## Why it doesn't scale: the cost of a thread

A thread is not free. Each one costs you:

| Cost                          | Approx. magnitude               |
| ----------------------------- | ------------------------------- |
| **Stack** (virtual addr space)| 2 MiB on macOS/Rust default     |
|                               | 8 MiB on Linux default          |
| Kernel task struct            | 2–10 KiB                        |
| Scheduler overhead            | non-trivial above ~thousands    |
| File descriptor               | 1 per open socket               |

The stack is the dominant cost. At Rust's 2 MiB-per-thread default:

```
   1,000 threads  →    2 GiB virtual address space
  10,000 threads  →   20 GiB
 100,000 threads  →  200 GiB
```

64-bit machines have huge virtual address spaces (terabytes), but the
kernel still tracks every reservation. Physical RAM is consumed
lazily (only the pages you actually touch), but you'll hit ulimits,
scheduler issues, and context-switch storms long before you hit RAM.

**This is the C10K problem** — the 1999 essay by Dan Kegel that
articulated the question: "How do we handle 10,000 concurrent
clients?" Thread-per-connection couldn't answer it. Async I/O
(epoll, kqueue) could. That's the journey we'll re-walk later.

---

## The Rust ergonomics: closures and `move`

```rust
thread::spawn(move || {
    if let Err(e) = handle_client(socket, addr) {
        eprintln!("error: {e:?}");
    }
});
```

Three pieces of Rust to understand:

### 1. `|| { ... }` is a closure — an anonymous function.

It captures variables from the surrounding scope. Like a function,
but with extra state.

### 2. By default, closures **borrow** captured variables.

That's safe in normal code because the closure doesn't outlive the
scope. But a thread can outlive the scope that spawned it. The
compiler refuses to let a thread borrow something that might be
destroyed underneath it.

### 3. `move` forces ownership transfer.

```rust
thread::spawn(move || { ... })
```

Now the closure **owns** `socket` and `addr`. The original bindings
in `main` are gone (moved-from). The new thread is self-sufficient;
it cannot dangle, because nothing else can drop the socket while it's
running.

This is the borrow checker doing thread-safety analysis at compile
time. C and C++ have no such mechanism — every thread-vs-scope bug
is a runtime mystery in those languages.

---

## What about `JoinHandle`?

`thread::spawn` returns a `JoinHandle<T>`. You can:

- ignore it (we do)
- call `.join()` to wait for the thread and get its return value
- drop it (the thread keeps running, becomes "detached")

For our server, we want fire-and-forget: handle the client and exit.
Letting the `JoinHandle` drop is correct.

---

## What thread-per-connection gives up

1. **Fairness.** The OS scheduler decides which threads run when.
   You have no control over slow clients hogging worker slots.

2. **Memory efficiency.** ~2 MiB per idle connection is a lot.
   For 10K idle keep-alive HTTP connections, you're at 20 GiB of
   stack reservation just to wait on `read`.

3. **Coordination ease.** As soon as workers need to share something
   (cache, rate limiter, connection counter), you need `Arc`,
   `Mutex`, channels — the synchronization complexity arrives in
   force.

4. **C10K.** Cannot serve 10,000 concurrent connections per process.
   Period.

These limits are real but **none of them matter at our current
scale**. Thread-per-connection is the right tool for now, and we
keep it until something genuinely breaks.

---

## Working rules

> The kernel and your application live on opposite sides of the
> `accept()` syscall. The kernel is always doing TCP work in
> parallel. User-space parallelism is about not making the kernel
> wait on you, not about doing the kernel's work for it.

> Thread-per-connection is correct, simple, and has a hard ceiling
> around ~10,000 connections per process. Use it until you bump
> the ceiling.

---

## Test your understanding

1. Your server spawns a thread per connection. A client connects but
   never sends bytes. The thread blocks in `read`. The OS scheduler
   does what with that thread?

2. You spawn 5,000 threads, each blocked in `read`. Top CPU usage in
   `top` is near 0%. Memory is huge. Explain both observations from
   first principles.

3. The dispatcher (main) thread spawns a worker, then loops back to
   `accept()`. The worker thread panics. Does the dispatcher die?
   Does the process die? Does any other worker die?

4. Could two worker threads ever accidentally `read` from the same
   socket? Why or why not? (Hint: who owns the `TcpStream`?)
