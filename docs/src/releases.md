# Releases and compatibility

`paredit-cli` follows [Semantic Versioning](https://semver.org/). From `1.0.0`
onward the surfaces listed below are stable for the whole `1.x` series: they
may gain new commands, flags, and fields, but nothing documented as stable is
removed, renamed, or given a different meaning without a major release.

Automation should still pin the `paredit-cli` version it validates and run
`paredit inspect capabilities --output json` during upgrades — the catalog is
generated from the same definition that parses the arguments, so it is the
authoritative diff between two versions.

## What `1.x` guarantees

Stable — changed only in a major release:

- **Command paths.** `paredit inspect check`, `paredit edit wrap`,
  `paredit refactor plan`, and every other leaf command keep their spelling
  and their positional arguments.
- **Flags.** Long names, value syntax, enum values, and documented defaults.
- **Exit codes.** The [exit-code table](agents.md#exit-codes) — `0` success,
  `1` operational failure, `2` usage error, `3` policy gate — keeps its
  meanings.
- **JSON reports.** For a given top-level `schema_version`, documented fields
  keep their names, types, and semantics.
- **The Rust library API** re-exported from the `paredit_cli` crate root.
- **The Nix interface**: the `default`, `lint`, `format`, and `format-files`
  packages, the `default`, `lint`, and `format` apps, the `default` overlay,
  and the `mkLintCheck` / `mkFormatCheck` / `treefmtFormatter` helpers
  exported from `lib` — see [Integrations](integrations.md).

Not stable — may change in any release, including a patch:

- **Human-readable text output.** Wording, ordering, column layout, and
  spacing of `--output text` and of default (non-JSON) reports. Parse
  `--output json` instead.
- **Diagnostic and error message text** on stderr. Branch on the exit code.
- **Everything below the crate root**: module paths, internal types, and any
  item not re-exported from `paredit_cli`.
- **Analysis breadth.** Lint and inspection commands may report *more*
  findings after an upgrade as rules are added or sharpened. Reports are not
  a fixed-size contract; gate on the fields, not on a finding count.

## Machine-readable output

Use `--output json` wherever a command offers it. JSON reports carry a
top-level `schema_version` (currently `1`): fields may be added within a
version, while field removals, renames, or semantic changes require a version
bump. Consumers should reject unsupported schema versions and tolerate unknown
fields in supported versions.

A `schema_version` bump is additive at the command level — a command keeps
emitting the previous version by default, and opt-in flags such as
`--schema-version` select a newer shape — so raising it is a minor release,
not a major one. Dropping support for a previously emitted `schema_version`
is a major release.

`inspect outline` returns a bare JSON array and is the documented exception to
the top-level-object convention.

## Deprecation

A stable surface is never removed without warning. The sequence is:

1. The replacement ships, and the old spelling is documented as deprecated in
   the command reference (a minor release).
2. The deprecated surface keeps working and warns on stderr; stdout, JSON
   output, and exit codes are unaffected, so automation does not break.
3. The removal happens in the next major release, called out in the release
   notes.

## Minimum supported Rust version

The MSRV is declared as `rust-version` in `Cargo.toml` and verified in CI.
Raising it is a minor release, not a major one, so pin a toolchain if you
build from source in automation.

## How releases are distributed

A release *is* an annotated Git tag, plus the GitHub release built from it.
The crate is not published to a package registry, so pin the tag — or a
reviewed commit — in whichever channel you use:

```sh
nix run github:nerima-lisp/paredit-cli/v1.0.0 -- --help
nix profile install github:nerima-lisp/paredit-cli/v1.0.0
cargo install --git https://github.com/nerima-lisp/paredit-cli --tag v1.0.0 --locked
```

Release notes must call out anything that affects automation: new or
deprecated commands and flags, `schema_version` bumps, exit-code behaviour,
Nix interface changes, and MSRV increases.

Maintainers follow
[releasing.md](releasing.md)
to verify the documentation and Nix checks before tagging.
