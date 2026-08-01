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
repeated shape already implies. A per-format schema (JSON-Lisp, EDN-style
maps, a project's own `defschema`) is deliberately out of scope here; see
`inspect data-check`'s own help for what ships today.

## Non-goals

- Validating a file against an external schema. That needs a schema
  language, which this package does not define.
- Anything that requires evaluating the data. This tool never evaluates the
  code — or the data — it inspects.
