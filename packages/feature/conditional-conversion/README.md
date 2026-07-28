# paredit-feature-conditional-conversion

Converting between equivalent conditional spellings.

## Responsibilities

Six conversions and the shared shape logic behind them, all answering the same
question: this branch could be written another way — rewrite it, without
changing what it does.

- **`convert-if-to-when` / `convert-when-to-if`**
- **`convert-if-to-unless` / `convert-unless-to-if`**
- **`convert-if-to-cond` / `convert-cond-to-if`**
- **`conditional_sugar`** — the shared reasoning about which conditional forms
  are interconvertible and under what conditions.

The conversions are only safe in one direction at a time. `if` becomes `when`
only when there is no else-branch; `cond` becomes `if` only when there is a
single clause plus a default. Each slice states its own precondition, and
`conditional_sugar` holds what they share.

### What this package does not own

- **No conditional linting.** `one-armed-if`, `if-to-unless`, `if-to-or`,
  `single-clause-cond` and thirty others are `feature/lint-conditional`'s. A
  rule that *reports* a convertible conditional and a command that *converts*
  one are different products, which is why they are separate packages even
  though they recognise the same shapes.
- **No general form reshaping.** Threading, unwrapping and replacement are
  `feature/form-transform`'s.

### Why this is its own package

§5.2.1 lists these six under F10 `form-transform`. F10 closed without them, and
they form a coherent group on their own: every one converts between two
spellings of a conditional, and they share `conditional_sugar`. Splitting them
out keeps F10 about reshaping call forms and this package about branch forms.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-syntax` | Conditionals are subtrees, and which head is a conditional is classified there. |
| `paredit-core-semantics` | Checking that a conversion does not change what a name in the branch refers to. |
| `paredit-core-edit` | Span replacement and the shared mutation-safety refusals. |
| `paredit-core-cli` | Input reading, atomic writes, shared argument types. |
| `clap`, `serde_json`, `anyhow`, `thiserror` | Arguments, JSON output, fallible paths. |
| `proptest` (dev) | Round-trip properties: converting and converting back must reproduce the input. |

## Public API

Two names per slice that owns a subcommand, per §4.2. `conditional_sugar`
publishes no command; it is the shared shape logic the six conversions call.

`#[non_exhaustive]` is deliberately absent (§9.4).

## Layout

Slice-first, per §3.1:

```text
src/
├── conditional_sugar/          shared shape logic, no command
├── convert_if_to_when/ … convert_when_to_if/
├── convert_if_to_unless/ … convert_unless_to_if/
└── convert_if_to_cond/ … convert_cond_to_if/
```

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| fixing a conversion that changes behaviour | the precondition lives in that slice's `domain` |
| adding a conversion between two conditional spellings | it is a new slice here |
| changing which forms count as interconvertible | `conditional_sugar` is the shared answer |

| You are… | and it does **not** belong here because… |
| --- | --- |
| adding a rule that reports a convertible conditional | that is `feature/lint-conditional` |
| reshaping a call rather than a branch | that is `feature/form-transform` |
| changing how a conditional head is classified | that is `core/syntax` |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
