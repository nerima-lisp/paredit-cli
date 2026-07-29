#!/usr/bin/env bash
# Compare this working tree's benchmarks against another revision, back to back.
#
#   ./scripts/bench-compare.sh                    # against origin/main
#   ./scripts/bench-compare.sh v1.2.0             # against a tag
#   THRESHOLD=15 ./scripts/bench-compare.sh       # allow 15% instead of 10%
#   REPORT=bench-report.md ./scripts/bench-compare.sh   # also write a Markdown table
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
#
# ## Why the confidence interval and not just the point estimate
#
# Criterion reports a mean change *and* a confidence interval for it. The point
# estimate alone does not say whether the number means anything: +10.3% with an
# interval spanning -5%..+25% is a measurement that could not tell the two
# revisions apart, and failing on it reports a regression that was never
# observed. On a shared CI runner that is the common case, not the rare one.
#
# So the gate fails only when the interval's *lower* bound clears the
# threshold — when the measurement is confident the regression is at least that
# large. A point estimate over the threshold whose interval is not is still
# printed, marked `noisy`, because it is worth a reviewer's glance; it just is
# not evidence.
#
# ## The Markdown report
#
# `REPORT=<path>` additionally writes every benchmark's change as a Markdown
# table — not just the ones over THRESHOLD, so a reviewer sees the whole
# picture rather than only what failed. It is written whether the comparison
# passes, fails, or (see below) cannot be made at all, so CI can attach it to
# a job summary unconditionally.

set -euo pipefail

baseline_ref="${1:-origin/main}"
threshold="${THRESHOLD:-10}"
report_path="${REPORT:-}"
repository_root="$(git rev-parse --show-toplevel)"
cd "$repository_root"

write_missing_report() {
  [ -n "$report_path" ] || return 0
  {
    printf '## Benchmark comparison: %s vs working tree\n\n' "$baseline_ref"
    printf 'The baseline could not be benchmarked, so nothing was compared. This is not a\n'
    printf 'verdict on the working tree — see the job log for why.\n'
  } >"$report_path"
}

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

# Both runs must write into ONE Criterion data directory, or there is nothing
# to compare against. They must NOT share a build directory.
#
# Sharing `CARGO_TARGET_DIR` did both at once, and the second half is wrong.
# The two trees hold packages of the same name and version — `paredit-core-syntax
# v1.2.1` is `paredit-core-syntax v1.2.1` in each — so the baseline's build
# lands on the same artifact filenames as the working tree's, and whichever ran
# first wins. It ran first, so the working tree linked the *baseline's* library.
#
# That is invisible while a branch only changes function bodies: the stale
# rlib has the same shape and links fine, and the numbers it produces are the
# baseline's, silently. It becomes visible the moment a branch adds a new item
# to a core package that a new member imports — which is how it was found
# (`selector::rewrite`, added for `query replace`): the working-tree build
# failed with `no plan_rewrite in selector`, naming a module that is right
# there in the source.
#
# So: separate build directories, one shared `CRITERION_HOME`. Criterion reads
# that in preference to `$CARGO_TARGET_DIR/criterion`, which is exactly the
# seam needed.
#
# The cost is that the baseline no longer reuses the working tree's compiled
# third-party dependencies and builds its own copy — a few minutes on a cold
# CI runner. That is the right trade: the alternative shares the dependencies
# *and* the local packages, and there is no way to have only the first.
export CRITERION_HOME="$repository_root/target/criterion"

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
if ! (
  cd "$worktree" &&
    CARGO_TARGET_DIR="$worktree/target" \
      cargo bench --quiet --package paredit-cli "${bench_targets[@]}" \
      -- --save-baseline bench-compare-base "${bench_args[@]}" >/dev/null
); then
  cat <<MESSAGE >&2

bench-compare: the baseline ($baseline_ref) could not be benchmarked.

