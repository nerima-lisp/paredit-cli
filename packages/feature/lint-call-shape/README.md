# paredit-feature-lint-call-shape

Lint rules about the *shape* of a call or a definition — how many things it
takes, how deeply it nests, and what it dispatches on — rather than about what
any single operator means.

Every rule here is a readability judgement, so every rule here is
`Fixability::ReportOnly`: there is no single correct rewrite for "this takes too
many parameters", and a rule that guessed one would be wrong more often than the
code it reported.

| rule | what it reports |
| --- | --- |
| `deeply-nested-anonymous-lambda` | three or more anonymous `lambda`s nested with no intervening named binding |
| `overly-long-parameter-list` | a definition whose *required* parameter count exceeds `max-required` |
| `stringly-typed-dispatch` | a `cond`/`if` chain of `(string= x "…")` branches on one subject, reading as an enum dispatch |
| `positional-argument-count-exceeds-readability` | a call inside a definition body passing more than `max-positional-literals` arguments, all of them literals |
| `nested-function-parameter-shadows-enclosing-parameter` | an `flet`/`labels`/nested-`defun` parameter reusing an enclosing function's parameter name |

## What is deliberately *not* here

Two rules were specified for this package and are not in it. Both need the same
thing — every call site of a definition, correlated with that definition's
signature — and neither can have it under `HeadFilter::Heads` without a
whole-file scan per matched definition, which is `O(definitions × file)`. See
[`crate`]'s module documentation for the full reasoning.

- `boolean-parameter-without-keyword`
- `multiple-return-values-ignored-by-convention`

## Cost

Every rule declares `HeadFilter::Heads`. Nothing here walks the document, and
nothing here is quadratic in the number of definitions:

- `overly-long-parameter-list` and `stringly-typed-dispatch` read only the
  matched node's own children.
- `positional-argument-count-exceeds-readability` walks the matched
  definition's own subtree, which the dispatcher has already materialized, and
  prunes at nested definitions — so a file costs one extra pre-order pass in
  total, not one per definition.
- `deeply-nested-anonymous-lambda` and
  `nested-function-parameter-shadows-enclosing-parameter` need ancestor
  context. They get it from a *bounded* descent through the one enclosing
  top-level form, located by binary search over `tree.root_children()` spans —
  never from `tree.root_view()`, which materializes the whole document.
