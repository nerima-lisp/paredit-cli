# Development

Everything a contributor needs is provided by the Nix flake; no manually
installed Rust toolchain is required. Before changing code, read the
[architecture guide](../reference/architecture.md) to know which
`packages/core/*` or `packages/feature/*` package a change belongs in — `src/`
is only the composition root that wires those packages into the `paredit`
binary. Layers (`domain`, `usecase`, `rule`, `cli`, ...) are a naming
convention for files *within* a package, not top-level directories.

## Environment

```sh
nix develop        # rustc, cargo, rust-analyzer, cargo-nextest, clippy, mkdocs-material
```

With [direnv](https://direnv.net/), `direnv allow` activates the same shell
automatically via the committed `.envrc`.

## Development loop

```sh
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo nextest run --locked
```

Formatting for the whole repository (Rust via rustfmt, Nix via nixfmt, and
Lisp sources via `paredit edit format`) is one command:

```sh
nix fmt
```

## Scaffolding a new rule or command

`cargo xtask` (see `xtask/`, aliased in `.cargo/config.toml`) generates the
boilerplate-shaped files a new lint rule or a new `inspect`/`edit`/`refactor`
command needs:

```sh
cargo xtask new-lint-rule <theme> <rule-name> --description "..."
cargo xtask new-command <inspect|edit|refactor> <package> <command-name> --description "..."
```

`new-lint-rule` scaffolds the rule's `domain`/`usecase`/`rule`/`cli` files
inside an existing `lint-<theme>` package and also registers it — bumping
`RULE_COUNT` and appending its entry in
`src/lint/registry/mod.rs`, and the matching pinned-count assertions
in `src/lint/registry/catalog.rs` — because that step is pure
arithmetic over one well-known pattern. `new-command` scaffolds the same
shape without the rule registration, for a command outside the lint suite.

Neither generator touches the composition root
(`src/presentation/cli/{command,dispatch,contract}.rs`,
`tests/cli/dialect_contract.rs`, `docs/src/reference/api.md`). Wiring a command in
by hand there is unavoidable — clap needs a real enum variant and match arm to
add — and a script that edits those files blind has, in this project's own
history, silently double-commaed `command.rs` or duplicated a
`contract.rs` entry. Instead both generators print the exact remaining
steps, with the current pinned counts read fresh from the repository, so
finishing the wiring is a short, reviewable diff rather than a search.

The `scripts/*-package.py` scripts are a separate, larger surface — moving
code between whole packages, rewriting `crate::` paths across a package
boundary — exercised only a few times a year. They stay Python; porting them
carries more risk than leaving them alone.

## The verification gate

Locally the gate is one command:

```sh
nix flake check
```

It builds and runs every check the project defines:

| Check | What it verifies |
| --- | --- |
| `treefmt` | Rust, Nix, and Lisp sources are canonically formatted |
| `actionlint` | GitHub Actions workflows are well-formed |
| `clippy` | No clippy warnings with `-D warnings`, across all targets and features |
| `nextest` | The full test suite under cargo-nextest |
| `msrv` | The workspace still compiles on the declared MSRV toolchain |
| `package` | The release binary builds |
| `documentation` | The MkDocs (Material) site builds to a valid `index.html` |
| `lint-format-integration` | The `paredit-lint` / `paredit-format` gates behave end to end |
| `treefmt-pr-check` | Same as `treefmt`, against a thin-LTO paredit binary — see "How CI runs it" |
| `lint-format-integration-pr-check` | Same as `lint-format-integration`, against a thin-LTO paredit binary |

Each check is defined exactly once, and each one is the only place its work
happens. `package` does not run the test suite — that is `nextest`'s job — and
`msrv` stops at `cargo check`, because "does it still compile on 1.85" is the
only question it exists to answer. Duplicating either would compile every test
target a second time under `lto = "fat"`, for no coverage the other checks do
not already give.

### How CI runs it

CI does not run `nix flake check` as one command. A `plan` job runs
`nix flake check --no-build`, which instantiates every flake output — packages,
apps, devShells, overlays, formatter, checks — without building any of them,
and then reads `lib.<system>.ciCheckNames` out of the flake to produce a job
matrix. Every check then builds on a runner of its own.

The reason is that a GitHub runner has four cores. Run as one command, Nix can
only interleave the checks on those four cores, so the gate costs the *sum* of
its checks; fanned out, it costs the *maximum*. The matrix comes from the flake
rather than from `ci.yml` so that adding a check to `mkCoreChecks` is enough to
get it verified.

That matrix is not identical on every event. `package`, `treefmt`, and
`lint-format-integration` all build the same fat-LTO (`lto = "fat"`,
`codegen-units = 1`) release binary, and a pull request cannot share a Cachix
build across its checks (a `pull_request` run is always Cachix pull-only, so
each check would cold-compile that binary independently on its own runner).
On `pull_request` events the `plan` job therefore drops `package` entirely —
deferred to `main`/release — and `treefmt`/`lint-format-integration` build
against a thin-LTO sibling instead (`treefmt-pr-check` /
`lint-format-integration-pr-check`, `[profile.pr-check]` in `Cargo.toml`),
which cuts a pull request's slowest path from several minutes of whole-program
LTO to a low-hundreds-of-seconds compile. A fat-LTO-only compile break is
therefore caught on `main` after a merge, not on the pull request itself.
Every other event (pushes to `main`, tag releases) builds the full,
unfiltered set — including the `-pr-check` checks — so `packagePrCheck` gets
built and pushed to Cachix somewhere a pull request can actually pull it from;
without that, the `-pr-check` cache key would never be seeded and every pull
request would cold-compile it regardless.

### Where the build artifacts come from

The Rust checks are [crane](https://github.com/ipetkov/crane) derivations
sharing pre-built `cargoArtifacts`:

```text
depsRelease ─▶ package
depsDev     ─▶ clippy, nextest
depsMsrv    ─▶ msrv                                    (pinned MSRV toolchain)
depsPrCheck ─▶ treefmt-pr-check, lint-format-integration-pr-check
```

Each `deps*` derivation is a separate cargo profile's artifacts, shared only
within that profile.

A `deps` derivation compiles the 181 locked dependencies from dummified
sources, so its hash depends on `Cargo.lock` and the member manifests and not
on any `.rs` file. A pull request that only edits Rust sources therefore
substitutes all four from the binary cache instead of recompiling the
dependency graph once per check.

### Which host CI verifies

CI runs the gate on Linux only. A flake check is evaluated for the host it runs
on, so the two legs of the old host matrix shared no work: a macOS runner was a
second full verification rather than an extension of the first. (The per-check
matrix above is a different axis — it splits one host's checks across runners
rather than repeating all of them on a second host.)

**Darwin is therefore unverified in CI, and that is a real gap rather than a
free simplification.** Most of the crate is portable, and the 237 `cfg(unix)`
blocks are exercised on Linux like anywhere else. But two areas are macOS-only
and Linux cannot even compile them:

| Code | What it does |
| --- | --- |
| `packages/core/cli/src/macos_acl.rs` | Reads and restores POSIX ACLs so an autofix preserves file permissions. Twelve `unsafe` blocks over `libc`. |
| the `target_os = "macos"` arm in `packages/core/cli/src/io.rs` | The Darwin path of the atomic file replacement used by every write. |

Both sit on the path that rewrites a user's source files, so a regression there
is expensive and CI will not catch it. Until that changes, **run
`nix flake check` on a Mac before tagging a release** — locally, that command
is the Darwin check, and the release checklist calls for it.

Restoring the matrix is a small change to `.github/workflows/ci.yml`: the
contract test beside it asserts the *gates* rather than the hosts, precisely so
that stays a free choice.

### Cheap checks before the full gate

`nix flake check` costs 35-40 minutes on a developer or agent machine, because
it builds roughly ten derivations, several of them on a different Rust
toolchain or profile than `nix develop` gives you: msrv's pinned 1.85
toolchain, clippy's and nextest's dev profile, package/treefmt/lint-format-integration's
release profile, plus the docs site build and actionlint. That cost is not
changing — it is what CI enforces and what the release checklist calls for —
but most iterations fail on something a plain `cargo` invocation catches in a
couple of minutes, long before it is worth paying for the rest. Run these, in
order, before invoking the full gate:

| Step | Command | Catches |
| --- | --- | --- |
| 1 | `cargo clippy --all-targets --all-features -- -D warnings` | The bulk of `clippy` failures, at local-toolchain speed |
| 2 | `cargo fmt --all` | Rust formatting — note this is `cargo fmt`, not `cargo clippy --fix`, which does not reformat anything |
| 3 | `cargo nextest run --locked` | The test suite, ~80-125s locally |
| 4 | `cargo test --doc -p <pkg>` | Doc tests for a package that moved — `cargo-nextest` does not run these, and a moved package's doc examples can break silently |
| 5 | `nix build .#checks.x86_64-linux.{clippy,formatting} --no-link` | The two cheapest full Nix checks, closest to what `nix flake check` actually runs |

Only then reach for `nix flake check`, and treat it as a background gate —
start it and keep working, rather than waiting on it synchronously:

```sh
nix flake check >flake-check.log 2>&1 &
```

Step 2 is deliberately `cargo fmt --all` rather than `nix fmt` in this inner
loop: `nix fmt` also reformats Lisp fixtures via `paredit edit format` and can
take minutes when it rebuilds the formatter derivation, which is wasted work
on every iteration where only Rust changed. Step 5 exists because the Nix
`clippy` derivation resolves a newer clippy than a locally installed one, so a
clean step 1 does not guarantee step 5 agrees — that gap is real and this
workflow does not try to close it, only to catch it before the full 35-40
minute run does. `scripts/precheck.sh` runs steps 1, 2, 3, and 5 in order,
optionally step 4 for a named package, and stops at the first failure; it does
not invoke `nix flake check` itself, matching the split above.

## Robustness: corpora and fuzzing

Three layers, from "runs on every commit" to "runs when a maintainer asks".

### The corpus test

`cargo test --test corpus` asserts five invariants over every file it reads:
parsing terminates without panicking, parsing is lossless, formatting is
idempotent, every path the tree reports resolves, and no line of formatted
output starts at or left of the column of its enclosing form's opening
delimiter.

The fifth is the only one that is not a round-trip through the tool's own
tree, which is what makes it worth having: a layout that is merely *wrong*
satisfies the first four as long as it is wrong consistently. Columns are
measured as display width, so a full-width character counts as two. Lines the
formatter reproduced verbatim — inside a multi-line token, or in a top-level
form carrying a comment — are exempt, because their indentation was chosen by
whoever wrote the file rather than by the formatter; the run reports how many
lines it actually compared so that an exemption swallowing the whole corpus is
visible rather than silent.

A sixth test in the same file asserts that the vendored corpus has a fixture
for every dialect except `Dialect::Unknown`, which no filename can reach.

It runs against the vendored fixtures in `tests/fixtures/corpus` with no
network access.

Point it at real code to make it mean something:

```sh
./scripts/fetch-corpus.sh                    # clones ~9 projects into .corpus/
PAREDIT_CORPUS_DIR=.corpus cargo test --test corpus -- --nocapture
```

It reads at most 4000 files per run and says so when it stops early. This is
where `#c(1.0 2.0)` — an ANSI complex literal the reader did not know — was
found, in alexandria's test suite.

### The semantic coverage baseline

`cargo test --test semantic_coverage_baseline` is the same idea applied to
`inspect semantic-coverage` instead of the parser: it runs the workflow over a
fixed corpus and asserts the resolved-binding and known-list-expression counts
have not dropped below a pinned floor, so a change that quietly narrows the
transparency table fails CI instead of only showing up next time someone runs
the command by hand.

It does not reuse `tests/fixtures/corpus` — that corpus is deliberately
adversarial ("constructs that have historically been awkward"), so its
resolution rate sits near zero and has no room to regress. A second vendored
corpus, `tests/fixtures/semantic_coverage_corpus`, holds a small sample of
*ordinary* Common Lisp instead, and follows the same convention:

```sh
PAREDIT_CORPUS_DIR=.corpus cargo test --test semantic_coverage_baseline -- --nocapture
```

Point it at the same `.corpus/` checkouts `./scripts/fetch-corpus.sh` clones,
or at `~/quicklisp/dists`, to see the resolution rate on a real Common Lisp
codebase rather than the two vendored files. Raising the pinned floor after a
real improvement is a one-line edit in the test, the same shape as the pinned
command counts in `tests/cli/dialect_contract.rs`.

### The robustness properties

`cargo test --test parser_robustness` drives the same invariants from proptest
on stable: reader-significant token soup, arbitrary text, deep nesting,
unbalanced input, and every structural edit at an arbitrary byte offset. It
also replays everything under `fuzz/corpus` and `fuzz/artifacts`, so a crasher
found by a nightly fuzzer becomes a permanent regression test the moment its
artifact is committed.

### The fuzz targets

`fuzz/` is a cargo-fuzz package, excluded from the workspace because it needs a
nightly toolchain and links libFuzzer:

```sh
cargo install cargo-fuzz
cargo +nightly fuzz run parse               # reader, every dialect
cargo +nightly fuzz run format_idempotence  # format(format(x)) == format(x)
cargo +nightly fuzz run edit_at_offset      # every edit at a caller's offset
```

When a run finds a crash, commit the artifact under `fuzz/artifacts/<target>/`.
The stable replay test picks it up without anyone needing nightly again.

## Performance and test quality

### Benchmark comparison

```sh
./scripts/bench-compare.sh                          # against origin/main
./scripts/bench-compare.sh v1.2.0                   # against a tag
THRESHOLD=15 ./scripts/bench-compare.sh             # allow 15% instead of 10%
REPORT=bench-report.md ./scripts/bench-compare.sh   # also write a Markdown table
```

The script checks the baseline out into a temporary git worktree, benchmarks
it, benchmarks the working tree, and compares the two runs Criterion just made.
Nothing is stored between invocations.

That structure is the point. Criterion's absolute numbers are a property of the
machine as much as of the code — a different runner model, a noisy neighbour,
or thermal state moves them by tens of percent — so a gate that compared
today's number against a remembered one would fire constantly and mean nothing.
Two revisions measured back to back on one machine is the only comparison that
survives it, and it is what the `benchmark` CI job runs on every pull request.

That job reports rather than blocks: a performance regression is information
for the reviewer, not grounds for stopping a correctness fix. It also writes
the `REPORT` table to the job's step summary, unconditionally — every
benchmark's change, not only the ones over threshold, so the reviewer sees
the whole picture in the PR's Checks tab instead of having to open the log.

#### What the benchmarks are built with

`[profile.bench]` in `Cargo.toml` overrides the release profile's `lto = "fat"`
and `codegen-units = 1` with thin LTO over four codegen units. Those release
settings are serial: measured cold at the four-way parallelism a GitHub runner
has, the two Criterion targets take 523s wall against 578s of CPU — a
parallelism of 1.1, three cores idle — where thin LTO takes 128s. The script
builds twice, once per revision, so that was the larger half of an 18-minute
CI job.

Fat LTO earns its cost in the binary that ships and nothing here: a comparison
of two revisions needs both sides built the *same* way, not maximally. The
script exports the same two settings as `CARGO_PROFILE_BENCH_*`, so a baseline
predating this profile is still built to match rather than being measured under
the old one and reported as a difference in the code.

CI also caches both builds' dependency and target directories across runs (see
`.github/workflows/ci.yml`'s `benchmark` job), since this job runs plain
`cargo bench` rather than a crane/Nix derivation and so never reaches Cachix
otherwise. That mostly avoids the double-build cost above on a repeat push to
the same PR; the job only runs on `pull_request`, never on `push`, so a brand
new PR's first run is still a full cold build.

### Mutation testing

```sh
cargo install cargo-mutants
./scripts/mutants.sh                       # the analysis core
./scripts/mutants.sh packages/core/syntax  # one package
```

Line coverage says a line ran. It does not say that changing the line would
have failed anything, and that distinction is the whole question here: a rule
with a fixture that exercises it and asserts only "does not crash" is fully
covered and pins nothing. `cargo-mutants` changes one comparison, constant or
boolean at a time and re-runs the tests; a mutant that *survives* is a
statement the tests never make.

A full run is hours, so it is not in the pull-request gate. `--in-diff` narrows
it to a change set. The exclusions in `.cargo/mutants.toml` each carry a
reason; an exclusion without one is how a mutation-testing setup becomes a way
of not looking at things.

## Documentation is tested

The repository treats documentation as part of the public contract. Tests in
`tests/cli/*_contract.rs` read `README.md`, `docs/src/*.md`, `action.yml`, and
`flake.nix` and fail when documented commands, integration surfaces, or policy
statements drift from reality. When you change behaviour, update the
documentation in the same commit — CI enforces it.

To preview the site locally:

```sh
nix build .#docs                   # rendered site in ./result
mkdocs serve -f docs/mkdocs.yml    # live-reloading preview from the dev shell
```

## MSRV

The minimum supported Rust version is declared in `Cargo.toml`
(`rust-version = "1.85"`). Verify it before touching parser, refactor,
packaging, or public API surfaces:

```sh
cargo +1.85 test --locked
```

## Releases

The [release and compatibility guide](../reference/compatibility.md) defines the machine-output
contract and upgrade expectations. Maintainers should use the
[release checklist](https://github.com/nerima-lisp/paredit-cli/blob/main/docs/notes/releasing.md)
before publishing.
