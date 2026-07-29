# Selecting forms

Most `paredit edit` and several `paredit refactor` commands operate on one
selected expression. There are eight ways to name it. Pass exactly one *base*
selector; `--parent`, `--child`, `--sibling` and `--all` modify whichever one
you chose.

| selector | names a form by | exact | needs a prior lookup |
| --- | --- | --- | --- |
| `--path 0.2.1` | tree position | yes | usually |
| `--at 120` | byte offset | yes | usually |
| `--line-column 12:5` | editor coordinate | yes | no |
| `--name parse-header` | definition name | yes | no |
| `--query '(defun ?n ...)'` | S-expression pattern | no | no |
| `--id sel:672ed5e165259604` | stable content id | yes | once |
| `--from` / `--to` | a range of siblings | yes | — |
| `--select name:foo` | any of the above, in one flag | — | — |

Use `paredit inspect resolve` to see what any of them names before acting on
it.

## Tree paths: `--path`

A path is a dot-separated list of zero-based child indexes, starting at the
top level of the document. Given:

```lisp
(defun foo (x)      ; top-level form 0
  (+ x 1))
(defvar *limit* 10) ; top-level form 1
```

- `--path 0` selects the whole `defun`.
- `--path 0.0` selects the atom `defun`.
- `--path 0.2` selects the parameter list `(x)`.
- `--path 0.3` selects the body form `(+ x 1)`.
- `--path 1.2` selects `10`.

Paths count every child expression, including the head atom. Comments and
whitespace are not children, so paths stay stable under reformatting — but
**not** under insertion: adding a form above `0` renumbers everything below
it. For a selector that survives that, see `--id`.

## Byte offsets: `--at`

`--at <offset>` selects the smallest expression containing the given byte
offset. Use it when another tool — a grep hit, or a previous paredit report —
already gives you a byte position:

```sh
paredit edit select --file source.lisp --at 42
```

## Editor coordinates: `--line-column`

`--line-column LINE[:COLUMN]` selects the smallest expression at a 1-based
line and column. The column is optional and defaults to 1, so a compiler
warning that gives only a line is enough:

```sh
paredit edit select --file source.lisp --line-column 12:5
paredit inspect form  --file source.lisp --line-column 12 --output json
```

Columns count **characters**, not bytes: on a line reading `(λ x)`, `x` is at
column 4. A line or column past the end of the file is refused rather than
clamped — clamping would point at a form you did not name.

## Definition names: `--name`

`--name <symbol>` selects the definition of that name, wherever it sits —
including one nested inside `eval-when` or a `progn`:

```sh
paredit edit select --file source.lisp --name parse-header
paredit refactor extract-function --file source.lisp --name parse-header ...
```

In Common Lisp the comparison folds case and package qualifiers, because the
reader does: `--name parse-header` finds `PARSE-HEADER` and `demo:parse-header`.
Every other dialect is compared exactly, because every other dialect's reader
is case-sensitive.

