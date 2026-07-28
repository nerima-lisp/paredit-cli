# Contributing to paredit-cli

Thanks for improving `paredit-cli`. The project accepts bug reports,
documentation corrections, tests, and focused implementation changes.

## Before opening an issue

- Search existing issues for the same behavior.
- Reduce bugs to a minimal, balanced S-expression and include the command,
  expected result, actual result, and `paredit --version` output.
- Do not report security-sensitive behavior in a public issue. Follow
  [security-policy.md](security-policy.md) instead.

## Development environment

The committed Nix flake provides the complete development toolchain:

```sh
nix develop
cargo test --locked
nix flake check
```

Enable the shared blame-ignore list once per clone, so bulk mechanical
revisions do not mask the real author of a line:

```sh
git config blame.ignoreRevsFile .git-blame-ignore-revs
```

`.git-blame-ignore-revs` lists only revisions with no behavioural change —
mechanical Clippy fixes and the pure file-move commits of the package
migration. GitHub applies it to its blame view automatically.

`nix flake check` is the required verification gate. It checks formatting,
GitHub Actions syntax, Clippy, the test suite, package construction, rendered
documentation, the exact MSRV build/test, and the lint/format integration
paths. See the full
[development guide](development.md) for the local development loop
and MSRV verification.

## Pull requests

- Keep each pull request focused on one user-visible problem.
- Add or update tests for behavior changes, including failure and safety paths.
- Update the command reference and workflow documentation when CLI behavior,
  JSON output, or Nix integration changes.
- Prefer preview and verification flows in examples; destructive file updates
  must require an explicit `--write` or apply action.
- Explain the user impact and the verification commands you ran in the pull
  request description.

## Design expectations

The CLI is structure-aware rather than text-replacement based. Changes must
preserve balanced S-expression syntax, avoid rewriting strings and comments
unless the command explicitly targets them, and retain the read/preview/write
separation documented in the project guides.

The codebase is a Cargo workspace: a thin composition root plus 24 packages
under `packages/core/` and `packages/feature/`. Find the package that owns what
you are changing and work inside it — one feature is one directory, so a change
should not need to span several. Each package's `README.md` says what it is for,
what it deliberately refuses, and where a change of a given kind belongs; the
[architecture guide](architecture.md) covers how the packages relate.

Develop against one package rather than the whole tree:

```sh
cargo nextest run -p paredit-feature-similarity   # ~1s
cargo nextest run                                 # the whole suite, ~60s
```

Adding a package means three things, in the same commit:

1. `packages/<kind>/<name>/Cargo.toml`, including `[lints] workspace = true` —
   without it the package silently loses `unsafe_code = "deny"`.
2. `packages/<kind>/<name>/README.md`, whose first heading is the package name.
3. `src/lib.rs` starting with `#![doc = include_str!("../README.md")]`, so a
   stale README shows up in rustdoc.

Contract tests in `tests/cli/architecture_contract.rs` enforce all three, plus
two rules that are otherwise invisible until broken: a core package must not
depend on a feature, and `clap` must not appear outside a `cli` module.

Label fenced code blocks in a package README `text` or `rust,ignore`. Because
`include_str!` embeds it, an unlabelled block is compiled as a doctest.
