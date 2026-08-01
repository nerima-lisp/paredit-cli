# paredit-feature-project-inventory

Eleven reports whose subject is the project, not a form in it.

## Responsibilities

- **`api-surface` / `api-diff`.** What a package promises, and how that promise
  changed. `api-surface` is a snapshot of the exported symbols with their
  signatures; `api-diff` compares two snapshots and answers the only question
  that matters at release time — is this major, minor, or patch. Removing an
  export or narrowing an arity is breaking; adding either is not.
- **`test-map`.** Which definitions have a test and which do not, paired by
  name convention. The pairing is the report: a list of tests and a list of
  definitions are each easy to get and neither answers the question.
- **`symbol-index`.** Every symbol to its definition site, in one pass. Built
  for a consumer that will ask thousands of "where is this defined" questions
  and should not re-parse the tree for each one.
- **`keyword-arity`.** `inspect signature` compares positional counts.
  This understands `&optional`, `&rest`, and `&key` — so it can see that a call
  passing `:widht` to a function taking `:width` is wrong, which a positional
  count never can.
- **`unreachable-expressions`.** Dead code *within* a body: forms after a
  `return-from`, `go`, or `throw` in the same implicit progn. `reachability`
  answers this between definitions; nothing answered it inside one.
- **`external-systems` / `licenses` / `serial-consistency`.** The ASDF layer.
  What this project depends on that it does not define (an SBOM, in effect),
  what licences those carry, and whether a `:serial t` claim matches the
  dependencies the files actually have.
- **`license-headers`.** Whether each file opens with a comment block that
  reads as a license notice, and, once every file in the analyzed set has been
  read, whether that text agrees with what the rest of the set carries. A
  `LICENSE` at the repository root says nothing about a file copied out of the
  tree or read in isolation — the header has to live in the file itself.
- **`blame`.** Last author and date per definition, so any other report's
  finding can be routed to someone.

## Boundaries

The API, ASDF, and package reports are Common Lisp only and say so.
`symbol-index`, `test-map`, `unreachable-expressions`, `license-headers`, and
`blame` are dialect-neutral: they read definition shapes, control operators,
comment syntax, and git, none of which is Common Lisp specific.

`blame` shells out to `git log`, and degrades the same way `hotspots` does —
it reports that git could not answer rather than reporting an empty author.
