# Agent interface

`paredit` is designed to be driven by AI coding agents and other automation.
This page collects the machine-facing contracts in one place.

## Discover the command surface

One call returns a catalog of every command, flag, default, and enum value,
generated from the same definition that parses the arguments:

```sh
paredit inspect capabilities --output json
paredit inspect capabilities --output text   # compact human-readable listing
```

The JSON shape is a tree: the root lists all top-level `commands`, including
namespaces, servers, interactive entrypoints, and meta commands,
each with nested `commands` and an `args` array. Every arg entry carries
`long`, `short`, `kind` (`option`, `flag`, or `positional`), `help`,
`required`, `repeatable`, `default_values`, and `possible_values`.

## Connect over MCP

```sh
paredit mcp               # a Model Context Protocol server, over stdio
paredit mcp --read-only   # refuses recognized write flags and write commands
```

The server offers fixed tools (`paredit_check`, `paredit_outline`,
`paredit_lint`, `paredit_format`, `paredit_diff`, `paredit_capabilities`) plus
`paredit_run`, which takes any command's argument vector. **It deliberately does
not expose one tool per command.** Exposing the entire catalog as tool descriptions
costs thousands of tokens of context before the agent has read a line of code,
and it makes selection harder rather than easier. The catalog is available as
the `paredit://capabilities` resource, and `paredit_run` reaches everything in
it.

When a child process completes, its tool result carries a `structuredContent`
object alongside its text:

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

Result and permission semantics:

- **`--read-only` checks recognized flags and command names, not all side effects.**
  It refuses `--write`, `--fix`, `--apply`, and `--in-place`, plus `fix apply`,
  `refactor create-checkpoint`, `refactor delete-checkpoint`, and `config init`,
  before starting the child. It does not block kill-ring writes (`--to-ring`),
  archive extraction (`--extract-to`), manifest output (`--manifest-out`), or
  lint baseline output (`--baseline-out`). It is not a filesystem sandbox.
- **Exit code 3 is a result, not an error.** A `--fail-on-*` gate reporting
  what it found comes back with `isError: false` and
  `structuredContent.gate_failed: true`, so an agent does not retry a command
  that worked.
- **`structuredContent.writes` reports that same classification for this call.**
  Fixed tools have static answers; `paredit_run` classifies its argument vector.
  A `false` value does not establish absence of side effects.
- **Each call re-executes the binary.** Its text joins stdout and stderr, with
  a newline between them when both are nonempty, and decodes invalid UTF-8
  lossily. Stream boundaries and interleaving are not preserved. Do not parse
  that text as a single JSON report when stderr is nonempty.

A read-only refusal returns `isError: true` and text without `structuredContent`.
Invalid tool names or arguments return JSON-RPC errors; failure to start the
child returns a JSON-RPC internal error. Handle these separately from child exit
codes. `paredit_run` takes argv strings without a leading `paredit`, not a shell
command line.

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

Generate types from it, or validate a response against it before parsing. Obtain
the schema from the same paredit release or binary that produced the document,
using its matching `schema_version`. The schema is strict:
`additionalProperties` is `false` throughout. Older cached schemas may reject
newly added fields even when the schema version and ID are unchanged; refresh
cached schemas when upgrading.

A test in this repository validates the live catalog against the emitted schema
at every version. Validate again when upgrading a cached catalog or schema.

## Discover how deep a dialect goes

Versions 2 and 3 include a `dialect_contract`: every command crossed with
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
| `unknown` | Not classified; do not infer support. |

Pay particular attention to `silent`. Almost every `inspect` command
exits `0` for every dialect, so a `finding_count` of `0` looks identical
whether the code is clean or the tool has nothing to say about it. Commands can
be `silent` outside Common Lisp, or for Common Lisp *itself*, because their subject
is another dialect entirely: `inspect elisp-file` and
`inspect division-result-precision-loss` (Emacs Lisp);
`inspect atom-swap-with-side-effect`, `inspect future-promise-never-realized`,
`inspect nested-get-chain`, `inspect redundant-into-empty-collection`,
`inspect with-open-returns-lazy-seq`, `inspect def-inside-function-body`,
`inspect single-key-nested-path`, `inspect apply-with-literal-collection`,
`inspect go-block-blocking-channel-op`,
`inspect parking-op-outside-go-machinery`,
`inspect contains-on-non-associative` and
`inspect reference-type-operator-mismatch` (Clojure); and
`inspect scheme-begin-single-form`,
`inspect scheme-let-star-independent-bindings`,
`inspect scheme-memq-assq-literal-key` and
`inspect scheme-named-let-never-recurs` (Scheme, and Racket for all but the
`memq`/`assq` one). Treat a `silent` report as absent rather than negative,
whatever the dialect.

