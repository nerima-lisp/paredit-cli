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

The statuses are checked against real invocations in the test suite: a cell
may not claim support for something the command refuses, and a `scope`-tier
report may not claim support while examining nothing.

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

## Fitting a report in a context window

`inspect agent-report` takes three flags that matter when the report has to
share a budget with everything else you are holding.

**`--verbosity quiet | normal | detailed`.** `quiet` drops the outline and atom
lists and keeps every count, so you still learn the file's shape — how many
top-level forms, how many definitions, how many atoms — and can decide whether
the detail is worth asking for. `detailed` adds the document digest and the
distinct-atom count. `normal` is the default and is unchanged.

**`--max-tokens <N>`.** An approximate ceiling. Lists are trimmed from the end,
so what remains is a prefix in source order, and the report says exactly what
went:

```json
"truncation": {
  "truncated": true,
  "budget_tokens": 1500,
  "approximate_tokens": 1498,
  "arrays": [{ "key": "atoms", "kept": 25, "total": 812, "dropped": 787 }]
}
```

The counts in `metrics` are never trimmed: they are how you learn what you are
missing. Atoms are given up before the outline, because an outline entry
carries far more per token and a specific atom can be fetched directly with
`inspect find-symbol`. A budget that is met leaves the report byte-identical to
one produced without the flag. A budget the envelope cannot meet is reported
honestly rather than faked.

**`--since <FILE>`.** Compare against a previous `--output json` report from the
same command and add a `delta`:

```json
"delta": {
  "comparable": true,
  "unchanged": false,
  "outline": {
    "added":   [{ "head": "defun", "name": "new", "path": "0" }],
    "removed": [],
    "moved":   [{ "name": "defun f", "from": "0", "to": "1" }]
  },
  "atom_occurrences": { "previous": 9, "current": 13 }
}
```

Definitions are matched on `head` plus `name`, not on path. That is what makes
`moved` meaningful: inserting one definition at the top of a file is one
addition and *n* moves, not *n+1* additions — and `from`/`to` is exactly what
you need to update stored `--path` selectors. `comparable` is `false` when the
baseline was written at `--verbosity quiet` and has no outline to compare
against.

## What to run next

Reports carry a `next_commands` array when their own contents justify one:

```json
"next_commands": [
  {
    "command": "paredit inspect lint --output json src/core.lisp",
    "why": "2 definition-like forms are present; lint reports logic bugs in them"
  }
]
```

Each `command` runs exactly as written, paths quoted where they need it. The
field is absent — not empty — when the report has nothing to suggest, so
"nothing to suggest" and "we did not look" stay distinguishable. Suggestions
are derived from what the report found, never emitted unconditionally.

## Error identity and repairs

An exit code says *that* a command failed. Every failure also carries a stable
code saying *what kind*, and whatever steps would plausibly get past it.

The rendering follows the format the command's own report would have used, so a
command that answers in JSON also fails in JSON — on stderr:

```json
{
  "schema_version": 1,
  "status": "error",
  "command": "inspect form",
  "error": {
    "code": "selection.path-not-reachable",
    "category": "selection",
    "retryable": false,
    "exit_code": 1,
    "message": "path segment 9 is out of range: the form at path 0 has 4 child expressions (valid indexes 0..=3)",
    "repairs": [
      {
        "action": "inspect-first",
        "detail": "list the paths this document actually has, then select one of them",
        "command": "paredit inspect outline --output json --file src/a.lisp"
      },
      {
        "action": "change-selection",
        "detail": "select by byte offset instead of by path, with --at <offset>",
        "command": null
      }
    ]
  }
}
```

For a command whose report is text, the same failure reads:

```
Error [selection.path-not-reachable]: path segment 9 is out of range: ...
  try: list the paths this document actually has, then select one of them — paredit inspect outline --output json --file src/a.lisp
  try: select by byte offset instead of by path, with --at <offset>
```

**Branch on `category`, not on the message.** There are seven, and they answer
different questions:

| Category | Meaning | Retryable |
| --- | --- | --- |
| `argument` | The command line does not describe a runnable request | no |
| `selection` | `--path` or `--at` did not resolve; a different one might | no |
| `input` | The source is not what the operation needs | no |
| `refusal` | Declined for safety; the state has to change first | no |
| `environment` | The filesystem or environment failed | **yes** |
| `gate` | A requested gate tripped. The report was printed first | no |
| `internal` | Unclassified — a defect in this tool, not in your call | no |

`retryable` is stated outright so an agent does not have to infer it. Only
`environment` is: re-running an identical command after a selection failure
will fail identically.

A `repairs[].command` is a command line that runs exactly as written; it is
`null` when the failure did not carry enough context to build one, and the
`detail` is then the whole answer. `action` is the machine-readable kind:
`inspect-first`, `change-selection`, `pass-flag`, `re-read`, `fix-source`,
`check-configuration`.

Codes are namespaced `<category>.<name>`. Adding one is a compatible change;
renaming one is not.

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
