# Integrations

## GitHub Actions

The repository ships a composite action that runs the structural lint and
canonical-format gates:

```yaml
- uses: nerima-lisp/paredit-cli@main
  with:
    mode: lint        # lint | format | fix
    paths: src tests  # files or directories, scanned recursively
```

| Input | Default | Meaning |
| --- | --- | --- |
| `mode` | `lint` | `lint` fails on structural parse errors; `format` fails when a source is not in canonical format; `fix` rewrites sources in place. |
| `paths` | `.` | Space-separated files or directories to scan. |
| `version` | pinned ref | `paredit-cli` git ref to run; defaults to the ref the action is pinned to. |
| `cachix-name` | `takeokunn-paredit-cli` | Public Cachix cache for prebuilt binaries. |

For ad-hoc use, invoke the Nix flake directly with canonical command paths:

```yaml
- name: Check Lisp source
  run: nix run github:nerima-lisp/paredit-cli -- inspect check --file source.lisp
```

## Nix flake

The flake exposes packages, apps, and reusable check helpers:

```sh
nix run github:nerima-lisp/paredit-cli -- inspect check --file source.lisp
nix run github:nerima-lisp/paredit-cli#lint -- .
nix run github:nerima-lisp/paredit-cli#format -- --check .
```

Downstream flakes can reuse the gates and the formatter:

- `lib.<system>.mkLintCheck { src = ./.; }` — a derivation that fails on
  structural parse errors, suitable for `checks`.
- `lib.<system>.mkFormatCheck { src = ./.; }` — the canonical-format gate as a
  derivation.
- `lib.<system>.treefmtFormatter` — a treefmt formatter entry covering every
  extension paredit detects a dialect for (see
  [Selectors](./selectors.md#files-and-stdin)).
- `overlays.default` — adds `paredit-cli`, `paredit-lint`, `paredit-format`,
  and `paredit-format-files` to nixpkgs.

## Nix development shell

```sh
nix develop
cargo test
paredit inspect check --file source.lisp
```

## AI coding agents

The `skills/paredit-cli/` directory packages the agent-facing skill contract:
when to reach for `paredit` instead of hand-editing delimiters, and which
plan/preview/verify sequences are safe to automate.

## GitHub Pages

This site is built from `docs/src` with MkDocs (Material) via the Nix flake
(`nix build .#docs`) and published by the `Publish documentation` workflow.
The same derivation runs as `checks.documentation` in `nix flake check`, so a
broken site fails CI before it can reach the site.
