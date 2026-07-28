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

### Which host CI verifies

CI runs the gate on Linux only. `nix flake check` evaluates the checks for the
host it runs on, so the two legs of the old matrix shared no work: a macOS
runner was a second full verification rather than an extension of the first.

**Darwin is therefore unverified in CI, and that is a real gap rather than a
free simplification.** Most of the crate is portable, and the 237 `cfg(unix)`
blocks are exercised on Linux like anywhere else. But two areas are macOS-only
and Linux cannot even compile them:

| Code | What it does |
| --- | --- |
| `src/presentation/cli/macos_acl.rs` | Reads and restores POSIX ACLs so an autofix preserves file permissions. Twelve `unsafe` blocks over `libc`. |
| the `target_os = "macos"` arm in `src/presentation/cli/io.rs` | The Darwin path of the atomic file replacement used by every write. |

Both sit on the path that rewrites a user's source files, so a regression there
is expensive and CI will not catch it. Until that changes, **run
`nix flake check` on a Mac before tagging a release** — locally, that command
is the Darwin check, and the release checklist calls for it.

Restoring the matrix is a small change to `.github/workflows/ci.yml`: the
contract test beside it asserts the *gates* rather than the hosts, precisely so
that stays a free choice.

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
contract and upgrade expectations. Maintainers should use the
[release checklist](releasing.md)
before publishing.
