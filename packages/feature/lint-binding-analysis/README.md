# paredit-feature-lint-binding-analysis

Lint rules whose subject is a *local binding* and which decide by resolving
references through the semantic binding table
(`RuleContext::binding_table()`), not by matching syntax.

Almost every other rule in the catalogue is syntactic: it matches a head, looks
at a few children, and reports. Those rules cannot answer "is this name ever
read", because that question is about the whole scope below the binder and
about which of several same-named bindings a reference resolves to. This
package is the set of rules that need the real answer.

Scope: Common Lisp only. `build_binding_table` analyses Common Lisp, Emacs
Lisp, Scheme and Racket and returns an *empty* table for everything else, and
the rules here additionally encode CLHS-specific facts (`&aux`,
`declare ignore`, `symbol-macrolet`), so Common Lisp is the only dialect they
are sound for.
