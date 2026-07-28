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

Locally the gate is one command:

```sh
nix flake check
```

It builds and runs every check the project defines:

| Check | What it verifies |
| --- | --- |
| `treefmt` | Rust, Nix, and Lisp sources are canonically formatted |
| `actionlint` | GitHub Actions workflows are well-formed |
| `clippy` | No clippy warnings with `-D warnings`, across all targets and features |
| `nextest` | The full test suite under cargo-nextest |
| `msrv` | The workspace still compiles on the declared MSRV toolchain |
| `package` | The release binary builds |
| `documentation` | The MkDocs (Material) site builds to a valid `index.html` |
| `lint-format-integration` | The `paredit-lint` / `paredit-format` gates behave end to end |

Each check is defined exactly once, and each one is the only place its work
happens. `package` does not run the test suite — that is `nextest`'s job — and
`msrv` stops at `cargo check`, because "does it still compile on 1.85" is the
only question it exists to answer. Duplicating either would compile every test
target a second time under `lto = "fat"`, for no coverage the other checks do
not already give.

### How CI runs it

CI does not run `nix flake check` as one command. A `plan` job runs
`nix flake check --no-build`, which instantiates every flake output — packages,
apps, devShells, overlays, formatter, checks — without building any of them,
and then reads `lib.<system>.ciCheckNames` out of the flake to produce a job
matrix. Every check then builds on a runner of its own.

The reason is that a GitHub runner has four cores. Run as one command, Nix can
only interleave the checks on those four cores, so the gate costs the *sum* of
its checks; fanned out, it costs the *maximum*. The matrix comes from the flake
rather than from `ci.yml` so that adding a check to `mkCoreChecks` is enough to
get it verified.

### Where the build artifacts come from

The Rust checks are [crane](https://github.com/ipetkov/crane) derivations
sharing pre-built `cargoArtifacts`:

```text
depsRelease ─▶ package                dev/release profiles are separate
depsDev     ─▶ clippy, nextest        artifacts, shared within a profile
depsMsrv    ─▶ msrv                   pinned MSRV toolchain
```

A `deps` derivation compiles the 181 locked dependencies from dummified
sources, so its hash depends on `Cargo.lock` and the member manifests and not
on any `.rs` file. A pull request that only edits Rust sources therefore
substitutes all three from the binary cache instead of recompiling the
dependency graph once per check.

### Which host CI verifies

CI runs the gate on Linux only. A flake check is evaluated for the host it runs
on, so the two legs of the old host matrix shared no work: a macOS runner was a
second full verification rather than an extension of the first. (The per-check
matrix above is a different axis — it splits one host's checks across runners
rather than repeating all of them on a second host.)

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
