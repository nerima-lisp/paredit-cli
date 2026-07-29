#!/usr/bin/env bash
# Compare this working tree's benchmarks against another revision, back to back.
#
#   ./scripts/bench-compare.sh                    # against origin/main
#   ./scripts/bench-compare.sh v1.2.0             # against a tag
#   THRESHOLD=15 ./scripts/bench-compare.sh       # allow 15% instead of 10%
#
# ## Why back to back
#
# Criterion's absolute numbers are a property of the machine as much as of the
# code: the same commit benchmarked on two different runners, or on one runner
# an hour apart, differs by tens of percent. Thermal state, a noisy neighbour,
# and a different CPU model all move it. A gate that compared today's number
# against a stored one would therefore fire constantly and mean nothing.
#
# What *is* comparable is two revisions measured on the same machine within the
# same minutes. This script arranges exactly that: it checks the baseline out
# into a temporary git worktree, benchmarks it, benchmarks the current tree,
# and compares the two runs Criterion just made. Nothing is stored between
# invocations and nothing is compared across machines.
#
# ## What it reports
#
# Criterion's own `--save-baseline` / `--baseline` machinery does the
# statistics; this reads the `change/estimates.json` it writes and fails when a
# benchmark's mean regressed by more than THRESHOLD percent. An improvement is
# reported and never fails.

set -euo pipefail

baseline_ref="${1:-origin/main}"
threshold="${THRESHOLD:-10}"
repository_root="$(git rev-parse --show-toplevel)"
cd "$repository_root"

if ! git rev-parse --verify --quiet "$baseline_ref" >/dev/null; then
  printf 'bench-compare: %s is not a revision in this repository\n' "$baseline_ref" >&2
  exit 2
fi

worktree="$(mktemp -d "${TMPDIR:-/tmp}/paredit-bench-baseline.XXXXXX")"
cleanup() {
  git worktree remove --force "$worktree" 2>/dev/null || rm -rf "$worktree"
}
trap cleanup EXIT

printf 'bench-compare: baseline %s -> %s\n' "$baseline_ref" "$worktree"
git worktree add --detach --quiet "$worktree" "$baseline_ref"

# The baseline is benchmarked into *this* tree's target directory, so both runs
# share one Criterion data directory and the comparison has something to
# compare against. CARGO_TARGET_DIR is what makes that happen without copying.
export CARGO_TARGET_DIR="$repository_root/target"

# Extra Criterion arguments, for a smoke run that does not take ten minutes:
#
#   BENCH_ARGS='--sample-size 10 --measurement-time 1' ./scripts/bench-compare.sh
#
# Deliberately not the default. A short measurement widens the confidence
# interval, and a gate whose threshold is smaller than its own noise floor
# reports regressions that are not there.
read -r -a bench_args <<<"${BENCH_ARGS:-}"

# The Criterion targets, named explicitly rather than selected with `--benches`.
#
# `--benches` also builds the library and binary targets, whose default libtest
# harness has no benchmarks and rejects `--save-baseline` outright. `bench =
# false` on those targets fixes it for *this* revision — and the baseline
# revision is an older checkout that may not carry the fix, which is precisely
# the case a comparison script has to survive.
bench_targets=()
while IFS= read -r name; do
  bench_targets+=(--bench "$name")
done < <(sed -n '/^\[\[bench\]\]/,/^$/ s/^name = "\(.*\)"/\1/p' Cargo.toml)

if [ "${#bench_targets[@]}" -eq 0 ]; then
  printf 'bench-compare: no [[bench]] targets declared in Cargo.toml\n' >&2
  exit 2
fi

printf 'bench-compare: measuring baseline\n'
# A baseline that cannot be benchmarked is not evidence about this branch.
#
# It happens for real and not rarely: the branch adds a benchmark the baseline
# does not have, or — as on the change that introduced this script — the branch
# *fixes* a benchmark that was broken on the baseline. Failing here would report
# the branch as regressed for having repaired something, and the only way to
# clear it would be to merge the fix first, which is the thing being gated.
#
# So: say so, loudly, and exit without a verdict. A missing comparison is
# visible in the log; a wrong one is not.
if ! (cd "$worktree" && cargo bench --quiet --package paredit-cli "${bench_targets[@]}" -- --save-baseline bench-compare-base "${bench_args[@]}" >/dev/null); then
  cat <<MESSAGE >&2

bench-compare: the baseline ($baseline_ref) could not be benchmarked.

This is not a verdict on the working tree. It usually means the branch adds or
repairs a benchmark that the baseline does not have or cannot run. Nothing was
compared.
MESSAGE
  exit 0
fi

printf 'bench-compare: measuring working tree\n'
cargo bench --quiet --package paredit-cli "${bench_targets[@]}" -- --baseline bench-compare-base "${bench_args[@]}" >/dev/null

printf 'bench-compare: comparing (threshold %s%%)\n' "$threshold"
python3 - "$repository_root/target/criterion" "$threshold" <<'PYTHON'
import json
import os
import sys

criterion_root, threshold = sys.argv[1], float(sys.argv[2])
regressions, improvements, compared = [], [], 0

for directory, _, files in os.walk(criterion_root):
    if os.path.basename(directory) != "change" or "estimates.json" not in files:
        continue
    with open(os.path.join(directory, "estimates.json")) as handle:
        estimates = json.load(handle)
    # Criterion reports the change as a fraction of the baseline mean.
    change = estimates["mean"]["point_estimate"] * 100.0
    name = os.path.relpath(os.path.dirname(directory), criterion_root)
    compared += 1
    if change > threshold:
        regressions.append((name, change))
    elif change < -threshold:
        improvements.append((name, change))

if compared == 0:
    print("bench-compare: no comparisons found; did both runs execute?", file=sys.stderr)
    sys.exit(2)

for name, change in sorted(improvements, key=lambda entry: entry[1]):
    print(f"  faster  {change:+7.1f}%  {name}")
for name, change in sorted(regressions, key=lambda entry: -entry[1]):
    print(f"  SLOWER  {change:+7.1f}%  {name}")

print(f"bench-compare: {compared} benchmark(s) compared, {len(regressions)} over threshold")
sys.exit(1 if regressions else 0)
PYTHON
