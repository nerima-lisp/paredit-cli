# paredit-cli

`paredit-cli` is a command-line tool for inspecting, editing, and safely
refactoring Lisp source. It parses first, edits only balanced S-expression
structure or exact atom tokens, and validates the result — so symbol-oriented
rewrites never touch strings or comments.

It supports Common Lisp, Emacs Lisp, LFE, Scheme, Racket, Clojure, Hy, Carp, Janet, and Fennel
sources, and is designed for both people and AI coding agents.

## Quick start

```sh
paredit inspect check --file source.lisp
paredit edit format --file source.lisp
paredit refactor rename-symbol --file source.lisp --from old-name --to new-name
```

The CLI has six source-facing namespaces:

- [`paredit inspect`](commands.md#inspect): read-only reports and analysis.
- [`paredit edit`](commands.md#edit): structural edits of a selected form —
  stdout by default, `--diff` for a unified diff, `--write` to update the
  file in place.
- [`paredit refactor`](commands.md#refactor): planned semantic changes with
  preview and verification workflows — see the
  [refactor workflow](workflows.md).
- [`paredit query`](commands.md#query): search, count, and rewrite by
  S-expression pattern, across a whole workspace.
- [`paredit fix`](commands.md#fix): apply the lint auto-fixes — the write side
  of `inspect lint`, under a name that says it writes.
- [`paredit migrate`](commands.md#migrate): run a named, ordered,
  dialect-scoped codemod recipe.

The first three split by what a change costs to undo; the last three by what
you are trying to do, over a file set rather than one form.

There are no legacy top-level command aliases. Beside the six there are
`paredit config` and `paredit generate`, the `lsp`/`mcp`/`serve`/`tui`
servers, and `paredit completions <shell>`. Forms are addressed with tree paths or byte
offsets — see [Selecting forms](selectors.md). Automation and AI coding
agents should start with the [agent interface](agents.md), including
`paredit inspect capabilities` for one-call discovery of the whole command
surface.

## Install

```sh
nix run github:nerima-lisp/paredit-cli -- inspect check --file source.lisp
```

See [Installation](installation.md) for Nix profiles, the flake overlay,
Cachix binary caches, and `cargo install`. Contributors should start with
[Development](development.md).
