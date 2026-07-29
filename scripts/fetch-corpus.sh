#!/usr/bin/env bash
# Fetch a real-world Lisp corpus for `cargo test --test corpus`.
#
# The corpus test always runs against the small vendored fixture set in
# tests/fixtures/corpus, which needs no network and covers the constructs that
# have historically been awkward. This script fetches the *other* kind of
# corpus: a few hundred thousand lines of code nobody on this project wrote,
# which is where the parser invariants get a real workout.
#
#   ./scripts/fetch-corpus.sh                 # into .corpus/
#   ./scripts/fetch-corpus.sh /path/to/dir    # somewhere else
#
# Then:
#
#   PAREDIT_CORPUS_DIR=.corpus cargo test --test corpus -- --nocapture
#
# Deliberately not wired into CI. A network fetch in a test run makes the run
# non-reproducible and non-hermetic, and the Nix build has no network at all.
# What CI gets is the vendored corpus; what a maintainer gets is this.

set -euo pipefail

destination="${1:-.corpus}"
mkdir -p "$destination"

# Shallow clones: this corpus is read, never built, and the histories are large.
#
# Each entry is "<name> <url> <ref>". Chosen for breadth of reader syntax rather
# than popularity: SBCL's contribs and ASDF between them exercise nearly every
# dispatch macro in the standard, and the Emacs and Scheme entries cover the
# dialects whose readers differ most from Common Lisp's.
repositories=(
  "sbcl https://github.com/sbcl/sbcl.git master"
  "asdf https://gitlab.common-lisp.net/asdf/asdf.git master"
  "alexandria https://gitlab.common-lisp.net/alexandria/alexandria.git master"
  "closer-mop https://github.com/pcostanza/closer-mop.git master"
  "bordeaux-threads https://github.com/sionescu/bordeaux-threads.git master"
  "cl-ppcre https://github.com/edicl/cl-ppcre.git master"
  "magit https://github.com/magit/magit.git main"
  "use-package https://github.com/jwiegley/use-package.git master"
  "chicken-core https://github.com/kmarkus/chicken-core.git master"
)

for entry in "${repositories[@]}"; do
  read -r name url ref <<<"$entry"
  target="$destination/$name"
  if [ -d "$target/.git" ]; then
    printf 'corpus: %s already present\n' "$name"
    continue
  fi
  printf 'corpus: fetching %s\n' "$name"
  # A failed fetch is not fatal: a corpus of eight repositories is still a
  # corpus, and a maintainer behind a proxy should get the ones that worked.
  if ! git clone --depth 1 --branch "$ref" --quiet "$url" "$target"; then
    printf 'corpus: WARNING could not fetch %s from %s\n' "$name" "$url" >&2
    rm -rf "$target"
  fi
done

count=$(find "$destination" \
  \( -name '*.lisp' -o -name '*.lsp' -o -name '*.asd' -o -name '*.el' \
     -o -name '*.scm' -o -name '*.ss' -o -name '*.rkt' -o -name '*.clj' \) \
  -type f 2>/dev/null | wc -l | tr -d ' ')

cat <<MESSAGE

corpus: $count Lisp-family source files under $destination

Run the invariant sweep with:

  PAREDIT_CORPUS_DIR=$destination cargo test --test corpus -- --nocapture

The test reads at most 4000 files per run and says so when it stops early.
Point PAREDIT_CORPUS_DIR at a subdirectory to cover a different slice.
MESSAGE
