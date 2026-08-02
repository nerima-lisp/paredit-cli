# paredit-feature-lint-concurrency

Lint rules for thread-safety patterns in Common Lisp and Clojure.

## Responsibilities

Seven rules about the two things concurrent code gets wrong that reading it
does not reveal: state shared without synchronization, and errors that vanish
because they were signalled on the wrong thread.

The third column is not in the two-column shape the other lint packages use.
It is here because this package is one of the few that spans two dialects, and
which dialect a rule is scoped to is the first thing a reader needs about it.

| Rule | Flags | Dialect |
| --- | --- | --- |
| `atom-swap-with-side-effect` | a `swap!`/`swap-vals!`/`alter`/`commute` whose inline update function calls something effectful, which its retries repeat | Clojure |
| `dynamic-var-bound-across-thread-boundary` | a `make-thread` thunk reading a special that an enclosing `let` rebinds, which the new thread does not inherit | Common Lisp |
| `future-promise-never-realized` | a `let`-bound `future`/`promise`/`delay` whose symbol the body never mentions at all | Clojure |
| `lock-acquired-not-released` | a manual `acquire-lock`/`grab-mutex` with no `unwind-protect` and no `with-…` scope to release it on a non-local exit | Common Lisp |
| `recursive-lock-reentry-risk` | the same non-recursive lock named again inside its own scope | Common Lisp |
| `thread-spawned-without-error-handler` | a `make-thread` thunk that inlines two or more forms with no handler anywhere in it | Common Lisp |
| `unsynchronized-shared-mutation` | an `*earmuffed*` global written inside a `make-thread` thunk with no lock scope around the write | Common Lisp |

That list is the package's real specification: §5.2.2 splits by subject matter,
so naming the rules is the only way to say why one belongs here.

### What this package does not own

- **No registry.** `REGISTRY` stays in the root and names each rule's `META`
  and `RULE` across this boundary. A registry here would be the cycle §4.2
  exists to prevent.
- **No engine.** The single pass, head index and rule trait are
  `paredit-core-lint-engine`'s, and so is `RuleDialectScope` — the declaration
  the Dialect column above is a rendering of.
- **No unconditional shared-state rule.** `global-mutation-in-function` in
  `feature/lint-safety` flags an earmuffed write inside *any*
  `defun`/`defmethod`/`defgeneric`/`lambda`, with no thread and no lock
  condition at all. `unsynchronized-shared-mutation` is deliberately the
  narrower claim on top of it — a write on a *new thread* with *nothing
  serializing it* — so the two co-fire on the plain shape and only this one
  goes quiet once a lock appears. That is not a duplicate: the messages say
  different things, and the second is the one that tells a reader what to do.
  Their spans differ too, so the pair does not read as one finding printed
  twice.
- **No condition-system rules.** How a handler is *written* — a `handler-case`
  that swallows, a clause that cannot run, a `signal` that returns silently —
  is `feature/lint-safety`'s and `feature/lint-condition-system`'s.
  `thread-spawned-without-error-handler` is here rather than there because what
  makes the missing handler a defect is the thread boundary: the same body
  inlined at the call site signals to its caller and is unremarkable.
- **No general resource-cleanup rule.** `lock-acquired-not-released` carries
  `RuleCategory::Resource` and is a sibling of `unclosed-stream` in what it
  claims, but it lives here because its whole vocabulary — `acquire-lock`,
  `grab-mutex`, `release-lock`, `with-lock-held` — is threading's. A rule about
  file handles, sockets or foreign pointers is not this package's.
- **No whole-program or cross-file reasoning.** Every rule reads one file, one
  form at a time, and none calls `RuleContext::binding_table`, `value_table` or
  `type_table`. "Is `*counter*` really special?" needs the `defvar`, which may
  be in another file; the rules say *special-looking* and mean it.
- **No fixes.** See the next section — this is a property of every rule here,
  not a gap waiting to be filled.

## Every rule is `Fixability::ReportOnly`

Deliberately, and without exception. Repairing a concurrency defect means
choosing a synchronization strategy — which lock, held for how long, or an
atomic instead, or a redesign that stops sharing the cell. Every one of those
is a decision about the program, not a rewrite of a form, and a generated
answer would be a guess presented as a fix.

## Every rule is `HeadFilter::Heads`

