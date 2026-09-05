# paredit-feature-lint-hy-depth

Lint rules for Hy based on the semantics of its generated Python AST.

## Scope

Related checks already live elsewhere:

| Python rule | Status | Where |
| --- | --- | --- |
| Ruff `E722` bare `except` | **taken** | `lint-hy-lfe-idiom::bare_except` |
| Ruff `B006` mutable default argument | **taken** | `lint-hy-lfe-idiom::mutable_default_argument` |
| CPython `SyntaxWarning`, `is` against a literal | **taken** | `lint-hy-lfe-idiom::identity_comparison_with_literal` |
| Leftover `print` debugging | **taken** | `lint-repl-debug::leftover_print_debug` |
| `hy.gensym` hygiene / `defmacro` capture | **taken** | `lisp-analysis::macro_hygiene_report` |

That last row is the one that does not show up in a rule-name scan, because it
is a *report* rather than a lint rule. `macro_hygiene_report` already knows Hy's
gensym spellings (`hy.gensym`, `gensym`), Hy's binding forms (`let`, `with`,
`for`) and reports `HygieneRisk::VariableCapture`. A macro hygiene lint rule
here would restate it.

Two other candidates are omitted:

- **Threading-macro arity (`->`, `->>`).** Not viable inside a macro template,
  which is where the interesting cases are. See the quoting note below.
- **Ruff `F403` `(import mod *)`.** Expressible — `*` arrives as a plain atom —
  but the audit corpus offers almost nothing for it to judge, and `import *` at
  module scope is a deliberate act rather than a mistake.

## The rule

| Rule | Category | Severity | Fixability | `Heads` | `dialect_scope` |
| --- | --- | --- | --- | --- | --- |
| `hy-unreachable-except-clause` | `Conditions` | `Error` | `ReportOnly` | `["try"]` | `[Hy]` |

An `except` clause whose exception type an earlier clause in the same `try`
already catches. Python runs the *first* handler whose type the exception is an
instance of, so a clause on a supertype makes every later clause it covers dead.
This is `pylint`'s `E0701 bad-except-order` and `W0705 duplicate-except`. Neither
Hy nor CPython rejects the shape — it compiles, it runs, and the branch is simply
never taken.

### Why it anchors on `try` and not on `except`

Reachability is a property of a clause's *position among its siblings*, and a
rule handed one `except` at a time cannot see it. That is also what makes this
non-overlapping with `hy-bare-except`, which anchors on `except` because breadth
is a property of one clause alone. The two report different spans:
`hy-bare-except` reports the over-broad clause, this reports the clauses that
clause kills.

### What it deliberately declines

Only Python's own builtin hierarchy is known here, as 55 `(subtype, supertype)`
edges with transitivity computed rather than listed. A clause on a project's own
exception class is never called shadowed by another *named* type, because this
layer cannot see what it inherits from. In particular an earlier `Exception` is
**not** treated as covering a user-defined class — such a class may derive from
`BaseException` directly, in which case `except Exception` does not catch it and
the later clause is live.

The two exceptions are sound rather than convenient: a bare `(except [] …)` and
an explicit `BaseException` really do catch every exception there is, so they
shadow even a class this layer has never heard of.

`IOError`, `EnvironmentError` and `WindowsError` are canonicalized to `OSError`.
They are not subclasses of it, they *are* it, so the relation has to hold in both
directions; a subtype edge would have caught only one.

## The third-party false-positive audit

Author-written fixtures encode the author's model of the language, not the
language, so the package is swept over real Hy:

```text
PAREDIT_HY_CORPUS=/path/to/corpus cargo test -p paredit-feature-lint-hy-depth \
  --test corpus_audit -- --ignored --nocapture
```

Over 3698 `.hy` files from `hylang/hy`, `hylang/hyrule` and ~40 third-party
repositories:

```text
files found                          3698
files parsed                         3594     (97.24%)
parse failures                        102

try forms                            1372
try with >= 2 except clauses           73   <- the only ones that can earn a finding
except clauses                       1397
bare (except [] ...) clauses           53
clauses naming a tuple of types       136

hy-unreachable-except-clause            1
```

**One finding, adjudicated as a true positive.** In `atisharma/hyjinx`,
`hyjinx/api.hy:226`:

```hy
(try
  ...
  (except [ImportError])
  (except [ModuleNotFoundError])
  (except [ValueError]))
```

