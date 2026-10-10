# paredit-feature-project-inventory

Reports whose subject is the project, not a form in it.

## Responsibilities

- **`api-surface` / `api-diff`.** What a package promises, and how that promise
  changed. `api-surface` is a snapshot of the exported symbols with their
  signatures; `api-diff` compares two snapshots and classifies observed
  export and arity changes as major, minor, or patch. This classification
  does not establish compatibility of behavior outside the snapshots.
- **`test-map`.** Definitions paired with tests by name convention, not by
  executed coverage. Common Lisp pairs must have the same package identity;
  unknown package contexts remain file-local. Emacs Lisp names are
  case-sensitive. Ambiguous duplicate definitions remain unattributed.
- **`test-risk`.** Rank named functions without a `test-map` link by static
  complexity. This is a review queue, not executed coverage or a proof of
  safety. `--top` limits display, not the `--fail-on-untested` gate.
- **`dotfiles`.** Bounded metadata inventory of `.paredit/`, without reading
  file bodies or following symlinks. `--root` is required; JSON is the default.
  An incomplete inventory carries diagnostics and exits with code 3.
- **`symbol-index`.** Every symbol to its definition site, in one pass. Built
  for a consumer that will ask thousands of "where is this defined" questions
  and should not re-parse the tree for each one.
- **`keyword-arity`.** `inspect signature` compares positional counts.
  This understands `&optional`, `&rest`, and `&key`, so it can see that a call
  passing `:widht` to a function taking `:width` is wrong, which a positional
  count never can.
- **`unreachable-expressions`.** Dead code *within* a body: forms after a
  `return-from`, `go`, or `throw` in the same implicit progn. `reachability`
  reports reachability between definitions instead.
- **`external-systems` / `licenses` / `serial-consistency`.** The ASDF layer.
  External system names referenced by the project, declared `:license` or
  `:licence` metadata in the input files' top-level `defsystem` forms, and
  whether a `:serial t` claim matches the dependencies the files actually have.
  License reporting does not fetch or resolve external dependency systems.
- **`license-headers`.** Whether each file opens with a comment block that
  reads as a license notice, and, once every file in the analyzed set has been
  read, whether that text agrees with what the rest of the set carries.
  This reports file-header metadata, not the legal effect of repository
  licenses or whether a file may be redistributed.
- **`blame`.** Last author and date per definition, so any other report's
  finding can be routed to someone.

## Boundaries

The API, ASDF, and package reports are Common Lisp only and say so.
`symbol-index`, `test-map`, `unreachable-expressions`, `license-headers`, and
`blame` accept multiple dialects; their name and control-flow rules remain
dialect-specific. `dotfiles` inventories filesystem metadata, not Lisp forms.
`test-risk` models Common Lisp and Emacs Lisp only; unsupported dialects carry
unknown counts and make `analysis_complete` false.

`blame` shells out to `git log`, and degrades the same way `hotspots` does:
it reports that git could not answer rather than reporting an empty author.
