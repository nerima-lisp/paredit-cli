# paredit-feature-data-report

Structural sanity checks for S-expression *data* files, with no schema
required.

## Responsibilities

A `.lisp`/`.edn`/`.clj`-style file is often not code at all — it is
configuration or a data table read back with `read`. Nothing else in this
tool looks at that kind of file: every other report assumes the forms it
walks are code, and reads them for operators, definitions, and call sites
that a data file simply does not have.

`data-check` is the first report built for the data case instead. It runs a
handful of shape-only checks that need no format description:

- A list that looks like a plist (alternating keyword and value) or an alist
  (a list of `(key . value)` or `(key value)` pairs) with the same key
  spelled twice, where the later value silently wins over the earlier one.
- A plist with a trailing keyword and no value to go with it.
- A top-level list of same-shaped tuples where one entry's arity does not
  match its siblings'.

None of this needs to know what the data *means* — only what its own
repeated shape already implies, and it runs for every file regardless of
which format below also applies.

On top of that, `--format` (auto-detected from a file's path and content
when omitted) turns on a handful of convention-specific checks: Emacs
`custom-set-variables` entry shape, EDN's ban on code-only Clojure reader
macros, `.dir-locals.el`'s alist-of-alist shape (and its `eval` key, flagged
for presence only — judging risk is a later phase's job), and routing
`.rktd` Racket data files into this report at all — `#lang` alone cannot mark
a Racket file as data, since every named language (`typed/racket` included)
is still executable code. `.paredit/rules`/`.paredit/migrations` are
deliberately not a format here: `inspect check --paredit-config` already
validates them (syntax, `RulesetError`s, and cross-file collision/dangling-
`deftest` checks a shape-only pass here could not add to). A per-format
*schema* (JSON-Lisp, EDN-style maps, a project's
own `defschema`) is still out of scope here; see `inspect data-check`'s own
help for what ships today.

## Non-goals

- Validating a file against an external schema. That needs a schema
  language, which this package does not define.
- Anything that requires evaluating the data. This tool never evaluates the
  code — or the data — it inspects.
