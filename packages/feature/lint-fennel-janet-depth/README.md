# paredit-feature-lint-fennel-janet-depth

Five lint rules for Fennel and Janet, the two dialects with the thinnest
coverage in the catalogue. Every rule here is transcribed from the language's
own tooling rather than invented:

| Rule | Source |
| --- | --- |
| `fennel-bad-unpack` | `fennel-ls`'s `bad-unpack` |
| `fennel-nested-associative-operator` | `fennel-ls`'s `nested-associative-operator` |
| `fennel-redundant-do` | `fennel-ls`'s `redundant-do` |
| `janet-dead-branch-on-constant-condition` | Janet's `janetc_throwaway` compiler lint |
| `janet-unreachable-match-clause` | Janet's documented `match` semantics |

`fennel-ls` is the linter Fennel's own `src/linter.fnl` plugin points at ("if
you want a real linter, use fennel-ls"); Janet's checks are `janetc_lintf` call
sites in `src/core/compile.c` and `maclintf` calls in `src/boot/boot.janet`.

## Three places these rules say less than their source

Each was established by running `fennel 1.6.1` or `janet 1.41.2`, not by
reading the implementation. Each is recorded in the rule's own module docs with
the transcript.

1. **`fennel-redundant-do` covers 13 heads, not `fennel-ls`'s 23.** That lint
   uses every form `(fennel.syntax)` marks `body-form?`, but nine of those
   accept exactly one body expression. `icollect` answers *"expected exactly
   one body expression. Wrap multiple expressions in do"* — so the `do` the
   lint calls redundant is the one thing making the form compile.

2. **`fennel-nested-associative-operator` covers 5 operators, not 7.** `+` and
   `*` are excluded because IEEE-754 addition and multiplication are not
   associative: `(* 1e300 (* 1e300 1e-300))` is `1e+300` and the collapsed
   `(* 1e300 1e300 1e-300)` is `inf`.

3. **`fennel-bad-unpack` exempts five one-argument operators.** Fennel compiles
   `(.. (table.unpack xs))`, and the unary `and`, `or`, `%` and `^`, down to
   nothing, so the multiple values pass through intact and there is no defect.
   `fennel-ls`'s documentation example for this lint is incorrect on 1.6.1 for
   exactly this reason.

## Constraints these two dialects put on any rule here

- **`head_key` returns the head verbatim** for every non-Common-Lisp dialect
  (`head_index.rs`), so `NormalizedHead` entries must match the source byte for
  byte. There is no case folding to rescue a typo, and `λ` is a different head
  from `lambda`.
- **`NormalizedHead::new` rejects `:`**, so a rule cannot anchor on Fennel's
  `:` method-call special at all. `fennel-ls`'s `unnecessary-method` lint is
  therefore not expressible as a `HeadFilter::Heads` rule and is absent here.
- **`binding_table()`, `value_table()` and `type_table()` are empty** for both
  dialects. Any rule needing to know what a name refers to cannot be written
  soundly, which rules out `fennel-ls`'s `unused-definition`, `unknown-module-field`,
  `not-enough-arguments`, `too-many-arguments` and `match-should-case`, and
  Janet's "binding is unused" / "binding is shadowing" compiler lints.
- **`RuleContext::scratch_cache` is not used** by anything here. It is a single
  slot already owned by `lint-repl-debug`.

## `support.rs` should move into core

`support.rs` is copied from `paredit-feature-lint-fennel-janet-idiom`, which in
turn copied its `QuoteState` from
`paredit-feature-lint-condition-system::support`. The two-counter quote model —
a `hard` flag that never clears for `'` and a `quasi` depth that `,` decrements
for `` ` ``/`~` — is not specific to any of the three packages. A single `i32`
depth counter cannot represent both states and produces false positives.

The other reason the file exists is that three of the shared one-liners in
`paredit_core_syntax::view_query` are actively wrong for these dialects:
`symbol_is`/`symbol_in`/`unqualified` case-fold and strip a package qualifier,
which is Common Lisp reader behaviour, whereas Fennel and Janet are
case-sensitive and use `:` for method multi-syms and keywords respectively.
