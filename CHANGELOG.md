# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
