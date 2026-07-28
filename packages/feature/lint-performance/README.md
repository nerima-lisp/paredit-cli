# paredit-feature-lint-performance

Lint rules whose subject is *cost*, not correctness: idioms that compute the
right answer by a route asymptotically worse than the obvious one, and
allocations nothing reads.

Two categories live here because they answer the same question — "what does
this cost that it need not?" — from the two directions a Lisp program pays:
traversal (`performance`) and consing (`allocation`).

Nothing here reports a bug. Every rule in this package flags code that works;
the finding is that it works more slowly than the alternative next to it in the
message. That is why they are warnings, and why the two whose rewrite is a
provable equivalence (`unnecessary-copy`, `copy-before-destructive`) are the
only fixable ones: the rest need a judgement about the data that no rule has.