This is not a verdict on the working tree. It usually means the branch adds or
repairs a benchmark that the baseline does not have or cannot run. Nothing was
compared.
MESSAGE
  write_missing_report
  exit 0
fi

printf 'bench-compare: measuring working tree\n'
cargo bench --quiet --package paredit-cli "${bench_targets[@]}" -- --baseline bench-compare-base "${bench_args[@]}" >/dev/null

printf 'bench-compare: comparing (threshold %s%%)\n' "$threshold"
python3 - "$CRITERION_HOME" "$threshold" "$baseline_ref" "$report_path" <<'PYTHON'
import json
import os
import sys

criterion_root, threshold, baseline_ref, report_path = (
    sys.argv[1],
    float(sys.argv[2]),
    sys.argv[3],
    sys.argv[4],
)
changes = []  # (name, change_percent)

for directory, _, files in os.walk(criterion_root):
    if os.path.basename(directory) != "change" or "estimates.json" not in files:
        continue
    with open(os.path.join(directory, "estimates.json")) as handle:
        estimates = json.load(handle)
    mean = estimates["mean"]
    # Criterion reports the change as a fraction of the baseline mean.
    change = mean["point_estimate"] * 100.0
    # The interval is what makes the point estimate evidence. Fall back to the
    # point estimate itself if a Criterion version ever stops writing one, so
    # the gate degrades to its old behaviour rather than silently passing
    # everything.
    interval = mean.get("confidence_interval") or {}
    lower = interval.get("lower_bound", mean["point_estimate"]) * 100.0
    name = os.path.relpath(os.path.dirname(directory), criterion_root)
    changes.append((name, change, lower))

compared = len(changes)
# Over the threshold *and* measured confidently enough to say so.
regressions = [entry for entry in changes if entry[1] > threshold and entry[2] > threshold]
noisy = [entry for entry in changes if entry[1] > threshold and entry[2] <= threshold]
improvements = [entry for entry in changes if entry[1] < -threshold]

if report_path:
    with open(report_path, "w") as handle:
        handle.write(f"## Benchmark comparison: {baseline_ref} vs working tree\n\n")
        if compared == 0:
            handle.write("No comparable benchmarks were found.\n")
        else:
            handle.write("| Benchmark | Change | At least | |\n| --- | ---: | ---: | --- |\n")
            for name, change, lower in sorted(changes, key=lambda entry: -entry[1]):
                if change > threshold:
                    flag = "SLOWER" if lower > threshold else "noisy"
                elif change < -threshold:
                    flag = "faster"
                else:
                    flag = ""
                handle.write(f"| `{name}` | {change:+.1f}% | {lower:+.1f}% | {flag} |\n")
            handle.write(
                f"\n{compared} benchmark(s) compared, {len(regressions)} over the "
                f"{threshold:g}% threshold, {len(noisy)} over it but within the "
                "measurement's own noise.\n\n"
                "\"At least\" is the lower bound of Criterion's confidence interval for "
                "the change. A benchmark is only counted as a regression when that bound "
                "clears the threshold too — a point estimate whose interval reaches back "
                "below it is a measurement that could not tell the two revisions apart.\n"
            )

if compared == 0:
    print("bench-compare: no comparisons found; did both runs execute?", file=sys.stderr)
    sys.exit(2)

for name, change, _ in sorted(improvements, key=lambda entry: entry[1]):
    print(f"  faster  {change:+7.1f}%  {name}")
for name, change, lower in sorted(noisy, key=lambda entry: -entry[1]):
    print(f"  noisy   {change:+7.1f}%  {name}  (interval reaches {lower:+.1f}%)")
for name, change, lower in sorted(regressions, key=lambda entry: -entry[1]):
    print(f"  SLOWER  {change:+7.1f}%  {name}  (at least {lower:+.1f}%)")

print(
    f"bench-compare: {compared} benchmark(s) compared, {len(regressions)} over threshold, "
    f"{len(noisy)} over threshold but within noise"
)
sys.exit(1 if regressions else 0)
PYTHON
