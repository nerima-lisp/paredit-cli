# paredit-feature-code-metrics

Eight reports that measure a codebase rather than judge one form in it.

## Responsibilities

Every other report in this tool answers a local question — is *this* form
malformed, does *this* symbol resolve. These answer questions whose subject is
the tree: where is the documentation missing, where is the work parked, which
files are long, which package is doing too many jobs, and which code changes
often *and* is complicated, which is where a refactor pays.

- **`docstrings`** — definitions with no docstring, and docstrings that name a
  parameter the lambda list does not have. The second is the one worth having:
  a stale docstring is worse than none, and only a mechanical check finds it.
- **`todo`** — `TODO`/`FIXME`/`XXX`/`HACK` markers, with the definition each
  one sits in. Comments are kept as trivia beside the tree, so this is the only
  report that reads them.
- **`line-metrics`** — line length, file length, and lines per definition,
  against thresholds a caller sets.
- **`indentation`** — deviation from the Emacs/SLIME convention, which is a
  different question from `format`: `format` states what *this* tool would
  print, and this states what an Emacs user's editor would.
- **`duplication-ratio`** — the aggregate of `duplicates`: what fraction of the
  tree is repeated structure, which is the number a decision gets made on.
- **`cohesion`** — per-package coupling and cohesion. A package whose
  definitions never call each other is a namespace, not a module.
- **`hotspots`** — git change frequency multiplied by complexity. Complexity
  alone ranks the code that is hard; churn alone ranks the code that moves;
  the product ranks the code where a refactor actually pays.
- **`debt-score`** — the above folded into one number per file, with the
  contribution of each input shown so the score can be argued with.

## Boundaries

Dialect-neutral. These measure shape, comments, and history, none of which is
Common Lisp specific — so unlike the semantic and reader reports, these answer
for every dialect this build parses.

`hotspots` is the one report that reads something outside the files it was
given. It shells out to `git log`, and when that fails — not a repository, no
`git` on `PATH`, a shallow clone — it says so in its output and reports
complexity alone, rather than reporting a zero that reads like "this file never
changes".
