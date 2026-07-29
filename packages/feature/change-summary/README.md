# paredit-feature-change-summary

Describing what an edit changed, in prose a pull request can use.

## Responsibilities

A unified diff says which bytes moved. It does not say *what happened*, and
"what happened" is what a reviewer reads and what a pull request description
has to contain. This package compares two versions of a source file
structurally and answers in those terms:

- **Which definitions were added, removed, or modified**, matched by identity
  rather than by position, so inserting one definition does not report every
  definition below it as changed.
- **Which were renamed.** A removal and an addition whose bodies are identical
  once the name is set aside is a rename, and calling it "one definition
  removed, one added" loses the only fact a reviewer wants.
- **Which merely moved**, with the old and new paths, for a caller holding
  stored `--path` selectors.
- **Whether anything changed at all beyond formatting**, which is the
  difference between a review that needs attention and one that does not.

The output is both prose and the structured facts the prose was rendered from,
so a caller can use the sentence or compute its own.

### What this package does not own

- **No diffing of bytes.** `paredit-core-cli` renders unified diffs; this
  answers a different question and deliberately does not repeat that one.
- **No judgement.** It reports that a definition's body changed, never whether
  the change was good. Nothing here reads as approval.
- **No guessing at intent.** A rename is inferred only from an exact body
  match; anything less certain is reported as the addition and removal it
  literally is, because a confidently wrong summary in a pull request costs
  more than a vague one.
