# paredit-feature-lint-custom

Lint rules a project writes for itself, in Lisp, without recompiling anything.

The shipped suite is 169 rules and every one of them took a Rust module, a
registry line, and a pinned count. That is the right cost for a rule everybody
gets; it is the wrong cost for "in *this* codebase, `defentity` must always be
given a `:table`". Rules of that second kind are the majority of what a mature
project wants and none of what a linter can ship.

So a project writes them in `.paredit/rules/*.lisp`, in the language it is
already writing:

```lisp
(defrule entity-needs-table
  :category malformed
  :severity error
  :description "a defentity with no :table option"
  :dialects (common-lisp)
  :pattern (defentity ?name ...)
  :message "defentity needs a :table")

(deftest entity-needs-table
  (:no-match "(defentity user :table \"users\")")
  (:matches  "(defentity user)"))

(deprecate legacy-connect :use connect :reason "removed in 3.0")

(defpattern bare-print (print ?x))
(defrule no-print-in-handler
  :pattern (handler-case (:fragment bare-print) ...)
  :message "do not print from inside a handler")
```

`:dialects` is optional and, like `defmigration`'s own clause, a guard rather
than a hint: naming dialects skips every file outside them entirely, rather
than matching them and finding nothing. `defpattern` registers a named
pattern fragment a `:pattern` can reference with `(:fragment name)`,
resolved (and cycle-checked) once, at load time.

## Why a separate pass

`RuleCatalog` holds `&'static [RuleEntry]`, because the shipped catalogue is
derived at compile time and that derivation is what keeps `RULES`,
`RULE_DOCS`, `FIXABLE_RULES`, and `WARNING_RULES` from drifting apart. A rule
read from a file at startup cannot join that array without giving up the
property.

So custom rules run as their own pass over the same tree, and their findings
are merged into the report. The two passes share the finding type, the
suppression mechanism, the severity gate, and every output format; they share
nothing else, and neither can weaken the other.

## Why patterns rather than predicates

A pattern is checkable. `deftest` can assert that a rule matches one string and
not another, which makes a rule file something a project can keep correct as it
changes. An arbitrary predicate could express more and could be tested for
much less.
