# paredit-core-jsonrpc

JSON-RPC 2.0 messages, and the two stdio framings this tool's protocol servers
speak.

## Responsibilities

Three servers — `paredit lsp`, `paredit mcp`, and `paredit serve` — all carry
JSON-RPC. They disagree only about how a message is delimited on the wire:

- **Header framing**, `Content-Length: N\r\n\r\n{…}`, which the Language Server
  Protocol specifies. A message is length-prefixed, so its payload needs no
  escaping and may contain anything.
- **Line framing**, one compact JSON object per line, which MCP's stdio
  transport specifies. A message may therefore contain no raw newline.

Both are here, behind one reader and one writer, because a framing bug is the
kind that presents as "the editor hangs" rather than as an error — and one
implementation with tests is cheaper to be sure of than three.

## What this package is not

It is not a dispatcher. It reads a message, hands it to a `Handler`, and writes
what comes back. Which methods exist, what they mean, and what state they read
are the servers' business, and those live in the composition root because they
aggregate features this package must not know about.

## Robustness

The reader is written against a hostile stream on purpose: a client that dies
mid-message, a `Content-Length` that lies, a header block with no length at
all, a body that is not JSON. Every one of those is a normal event in an editor
session — the editor was killed, the extension restarted — and none may hang the
server or lose the connection's remaining messages.