Two definitions of one name is an ambiguity, not a pick-the-first: see
[`--all`](#acting-on-every-match-all).

## Patterns: `--query`

`--query` selects by shape. A pattern is written in the language it matches,
with three tokens given meaning:

| token | meaning |
| --- | --- |
| `_` | one form of any shape |
| `?name` | one form of any shape, bound to `name` |
| `...` | zero or more forms; at most one per list |

Everything else is a literal and matches itself.

```sh
# Every defun.
paredit inspect resolve --file source.lisp --query '(defun ?name ...)'

# Two-branch `if`s only -- arity is exact without a `...`.
paredit inspect resolve --file source.lisp --query '(if ?test ?then)'

# Self-comparisons: repeating a name constrains it to the same text.
paredit inspect resolve --file source.lisp --query '(eq ?x ?x)'

# A `...` may sit in the middle and anchor both ends.
paredit inspect resolve --file source.lisp --query '(defun ?name ... ?last)'
```

**Kinds.** A capture may be constrained: `?name:list`, `?name:atom`,
`?name:symbol`, `?name:keyword`, `?name:string`, `?name:number`, `?name:any`.
The anonymous wildcard takes the same suffix (`_:list`).

**Rest captures.** `?body...` binds the whole run of forms the `...` swallowed,
not just one.

**Selecting a capture.** `--capture <name>` selects the bound sub-form instead
of the whole match — which is how you point an edit at a definition's *name*
rather than the definition:

```sh
paredit edit select --file source.lisp --query '(defun ?name ...)' \
  --capture name --all
```

**Reader prefixes.** A prefix written in the pattern is required; one omitted
is not excluded. `#'?fn` matches `#'handler` and not `handler`; a bare `?fn`
matches both. Literal atoms and lists are strict in both directions, so `foo`
does not match `'foo` — a quoted symbol is data where a bare one is a
reference.

**Dialects.** The pattern is read with the *file's* reader, so strings,
character literals, bracket forms, and `#lang` rules behave the way the same
text behaves in a file. Common Lisp folds case and package qualifiers; the
other dialects do not.

## Stable ids: `--id`

`paredit inspect resolve` prints a 16-character id per match. Unlike a path,
it keeps naming the same form after edits elsewhere in the file:

```sh
$ paredit inspect resolve --file source.lisp --query '(format ...)' --output text
matches	1	selector	--query '(format ...)'
2.4	11:3-11:29	list	f2086c824be5806f	(format stream "~a" value)

$ paredit edit wrap --file source.lisp --id f2086c824be5806f --write
```

An id is derived from the enclosing definition, the form's own text with
whitespace collapsed, and its position among identical siblings. So it
survives insertions, deletions elsewhere, and reformatting — and deliberately
*does not* survive editing the form itself or renaming its enclosing
definition. Both of those change what the id names, and an id that silently
followed them would be worse than one that reports `no form carries selector
id …`, which you can recover from by resolving again.

Ids are accepted with or without a `sel:` prefix.

## Ranges: `--from` / `--to`

`--from` and `--to` select a contiguous run of siblings. Each takes a *compact
selector* — a whole selector in one value:

| spelling | means |
| --- | --- |
| `0.2` or `path:0.2` | a tree path |
| `at:120` | a byte offset |
| `line:12:5` | a coordinate |
| `name:parse-header` | a definition name |
| `sel:672ed…` or `id:672ed…` | a stable id |
| `query:(defun ?n ...)` | a pattern |

```sh
paredit edit select --file source.lisp \
  --from name:parse-header --to name:write-header
```

Both ends must sit in the same list, and `--from` must not come after `--to`.
Commands that act on one form refuse a multi-form range rather than editing
its first form.

## One flag for all of it: `--select`

Every selector-taking command also accepts `--select`, carrying the same
compact grammar as `--from`/`--to`:

```sh
paredit edit wrap --file source.lisp --select 'query:(defun ?n ...)'
paredit edit wrap --file source.lisp --select name:parse-header
```

For most commands this is a convenience. For a handful it is the *only* way
to reach the richer selectors, because their own flags already claim the
names: `refactor introduce-let --name` is the new binding's name,
`refactor rename-binding --from`/`--to` are symbols, and so on. Those
commands take `--path`, `--at`, and `--select`, and nothing else:

```sh
# --select picks the form; --name names the binding being introduced.
paredit refactor introduce-let --file source.lisp \
  --select 'query:(+ ?a ?b)' --name sum --output json
```

Affected commands: `refactor introduce-let`, `inline-let`,
`remove-unused-binding`, `thread-expression`, `unthread-expression`,
`unwrap-call`, `extract-function`, `extract-constant`. They act on one form,
so they do not take `--all` or the relative moves.

## Relative moves: `--parent`, `--child`, `--sibling`

Any base selector can be adjusted without recomputing it:

```sh
# The definition's name atom.
paredit edit select --file source.lisp --name parse-header --child 1

# The form after the one at line 6.
paredit edit select --file source.lisp --line-column 6:5 --sibling 1

# The enclosing form. Repeat --parent to climb further.
paredit edit select --file source.lisp --line-column 6:5 --parent --parent
```

They are applied in a fixed order — every `--parent`, then `--sibling`, then
`--child` — so "up, across, down" reads left to right regardless of how you
typed them. A move past the edge of the tree is refused and says which edge.

## Acting on every match: `--all`

A selector that names more than one form is refused by default:

```
--query '(cleanup ?x)' matches 3 forms; pass --all to act on every match,
or narrow the selector (see `paredit inspect resolve`)
```

That refusal is the point: silently editing the first of three matches is a
wrong edit rather than a failed one. `--all` turns it into a fan-out:

```sh
paredit edit kill --file source.lisp --query '(cleanup ?x)' --all --write
```

Matches are applied right to left and the document is re-parsed between them,
so an edit never invalidates a match still to come. If one nonetheless does —
`slurp-forward` swallowing the next match, say — the run stops with a refusal
instead of rewriting the wrong form.

`inspect resolve` never refuses an ambiguous selector: showing all the matches
is how you decide whether to narrow.

## Seeing what a selector names: `inspect resolve`

```sh
paredit inspect resolve --file source.lisp --query '(defun ?name ...)' --output json
```

Each match reports its path, byte span, start and end line/column, kind, head
symbol, stable id, a one-line preview, and every pattern capture with its own
path and text. `--output text` prints one tab-separated line per match, with
captures on indented continuation lines. `--fail-on-empty` makes "no match"
an exit code for scripts; by default it is an answer with `matchCount: 0`.

This is both a debugger for `--query` and the first half of a two-step edit:
resolve to get an id, then feed the id to the editing command.

## Getting paths and spans from reports

These commands print paths and byte spans for everything they report:

```sh
# Top-level forms with paths, spans, and definition hints.
paredit inspect outline --file source.lisp --output json

# One form with its local structure (children, paths, spans).
paredit inspect form --file source.lisp --path 0 --include-source --output json

# Exact atom occurrences with spans, ready for --at.
paredit inspect find-symbol --file source.lisp --symbol foo --output json

# Everything at once, for agent planning.
paredit inspect agent-report --file source.lisp
```

`inspect form` accepts the whole selector surface too, so
`inspect form --name parse-header` turns a name into a path in one call —
previously that took an `outline` pass first.

## Walking from a path you already have

`edit navigate` answers "which path is one step that way" without another
report. In text mode it prints the bare path, so it substitutes directly into
the next command:

```sh
# The sibling after --path 0.2.
paredit edit navigate --file source.lisp --path 0.2 --direction forward

# Compose it.
paredit edit select --file source.lisp \
  --path "$(paredit edit navigate --file source.lisp --path 0.2 --direction forward)"
```

`--direction forward|backward` move between siblings, `up` goes to the
enclosing expression, and `down` goes to the first child. Each is exactly one
step: at the end of a list, `forward` fails rather than moving out of it, so a
composed sequence never silently changes depth. `--output json` reports the
span, kind, and head of both ends of the move.

## Asking what is at an offset

`--at` selects the smallest *expression* containing an offset, which says
nothing about whether the offset is inside a string, a comment, or a
delimiter. `inspect context-at` answers that, and is what the character edits
(`edit delete-forward`, `edit delete-backward`, `edit newline`) check before
refusing:

```sh
paredit inspect context-at --file source.lisp --at 42 --output json
```

It reports the kind of text at the offset, whether a character edit there is
structurally inert, the innermost expression and enclosing list, the nesting
depth, and the stack of open delimiters. `--fail-on-structural` turns "not
inert" into exit code 3 for use as a gate.

## Files and stdin

Single-document commands read `--file` when given and stdin otherwise.
Dialect detection uses the file extension: `.lisp`/`.lsp`/`.cl`/`.asd`
(Common Lisp), `.el` (Emacs Lisp), `.lfe` (LFE), `.scm`/`.ss`/`.sld`/`.sls`/
`.sps` (Scheme), `.rkt`/`.rktl`/`.rktd` (Racket), `.clj`/`.cljc`/`.cljs`/
`.cljd`/`.edn`/`.bb` (Clojure), `.hy` (Hy), `.carp` (Carp), `.janet` (Janet),
and `.fnl` (Fennel). Pass `--dialect` explicitly for stdin input or unusual
extensions where the command accepts it. The dialect also decides how
`--query` and `--name` compare symbols, so it matters for more than parsing.

Report commands that take multiple files (`symbols`, `calls`, `signature`,
…) require explicit file arguments, while `workspace` and the
`refactor workspace-*` commands discover sources under directory roots.
