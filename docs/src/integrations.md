# Integrations

## Report output formats

Every report whose output is a list of located findings accepts the same set of
`--output` values. The two native formats are this tool's own contract; the rest
are other people's schemas, offered so a report can be fed to a system that
already knows how to display findings.

| `--output` | What it is | Where it goes |
| --- | --- | --- |
| `text` | Tab-separated rows, one per finding | A terminal, `grep`, `awk` |
| `json` | The tool's own envelope, with per-file summaries | Agents and scripts |
| `sarif` | SARIF 2.1.0 | GitHub code scanning, Azure DevOps |
| `junit` | JUnit XML | A CI system's test-report panel |
| `code-climate` | Code Climate issue JSON | GitLab Code Quality |
| `csv` / `tsv` | A header row and one row per finding | A spreadsheet, `cut` |
| `html` | A standalone page, no external assets | A CI artifact, a shared link |
| `markdown` | A table | A pull request comment, an issue |
| `github` | GitHub Actions workflow commands | Inline annotations on the diff |

Two properties are worth knowing before wiring one of these into a gate.

**An unexamined file is reported, not omitted.** Most of these analyses model
Common Lisp only. A Fennel file handed to one of them is not clean — it was
never read — and every format says so: SARIF emits a `note`-level result, JUnit
a `<skipped>` testcase, the tables a "not examined" section. A CI panel showing
zero findings for a file the tool never looked at is the failure mode these
formats invite, and this is the guard against it.

**The gate's verdict travels separately.** A `--fail-on-*` flag still decides
the exit status; only SARIF has somewhere to record it inside the document
(`runs[].invocations[].executionSuccessful`). Keep reading the exit code.

```sh
paredit inspect todo --output sarif . > paredit.sarif
paredit inspect macro-hygiene --output junit . > junit.xml
paredit inspect restarts --output markdown . >> "$GITHUB_STEP_SUMMARY"
```

`inspect lint` reaches the same formats through `--emit` rather than `--output`,
because its `--output` also governs a dozen catalogue-only modes (`--explain`,
`--list-rules`, `--stats`, `--timings`) that produce no findings at all:

```sh
paredit inspect lint --emit code-climate . > gl-code-quality-report.json
paredit inspect lint --emit junit . > junit.xml
```

`--emit sarif` and `--emit github` are the older `--sarif` and `--github` flags
under one option; those flags still work and produce byte-identical output.

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
