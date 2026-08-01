#!/usr/bin/env bash
# Run the cheap checks before paying for `nix flake check`.
#
#   ./scripts/precheck.sh                          # steps 1-3 and 5
#   ./scripts/precheck.sh paredit-core-syntax       # also step 4, doc tests for that package
#
# ## Why this exists
#
# `nix flake check` is the real gate - it is what CI enforces and what the
# release checklist calls for - but it costs 35-40 minutes on a developer or
# agent machine, building roughly ten derivations, several of them on a
# different Rust toolchain or profile than `nix develop` gives you (msrv's
# pinned 1.85 toolchain; clippy's and nextest's dev profile; package,
# treefmt, and lint-format-integration's release profile). Most iterations
# fail on something a plain `cargo` invocation would have caught in under two
# minutes. This script runs those cheap checks in the order most likely to
# fail fastest, and stops at the first failure, so the expensive gate is only
# reached once the fast ones are clean.
#
# It deliberately does NOT run `nix flake check` itself. That stays a
# separate, explicit step once this script is clean - background it and keep
# working rather than waiting on it synchronously:
#
#   nix flake check >flake-check.log 2>&1 &
#
# ## What passing here does not guarantee
#
# The Nix clippy derivation resolves a newer clippy than whatever `cargo
# clippy` picks up locally, so a clean run of step 1 is not a guarantee the
# Nix check's clippy agrees - that gap is real and not something this script
# can close. `msrv`, `package`, `documentation`, and `lint-format-integration`
# each build something this script never touches; step 5 only narrows the gap
# for the two cheapest full Nix checks, clippy and formatting.

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

doc_test_package="${1:-}"

run() {
  printf 'precheck: %s\n' "$*"
  "$@"
}

# 1. Broadest, fastest signal: most iterations die here.
run cargo clippy --all-targets --all-features -- -D warnings

# 2. `cargo fmt --all` reformats in place. It is not `cargo clippy --fix`,
# which does not touch formatting, and it is deliberately not `nix fmt` here -
# `nix fmt` rewrites Lisp fixtures too and can take minutes when it rebuilds
# the formatter derivation, which is wasted work every time only Rust changed.
run cargo fmt --all

# 3. The full test suite under cargo-nextest, ~80-125s locally.
run cargo nextest run --locked

# 4. cargo-nextest does not run doc tests. They are usually unaffected by a
# change, but not when a package moved - `crate::` paths inside doc examples
# break silently otherwise. Pass a package name to check it.
if [ -n "$doc_test_package" ]; then
  run cargo test --doc -p "$doc_test_package"
fi

# 5. The two cheapest full Nix checks, closest to what the real gate runs:
# the pinned-newer clippy and the treefmt formatting check (Rust, Nix, and
# Lisp sources together). Still short of the full gate - see above.
run nix build ".#checks.x86_64-linux.clippy" ".#checks.x86_64-linux.formatting" --no-link

printf 'precheck: cheap checks passed - `nix flake check` is the remaining gate\n'
