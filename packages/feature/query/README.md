# paredit-feature-query

The query namespace: workspace-wide pattern search, tallies, and structural
search-and-replace.

## Why it is its own namespace

`--query` already existed, as a *selector*: one of eight ways to name the form
a single-file command should act on. That framing put a ceiling on it. A
pattern language is a search language, and a search language wants a whole
repository, a count, and a rewrite — none of which fit inside "which form does
this flag select".

So `query` is not a re-slicing of `inspect`; it is the capability `--query` was
too small a hole to get out of:

| command | question |
| --- | --- |
| `query find` | which forms in this *repository* have this shape? |
| `query count` | how many, per file and per pattern, for these patterns side by side? |
| `query replace` | and what should they become? |

`query replace` is the one with no predecessor anywhere in the tool. Every
other write command in `paredit` knows the shape of its edit at compile time —
`convert-if-to-when` converts `if` to `when` and nothing else. `query replace`
takes the shape from the caller, which is what makes a codemod a thing a user
can write rather than a thing this repository has to ship.

## Boundaries

The matching and rewriting engines are **not** here. They are
`paredit_core_syntax::selector::{pattern, matcher, rewrite}`, because
`paredit-core-cli` flattens `SelectorArgs` into every command that takes a
target and a core package cannot depend on a feature package. `migrate` builds
its recipes on the same core module for the same reason.

What lives here is the three commands' argument surfaces, their workspace
iteration, and their rendering. No matching logic, and no rewriting logic.

## Safety

`query replace` writes only under `--write`, and refuses two situations that
would otherwise produce source that still parses and is still wrong:

- a match nested inside another match it already rewrote, and
- a match whose rewrite would delete a comment no capture carries over.

Both are counted and reported rather than dropped, so "37 matched, 35
rewritten" never has to be discovered by reading a diff.