`ModuleNotFoundError` is a subclass of `ImportError`, so the second clause never
runs. Zero false positives.

The denominator is the part that makes this mean anything. 73 multi-clause `try`
forms is the number of opportunities the rule actually had; the 1372 `try` forms
figure would have inflated it into meaninglessness, since a single-clause `try`
can never earn a finding. The corpus test asserts the multi-clause count is
non-zero for exactly this reason — a clean sweep over no candidates is a false
clean, not a pass.

## Hy's parse rate, and two reader gaps

The audit's 102 parse failures were re-checked against **Hy 1.3.1's own reader**
(`hy.read_many`) as an oracle. 100 of them are files Hy itself rejects — broken,
generated, or not Hy at all (one is a GPL licence text with a `.hy` extension;
one is Hy's own `compiler_error.hy` test resource, deliberately malformed).

**The real parse rate against valid Hy is 3594/3596, and the two residual
over-refusals do *not* share a cause.** Both are in `packages/core/syntax`,
outside this crate.

### Gap 1 — `~` immediately followed by an f-string

```text
~f"+{c}"                 refused: UnterminatedString    Hy: (unquote (FString …))
~@f"+{c}"                refused: UnterminatedString
`(deftile ~f"+{c}" X)    refused
```

Minimised against this workspace's reader, with the discriminating neighbours:

| input | result |
| --- | --- |
| `f"+{c}"` | accepted |
| `~f"abc"` (no interpolation) | accepted |
| `~"abc"`, `~b"abc"`, `~r"abc"` | accepted |
| `'f"+{c}"`, `` `f"+{c}" `` | accepted |
| `~ f"+{c}"` (with a space) | accepted |
| `~f"+{c}"` | **refused** |

So the trigger is exactly an unquote sigil adjacent to an f-string that contains
an interpolation. `'` and `` ` `` are real Hy reader prefixes and are consumed as
such, after which the f-string is scanned fresh; `~` is not a prefix (see below),
so it becomes part of the atom and the string-prefix scan sees `~f` rather than
`f`. This accounts for both refused forms in `hylang/simalq`, which is Hy's own
ecosystem, and the construct is idiomatic in Hy macros that build names.

### Gap 2 — `'` does not terminate a symbol

Hy's `NON_IDENT` is ``set("()[]{};\"'`~")`` and lists `'`, so a trailing quote
ends the symbol and begins a new reader macro. This reader keeps it:

```text
'Pebaz' "tail"
  Hy       -> (quote Pebaz), (quote "tail")
  paredit  -> one atom `'Pebaz'`, one atom `"tail"`
```

Hy refuses `'abc'` on its own with `PrematureEndOfInput` — proof that the
trailing `'` is a dangling quote prefix — while this reader accepts it as a
single atom. In isolation that is an *under*-refusal, but at file scale it
inverts the string phase and turns into an over-refusal: `Pebaz_j2do/j2do.hy`
has a module docstring containing an unescaped `"`, and the two readers
resynchronize differently from there.

### Hy's `~` is not a reader prefix, and that is deliberate

`reader_policy.rs::classify_hy` does **not** map `~` to `ReaderPrefix::Unquote`.
The arms that look like Hy's at `reader_policy.rs:892` belong to
`classify_clojure`. Upstream's comment records that the change was implemented
and measured and then withheld: several formatter paths open a child list
without writing its reader prefixes, so a prefixed list is emitted with its
prefix deleted, and enabling `~` would aim that straight at Hy macro bodies —
measured as 14 files changing meaning over the 2825-file corpus.

The consequence for every rule here, verified by parsing rather than assumed:
`` ` `` *is* `ReaderPrefix::Quasiquote`, so the quote counter counts **up** and
never **down** for Hy. Everything textually inside a Hy quasiquote reads as data,
and **no rule in this package can fire inside a macro template**. That is a
false-*negative* direction, so it is safe to ship against; it is a limitation to
be aware of, not a bug in this package.

One thing that does *not* follow, and which the sibling Carp package could not
rely on: `~name` and `~@body` scan as **single atoms** (`"~name"`), so they add
no sibling and do not inflate the enclosing form's arity. Only `~(` — a tilde
immediately before a delimiter — produces a bare `~` atom. Arity counting inside
a Hy template is therefore sound except across `~(`.

### The quote model should move

`support.rs` carries its own copy of the two-counter `QuoteState`
(`hard: bool` + `quasi: u32`) from
`packages/feature/lint-condition-system/src/support.rs`, as the other packages
do. A single `i32` depth counter cannot distinguish hard quotes from nested
quasiquotes and therefore produces false positives. Any shared implementation
must preserve the `~(`-produces-a-bare-`~` detail.

## Cost

```text
cargo test -p paredit-feature-lint-hy-depth --release --lib cost_ -- --ignored --nocapture
```

Measured in release, with the shipped `lint-hy-lfe-idiom` rules timed in the
**same pass** via a temporary dev-dependency (removed again: a feature-to-feature
edge needs an entry in the dependency allowlist contract, and a scratch benchmark
does not earn one).

```text
rule                                   ns/call@x1  ns/call@x8   ratio  calls@x8
cost-control-noop                              34          33    0.95      2000
hy-bare-except              (shipped)          40          35    0.87      1200
hy-mutable-default-argument (shipped)         476          40    0.08       400
hy-unreachable-except-clause                  824         622    0.75       400
```

`uptime` during the run reported a load average between 45 and 120 — this
workspace runs many agents at once — so the absolute numbers are noise-dominated
and the `@x1` column, with only 50 invocations, is unreliable. **The ratio is the
result**: 0.75 across an 8× range of file sizes is flat per invocation. Work that
is linear per call, and so quadratic per file, shows ≈8 here; that shape is what
got rules dropped from two sibling packages.

Two things keep the number where it is:

- `is_unevaluated_at` — the only call that touches an ancestor walk — runs
  **after** the findings are otherwise ready, never before the shape checks. A
  sibling package measured 450843 ns/call against 28 ns/call purely from that
  ordering.
- `Caught` borrows its type names out of the tree instead of allocating a
  `String` each. That change alone took the rule from 835 to 622 ns/call.

The benchmark corpus gives every `try` a three-clause chain on purpose, so this
measures the comparison path. Real code is much cheaper: of 1372 `try` forms in
the audit corpus only 73 had two or more clauses, so ~95% of real invocations
return at the `clauses.len() < 2` check before allocating anything.

## Mutation testing

Every guard was removed, rebuilt, run, and restored. Eleven of twelve are killed
by at least one test:

| mutation | tests killed |
| --- | --- |
| drop `dialect_scope()` override | 9 |
| drop `is_unevaluated_at` guard | 1 |
| `is_call` accepts reader prefixes | 1 |
| drop `BaseException` catch-all in `is_subtype_of` | 1 |
| drop alias canonicalization | 1 |
| drop the `except`-head filter | 7 |
| `covers`: `Unknown` treated as covering | 1 |
| `caught_by`: `BaseException` not a catch-all | 1 |
| `HeadFilter` loses its only head | 2 |
| multi-type coverage: `any` instead of `all` | 1 |

Two survivors were chased rather than left:

- **The explicit `Caught::Unknown` skip in `examine_try` killed nothing, and was
  deleted.** It was a second copy of a guard that lives in `covers`, which does
  kill a test. Duplicated guards are how the sibling packages' two-walk bugs got
  in.
- **The byte-exact `hy_head(view) != Some("try")` comparison kills nothing, and
  was kept.** `head_key` returns the head verbatim for every dialect except
  Common Lisp (`head_index.rs:81-86`), so `(TRY …)` is keyed `TRY` and never
  arrives, and the index does not offer `#(try …)` either. It stays because the
  index documents itself as a pre-filter that may be *wider* than a rule's notion
  of the operator: a rule that took the index's word for which operator it had
  would be correct only by accident of the dispatcher's current shape. The
  sibling Hy package keeps the same guard, for the same reason, with the same
  mutation result recorded.

Counts are from a `--no-fail-fast` run. Without it `cargo test` stops at the
first failing binary and every figure is a lower bound.

## Registration

This package is deliberately **not** registered. `src/lint/registry/**` names
each rule's `META` and `RULE` across the crate boundary, and a separate wiring
pass handles it; adding a rule moves six pinned counts plus the lint goldens.
The integration tests build their own `RuleCatalog` in `tests/support/mod.rs` and
run it through the real dispatcher, which is what proves the `HeadFilter` and the
`dialect_scope` are right — calling `examine_try` directly bypasses both.
