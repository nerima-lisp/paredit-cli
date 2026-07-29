# paredit-feature-structural-diff

Comparing two S-expression documents by their parse, and carrying a difference
from one pair of files onto a third.

## Responsibilities

Two slices, one built on the other.

- **`structural_diff`** — `inspect diff`. Aligns two parsed documents and
  reports what changed in terms of *forms*: which one was inserted, which was
  deleted, which was replaced, and at what path. Whitespace, indentation, and
  comments are not part of the comparison, because they are not part of the
  tree.

- **`structural_patch`** — `refactor patch`. Takes the difference between two
  versions of one file and applies it to a *different* file, matching each
  change's "before" side by structure rather than by position. This is how a
  fix made in one place is carried to the other places that have it.

## Why this exists

A text diff of Lisp answers a question nobody asked. Re-indenting a `let`
rewrites every line beneath it. Adding one argument reports the whole wrapped
line. Moving a definition shows as a deletion and an unrelated insertion. Each
of those costs a reviewer attention on something that is not a change to the
program.

Comparing the parse instead makes the report narrow: an edited argument reports
as that argument, at its path, and a reformatted file reports as nothing at
all.

## The limit, stated

The comparison is blind to comments and whitespace. That is the point, and it
is also the boundary: a comment left contradicting the code above it is
invisible here. `inspect diff` says so in its own summary rather than leaving a
caller to infer it, and this is not a diff to review a change *only* through.

`refactor patch` inherits the same blindness, plus one of its own: an insertion
has no "before" side to match against, so there is no structural anchor for
where it belongs in a third file. Those changes are reported as unportable
rather than guessed at.
