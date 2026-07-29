#!/usr/bin/env bash
# Measure how much of this codebase's behaviour the tests actually pin.
#
#   ./scripts/mutants.sh                              # the default scope
#   ./scripts/mutants.sh packages/core/syntax         # one package
#   ./scripts/mutants.sh --list                       # what would be tried
#
# ## What this answers that coverage does not
#
# Line coverage says a line ran. It does not say that changing the line would
# have failed anything, and in a tool of this shape that distinction is the
# whole question: a rule with a fixture that exercises it and asserts only
# "does not crash" is fully covered and pins nothing.
#
# cargo-mutants changes one thing — a comparison operator, a returned constant,
# a boolean — and re-runs the tests. A mutant that *survives* is a statement
# the tests never make. Surviving mutants are the output; the count is not a
# score to optimise but a list to read.
#
# ## Why it is not in CI
#
# A full run compiles and tests the workspace once per mutant, which is hours.
# It belongs in a scheduled job or a maintainer's afternoon, not in the gate
# every pull request waits on. `--in-diff` narrows it to a change set when a
# targeted run is wanted.

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

if ! command -v cargo-mutants >/dev/null 2>&1; then
  cat <<'MESSAGE' >&2
mutants: cargo-mutants is not installed.

  cargo install cargo-mutants

It is deliberately not a workspace dependency: it is a development tool that
rebuilds the workspace per mutant, and pinning it here would put it in every
contributor's dependency graph for a job almost nobody runs.
MESSAGE
  exit 127
fi

# Default scope: the analysis core. These are the packages where a surviving
# mutant means an *analysis* is unpinned, which is the interesting kind. The
# CLI layer's mutants are mostly in argument plumbing that the integration
# tests cover end to end.
scope=(
  --package paredit-core-syntax
  --package paredit-core-edit
  --package paredit-core-semantics
  --package paredit-core-lint-engine
  --package paredit-core-safety
)

if [ "$#" -gt 0 ] && [ "${1#-}" = "$1" ]; then
  # A path argument replaces the package scope entirely.
  scope=(--file "$1/**/*.rs")
  shift
fi

printf 'mutants: this takes hours; ^C is safe and loses only the current mutant\n'
exec cargo mutants "${scope[@]}" "$@"