Also without exception, and also deliberately. The `clean/forms/*` benchmarks
lint files with **zero** findings, which measures exactly the per-file cost a
rule pays when it matches nothing; a rule that walks the whole tree pays it on
every file. Each rule here declares the narrowest head set it can act on
(`make-thread`, `swap!`, `acquire-lock`, `with-lock-held`, …), allocates
nothing until a head has matched, and never touches
`RuleContext::binding_table()`, `value_table()` or `type_table()` at all — an
eager call to any of those rebuilds a whole-file semantic pass per file.

The two rules that need to know what encloses a node —
`lock-acquired-not-released` and `dynamic-var-bound-across-thread-boundary` —
use `support::with_ancestor_chain`, which descends from the root along the one
chain of nodes containing the target. That costs the node's *depth*, not the
file's size, and it runs only after the head has already matched.

## False negatives on purpose

A wrong finding about concurrent code is worse than no finding: it sends
someone to rewrite correct synchronization. Every rule here is scoped to a
shape it can establish and bails out when it cannot, and each module's
documentation has a "what it does not attempt" section listing what that costs.
The largest deliberate gaps:

- `thread-spawned-without-error-handler` ignores a thread body that is a single
  call, because that call is usually a function which handles its own errors
  and nothing visible at the spawn site can tell the two apart. It also does not
  cover Clojure at all — see below.
- `future-promise-never-realized` fires only when the bound symbol appears
  **nowhere** in the body — not merely when it is not dereferenced.
- `unsynchronized-shared-mutation` and
  `dynamic-var-bound-across-thread-boundary` only look inside a *literal*
  `lambda`, so `(make-thread #'worker)` is never flagged.
- `recursive-lock-reentry-risk` is a heuristic, says so in its module
  documentation, and phrases its finding as a risk rather than a proven
  deadlock. It also stops at any nested closure, missing a genuine same-thread
  reentry through an immediately-applied `lambda`.
- `atom-swap-with-side-effect` does not descend into a `fn`/`#(…)`/`delay`
  written inside the update, because those bodies are stored rather than run.

Eleven false positives found by an adversarial review against realistic correct
concurrent code were fixed rather than documented away; the regression test for
each one is in the relevant rule's `mod tests`, named for the shape it pins.
Two of them changed a rule's scope rather than a guard:

1. **`thread-spawned-without-error-handler` lost its Clojure half.**
   Dereferencing a future rethrows the stored exception in the calling thread,
   so `(let [users (future (fetch) (normalize))] {:users @users})` — a plain
   fan-out/join — is correct code with no handler in sight. The genuinely
   defective case is a future nobody reads, which `future-promise-never-realized`
   already covers, so the Clojure half was firing on precisely the correct half
   of that dichotomy. No rule in this package now covers more than one dialect.
2. **`unsynchronized-shared-mutation` learned about hand-held locks and local
   `with-…` wrappers.** It had been reporting the exact manual acquire/release
   idiom that `lock-acquired-not-released` endorses, so the package contradicted
   itself; and it only knew the library's own macro names, so every project that
   wraps its lock in `(with-registry-lock …)` would have seen the wrapper's
   every use reported.

## Library vocabulary, as verified

The spellings the rules match were checked against the libraries that define
them rather than assumed:

- **`bordeaux-threads`** — `make-thread`, `with-lock-held`,
  `with-recursive-lock-held`, `acquire-lock`, `release-lock`.
- **`sb-thread` / `sb-ext`** — `make-thread`, `with-mutex`,
  `with-recursive-lock`, `grab-mutex`, `release-mutex`,
  `with-locked-hash-table`.
- **Clojure** — `future`, `future-call`, `promise`, `delay`, `deref`, `swap!`,
  `swap-vals!`, `reset!`, `atom`, `alter`, `commute`, `locking`.

Two corrections came out of that check, and both changed a rule:

1. **`locking` is reentrant.** It compiles to a JVM `monitorenter`, and JVM
   monitors may be re-entered by the thread that holds them, so
   `(locking o (locking o …))` is correct Clojure.
   `recursive-lock-reentry-risk` therefore excludes it and is Common Lisp only.
2. **`force` is for `delay`, not for futures and promises.** Clojure reads a
   future or a promise with `@`/`deref`; `force` realizes a `delay`.
   `future-promise-never-realized` does not depend on the distinction — any
   mention of the symbol silences it — but `delay` is in its constructor list
   for the property it actually shares.

## Known engine gap: Clojure namespace qualifiers

