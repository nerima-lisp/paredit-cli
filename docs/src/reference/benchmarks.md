# Benchmarks

`paredit-cli` carries three Criterion benchmark targets, declared as `[[bench]]`
entries in the root `Cargo.toml`:

| Target | Measures |
| --- | --- |
| `benches/lint_report.rs` | One lint pass over dense and rule-clean documents. |
| `benches/similarity_report.rs` | The pruning that keeps near-duplicate detection off its worst case. |
| `benches/cache_dir.rs` | Whether `--cache-dir` is worth turning on. |

Run one directly:

```console
$ cargo bench --bench cache_dir
```

## How the numbers are read

Criterion's absolute timings are a property of the machine as much as of the
code. The same commit measured on two runners, or on one runner an hour apart,
differs by tens of percent; thermal state and a noisy neighbour are enough. A
gate comparing today's number against a stored one would fire constantly and
mean nothing.

So nothing is stored. `scripts/bench-compare.sh` checks a baseline revision out
into a temporary worktree, benchmarks it, benchmarks the working tree, and
compares the two runs it just made:

```console
$ ./scripts/bench-compare.sh                  # against origin/main
$ ./scripts/bench-compare.sh v1.3.0           # against a tag
$ THRESHOLD=15 ./scripts/bench-compare.sh     # allow 15% instead of 10%
```

The script reports each benchmark's mean change **and** the lower bound of
Criterion's confidence interval for it, and fails only when *both* clear the
threshold. A point estimate over the threshold whose interval reaches back
below it is printed as `noisy` rather than failed: it is a measurement that
could not tell the two revisions apart, and reporting it as a regression would
report something nobody observed.

!!! warning "A new benchmark must be declared in `Cargo.toml`"

    `bench-compare.sh` builds its `--bench` list by parsing the `[[bench]]`
    blocks out of the root `Cargo.toml`, not with `--benches`. A benchmark file
    added to `benches/` without its own `[[bench]]` block compiles, runs
    locally, and is silently never measured by CI.

## `--cache-dir` effectiveness

`--cache-dir` reuses a previous scan's file list instead of walking the tree
again. It is deliberately opt-in, because discovery is not the expensive part
of a lint run — parsing is — and the flag only pays off where the walk
genuinely dominates: a large tree scanned repeatedly by an editor or an agent
running several commands over the same roots.

`benches/cache_dir.rs` is what keeps that claim honest. It builds a synthetic
workspace (16 files per directory, the fanout of a realistic source tree) and
measures three arms:

- **`cold`** — no `--cache-dir`. The walk, and the baseline for the other two.
- **`populate`** — `--cache-dir` with nothing usable in it: the first run, or
  any run after `--clear-cache`. Walks *and* writes the entry, so it is
  necessarily the slowest arm.
- **`warm`** — `--cache-dir` reusing a valid entry. What the flag is for.

The fanout is the number that decides whether the benchmark tells the truth.
Validating a hit re-stats every directory the walk listed and compares its entry
count, so the cache's saving is per *file*, not per path. A fixture with one
directory holding a thousand files would make a hit look nearly free and prove
nothing about a real project.

A single run on one developer machine, to show the shape of the result — these
are **not** targets, and a different machine will produce different numbers:

| Files | `cold` | `populate` | `warm` | warm vs. cold |
| ---: | ---: | ---: | ---: | ---: |
| 128 | 1.59 ms | 1.76 ms | 0.23 ms | 6.9x |
| 1024 | 14.23 ms | 14.11 ms | 1.46 ms | 9.8x |

Two things in that table are worth carrying away. The cache's advantage *grows*
with the file count, which is what the per-directory validation predicts and the
reason the flag is aimed at large trees. And `populate` costs about 11% over
`cold` at 128 files but nothing measurable at 1024 — the entry write is a fixed
cost, so `--clear-cache` is cheap to reach for on exactly the workspaces where
the cache matters most.

The benchmark also asserts, on every iteration, that the `warm` arm actually
reports a cache hit. A change that quietly stops the cache from hitting would
otherwise be measured as a second `cold` arm and reported under a name saying
the opposite.

## `similarity` and the clone reports

`benches/similarity_report.rs` covers the near-duplicate detection paths, whose
cost is quadratic in the candidate count before pruning. Its scenarios exist to
pin that pruning in place. Anything touching scoring, overlap policy, or clone
classification should be measured there — see the *Complexity* section of
`packages/feature/similarity/README.md` for which behaviours are the sensitive
ones.
