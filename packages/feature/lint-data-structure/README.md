# paredit-feature-lint-data-structure

Lint rules for Common Lisp's aggregate data structures — `defstruct`, hash
tables, and arrays. Six rules, all Common Lisp only.

| Rule | Category | Severity | Heads |
| --- | --- | --- | --- |
| `defstruct-boa-aux-uninitialized-slot` | `suspicious` | warning | `defstruct` |
| `defstruct-include-type-mismatch` | `malformed` | error | `defstruct` |
| `hash-table-literal-string-key-under-eql` | `suspicious` | error | `gethash`, `remhash` |
| `make-array-conflicting-initializers` | `malformed` | error | `make-array` |
| `maphash-mutates-other-entry` | `suspicious` | warning | `maphash` |
| `vector-push-without-fill-pointer` | `malformed` | error | `vector-push`, `vector-push-extend` |

Every rule declares `HeadFilter::Heads`; none uses `WholeTree`, and
`lib.rs`'s `engine_dispatch` tests assert both that fact and that each rule's
declared heads actually reach its own `examine`. A file spelling none of the
seven anchor heads above reaches no rule here at all.

## Status

This package is deliberately **unregistered**. A separate pass wires it into
`src/lint/registry`. None of the six rules is `Fixability::Fixable`, so the
`fixable_rules_match_the_fix_engine` contract in
`src/presentation/cli/lint_report/workflow.rs` needs no fixture for them.

## What each rule is careful about

Three of the eight rules originally proposed for this package were built on
premises that turned out to be **false**, and each rule's `domain` module
records the SBCL run that refuted it:

- Omitting a slot from a BOA constructor does *not* skip its `:initform` —
  CLHS says the initform still runs, and SBCL agrees. What actually leaves a
  slot uninitialized is a bare `&aux` variable, which is what
  `defstruct-boa-aux-uninitialized-slot` reports.
- `:type list` with `:include` is *not* constrained — the pair works. What
  CLHS constrains is that the two representations must *agree*, which is what
  `defstruct-include-type-mismatch` reports.
- `vector-push-extend` does *not* require `:adjustable t` — a vector with only
  `:fill-pointer 0` accepts it and reports `adjustable-array-p` true. The fill
  pointer is the requirement, which is what
  `vector-push-without-fill-pointer` reports.

`maphash-mutates-other-entry` is scoped by the same discipline: CLHS 18.2
*explicitly permits* `setf` of `gethash` on the current entry's value and
`remhash` of that same entry, so the rule reports only keys that are not the
lambda's key parameter. The naive form of that rule fires on the single most
common correct idiom there is.

## Cost

Every rule performs its cheap, local, allocation-free domain check before
touching anything that descends from the tree root or builds a semantic table.
The two rules that resolve a variable to its constructor take the binding table
as a **closure**, not a value, so a `gethash` on a symbol key never pays for a
whole-file semantic build. That ordering is worth roughly two orders of
magnitude and is documented at each call site, because getting it wrong is
invisible in every functional test.
