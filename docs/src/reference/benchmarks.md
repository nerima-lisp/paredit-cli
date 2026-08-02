# Benchmarks

`paredit-cli` carries five Criterion benchmark targets, declared as `[[bench]]`
entries in the root `Cargo.toml`:

| Target | Measures |
| --- | --- |
| `benches/lint_report.rs` | One lint pass over dense and rule-clean documents. |
| `benches/similarity_report.rs` | The pruning that keeps near-duplicate detection off its worst case. |
| `benches/cache_dir.rs` | Whether `--cache-dir` is worth turning on. |
| `benches/parse_scaling.rs` | How one parse scales with the size of the file. |
| `benches/edit_all_loop.rs` | What one `--all` edit costs per match it applies. |

One measurement does not fit in Criterion at all: peak memory. `tests/parse_memory.rs`
is an ignored test rather than a benchmark, for the reasons in
[Parse cost](#parse-cost) below.

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

## Parse cost

Every command begins by reading a file into a `String` and handing it to
`SyntaxTree::parse_with_dialect`. That is the floor under every other number on
this page, and until `benches/parse_scaling.rs` existed nothing would have
noticed a parser that became quadratic in document length.

Both the benchmark and `tests/parse_memory.rs` build the same synthetic
document: one `defun` template repeated to reach a target size, in two shapes.
`plain` is ordinary Common Lisp. `reader-conditional` is the same forms with
`#+sbcl` in front of the body, which folds the conditional *and the whole form
it guards* into a single opaque atom — so nearly the same bytes produce about
a quarter of the nodes. Every dialect reader does this, the permissive
`Dialect::Unknown` one included; it used to be specific to
`Dialect::CommonLisp`. Keeping both arms
means a change that stopped folding shows up as the two arms converging,
instead of as a silent 3x increase in memory.

### Memory, and why it is not a benchmark

Criterion measures time, not memory, and the memory question here needed an
answer: `SyntaxTree` keeps its own copy of the source *plus* a flat `Vec<Node>`
whose entries are much larger than the tokens they describe. `tests/parse_memory.rs`
measures the peak resident set around a parse — `VmHWM` from `/proc/self/status`
on Linux, `getrusage(RUSAGE_SELF)` on macOS — at sizes up to 128 MiB:

```console
$ cargo test --profile bench --test parse_memory -- --ignored --nocapture
```

It is `#[ignore]`d because it allocates about two gigabytes at the largest size,
and it reports rather than asserts: a threshold would only describe the machine
it was last calibrated on. Each size runs in a *fresh child process*, which is
not incidental — both platforms report a high-water mark that never falls, so
measuring several sizes in one process would report every one of them at the
largest one's peak.

The two largest sizes are deliberately past what the CLI will read. A document
reaching the parser through a command is capped at `DEFAULT_MAX_INPUT_BYTES`
(64 MiB), and `--max-input-bytes` can only lower that ceiling, never raise it —
so the 32 and 128 MiB rows below measure the parser as a library, and exist to
answer the linearity question rather than to describe a reachable run. The
worst case a command can actually reach is the 64 MiB point on the same line.

A single run on one developer machine (Apple M-series, `--profile bench`) —
again **not** targets:

| Shape | MiB | Nodes | Peak RSS | Peak / source | Bytes / node |
| --- | ---: | ---: | ---: | ---: | ---: |
| `plain` | 1 | 183 039 | 21.5 MB | 16.7x | 89 |
| `plain` | 8 | 1 464 053 | 133.3 MB | 15.4x | 83 |
| `plain` | 32 | 5 856 212 | 516.0 MB | 15.3x | 82 |
| `plain` | 128 | 23 424 811 | 2050.7 MB | 15.3x | 82 |
| `reader-conditional` | 1 | 43 101 | 9.5 MB | 5.3x | 101 |
| `reader-conditional` | 128 | 5 515 803 | 660.0 MB | 4.9x | 95 |

The ratio is flat across two orders of magnitude, so the cost is linear in file
size, and essentially all of the constant is the node arena rather than the
retained source: this fixture produces one node per 5.7 source bytes, so a
`Node`'s 72 bytes is most of the 15x. Reader conditionals fold, so they cost
~5x on the same bytes; per *node* the two shapes agree, which confirms node
count is the whole story.

`Node` was 152 bytes and the ratio 30x until the layout was tightened —
`ByteOffset` and `NodeId` narrowed to `u32`, and the two reader-prefix vectors
moved behind one `Option<Box<_>>`. A `const` assertion beside the struct now
holds it at 72. Nothing about a node's meaning changed: the same document
produces bit-identical node counts before and after.

!!! note "The reallocation spike is a myth, at least on 64-bit"

    The node arena is grown by doubling and never pre-sized, which looks like
    it should leave a copy of the whole arena in the high-water mark. Measured,
    it does not: parsing 128 MiB with `Vec::with_capacity` reserved up front
    peaks at 2050.70 MB and with `Vec::new()` at 2050.57 MB — a 0.006%
    difference, i.e. none. A large `Vec` is backed by its own mapping, and
    growing it remaps pages rather than copying into a second resident
    allocation. Pre-sizing the arena was tried for this reason and removed
    again: it bought nothing measurable and cost two invented constants.

### Time

`cargo bench --bench parse_scaling`, same machine, same fixture:

| Shape | MiB | Time | Throughput |
| --- | ---: | ---: | ---: |
| `plain` | 1 | 9.90 ms | 101 MiB/s |
| `plain` | 8 | 68.5 ms | 117 MiB/s |
| `reader-conditional` | 1 | 5.84 ms | 171 MiB/s |
| `reader-conditional` | 8 | 45.7 ms | 175 MiB/s |

Flat throughput, i.e. linear time, matching the memory result.

The target sets `SamplingMode::Flat` where its neighbours leave the default.
Criterion's linear sampling runs 1275 iterations to collect fifty samples,
which is right for a routine measured in microseconds and would be a hundred
seconds for the 8 MiB arm. Flat sampling bounds the target's cost by its
`measurement_time` instead. It also times with `iter_custom`, to keep the
tree's destructor — freeing 1.4 million nodes and their child vectors — out of
a measurement of parsing, without Criterion retaining a hundred-megabyte tree
per iteration the way `iter_with_large_drop` would.

!!! note "The fixture is uniform, and real source is not"

    Every form in the generated document has the same depth, arity and token
    lengths. That is what makes the two shapes comparable and the numbers
    reproducible, but it fixes the nodes-per-byte ratio at a value real source
    will not share: long docstrings, long string literals and wide `case` forms
    all push it down, deep macro nesting pushes it up. The *shape* of the curve
    is what these targets measure reliably. The 15x constant is indicative.

## `--all` edits, and what a match costs

`benches/edit_all_loop.rs` measures the other direction: not one parse of a
large file, but many parses of a medium one.

Every structure-editing command that takes a selector shares one loop. Given
`--all`, it applies one edit per match, right to left, and re-parses the whole
document between them — deliberately, so that a match resolved earlier cannot
be applied to text a later edit has moved. The cost of a `--all` run is
therefore quadratic in the match count, and this is the only target on this
page whose work grows with anything other than document size.

The swept axis is the number of matches, with document size held roughly
proportional to it: a file with four hundred call sites is not the size of one
with twenty-five, and holding it fixed would measure a shape no real invocation
has.

The loop parses once per match. It is worth stating as a number, because it was
twice per match until the parse `Edit::normalize_changed_line_trivia` makes —
of the rewrite it is handed, to tell trailing whitespace apart from the inside
of a string — was lent back to the caller instead of dropped. Those two parses
were about 95% of the loop, and removing one of them roughly halved it at every
arm. Nothing about *what* is parsed changed: there is no incremental parsing
here, and a parse is only carried forward when it still describes the document
byte for byte, which `edit_target_with` asserts on every pass in a debug build.

Once per match is the ordinary case rather than a guarantee: a pass whose
rewrite left trailing whitespace behind has its parse invalidated by the
removal, and the next pass parses again. That is rare — most edits never
produce trailing whitespace at all — so it moves the constant, not the shape.

!!! warning "`parse_scaling` would not notice this loop regressing"

    A change that made the loop parse twice per match again leaves
    `benches/parse_scaling.rs` completely flat — each individual parse is
    exactly as fast as it was — while doubling the cost of every `--all`
    invocation in the tool. That is why this target exists separately.

## `similarity` and the clone reports

`benches/similarity_report.rs` covers the near-duplicate detection paths, whose
cost is quadratic in the candidate count before pruning. Its scenarios exist to
pin that pruning in place. Anything touching scoring, overlap policy, or clone
classification should be measured there — see the *Complexity* section of
`packages/feature/similarity/README.md` for which behaviours are the sensitive
ones.
