# paredit-feature-lint-documentation

Lint rules for docstring and comment quality.

## Responsibilities

Four rules about what a file says about *itself* — its docstrings and its
comments — rather than about what it computes. §5.2.2 splits by subject matter,
so naming the rules is the only way to say why one belongs here.

| Rule | Flags |
| --- | --- |
| `docstring-example-stale-arity` | a worked example in a docstring calling its own `defun`/`defmacro` with an argument count the lambda list rejects |
| `docstring-summary-line-too-long` | a docstring whose first line — the one every doc generator shows on its own — is wider than a configurable limit |
| `todo-fixme-no-attribution` | a `TODO`/`FIXME`/`XXX`/`HACK`/`BUG` marker with no owner, ticket reference, or date |
| `missing-package-docstring` | a `defpackage` with no `(:documentation "…")` option and no comment describing the package |

## Two data sources

This package is unusual in reading from both of the places a Lisp file keeps
prose, and the split runs right through it:

| | Data source | Reached by |
| --- | --- | --- |
| `docstring-example-stale-arity` | node tree | the matched `ExpressionView` |
| `docstring-summary-line-too-long` | node tree | the matched `ExpressionView` |
| `missing-package-docstring` | node tree **and** `tree.comments()` | the matched view, then the comment list |
| `todo-fixme-no-attribution` | `tree.comments()` only | `RuleContext::tree()` |

A docstring is a node: a string literal sitting in a definition's body or in a
fixed slot. A comment is not — the parser keeps comments in a list *beside* the
tree, by design, so a rule that walks `ExpressionView` children cannot see one
at all. That single fact is why the last rule is `HeadFilter::WholeTree` and the
other three are `HeadFilter::Heads`: a comment has no head to filter on.

`crate::support` documents both access paths, along with the quote handling
every node-based rule here depends on.

## What this package deliberately does not do

**Docstring/parameter agreement** is not here. It already exists, in both
directions, in `paredit-feature-code-metrics`'s `docstring_report`
(`DocstringIssue::StaleParameter` and `DocstringIssue::UndocumentedParameter`),
including the single-letter guard that keeps prose "A" and "I" from reading as
stale parameter names. A second implementation would be a second heuristic to
keep in agreement with the first.

**Comment language consistency** and **comments that restate their code** are
not here either. Both were considered and dropped: neither can be scoped to a
set of cases worth being confident about without an English-morphology model
this codebase does not have, and a documentation rule that nags on good code
gets the whole category disabled. Each rule's module documentation states its
own limits in the same spirit — every one of them prefers a false negative.

## Autofix

Every rule is `Fixability::ReportOnly`. What a docstring *should* say is not
something a rewrite can infer, and comment edits are especially dangerous here:
this project has already shipped a write command that silently deleted every
comment in a file, because comments sit outside the node tree its equivalence
guard checked.

## Registration

The rules are not registered with the root `REGISTRY` by this package — the
registry lives outside every feature crate (§4.2), and a separate integration
pass wires it. Until then these rules compile, are unit-tested, and are
exercised through the real engine by `lib.rs`'s `engine_pass_tests`, but are
not reachable from the CLI.
