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

## Connect over MCP

```sh
paredit mcp               # a Model Context Protocol server, over stdio
paredit mcp --read-only   # …that refuses every command which would write
```

The server offers a handful of tools — `paredit_check`, `paredit_outline`,
`paredit_lint`, `paredit_format`, `paredit_diff`, `paredit_capabilities` — plus
`paredit_run`, which takes any command's argument vector. **It deliberately does
not expose one tool per command.** There are 432 of them; that many descriptions
costs thousands of tokens of context before the agent has read a line of code,
and it makes selection harder rather than easier. The catalog is available as
the `paredit://capabilities` resource, and `paredit_run` reaches everything in
it.

Every call's result carries a `structuredContent` object alongside its text:

```json
{
  "content": [{ "type": "text", "text": "..." }],
  "isError": false,
  "structuredContent": {
    "exit_code": 0,
    "gate_failed": false,
    "writes": true
  }
}
```

Four things worth knowing:

- **`--read-only` is a promise, not a report.** A command carrying `--write`,
  `--fix`, or `--apply` is refused before the process starts, and so is one
  whose write is baked into its own name, like `fix apply` or `refactor
  create-checkpoint`.
- **Exit code 3 is a result, not an error.** A `--fail-on-*` gate reporting
  what it found comes back with `isError: false` and
  `structuredContent.gate_failed: true`, so an agent does not retry a command
  that worked.
- **`structuredContent.writes` says whether *this* call asked for a write.**
  The seven fixed tools each have a static answer, but `paredit_run` carries
  any argument vector, so its answer varies call to call — this is the same
  check `--read-only` gates on, reported after the fact for the one tool
  whose write-or-not is not knowable from its name alone.
- **Each call re-executes the binary**, so a tool's behaviour is byte-identical
  to the same command typed at a shell.

Resources: `paredit://capabilities` (the catalog), `paredit://capabilities-schema`
(its JSON Schema), and `paredit://lint-rules` (the full rule reference as
Markdown).

## Type the catalog

The catalog conforms to a published JSON Schema (draft 2020-12), which the same
command emits:

```sh
paredit inspect capabilities --emit schema                     # for the v1 catalog
paredit inspect capabilities --emit schema --schema-version 3  # for the v3 catalog
```

Generate types from it, or validate a response against it before parsing. The
schema is versioned with the catalog and is *strict* — `additionalProperties`
is `false` throughout — so a version 1 schema rejects a version 3 document
rather than quietly accepting a field it does not know. Ask for the schema whose
version matches the `schema_version` in the document you hold, not the newest
one.

A test in this repository runs the live catalog through the emitted schema at
every version, so the two cannot drift.

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
whether the code is clean or the tool has nothing to say about it. 234 of the
432 commands are `silent` for at least one dialect outside Common Lisp, and
three of those — `inspect elisp-file`, `inspect atom-swap-with-side-effect`
and `inspect future-promise-never-realized` — are `silent` for Common Lisp
itself, because their subject is another dialect entirely. Treat a `silent`
report as absent rather than negative, whatever the dialect.

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

## Run-wide controls

Three flags apply to every command, so a harness can append them to a command
line it did not construct.

**`--dry-run`** (or `PAREDIT_DRY_RUN=1`) writes nothing, and that is enforced
in two places.

At the argument layer it removes `--write` before the command sees it, and says
on stderr that it did. `--write` is how ~115 mutating commands spell writing, so
those turn into a preview: the result still goes to stdout.

At the write layer, every write in this tool funnels through one function, and
that function refuses outright while `--dry-run` is in force. That covers the
commands that spell writing some other way — `inspect lint --fix` is the live
example — and it covers commands added later without anyone remembering to. You
get `refusal.dry-run` and a pointer to that command's own preview flag, rather
than a silent write:

```
$ paredit inspect lint --fix --dry-run src/
Error [refusal.dry-run]: refusing to write: --dry-run is in force. ...
  try: or use this command's own preview: --diff on an edit, --fix --diff on lint
```

Refusing rather than skipping is deliberate: a command reporting success
without doing what it said is worse than an error naming the flag.

**`--progress`** emits JSON Lines on stderr, one object per line:

```
{"event":"discovered","files":214,"root":"src"}
{"event":"file","sequence":1,"path":"src/core.lisp"}
{"event":"file","sequence":2,"path":"src/reader.lisp"}
```

stderr because stdout is the report contract; JSON Lines because a line is
complete the moment it is written, so a reader can act on it before the run
ends. `sequence` counts up, so a consumer can tell it missed a line. Progress
never changes stdout.

**`--no-config` / `--config` / `--no-config-env`** are the configuration
controls — see [Configuration](../reference/configuration.md).

## Discovering the gates

