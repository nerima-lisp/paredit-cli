# Error Codes

Every failure this tool can produce is classified into a stable, namespaced
error code — `<category>.<name>`, always lowercase kebab-case. A code is a
compatible-forever contract: a new one may be added, but an existing one is
never renamed or reused for a different meaning. Match on the code, not on
the message text next to it; the message is prose and may be reworded.

A code always carries:

- **category** — one of `argument`, `selection`, `input`, `refusal`,
  `environment`, `gate`, `internal`. Coarser than the code itself, so a
  consumer can decide "is this worth retrying" without enumerating every code
  a given build happens to ship.
- **retryable** — whether re-running the identical command line could
  plausibly succeed. Only `environment` failures are; a `selection` failure
  will not resolve differently on a second attempt with the same arguments.
- **exit code** — `3` for `gate.failed`, `1` for everything else. `2` is
  reserved for a command line `clap` itself rejected, before this
  classification ever runs.

Every command that failed under `--output json` reports this table's shape on
stderr:

```json
{
  "schema_version": 1,
  "status": "error",
  "command": "edit wrap",
  "error": {
    "code": "argument.target-required",
    "category": "argument",
    "category_description": "the command line does not describe a runnable request",
    "retryable": false,
    "exit_code": 1,
    "message": "target required: pass --path or --at",
    "repairs": [
      {
        "action": "inspect-first",
        "detail": "list the paths this document actually has, then select one of them",
        "command": "paredit inspect outline --output json --file src/core.lisp"
      }
    ],
    "offset": null,
    "doc_url": "https://nerima-lisp.github.io/paredit-cli/errors/#argument.target-required"
  }
}
```

`repairs` is what the classification believes would get past the failure —
each entry a plain-language `detail` and, when the failure carried enough
context, a `command` that can be run exactly as written. `offset` is the byte
position the failure named, when it named one (a parse failure always does; a
shape refusal like "cannot raise a top-level expression" does not, because
it is not about one place in the source). The human-readable rendering shows
the same offset as a caret under the source line, the way `rustc` does.

## Argument

The command line does not describe a runnable request. Retrying the same
invocation cannot help; the arguments have to change.

### `argument.no-input` { #argument.no-input }

No source was named. Pass `--file <path>`, or pipe source into stdin —
this tool refuses to wait on an interactive terminal.

### `argument.target-required` { #argument.target-required }

Neither `--path` nor `--at` (nor, on a command with the full selector
surface, any selector) was given.

### `argument.target-ambiguous` { #argument.target-ambiguous }

Both `--path` and `--at` were given; only one names the target.

### `argument.write-requires-file` { #argument.write-requires-file }

`--write` was passed without `--file`. `--write` updates the named file in
place, so there has to be one.

### `argument.conflicting-inputs` { #argument.conflicting-inputs }

More than one of `--since`, `--from-git`, `--from-manifest`,
`--from-archive`, `--paths-from` was given. Each replaces the whole file set,
so combining them would let one win silently.

### `argument.invalid-glob` { #argument.invalid-glob }

A glob pattern (for example on `--exclude`) is not well-formed.

### `argument.needs-repository` { #argument.needs-repository }

`--since <git-ref>` was used outside a git repository containing the scanned
roots.

### `argument.no-inputs-produced` { #argument.no-inputs-produced }

The input selector — `--from-manifest`, `--paths-from`, or `--from-archive` —
resolved to no file inside the scanned roots.

### `argument.archive-destination` { #argument.archive-destination }

`--from-archive` was given without `--extract-to <DIR>`.

### `argument.flag-combination` { #argument.flag-combination }

Two flags that do not combine were given together, or one that needs a
companion was given alone — `--insert before` without `--anchor-path`,
`--name` together with `--all-bindings`, `--from-file` equal to `--to-file`.

The message names the flags involved. Nothing about the file is at fault, and
re-running the same command line cannot help.

### `argument.kill-ring-index` { #argument.kill-ring-index }

`--index` named an entry the kill ring file does not have.

## Selection

The selection did not resolve. A different `--path`, `--at`, or selector
might; retrying the identical one will not.

### `selection.path-not-reachable` { #selection.path-not-reachable }

The dot-separated child-index path (or a navigation step: parent, sibling,
child) does not exist in this document's tree.

### `selection.path-invalid` { #selection.path-invalid }

The path string itself is not shaped like a path — it is not
dot-separated non-negative integers.

### `selection.offset-not-found` { #selection.offset-not-found }

No expression in the document contains the byte offset `--at` named.

### `selection.stale` { #selection.stale }

The selection was built from source text that does not match — or a tree
that is not — the one the operation is now being applied to.

### `selection.span-invalid` { #selection.span-invalid }

A byte span (start/end) does not fit the current source: the end exceeds
the input's length, the start exceeds the end, or the span does not land on
UTF-8 character boundaries.

### `selection.no-match` { #selection.no-match }

A `--query` pattern or other selector matched nothing.

### `selection.ambiguous` { #selection.ambiguous }

A selector matched more than one form, and the command needs exactly one.
Pass `--all` to act on every match, or narrow the selector.

### `selection.pattern-invalid` { #selection.pattern-invalid }

