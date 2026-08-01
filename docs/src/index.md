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

- [`paredit inspect`](reference/api.md#inspect): read-only reports and analysis.
- [`paredit edit`](reference/api.md#edit): structural edits of a selected form —
  stdout by default, `--diff` for a unified diff, `--write` to update the
  file in place.
- [`paredit refactor`](reference/api.md#refactor): planned semantic changes with
  preview and verification workflows — see the
  [refactor workflow](guide/workflows.md).
- [`paredit query`](reference/api.md#query): search, count, and rewrite by
  S-expression pattern, across a whole workspace.
- [`paredit fix`](reference/api.md#fix): apply the lint auto-fixes — the write side
  of `inspect lint`, under a name that says it writes.
- [`paredit migrate`](reference/api.md#migrate): run a named, ordered,
  dialect-scoped codemod recipe.

The first three split by what a change costs to undo; the last three by what
you are trying to do, over a file set rather than one form.

There are no legacy top-level command aliases. Beside the six there are
`paredit config` and `paredit generate`, the `lsp`/`mcp`/`serve`/`tui`
servers, and `paredit completions <shell>`. Forms are addressed with tree paths or byte
offsets — see [Selecting forms](reference/selectors.md). Automation and AI coding
agents should start with the [agent interface](guide/agents.md), including
`paredit inspect capabilities` for one-call discovery of the whole command
surface.

## Install

```sh
nix run github:nerima-lisp/paredit-cli -- inspect check --file source.lisp
```

See [Getting Started](getting-started.md) for Nix profiles, the flake overlay,
Cachix binary caches, and `cargo install`. Contributors should start with
[Development](project/development.md).

## Contributing and support

A typed Rust library API sits behind the CLI, documented in its
[source](https://github.com/nerima-lisp/paredit-cli/blob/main/src/lib.rs). The
crate is not published to a registry; build the API documentation from a
checkout with `cargo doc --no-deps --open`.

Project participation and operational policies are org-wide and live in
[`nerima-lisp/.github`](https://github.com/nerima-lisp/.github): the
[contribution guide](https://github.com/nerima-lisp/.github/blob/main/CONTRIBUTING.md),
the [code of conduct](https://github.com/nerima-lisp/.github/blob/main/CODE_OF_CONDUCT.md),
the [security policy](https://github.com/nerima-lisp/.github/blob/main/SECURITY.md),
and [support](https://github.com/nerima-lisp/.github/blob/main/SUPPORT.md).
Bugs and feature requests go to the
[issue tracker](https://github.com/nerima-lisp/paredit-cli/issues).
