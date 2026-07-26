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

The codebase is layered `domain` → `application` → `infrastructure` →
`presentation`, with dependencies pointing only inward. Add a change in the
innermost layer that owns it — parsing and safety rules in `domain`, workflow
orchestration in `application` behind a source port, source discovery in
`infrastructure`, and command wiring in `presentation`. The
[architecture guide](architecture.md) explains the boundaries and
where each kind of change belongs.