`paredit_core_syntax::view_query::unqualified` strips the Common Lisp `:`
package marker but not Clojure's `/`, so `clojure.core/swap!` does not
normalize to `swap!` and neither the engine's head index nor this package's
comparisons match it. That is engine-wide rather than local to these rules; the
effect here is a false negative on fully qualified core calls, which is the
direction this package errs in throughout.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-lint-engine` | `LintRule`, `RuleMeta`, `HeadFilter`, `RuleContext`, and `RuleDialectScope` — which two rules here are the codebase's first users of in its `CLOJURE_ONLY` form. |
| `paredit-core-syntax` | Rules match on parsed forms, on `bordeaux-threads`/`sb-thread` operator spelling, and on Clojure's `[…]` binding vectors, which the dialect-aware parser is what distinguishes. |
| `paredit-core-cli` | Input reading, shared argument types, the report envelope. |
| `clap`, `serde_json` | Arguments and JSON output for each rule's own subcommand. |

## Layout

One rule, one directory — the four files a rule is made of, plus one shared
module:

```text
src/
├── lib.rs               the module list, plus `engine_pass_tests`
├── support.rs           quote-aware traversal, ancestor chains, threading vocabulary
└── <rule>/
    ├── rule.rs          META, RULE, the head filter, the dialect scope
    ├── domain.rs        the detection itself
    ├── usecase.rs
    └── cli/             the `inspect <rule>` subcommand
```

`support.rs` exists for three separate reasons, and each one is a thing that
goes wrong when a rule does it privately:

1. **What counts as code.** The quote machinery
   (`for_each_evaluated_subview`, `is_unevaluated_at`) is a deliberate
   byte-for-byte **copy** of `feature/lint-condition-system`'s, not a
   dependency on it: two feature packages must not couple (§4.2). Copied
   exactly because the two subtleties are ones a hand-rolled version keeps
   getting wrong — `'` and `` ` `` are two counters, not one, and the verdict
   is read *at* the node rather than from its own `reader_prefixes`. The
   dispatcher hands a rule head-matched nodes inside `'(…)` like any other, so
   without this every quoted example in a macro's documentation is a finding.
2. **Where a node sits.** `with_ancestor_chain` descends from the root along
   the one chain containing a target span, for the two rules that must know
   what encloses them. It costs the node's *depth*, not the file's size, and it
   is called only after a head has already matched.
3. **One threading vocabulary.** `MUTATOR_HEADS`, `LOCK_SCOPE_HEADS`,
   `MANUAL_ACQUIRE_HEADS`, `is_lock_scope`, `locked_designator` and
   `looks_special` are shared so that no two rules can disagree about whether a
   form takes a lock or a name is special — which is exactly how a package
   comes to contradict itself, and did once before the adversarial round.

`looks_special` is additionally a copy of `feature/lint-safety`'s, for the same
"no feature→feature dependency" reason; its doc comment names the other copy and
says the two must be changed together.

`lib.rs`'s `engine_pass_tests` builds a local `RuleCatalog` over all seven rules
and drives the real dispatcher. It is the only thing that exercises the two
declarations a `domain.rs` test cannot see — the `HeadFilter::Heads` list and
the `RuleDialectScope` — and a wrong head list is otherwise invisible: every
`examine_*` test stays green while the rule never receives a node from the CLI.

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| adding a rule about threads, locks, futures, promises or atoms | it is a new slice here, plus one line in the root's REGISTRY, plus an entry in `lib.rs`'s `engine_pass_tests` |
| changing what one of the seven flags, or how it phrases it | that rule's `domain.rs` |
| changing which forms a rule is shown, or which dialects it runs in | that rule's `rule.rs` — and `engine_pass_tests`, which pins both |
| teaching the rules a new lock macro, spawn form or mutating operator | `support.rs`'s vocabulary tables, so every rule learns it at once |
| changing what "special-looking" means | `support.rs`'s `looks_special` **and** `feature/lint-safety`'s copy of it |

| You are… | and it does **not** belong here because… |
| --- | --- |
| flagging a global write with no thread and no lock condition | that is `feature/lint-safety`'s `global-mutation-in-function` |
| writing a rule about how a handler is written | that is `feature/lint-safety` / `feature/lint-condition-system` |
| writing a rule about a non-lock resource that leaks | that is the `Resource` category's home for streams and handles, not this package |
| changing how rules are dispatched, ordered, or scoped by dialect | that is `core/lint-engine` |
| changing `inspect lint` itself | that is the root, which owns the registry |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
