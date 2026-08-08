# paredit-cli

[![CI](https://github.com/nerima-lisp/paredit-cli/actions/workflows/main.yml/badge.svg)](https://github.com/nerima-lisp/paredit-cli/actions/workflows/main.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://github.com/nerima-lisp/paredit-cli/blob/main/LICENSE)
[![Documentation](https://img.shields.io/badge/docs-MkDocs%20Material-3949ab)](https://nerima-lisp.github.io/paredit-cli/)

`paredit` is a structure-aware CLI for inspecting and safely refactoring Lisp
S-expressions, designed for both people and AI coding agents. It supports
Common Lisp, Emacs Lisp, LFE, Scheme, Racket, Clojure, Hy, Carp, Janet, and
Fennel.

Full documentation — command reference, safe editing workflows, the agent
interface, and integration guides — is published at
<https://nerima-lisp.github.io/paredit-cli/>. The source for that site lives in
[docs/src/](docs/src/index.md).

## Commands

```sh
paredit inspect <report> [args]    # read-only inventory, validation, analysis
paredit edit <transform> [args]    # one structural edit (stdout, --diff, or --write)
paredit refactor <workflow> [args] # plan, preview, verify, and apply changes
paredit query <find|count|replace> # search and rewrite by S-expression pattern
paredit fix <apply|check|plan|list> # apply the lint auto-fixes
paredit migrate <list|explain|run> # run a named, dialect-scoped codemod recipe
paredit schema check <file> --schema <file> # validate Lisp data against a defschema
paredit config <command>           # inspect and validate the layered paredit.toml
paredit generate <generator> [args] # generate new source: defpackage, defsystem, tests, ...
paredit lsp                        # Language Server Protocol server over stdio
paredit mcp                        # Model Context Protocol server over stdio (agents)
paredit serve                      # resident analysis server over HTTP/JSON-RPC
paredit tui <file>                 # interactively browse a file's tree; prints --path on exit
paredit editor <file>              # interactively edit one file in a terminal
paredit completions <shell>        # shell completion scripts (bash/zsh/fish/...)
```

Run `paredit --help`, then `paredit <namespace> --help` for the complete
command list. For machine-readable discovery, run
`paredit inspect capabilities --output json`.

## Quick Start

```sh
paredit inspect check --file src/example.lisp
paredit edit wrap --file src/example.lisp --path 0.2 --diff
paredit refactor plan --symbol old-name src/example.lisp

# Name a form without counting parentheses: by definition name, by editor
# coordinate, or by shape. `inspect resolve` shows what a selector matches.
paredit inspect resolve --file src/example.lisp --query '(defun ?name ...)'
paredit edit wrap --file src/example.lisp --name parse-header --diff

# The same pattern language, applied to the whole workspace -- and rewritten.
paredit query find --query '(eq ?x ?x)' --fail-on-match src/
paredit query replace --query '(if ?t ?a nil)' --rewrite '(when ?t ?a)' --diff src/
paredit migrate run elisp-cl-lib --diff lisp/
paredit fix apply src/
```

## Install

```sh
nix run github:nerima-lisp/paredit-cli -- --help    # run without installing
nix profile install github:nerima-lisp/paredit-cli # install via Nix
cargo install --git https://github.com/nerima-lisp/paredit-cli --locked
nix develop -c cargo install --path . --locked   # from a local checkout
```

The current minimum supported Rust version is `1.85`. See the
[getting started guide](https://nerima-lisp.github.io/paredit-cli/getting-started/)
for the Cachix binary cache, flake overlay, and commit-pinning for automation.

## Stability

`paredit-cli` follows Semantic Versioning. From `1.0.0` onward, command paths,
flags, exit codes, documented JSON fields, the `paredit_cli` crate-root API,
and the Nix outputs are stable within the `1.x` series; human-readable text
output is not a machine contract. The
[compatibility guide](https://nerima-lisp.github.io/paredit-cli/reference/compatibility/) lists
exactly what an upgrade may and may not change.

## Development

```sh
nix develop
cargo test
nix flake check
```

Pull requests run a fast-path subset of `nix flake check`'s outputs, one CI job
per check, so the gate costs the slowest check rather than the sum of all of
them — the fat-LTO release build is deferred to `main` and pushes to `main`
run every output. A typed Rust library API behind the CLI
is documented in [`src/lib.rs`](src/lib.rs); build it locally with
`cargo doc --no-deps --open`. The rendered command reference and guides live
at the [documentation site](https://nerima-lisp.github.io/paredit-cli/).

## Community and security

- [Contributing](https://github.com/nerima-lisp/.github/blob/main/CONTRIBUTING.md)
- [Code of Conduct](https://github.com/nerima-lisp/.github/blob/main/CODE_OF_CONDUCT.md)
- [Support](https://github.com/nerima-lisp/.github/blob/main/SUPPORT.md)
- [Security](https://github.com/nerima-lisp/.github/blob/main/SECURITY.md)
- [Releasing](https://github.com/nerima-lisp/paredit-cli/blob/main/docs/notes/releasing.md)

## License

MIT. See [LICENSE](https://github.com/nerima-lisp/paredit-cli/blob/main/LICENSE).
