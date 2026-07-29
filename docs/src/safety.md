# Safety reference

`paredit-cli` keeps inspection, source edits, and semantic refactorings separate so automated clients can choose an appropriate review path.

## Inspect is read-only

All `paredit inspect` commands report information without modifying source files. Prefer these commands for discovery, impact analysis, and preflight checks.

## Edit previews before it writes

`paredit edit` commands return transformed source on standard output by default and never touch the file. Preview the change as a diff, then apply it in place with `--write`:

```sh
paredit edit format --file source.lisp --diff
paredit edit format --file source.lisp --write
```

`--write` refuses to persist a result that no longer parses, and writes are staged with automatic rollback, so a failed write cannot leave a truncated or unbalanced file behind.

## Refactor is explicit

Use `paredit refactor plan`, `paredit refactor preview`, and `paredit refactor verify` before `paredit refactor apply` when the workflow is available. These commands make planned changes and verification results visible before a write is requested.

## A multi-file write is one transaction

`refactor apply --write` stages every file it will change, then publishes them
all. If any file fails at either step, the files already published are restored
from their backups and the staged copies are removed. There is no state in
which some of a refactor's files are rewritten and the rest are not — the
alternative, a loop that writes each file as it goes, would leave exactly that
on the first permission error.

The guarantee covers four ways a batch can die, each of which has a test:

- a rewritten file that would no longer parse (checked before anything is
  staged, so nothing is written at all);
- a target that cannot be staged — a symlink, a non-regular file;
- a target that changes underneath the writer between staging and publishing;
- the same file named twice, which has no well-defined result and is refused.

## Undoing a write

`refactor apply --write --undo-out <path>` records a journal of *reverse edits*
— the text each edit replaced, in the coordinates of the file that was
produced. `refactor undo --journal <path> --write` puts it back:

```sh
paredit refactor apply --manifest preview.json --write --undo-out .paredit/undo.json
paredit refactor undo --journal .paredit/undo.json          # report only
paredit refactor undo --journal .paredit/undo.json --write  # restore
```

Both ends are hash-guarded. An undo refuses unless every file is byte-for-byte
what the write produced, and refuses again if the restored text is not
byte-for-byte what the write replaced. A journal therefore cannot be applied
twice, and cannot silently discard an edit somebody made after the refactor.

This complements version control rather than replacing it: a refactor applied
on top of uncommitted work cannot be reverted with `git checkout` without
taking the uncommitted work with it.

## Letting your own checks decide

`--verify-command` runs a command after the write and restores every written
file when it exits non-zero:

```sh
paredit refactor apply --manifest preview.json --write \
  --verify-command 'make test' --verify-timeout-ms 600000
```

The command runs through the platform shell, in the current directory, with
this process's environment. Its output is echoed to standard error on failure.
A command that exceeds `--verify-timeout-ms` is killed and treated as a
failure: a check that did not finish is not a check that passed.

This is also the supported route to stronger, implementation-specific
verification — running a property suite, comparing behaviour before and after —
without tying the tool itself to one Lisp implementation. A check that loads
the system and compares results before and after a refactor is a
`--verify-command`; it is deliberately not a built-in, because a built-in one
would make every CI run depend on a particular implementation being installed
and on the code being safe to evaluate.

## Asking the implementation

`inspect external-diagnostics` compiles each file with a real Common Lisp and
reports its own diagnostics, placed at the definition it named:

```sh
paredit inspect external-diagnostics --implementation sbcl --save-baseline before.json src/
# ... apply the refactor ...
paredit inspect external-diagnostics --implementation sbcl \
  --baseline before.json --fail-on-introduced src/
```

SBCL's compiler covers exactly the class of mistake a syntactic refactor can
introduce — an undefined variable a rename missed, an arity a signature change
broke, a `defmethod` with no matching generic — and comparing the diagnostic
sets before and after is a stronger safety argument than any analysis in this
tool can make alone. A diagnostic that was already there is not evidence
against the refactor, which is why the baseline is a first-class input rather
than something the caller diffs.

**Compiling is executing.** `compile-file` runs the file's macros, its
`eval-when (:compile-toplevel)` forms, and its `#.` read-time evaluation.
Pointing this command at code is the same act as running it, which is why
`--implementation` has no default. The compilation output goes to a temporary
directory, so the source tree is untouched.

An implementation that fails in a way this tool cannot read as diagnostics — a
missing binary, an exhausted heap — is an error, never an empty report. A
caller gating on this command must not read a check that did not run as a check
that passed.

## Knowing the blast radius first

`refactor apply` reports a `write_scope` in both output formats, in the dry run
as well as the write:

```json
"write_scope": {
  "confined": true,
  "root": "/repo",
  "target_count": 2,
  "targets": ["/repo/src/core.lisp", "/repo/src/util.lisp"],
  "unchanged_count": 5,
  "escaping_paths": []
}
```

`--root` confines every path to one directory through a capability handle, so a
manifest naming `../../etc/hosts` is refused rather than followed.
`escaping_paths` re-derives that claim from the resolved paths instead of
restating it, so a disclosure that contradicts itself is visible rather than
printed.

## Bounding a run

Every command accepts the same budget flags, and each may lower a built-in
ceiling but never raise it:

| Flag | Environment variable | Bounds |
| --- | --- | --- |
| `--timeout-ms` | — | Wall-clock budget, checked between files and during a lint walk |
| `--max-input-bytes` | `PAREDIT_MAX_INPUT_BYTES` | One document read |
| `--max-file-bytes` | `PAREDIT_MAX_FILE_BYTES` | One file found by a directory scan |
| `--max-total-bytes` | `PAREDIT_MAX_TOTAL_BYTES` | Bytes one scan reads in total |
| `--max-files` | `PAREDIT_MAX_FILES` | Files one scan may yield |

Unset, they behave exactly as before they existed. A timeout names the file
that was in flight and how many were already done, so a bounded run reports
progress rather than only failure.

## Parallelism does not change the answer

`--jobs` controls how many workers a multi-file analysis uses: `0` (the
default) uses every core, `1` is fully serial. On 1065 real Common Lisp files
`inspect lint` runs about 5× faster on 16 cores than on one, and the report is
**byte-identical** at every worker count — as are `--sarif` and `--github`.

That is a property of the design rather than a happy accident. Per-file results
are written into pre-indexed slots and read back in input order, so the output
cannot depend on which worker finished first; and the first failure by *input*
order is the one reported, so a tree with two broken files names the same one
on every run. A test asserts both at four different worker counts.

Two paths stay serial on purpose. `--fix` writes files, and `--timings`
measures per-rule cost, which sixteen workers contending for memory bandwidth
would turn into a measurement of the machine.

## Workspace scope

For workspace operations, start with `paredit inspect workspace` to identify the affected files. Use the workspace planning and preview commands before `paredit refactor workspace-execute`.

## Automation guidance

1. Discover with `paredit inspect`.
2. Review an `edit` result (`--diff` or stdout) before passing `--write`.
3. Plan, preview, and verify a `refactor` before applying it.
4. Treat non-zero exits and validation failures as blockers.

See the [agent interface](agents.md) for exit codes, the JSON output
contract, and a complete safe editing loop.
