# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.1.0] - 2026-07-26

No command, flag, exit code, or JSON field changed in this release: the
capability catalogue is byte-identical to v1.0.0. What changed is what the
existing rules can prove, and a new public analysis layer beneath them.

### Added

- A static semantic analysis layer (`paredit_cli::domain::semantics`) of
  read-only side tables beside the syntax tree: bindings, constant values,
  types, and project-wide symbol identity. Facts are recorded only when
  provable — anything uncertain is absent rather than guessed.
- `defconstant` values now cross file boundaries. A constant defined exactly
  once project-wide resolves in the other files of its package; a file's own
  definition always wins, and a file with no `in-package` is unaffected.
- `lint_report::collect_lint_findings_and_fixes`, which answers both halves of
  a lint run from one dispatch pass.

### Changed

- `char-op-string` now flags any argument the type layer proves cannot be a
  character, not only a string literal — `(char= (length xs) c)` is the same
  guaranteed type error as `(char= "a" c)`.
- `redundant-the` now flags an assertion the form already satisfies, such as
  `(the integer (length xs))`, not only the vacuous `(the t x)`.
- Both changes can only add findings. If you gate CI on the lint exit code,
  expect new true positives on code that was always wrong.
- CI runs on Linux only and shares one Nix setup across workflows. Darwin is
  no longer verified in CI; see the development guide.
- The community and policy files (contributing, code of conduct, support,
  security, releasing) moved onto the documentation site.

### Fixed

- `undefined-package` no longer reports a package's own declared `:nicknames`
  as undefined. `(defpackage :app (:nicknames :a))` followed by
  `(in-package :a)` is correct code and was flagged as a typo.
- A `defconstant` is now found by a reference written in any case. The reader
  folds a symbol's case, so `+limit+` and `+LIMIT+` name one constant; the
  value table had keyed them separately.
- Quoted data no longer stops the value layer from reasoning about the
  surrounding scope. `'(setq x 2)` is a list, not an assignment.
- Declaration specifiers are no longer read as calls, so `(declare (ignore x))`
  no longer makes its enclosing scope unanalysable.
- `check-type`, `assert`, `remf`, and `multiple-value-setq` are now recorded as
  writing to their places, so a value is not propagated through a binding the
  program may replace.

### Performance

- Lint runs about 8% faster on finding-dense input, measured against v1.0.0
  back to back on one machine.

## [1.0.0] - 2026-07-26

### Added

- 49 new lint rules since v0.8.0, covering redundant forms, degenerate
  conditionals, explicit-default keyword arguments, string and character
  comparison simplification, and suspicious arithmetic.
- 276 leaf commands across inspect, edit, refactor, and completions.

### Changed

- First stable release: paredit-cli now follows semantic versioning. Within
  the 1.x series, command paths, flag names and documented defaults, the
  exit-code table, documented JSON fields for a given `schema_version`, the
  `paredit_cli` crate-root API, and the Nix packages, apps, overlay, and lib
  helpers change only in a major release.
- Repository moved to github.com/nerima-lisp/paredit-cli and documentation
  moved to nerima-lisp.github.io/paredit-cli.
- paredit-cli is not published to a package registry; the git tag remains
  the release artifact (install via `nix run`, `nix profile install`, or
  `cargo install --git`).

### Fixed

- `paredit edit ... --write` and `paredit refactor ... --write` failed for
  any file named without a directory component. Editing a file in the
  working directory by bare name now works.
- A symlinked input was refused with the raw `O_NOFOLLOW` errno, which read
  as a link cycle. The refusal now names the policy and the remedy.
