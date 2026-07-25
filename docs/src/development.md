# Development

Everything a contributor needs is provided by the Nix flake; no manually
installed Rust toolchain is required. Before changing code, read the
[architecture guide](architecture.md) to know which of the four layers
(`domain`, `application`, `infrastructure`, `presentation`) a change belongs
in.

## Environment

```sh
nix develop        # rustc, cargo, rust-analyzer, cargo-nextest, clippy, mkdocs-material
```

With [direnv](https://direnv.net/), `direnv allow` activates the same shell
automatically via the committed `.envrc`.

## Development loop

```sh
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo nextest run --locked
```

Formatting for the whole repository (Rust via rustfmt, Nix via nixfmt, and
Lisp sources via `paredit edit format`) is one command:

```sh
nix fmt
```

## The verification gate

Pull requests run exactly one command, and the same command works locally:

```sh
nix flake check
```

It builds and runs every check the project defines:

| Check | What it verifies |
| --- | --- |
| `treefmt` | Rust, Nix, and Lisp sources are canonically formatted |
| `actionlint` | GitHub Actions workflows are well-formed |
| `clippy` | No clippy warnings with `-D warnings` |
| `nextest` | The full test suite under cargo-nextest |
| `package` | The crate builds and its `cargo test` suite passes |
| `documentation` | The MkDocs (Material) site builds to a valid `index.html` |
| `lint-format-integration` | The `paredit-lint` / `paredit-format` gates behave end to end |

## Documentation is tested

The repository treats documentation as part of the public contract. Tests in
`tests/cli/*_contract.rs` read `README.md`, `docs/src/*.md`, `action.yml`, and
`flake.nix` and fail when documented commands, integration surfaces, or policy
statements drift from reality. When you change behaviour, update the
documentation in the same commit — CI enforces it.

To preview the site locally:

```sh
nix build .#docs                   # rendered site in ./result
mkdocs serve -f docs/mkdocs.yml    # live-reloading preview from the dev shell
```

## MSRV

The minimum supported Rust version is declared in `Cargo.toml`
(`rust-version = "1.85"`). Verify it before touching parser, refactor,
packaging, or public API surfaces:

```sh
cargo +1.85 test --locked
```

## Releases

The [release and compatibility guide](releases.md) defines the machine-output
contract and upgrade expectations. Maintainers should use the root
[release checklist](https://github.com/takeokunn/paredit-cli/blob/main/RELEASING.md)
before publishing.