The `--query` pattern language itself does not parse — a malformed capture,
an unbalanced `...`, and the like.

### `selection.malformed` { #selection.malformed }

The selector text is not well-formed: an unknown prefix, a malformed
position, a malformed stable ID.

## Input

The input is not what the operation needs. The source itself has to change.

### `input.unparsable` { #input.unparsable }

The source failed to parse. The failure names the byte position — see the
`offset` field, or the caret excerpt in the human-readable rendering.
`paredit inspect check` reports every syntax problem it can find in one
pass, not only this first one — see its `errors` array.

### `input.not-utf8` { #input.not-utf8 }

The input bytes are not valid UTF-8. This tool reads UTF-8 only.

### `input.shape-refused` { #input.shape-refused }

The selected form is not the shape this operation works on — for example,
splitting something that is not inside a list, or joining two lists that use
different delimiters. Every structural refusal in `paredit_core_syntax`
answers this way: try a different selection, a parent, or a sibling.

### `input.symbol-invalid` { #input.symbol-invalid }

A symbol argument was empty, or contained whitespace or a reader delimiter.

### `input.dialect-unsupported` { #input.dialect-unsupported }

The command is not defined for this file's dialect — `refactor
merge-package-options` is Common Lisp only, several `refactor` edits are
Common Lisp and Emacs Lisp only.

Distinct from [`input.shape-refused`](#input.shape-refused) because the two
call for opposite actions. A shape refusal means "select a different form in
this file"; this one means the file is the wrong language for this command, so
no selection in it will help and it should not be retried here. Nothing about
the file needs fixing.

## Refusal

The tool declined for safety. The state — the file, the flags — has to
change before retrying helps.

### `refusal.input-too-large` { #refusal.input-too-large }

The input exceeds the configured size ceiling (`--max-input-bytes` or its
default).

### `refusal.write-target` { #refusal.write-target }

The write target is a symlink, not a regular file, has no file name, would
collide with another target in the same run, or sits under a writable parent
directory this tool refuses to trust.

### `refusal.target-changed` { #refusal.target-changed }

The file changed since it was read — replaced, hard-linked, or removed —
between parsing and writing. One code covers all six variants of this
time-of-check-to-time-of-use guard, because the response is the same: re-read
and retry.

### `refusal.rewrite-does-not-reparse` { #refusal.rewrite-does-not-reparse }

The rewritten document would not parse back as valid source. Nothing was
written; this is a defect in the transform, not in the input.

### `refusal.overlapping-spans` { #refusal.overlapping-spans }

Two edit spans in one rewrite plan overlap.

### `refusal.span-out-of-bounds` { #refusal.span-out-of-bounds }

An edit span lies outside the input, or is not aligned to a UTF-8 character
boundary.

### `refusal.workspace` { #refusal.workspace }

A workspace-discovery safety check declined — for example, a path outside
the scanned roots that a symlink would otherwise have reached.

### `refusal.dry-run` { #refusal.dry-run }

`--dry-run` (or `PAREDIT_DRY_RUN`) suppressed a write the command line asked
for.

### `refusal.match-shifted` { #refusal.match-shifted }

`--all` reached a match whose position an earlier edit in the same run had
already moved — slurp and barf can move a sibling across a form boundary.
Re-run without `--all`, or narrow the selector so the matches do not collide.

### `refusal.encoding-write` { #refusal.encoding-write }

`--encoding` declared a source encoding other than UTF-8, and the command
asked to write. Reading works: the file is decoded to the UTF-8 this tool
works in internally. Writing that back out would silently re-encode the file,
so it is refused rather than guessed at. Read the report, or convert the file
to UTF-8 first.

## Environment

The filesystem or the environment failed. This is the one category worth
retrying: nothing about the input caused it.

### `environment.io` { #environment.io }

An I/O operation failed: permissions, a missing file, a full disk.

### `environment.workspace-limit` { #environment.workspace-limit }

A workspace scan hit `--max-files`, `--max-total-bytes`, `--max-file-bytes`,
or `--max-depth`.

### `environment.unavailable` { #environment.unavailable }

A resource the operation needed — for example, a git repository for
`--since` — is not available in this environment.

### `environment.rollback-failed` { #environment.rollback-failed }

A write failed, and undoing it also failed. The working tree may be
partially written; check it before retrying.

### `environment.cleanup-after-commit` { #environment.cleanup-after-commit }

The writes landed successfully; only removing a backup file afterward
failed. Do not redo the operation — remove the leftover backup by hand.

### `environment.timeout` { #environment.timeout }

The wall-clock `--timeout-ms` budget ran out before the work did. Nothing is
wrong with the input: the same command with a larger budget, or none, would
succeed.

## Gate

A requested policy gate tripped after the report was printed. This is the
gate doing its job, not a malfunction — see the report above this message
for what tripped it.

### `gate.failed` { #gate.failed }

A `--fail-on-*` / `--require-*` flag's condition was met. Exit code `3`,
distinct from every other failure's `1`, so automation can tell "the report
found what it was told to fail on" apart from "the tool itself broke".

## Internal

Not classified. This is a defect in this tool's classification table, not
in the command line or the input — please report it with the command that
produced it.

### `internal.unclassified` { #internal.unclassified }

The failure did not match any known variant.
