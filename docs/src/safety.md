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

## A rewrite that parses can still be wrong

Everything above rests on one guard: the result has to reparse. That guard is
complete for a transform whose shape was decided when it was written —
`convert-if-to-when` converts `if` to `when` and nothing else.

`paredit query replace` and `paredit migrate run` take the shape from the
caller, and three of their failure modes produce source that **parses
cleanly and is still wrong**. The reparse guard cannot see any of them, so
they are refused up front instead:

| skipped as | why | override |
| --- | --- | --- |
| `quoted` | The match is inside quoted data. `'(a (if x y nil) b)` is a *list literal* — it has the shape the pattern matches, and rewriting it changes the program's data rather than its code. | `--include-quoted` |
| `comment-loss` | A comment inside the match is carried by no capture the template uses, so the splice deletes it. Comments live outside the node tree, so the loss is invisible to everything downstream. | `--allow-comment-loss` |
| `overlapping` | An enclosing match was already rewritten; splicing into text it discarded would corrupt both. Run the command again to reach the nested one. | — |

All three are counted in every output format, **including when the count is
zero**. Read the `skipped` counts before the diff: a skip count that only
appears when it is non-zero reads as "this cannot happen" until the day it
does.

Neither command writes without `--write`. What is *not* guaranteed is that a
rewrite preserves meaning: `--query '(car ?x)' --rewrite '(cdr ?x)'` is a
valid instruction and both commands carry it out. The reviewable artifact is
the plan, which is why it prints by default.

`paredit fix apply` is the one writing command with no `--write` — it inherits
`inspect lint --fix`'s behaviour exactly, which is what makes the two
spellings produce the same bytes. Preview it with `--diff`, gate it with
`paredit fix check`, and refuse it with the global `--dry-run`.

## Refactor is explicit

Use `paredit refactor plan`, `paredit refactor preview`, and `paredit refactor verify` before `paredit refactor apply` when the workflow is available. These commands make planned changes and verification results visible before a write is requested.

## Names a rename cannot reach

Renaming here is syntactic: it rewrites the atoms whose text is the symbol.
A name that is *assembled* is not one of those atoms —

```lisp
(defmacro define-handler (name)
  `(defun ,(intern (format nil "HANDLE-~a" name)) (event) ...))

(funcall (intern "HANDLE-CLICK") x)
```

— and a rename reporting "2 occurrences renamed" while leaving both of those
behind would have said something false.

`refactor verify` reports them as a `macro-constructed-symbols` check, naming
the file and line of each site and distinguishing two cases: a string literal
that names the target (the rename will certainly miss it) from a computed name
(it may, and only running the code could say). The check does not attempt to
*follow* the construction — that would mean evaluating arbitrary Lisp at
analysis time — so it makes the gap visible rather than closing it.

It is a warning rather than an error. A construction site is not proof the
rename is wrong, and blocking every rename in a file that calls `intern` once
would make the check the first thing anyone turned off.

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

## Not analysing a file twice

`inspect lint --cache-dir <dir>` reuses the findings for any file whose bytes
have not changed. On the same 1065-file corpus, a warm run is about 4× faster
than a cold one and reports byte-identical output.

The cache is **content-addressed**. The key is a hash of everything that can
change the answer — the tool version, the analysis, the active rule set, the
rule settings, and the file's own bytes — so a hit means "this exact question
was asked and answered", never "a file with this name was seen before". Three
consequences follow:

- There is no invalidation logic, because there is nothing to invalidate. A
  stale entry is unreachable rather than wrong.
- No mtime, size, or inode is consulted. All three are proxies for content and
  all three lie: a fresh checkout, a `touch`, a filesystem without sub-second
  timestamps.
- Upgrading `paredit` cannot serve an answer computed by the old build, and a
  narrowed `--rule` selection cannot serve the full suite's answer.

A `--baseline` is a filter over the answer rather than part of the question, so
the *pre-baseline* findings are what get cached: changing a baseline does not
throw the analysis away.

A cache directory that cannot be opened is an error, not a silent fallback. A
run that quietly did not use the cache it was asked to use looks identical to
one that did, only slower.

## Workspace scope

For workspace operations, start with `paredit inspect workspace` to identify the affected files. Use the workspace planning and preview commands before `paredit refactor workspace-execute`.

## Automation guidance

1. Discover with `paredit inspect`.
2. Review an `edit` result (`--diff` or stdout) before passing `--write`.
3. Plan, preview, and verify a `refactor` before applying it.
4. Treat non-zero exits and validation failures as blockers.

See the [agent interface](agents.md) for exit codes, the JSON output
contract, and a complete safe editing loop.
