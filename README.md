# paredit-cli

[![CI](https://github.com/takeokunn/paredit-cli/actions/workflows/main.yml/badge.svg)](https://github.com/takeokunn/paredit-cli/actions/workflows/main.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://github.com/takeokunn/paredit-cli/blob/main/LICENSE)
[![Documentation](https://img.shields.io/badge/docs-MkDocs%20Material-3949ab)](https://takeokunn.github.io/paredit-cli/)

`paredit` is a structure-aware CLI for inspecting and safely refactoring Lisp
S-expressions, designed for both people and AI coding agents. It supports
Common Lisp, Emacs Lisp, LFE, Scheme, Racket, Clojure, Hy, Carp, Janet, and
Fennel.

Full documentation — command reference, safe editing workflows, the agent
interface, and integration guides — is published at
<https://takeokunn.github.io/paredit-cli/>. The source for that site lives in
[docs/src/](docs/src/README.md).

## Commands

```sh
paredit inspect <report> [args]    # read-only inventory, validation, analysis
paredit edit <transform> [args]    # one structural edit (stdout, --diff, or --write)
paredit refactor <workflow> [args] # plan, preview, verify, and apply changes
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
```

## Install

```sh
nix run github:takeokunn/paredit-cli -- --help    # run without installing
nix profile install github:takeokunn/paredit-cli # install via Nix
cargo install --git https://github.com/takeokunn/paredit-cli --locked
nix develop -c cargo install --path . --locked   # from a local checkout
```

The current minimum supported Rust version is `1.85`. See the
[installation guide](https://takeokunn.github.io/paredit-cli/installation/)
for the Cachix binary cache, flake overlay, and commit-pinning for automation.

## Development

```sh
nix develop
cargo test
nix flake check
```

Pull requests run `nix flake check`. A typed Rust library API behind the CLI
is available in the [`paredit_cli` documentation](https://docs.rs/paredit-cli).

## Community and security

- [Contributing](CONTRIBUTING.md)
- [Code of Conduct](CODE_OF_CONDUCT.md)
- [Support](SUPPORT.md)
- [Security](SECURITY.md)
- [Releasing](RELEASING.md)

## License

MIT. See [LICENSE](LICENSE).
