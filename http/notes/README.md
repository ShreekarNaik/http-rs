# osi-rs / http — Learning Notes

A long-form, evolving knowledge base built alongside the code.
The codebase teaches *how*; these notes teach *why*.

## How notes are organized

Files are numbered in roughly the order we encounter the ideas.
Topics aren't siloed — networking, Rust, and systems thinking
will weave through every file.

- `00-orientation.md` — the mental model we start from: OSI, TCP/IP,
  where our code sits relative to the kernel, what HTTP actually is.
- (more files appear as we discover the need for them)

## How to use these notes

- Read top-to-bottom on first pass.
- Re-read after each milestone — earlier ideas will land differently
  once you've felt the pain that motivated them.
- Grep liberally. These notes are meant to be a long-term reference,
  not a one-time read.

## Conventions used in the notes

- **Bold** = a term you should be able to define from memory later.
- *Italic* = emphasis / nuance, not a glossary term.
- "Quoted phrases" = the exact words a textbook or man page would use.
- Diagrams are plain ASCII so they survive any editor and grep cleanly.
