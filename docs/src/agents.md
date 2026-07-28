# Agent interface

`paredit` is designed to be driven by AI coding agents and other automation.
This page collects the machine-facing contracts in one place.

## Discover the command surface

One call returns a catalog of every command, flag, default, and enum value,
generated from the same definition that parses the arguments — it cannot
drift from the real interface:

```sh
paredit inspect capabilities --output json
paredit inspect capabilities --output text   # compact human-readable listing
```

The JSON shape is a tree: the root lists top-level `commands` (the
`inspect`/`edit`/`refactor` namespaces plus the `completions` meta command),
each with nested `commands` and an `args` array. Every arg entry carries
`long`, `short`, `kind` (`option`, `flag`, or `positional`), `help`,
`required`, `repeatable`, `default_values`, and `possible_values`.

## Discover how deep a dialect goes

`--schema-version 3` adds a `dialect_contract`: every command crossed with
every dialect, so you can tell before invoking anything whether a command
knows the dialect you are pointing it at.

```sh
paredit inspect capabilities --schema-version 3
```

Each cell carries one of four statuses:

| Status | Meaning |
| --- | --- |
| `supported` | The command's analysis is implemented for this dialect. |
| `silent` | **The command succeeds and reports nothing, because it has no rules for this dialect.** An empty report is not a clean bill of health. |
| `unsupported` | The command refuses and exits non-zero. |
| `unknown` | Not classified. No cell answers this today. |

`silent` is the one worth reading carefully. Almost every `inspect` command
exits `0` for every dialect, so a `finding_count` of `0` looks identical
whether the code is clean or the tool has nothing to say about it. Roughly 155
of the 275 commands are `silent` outside Common Lisp; treat their output as
absent rather than negative.

Each command also reports the `tier` it needs from a dialect — `syntax`
(balanced parens only), `scope` (lexical binder and definition shapes),
`common-lisp-family` (Common Lisp and Emacs Lisp), or
`common-lisp-semantics` (the operator, package and CLOS model). A
`dialect_depth` array summarises the counts per dialect and records whether
the dialect resolves call heads in a separate namespace, which decides whether
renaming a local also rewrites `(f x)`.

A tier says what a command *needs*, not what every dialect delivers, so a
handful of cells are overridden by measurement where the two disagree. The
statuses are checked against real invocations in the test suite: a cell may
not claim support for something the command refuses, and a `scope`-tier report
may not claim support while examining nothing.

Schema versions 1 and 2 predate `silent` and fold it onto `unsupported`.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | Success. For plan/preview commands: the report was produced and no requested gate failed. |
| `1` | Operational failure: parse errors, missing targets, refused writes. |
| `2` | Usage error: unknown command, unknown flag, or invalid value (from argument parsing). |
| `3` | Policy gate failure: a requested `--fail-on-*` / `--require-*` gate tripped after the report was printed. The invocation itself was valid — read the report and decide. |

Treat any non-zero exit as a blocker, but branch on the code: `3` means "the
tool worked and told you no", `1` means the invocation itself broke. Policy
gates exist so a command exits non-zero instead of silently under-matching;
prefer running plan/preview commands
with explicit gates such as `--fail-on-blocking-gate`, `--require-edits 1`,
or `--require-definitions 1`. Occurrence reports gate the same way:
`inspect find-symbol`/`inspect symbols` take `--require-occurrences N` and
`inspect calls` takes `--require-calls N`, so an expected-but-missing symbol
fails loudly instead of returning an empty report. Every `rename-*` command
accepts `--fail-on-no-change`, which turns a zero-match rename from a silent
no-op into an exit-1 failure — pass it whenever you expect the rename to do
something.

## Output contract

- `--output json` is the stable, parseable contract; prefer it everywhere it
  is offered. Text output is for humans and may change freely.
- Every object-shaped JSON report carries a top-level `schema_version`
  (currently `1`). New fields may be added within a version; renames or
  removals bump it. (`inspect outline` emits a bare array and is the one
  exception.)
- JSON reports go to stdout; diagnostics and errors go to stderr as text.
- `paredit edit` commands print the whole rewritten document to stdout by
  default. `--diff` switches stdout to a unified diff; `--write` persists the
  result to `--file` instead and prints nothing (combine with `--diff` to
  write and see the diff at once).

Command paths, flags, exit codes, and documented JSON fields are covered by
semantic versioning from `1.0.0` onward — see
[Releases and compatibility](releases.md) for the full list of what a `1.x`
upgrade may and may not change.

## Safe editing loop

The recommended loop for one file:

```sh
# 1. Validate before touching anything.
paredit inspect check --file source.lisp

# 2. Locate the target form (paths and spans — see Selecting forms).
paredit inspect outline --file source.lisp --output json

# 3. Preview the structural edit as a diff.
paredit edit wrap --file source.lisp --path 0.2 --diff

# 4. Apply it in place. The write is validated and rolled back on failure.
paredit edit wrap --file source.lisp --path 0.2 --write

# 5. Validate again.
paredit inspect check --file source.lisp
```

`--write` refuses to write when the rewritten document no longer parses, and
file writes are staged with automatic rollback, so a failed write never
leaves a truncated or unbalanced file behind.

For semantic, multi-file changes use the gated
[refactor workflow](workflows.md): `plan` → `preview` → `verify --phase pre`
→ `--write` (or manifest `apply` with hash guards) → `verify --phase post`.

## Rules of thumb for agents

1. Never hand-edit balanced delimiters; every structural change goes through
   a paredit command.
2. Run `paredit inspect check` before and after a batch of edits.
3. Never pass `--write` until a no-write preview (`--diff`, plan JSON, or
   preview manifest) has been reviewed.
4. Use the narrowest command that matches the binding kind:
   `rename-function`, `rename-binding`, `rename-macrolet`, … before falling
   back to the generic `rename-symbol`.
5. Prefer `--path` from a report over `--at` guesses; reserve `--at` for
   offsets sourced from another tool.

The repository also ships this contract as an agent skill in
`skills/paredit-cli/SKILL.md`, ready to drop into a Claude Code or similar
agent configuration.
