# Selecting forms

Most `paredit edit` and several `paredit refactor` commands operate on one
selected expression. There are two selectors, and every report command that
prints locations emits values you can pass straight back in.

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
whitespace are not children, so paths stay stable under reformatting.

Use `--path` when scripting deterministic edits: the same document always
yields the same path.

## Byte offsets: `--at`

`--at <offset>` selects the smallest expression containing the given byte
offset. Use it when another tool — a grep hit, a compiler message column, or
a previous paredit report — already gives you a byte position:

```sh
paredit edit select --file source.lisp --at 42
```

`--path` and `--at` are mutually exclusive; pass exactly one.

## Getting paths and spans from reports

You never need to count parentheses by hand. These commands print paths and
byte spans for everything they report:

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

A typical loop: run `outline` to find the top-level form, run `form` on that
path to see its children, then pass the child path to the edit or refactor
command.

## Files and stdin

Single-document commands read `--file` when given and stdin otherwise.
Dialect detection uses the file extension: `.lisp`/`.lsp`/`.cl`/`.asd`
(Common Lisp), `.el` (Emacs Lisp), `.lfe` (LFE), `.scm`/`.ss`/`.sld`/`.sls`/
`.sps` (Scheme), `.rkt`/`.rktl`/`.rktd` (Racket), `.clj`/`.cljc`/`.cljs`/
`.cljd`/`.edn`/`.bb` (Clojure), `.hy` (Hy), `.carp` (Carp), `.janet` (Janet),
and `.fnl` (Fennel). Pass `--dialect` explicitly for stdin input or unusual
extensions where the command accepts it.

Report commands that take multiple files (`symbols`, `calls`, `signature`,
…) require explicit file arguments, while `workspace` and the
`refactor workspace-*` commands discover sources under directory roots.