Every command that can fail on a policy publishes its gates in
`inspect capabilities`:

```json
{
  "name": "lint",
  "gates": [
    { "flag": "--fail-on", "kind": "severity", "exit_code": 3, "help": "..." },
    { "flag": "--fail-on-finding", "kind": "presence", "exit_code": 3, "help": "..." }
  ]
}
```

There are three kinds, and the spelling tells you which:

| Spelling | `kind` | Fails when |
| --- | --- | --- |
| `--fail-on <severity>` | `severity` | a finding at or above the level |
| `--fail-on-<thing>` | `presence` | any `<thing>` was found |
| `--require-<thing> <N>` | `minimum` | fewer than `N` were found |

The field is **absent**, not empty, on a command with no gate, so "cannot fail
on a policy" and "we did not look" stay distinguishable. A contract test
enforces the convention, so a gate that does not follow it cannot ship.

## Discovering what a command can write and how it can fail

Every invocable command (a leaf, not a namespace like `inspect` or `edit`)
also carries `writes` and `possible_error_codes` in `inspect capabilities`:

```json
{
  "name": "create-checkpoint",
  "writes": true,
  "possible_error_codes": [
    "argument.flag-combination",
    "argument.no-input",
    "environment.io",
    "input.dialect-unsupported",
    "refusal.write-target",
    "..."
  ]
}
```

`writes` is `true` when the command is capable of modifying a file under some
argument combination — a `--write`/`--fix`/`--apply`/`--in-place` flag, or a
command whose name is itself the write, such as `fix apply` or `refactor
create-checkpoint`. It mirrors the same check `paredit mcp --read-only` gates
on (see below), so a `false` here is a promise the MCP server keeps too. It is
`false` on a command that can still touch disk outside this promise — the
kill ring (`--to-ring`) and archive extraction (`--extract-to`) are not
counted.

`possible_error_codes` is a superset of the [error codes](#error-identity-and-repairs)
this command can realistically exit with, gathered from its own argument
shape rather than a proof of completeness: absence of a code here means no
known signal predicts it, not that it is provably unreachable. Use it to know
ahead of a call which `category`/`repairs` branches are worth handling for
this specific command, rather than the full 44-code table every command could
theoretically need.

Both fields are present on every schema version (1, 2, and 3) — unlike
`dialect_contract`, which only versions 2 and beyond carry, they are
additive-only leaf properties and do not change the shape of an existing
collection, so there is no older document shape a schema revision has to keep
validating around them.

## Determinism

Same input, same bytes. Identical invocations over identical sources produce
byte-identical stdout, across processes and across runs — which is what makes
it safe to cache a report, diff two of them, or hash one into a manifest.

This is checked rather than intended: a contract test runs a sample of reports
and edits twice in separate processes and compares bytes. Rust randomises its
hash seed per process, so a finding rendered in `HashMap` iteration order fails
that test immediately rather than intermittently in your pipeline.

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

## Explaining a change

`inspect change --before <FILE> --after <FILE>` compares two versions
structurally and answers in a reviewer's terms rather than a diff's:

```
$ paredit inspect change --before old.lisp --after new.lisp --output text
1 added, 1 renamed and 1 modified definitions.

- Added `defun read-config` (line 2).
- Changed the body of `defun parse-header` (line 3, was line 2).
- Renamed `old-body` to `new-body` (line 4); the body is unchanged.
```

The JSON carries both that draft and the facts it was rendered from, so you
can paste one or compute with the other.

Three properties make it worth reading:

- **A rename is a rename.** A removal and an addition whose bodies match once
  the name is set aside is reported as one rename, not two changes. It is only
  inferred from an exact match: a rename that also changed the body is reported
  as the addition and removal it literally is, because a confidently wrong
  summary in a pull request costs more than a vague one.
- **Formatting is called formatting.** The comparison runs over the normalised
  shape, so reindenting a definition is not a change to it. `formatting_only`
  is the single most useful thing this command can tell a reviewer.
- **Definitions are matched by identity, not position.** Inserting one
  definition at the top of a file is one addition and *n* moves, each with its
  old and new path.

`--fail-on-change` gates on substance: a reformat does not trip it.

## Message language

`output.language = "ja"` (or `PAREDIT_OUTPUT_LANGUAGE=ja`) translates
diagnostics — the error prefix, the repair suggestions, the failure-category
descriptions, and the run-control notes.

Everything a *program* matches on stays English in every language: error
`code`s, `category` labels, repair `action`s, finding `kind`s, rule names, and
every JSON key. Report payloads stay English too, and are byte-identical
whatever the language is set to. Translating an identifier would break every
consumer to help nobody.

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
[Releases and compatibility](../reference/compatibility.md) for the full list of what a `1.x`
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