Each command also reports the `tier` it needs from a dialect: `syntax`
(balanced parens only), `scope` (lexical binder and definition shapes),
`common-lisp-family` (Common Lisp and Emacs Lisp), or
`common-lisp-semantics` (the operator, package and CLOS model). A
`dialect_depth` array summarises the counts per dialect and records whether
the dialect resolves call heads in a separate namespace, which decides whether
renaming a local also rewrites `(f x)`.

The statuses are checked against real invocations in the test suite: a cell
may not claim support for something the command refuses, and a `scope`-tier
report may not claim support while examining nothing.

Version 1 omits `dialect_contract`. Version 2 folds `silent` onto `unsupported`;
version 3 distinguishes them. Use version 3 to distinguish refusal from an empty
report with no implemented analysis.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | Success. For plan/preview commands: the report was produced and no policy gate failed. |
| `1` | Operational failure: parse errors, missing targets, refused writes. |
| `2` | Usage error: unknown command, unknown flag, or invalid value (from argument parsing). |
| `3` | Policy gate failure: a requested `--fail-on-*` / `--require-*` gate or an unconditional gate tripped after the report was printed. The invocation itself was valid; read the report and decide. |

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
no-op into an exit-3 policy failure; pass it whenever you expect the rename to do
something.

## Run-wide controls

These global controls apply across commands. Place them before the `--` argument
separator; consult the catalog for each command's own preview and write flags.

**`--dry-run`** (or `PAREDIT_DRY_RUN=1`) suppresses `--write` and refuses writes
that reach the guarded write layer. It is not a filesystem sandbox: auxiliary
writers, including lint baseline output, can bypass that layer.

At the argument layer it removes `--write` before the command sees it, and says
on stderr that it did. Commands that use `--write` turn into a preview:
the result still goes to stdout.

At the guarded write layer, writing is refused while `--dry-run` is in force.
This covers source writes by `inspect lint --fix`, which does not use `--write`.
Such a refusal carries `refusal.dry-run` and a pointer to a preview flag:

```
$ paredit inspect lint --fix --dry-run src/
Error [refusal.dry-run]: refusing to write: --dry-run is in force. ...
  try: or use this command's own preview: --diff on an edit, --fix --diff on lint
```

Refusing rather than skipping is deliberate: a command reporting success
without doing what it said is worse than an error naming the flag.

**`--progress`** emits progress events as JSON Lines on stderr:

```
{"event":"discovered","files":214,"root":"src"}
{"event":"file","sequence":1,"path":"src/core.lisp"}
{"event":"file","sequence":2,"path":"src/reader.lisp"}
```

stderr because stdout is the report contract; JSON Lines because a line is
complete the moment it is written, so a reader can act on it before the run
ends. `sequence` counts up, so a consumer can tell it missed a line. Progress
never changes stdout. Stderr can also contain plain run-control notes, formatted
errors, and multi-line JSON error objects. Do not assume every stderr line is a
progress object.

**`--no-config` / `--config` / `--no-config-env`** are the configuration
controls; see [Configuration](../reference/configuration.md).

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

There are three kinds, identified by the flag's spelling:

| Spelling | `kind` | Fails when |
| --- | --- | --- |
| `--fail-on <severity>` | `severity` | a finding at or above the level |
| `--intended-bump <bump>` | `severity` | API impact exceeds the intended bump |
| `--fail-on-<condition>` | `presence` | the named failure condition holds, including `no-change`, `no-match`, or `empty` |
| `--fail-on-risk-level <level>` | `presence` | risk is at or above the level |
| `--fail-on-max-depth <N>` | `presence` | depth exceeds `N` |
| `--require-<thing> <N>` | `minimum` | fewer than `N` were found |

