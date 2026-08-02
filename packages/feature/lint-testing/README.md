# paredit-feature-lint-testing

Lint rules for anti-patterns inside test definition forms.

## Responsibilities

Six rules about what is written *inside* a test definition — whether it asserts
anything, whether it can run at all, and whether its result means what a green
suite implies.

| Rule | Flags |
| --- | --- |
| `disabled-test-left-in` | a test switched off in place by a marker a framework actually honours |
| `duplicate-test-name` | two top-level test definitions in one file sharing a name, so the earlier never runs |
| `empty-test-body` | a test definition with no body, reported as a pass having checked nothing |
| `sleep-in-test` | a wall-clock sleep *sequenced* in a test body, which makes the result depend on machine load |
| `test-asserts-constant` | an assertion whose truth is settled by the source (`(is t)`, `(is (= 1 1))`) |
| `test-without-assertion` | a test whose body runs code but never asserts |

Every rule is `Fixability::ReportOnly`. What a test *should* assert, whether a
disabled test should come back or be deleted, and what a sleep should wait on
instead are all questions a rewrite cannot answer.

Every rule anchors on the *test definition* — never on the inner call — and
searches inward from there. A rule about `sleep` that filtered on `sleep` would
match every sleep in the program and then need ancestor context, which a
per-node predicate does not have.

Five of the six say that with `HeadFilter::Heads` over the test-definition
heads, so they run only once such a head has matched. `duplicate-test-name` is
`HeadFilter::WholeTree`, because whether one definition shadows another is not
a question about a single node: a per-node filter can only answer it by
re-deriving the file's other definitions on every match, which is quadratic in
the number of tests and was — a 4000-`deftest` file cost 13.8s against the rest
of the suite's 0.03s. It is handed the document once per file instead and
hashes each top-level name.

### The dialects, and the frameworks behind them

`RuleDialectScope` is Common Lisp, Emacs Lisp and Clojure. Every operator name
in this package was read out of the framework's own source rather than recalled,
because the guesses that look most obvious here are wrong:

| Framework | Defines | Asserts |
| --- | --- | --- |
| FiveAM | `def-test` (**not** `deftest`) | `is`, `is-true`, `is-false`, `is-every`, `signals`, `finishes`, `pass`, `fail`, `skip` |
| lisp-unit | `define-test` | `assert-equal`, `assert-true`, `assert-error`, … |
| `rt` / regression-test | `deftest` | *nothing* — it compares values positionally |
| ERT | `ert-deftest` | `should`, `should-not`, `should-error`, `ert-fail`, `ert-skip` |
| clojure.test | `deftest` | `is`, `are` |
| test.check | `defspec` | a generated property |

Two consequences are load-bearing:

- **`deftest` means different things in different dialects.** In Common Lisp it
  is `rt`'s positional test, which contains no assertion form *by design*; in
  Clojure it is a body of assertions. `TestKind::body_is_assertions` is what
  keeps `test-without-assertion` from firing on every `rt` test in a file.
- **FiveAM's `test` is not modelled.** It is the same macro as `def-test`, but
  `(test …)` is too generic a head to assume belongs to a test framework.

## What this package does not own

- **No registry.** `REGISTRY` stays in the root and names each rule's `META`
  and `RULE` across this boundary. A registry here would be the cycle §4.2
  exists to prevent.
- **No engine.** The single pass, head index and rule trait are
  `paredit-core-lint-engine`'s.
- **No `print-in-test` rule.** Asked for, and deliberately **not implemented**:
  `paredit-feature-lint-repl-debug`'s `leftover-print-debug` already flags bare
  debug prints in committed source across eight dialects, with no test-context
  restriction — so it already fires inside `deftest` bodies today. A second rule
  would report every one of those spans twice under a different name.
- **No `focused-test-left-in` rule.** Asked for, and **not implemented for lack
  of anything to detect**: no focus marker exists. FiveAM's `def-test` accepts
  exactly `:depends-on`, `:suite`, `:fixture`, `:compile-at` and `:profile`;
  ERT has no focus selector in source at all; and in Clojure, Kaocha's
  `--focus-meta` takes a user-chosen keyword with no shipped default, while
  eftest, Midje and `lein test` recognise no focus metadata whatsoever. A rule
  matching `^:focus` would be matching a convention, not a framework.
- **No `test-missing-teardown-pair` rule.** Asked for, and **dropped as
  unreliable**: teardown legitimately lives in an `unwind-protect`, a fixture
  macro, an ERT `cl-macrolet` wrapper, or a namespace-level
  `clojure.test/use-fixtures` — three of which are not in the test form at all.
  There is no framework-defined setup/teardown vocabulary to anchor on, so the
  rule would have had to guess at project-specific function names. The one
  Common Lisp pair that *is* nameable, `open`/`close`, is already
  `feature/lint-safety`'s `unclosed-stream`.
- **No coverage reporting.** Which definitions have tests at all is
  `feature/project-inventory`'s `inspect test-map`, which pairs tests to
  subjects by name and never looks inside a body.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-lint-engine` | `LintRule`, `RuleMeta`, `HeadFilter`, `RuleContext`, `NormalizedHead`. |
| `paredit-core-syntax` | Rules match on parsed forms and on per-dialect operator spelling. |
| `paredit-core-cli` | Input reading, shared argument types, the report envelope. |
| `clap`, `serde_json` | Arguments and JSON output for each rule's own subcommand. |

## Layout

One rule, one directory — the four files a rule is made of, plus one shared
module:

```text
src/
├── support.rs           quote-aware traversal, test-form reading, the operator tables
└── <rule>/
    ├── rule.rs          META, RULE, the head filter: what the registry registers
    ├── domain.rs        the detection itself
    ├── usecase.rs
    └── cli/             the `inspect <rule>` subcommand
```

`support.rs` exists because all six rules must agree on three things: what
counts as unevaluated data, which macros define a test, and where a test's body
starts. Its `QuoteState`/`for_each_evaluated_subview` pair is copied from
`paredit-feature-lint-condition-system`, tests included — a copy rather than a
dependency, because a feature→feature edge for a hundred lines of traversal is
not a trade worth making.

None of it runs per visited node, and none of it is quadratic in the number of
test definitions. The five `HeadFilter::Heads` rules are paid only once a
test-definition head has matched — which, in the `clean/forms/*` benchmarks
that lint files with no findings, is never — and each then reads only the
matched form's own subtree. `duplicate-test-name` is paid once per file, for
one pass over the top-level forms. No rule here calls `binding_table()`,
`value_table()` or `type_table()`.

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| adding a rule about what is inside a test body | it is a new slice here, plus one line in the root's REGISTRY |
| teaching the suite a new test framework's spelling | `support.rs`'s `TestKind`, `assertion_heads` and `read_test_form` |
| changing what one of the six flags, or how it phrases it | that rule's `domain.rs` |
| changing which forms a rule is shown | that rule's `rule.rs` head filter |

| You are… | and it does **not** belong here because… |
| --- | --- |
| flagging a debug print, anywhere | that is `feature/lint-repl-debug`'s `leftover-print-debug` |
| flagging a comparison of a value with itself | that is `feature/lint-numeric`'s `self-comparison` |
| flagging an empty `when`/`unless`/`dolist` body | that is `feature/lint-conditional`'s `empty-body` |
| asking which definitions have tests | that is `feature/project-inventory`'s `inspect test-map` |
| finding a definition redefined across files | that is `feature/project-analysis`'s `inspect redefinition` |
| changing how rules are dispatched or ordered | that is `core/lint-engine` |
| changing `inspect lint` itself | that is the root, which owns the registry |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
