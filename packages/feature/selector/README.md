# paredit-feature-selector

Selector resolution as a command: patterns, names, coordinates and stable ids.

## Responsibilities

One slice, `resolve_report`, backing `paredit inspect resolve`. It answers the
question every other selector-taking command asks internally — *which forms
does this selector name?* — and prints the answer instead of acting on it.

That makes it two things at once:

- **A debugger for selectors.** `--query` is a small language; getting a
  pattern wrong is normal, and a command that edits on a wrong pattern is
  expensive to undo. Resolving first is cheap.
- **The first half of a two-step edit.** `inspect resolve` prints a stable
  selector id per match; `--id` feeds one back to any editing command. The id
  survives the edits that invalidate a `--path`, which is what makes a
  multi-step refactor possible without re-deriving paths after every step.

## Boundaries

The selector *semantics* — the pattern language, the matcher, the line index,
stable ids — live in `paredit-core-syntax::selector`, not here. They have to:
`paredit-core-cli` flattens `SelectorArgs` into every command that takes a
target, and a core package cannot depend on a feature package. This crate is
the reporting surface over that engine and holds no matching logic of its own.