`presence` describes a named policy condition, not the argument's type. The
risk-level and maximum-depth gates use this kind; their values
configure the condition. Use the flag's help and possible values when building
an invocation, rather than assuming every `presence` gate is boolean.

`gates` contains only flag-controlled gates. Commands that apply a policy
without an enabling flag publish `unconditional_gates` instead. For example,
`inspect dotfiles` exits with code 3 after printing an incomplete inventory:

```json
{
  "name": "dotfiles",
  "unconditional_gates": [
    { "kind": "presence", "exit_code": 3, "help": "Inventory is incomplete; see diagnostics." }
  ]
}
```

Each field is **absent**, not empty, when there is no gate of that type.
Both fields are absent on a command with no known policy gate. Contract tests
enforce the flag convention and the unconditional gate metadata.

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

`writes` is a static classification of recognized write flags and command names,
not an inventory of all filesystem side effects. It uses the flag and command
lists described in [Connect over MCP](#connect-over-mcp). A leaf is `true` if it
offers one of those flags or has one of those names; MCP checks the actual argv.
Kill-ring writes, archive extraction, manifest output, and lint baseline output
are not counted. Do not treat `false` as a safety boundary.

`possible_error_codes` predicts [error codes](#error-identity-and-repairs)
from the command's argument shape; it is not a complete set. Absence means no
known signal predicts it, not that it is provably unreachable. Use it to know
ahead of a call which `category`/`repairs` branches are worth handling for
this specific command. Always retain a fallback for an unlisted code.

Both fields are present on every schema version (1, 2, and 3), unlike
`dialect_contract`, which only versions 2 and beyond carry. These are additive
leaf properties within existing schema versions. This does not guarantee that
older strict schemas accept newer documents; refresh cached schemas when
upgrading.

## Determinism

A contract test runs a sample of reports and edits twice in separate processes
and compares stdout bytes. This does not establish determinism for every command
or external-state-dependent report. Cache only with the same binary, argv,
configuration, relevant environment, paths, source and auxiliary inputs, and
external state. Source contents alone are not a sufficient cache key.

## Fitting a report in a context window

`inspect agent-report` takes three flags that matter when the report has to
share a budget with everything else you are holding.

**`--verbosity quiet | normal | detailed`.** `quiet` drops the outline and atom
lists and keeps every count, so you still learn the file's shape (how many
top-level forms, definitions, and atoms) and can decide whether
the detail is worth asking for. `detailed` adds the document digest and the
distinct-atom count. `normal` is the default.

**`--max-tokens <N>`.** A soft budget estimated as the compact JSON UTF-8 byte
length divided by four, rounded up, not a model tokenizer count. Zero means
unlimited. Lists are trimmed from the end,
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
one produced without the flag. An irreducible envelope can exceed the budget
without failing the command. The estimate excludes subsequently added truncation
metadata and next-command suggestions, and pretty-print whitespace; it does not
bound the final output size. Global input/work budgets are separate: their
failures can emit plain stderr and exit 2 before report dispatch.

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

Outline entries are matched on `head` plus `name`, with a path-based fallback
for unnamed entries. Duplicate identities collapse to one entry. `moved`
reports path changes, not body changes. `unchanged` checks only outline additions,
removals, and moves plus comparability; it ignores body edits, atom counts, and
the detailed digest. Do not use it to skip validation or update selectors
without resolving them again.

`comparable` checks whether the baseline outline length equals its recorded
`metrics.outlineEntries`; a missing metric falls back to the outline length.
It does not validate the file, dialect, or report provenance. A quiet baseline
with forms is not comparable, but a zero-form baseline can be. Use a full,
untruncated baseline from the same file and dialect.

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

Each `command` is a POSIX-shell command string, not an argv array. Inspect and
validate it before execution, including path quoting and shell expansion; pass
parsed argv without the executable name to `paredit_run`. The field is absent,
not empty, both when suggestions are disabled and when none are available.
Absence does not distinguish those cases. Suggestions depend on report contents.

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

Three properties distinguish the report:

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
diagnostics: the error prefix, the repair suggestions, the failure-category
descriptions, and the run-control notes.

Everything a *program* matches on stays English in every language: error
`code`s, `category` labels, repair `action`s, finding `kind`s, rule names, and
every JSON key. Report payloads stay English too, and are byte-identical
whatever the language is set to. Translating an identifier would break every
consumer to help nobody.

## Error identity and repairs

An exit code says *that* a command failed. Failures handled by the report
dispatcher also carry a stable code saying *what kind*, and repair suggestions.

The rendering follows the format the command's own report would have used, so a
dispatcher error for a JSON-report command is JSON on stderr. Argument-parser,
configuration-loading, and global budget failures can occur before dispatch and
use plain stderr instead. Policy gates print the report before exiting 3.
For example, a dispatched selection failure is:

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
  try: list the paths this document actually has, then select one of them; paredit inspect outline --output json --file src/a.lisp
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
| `gate` | A policy gate tripped. The report was printed first | no |
| `internal` | Unclassified; investigate the tool's diagnostic | no |

`retryable` is stated outright so an agent does not have to infer it. Only
`environment` is: re-running an identical command after a selection failure
will fail identically.

A `repairs[].command` is a suggested POSIX-shell command string to inspect and
validate before running, not an argv array or an execution guarantee. It is
`null` when the failure did not carry enough context to build one, and the
`detail` is then the whole answer. `action` is the machine-readable kind:
`inspect-first`, `change-selection`, `pass-flag`, `re-read`, `fix-source`,
`check-configuration`.

Codes are namespaced `<category>.<name>`. Adding one is a compatible change;
renaming one is not.

## Output contract

- `--output json` is the stable, parseable contract; prefer it everywhere it
  is offered. Text output is for humans and may change freely.
- Most object-shaped JSON reports carry a top-level `schema_version` (currently `1`).
  The capabilities catalog defaults to that version and also supports versions
  2 and 3; `inspect outline` emits a bare array.
  If `schema_version` is absent, use the command's documented contract;
  do not infer a version from its absence.
  New fields can be added within a version; refresh strict cached schemas.
- JSON reports go to stdout. Dispatched errors use the selected report format
  on stderr; pre-dispatch parser, configuration, and global budget errors can
  be plain text. Stderr also contains run-control notes and progress events.
  Do not parse all stderr as one JSON document or assume every line is a
  progress event; see [Run-wide controls](#run-wide-controls) for stream formats.
- For a single-file source rewrite, `paredit edit` commands offering `--diff`
  and `--write` print the whole rewritten document to stdout by default.
  `--diff` switches stdout to a unified diff; `--write` persists the result to
  `--file` instead and prints nothing (combine with `--diff` to write and see
  the diff at once). `edit select` and `edit copy` print selected text;
  `edit navigate` prints a path or a JSON navigation report.

Command paths, flags, exit codes, and documented JSON fields are covered by
semantic versioning from `1.0.0` onward; see
[Releases and compatibility](../reference/compatibility.md) for the full list of what a `1.x`
upgrade may and may not change.

## Safe editing loop

The recommended loop for one file uses paths from
[Selecting forms](../reference/selectors.md). Replace the example `0.2` with an observed
target path and use the same path for preview and apply:

```sh
# 1. Validate before touching anything.
paredit inspect check --file source.lisp

# 2. Locate the target form and choose its path from this report.
paredit inspect outline --file source.lisp --output json

# 3. Preview the structural edit as a diff.
paredit edit wrap --file source.lisp --path 0.2 --diff

# 4. Apply the reviewed edit in place.
paredit edit wrap --file source.lisp --path 0.2 --write

# 5. Validate again.
paredit inspect check --file source.lisp
```

`--write` refuses to write when the rewritten document no longer parses, and
file writes are staged with rollback on failure. Rollback itself can fail;
inspect diagnostics and transaction artifacts, re-read affected files, and
validate them before proceeding after any write failure.

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

The repository also ships operational patterns as an agent skill in
`skills/paredit-cli/SKILL.md`, ready to drop into a Claude Code or similar
agent configuration.
