# paredit-core-syntax

Lisp syntax kernel: parsing, dialects, Common Lisp forms, and definition shapes.

## Responsibilities

Everything the rest of the workspace needs in order to read Lisp source as a
structure rather than as text:

- **Parsing and the tree.** `SyntaxTree`, the borrowed `ExpressionView`
  cursor, byte-accurate `ByteSpan`/`ByteOffset`, and `ExpressionPath`
  addressing. Balanced edits that preserve delimiters live here too.
- **Dialects.** Which dialect a file is, what its reader macros mean, and which
  semantic policies it permits.
- **Common Lisp form knowledge.** Operator classification, binding forms,
  reader conditionals, labels and literals, package designators.
- **Definition shapes.** What counts as a definition, its category, and the
  span of a macro expander's body.
- **Small shared queries.** Structural equality, form shape, leading trivia,
  subview traversal, and a generic Tarjan SCC helper.

### What this package does not own

- **No report, lint rule, or refactoring operation.** It answers "what is this
  code?", never "what is wrong with it?" or "how should it change?". Anything
  producing a finding or a plan belongs in a `feature/*` package.
- **No CLI knowledge.** `clap` must never appear here, and no type in this
  package may exist to serve an output format. Argument parsing, rendering and
  exit codes are the composition root's business.
- **No filesystem or workspace discovery.** It is handed source text; it never
  goes looking for it.
- **No lint engine.** The rule trait and its single-pass runner are a separate
  concern, and the rule *registry* deliberately lives in neither.

### Why this package is larger than the spec's `core/sexpr`

`SPEC-package-by-feature.md` §5.1 planned `core/sexpr` as a dependency-free
leaf with `core/dialect` layered above it, and `definition` further up in
`core/semantics`. Measurement contradicts that: `sexpr`, `dialect`,
`common_lisp` and `definition` form a single strongly connected component.
`sexpr` needs `Dialect` and the Common Lisp operator helpers; `common_lisp` and
`dialect` need `SyntaxTree`, `ExpressionView` and `DefinitionCategory` back.
Cargo cannot express a dependency cycle, so the whole component is one package.

It is named `syntax` rather than `sexpr` because it owns dialect and Common
Lisp knowledge, not just s-expressions.

## Dependencies

| Crate | Why |
| --- | --- |
| `anyhow` | Fallible parse and edit paths still return `anyhow::Result`. This is a carry-over, not a design choice: §9.2 of the migration spec replaces it with `thiserror` enums so callers can match on failures instead of reading strings. Until then it stays, to keep the move reviewable. |
| `thiserror` | The typed errors that already exist, and the target shape for the rest. |
| `proptest` (dev) | Round-trip and re-parse properties over generated source. |

No `clap`, no `cap-std`, no `serde_json`: a dependency here that implies
delivery or I/O means something has been put in the wrong package.

## Public API

Entry points other packages actually use, in rough order of traffic (~1,580
references from the rest of the tree):

| Module | Principal items |
| --- | --- |
| `sexpr` | `SyntaxTree`, `ExpressionView`, `ExpressionKind`, `ExpressionPath`, `ByteSpan`, `ByteOffset`, `SymbolName`, `Delimiter`, `ReaderPrefix`, the `formatter` and `reader` submodules |
| `dialect` | `Dialect` (`CommonLisp`, `EmacsLisp`, … , `Unknown`), `Dialect::from_extension`, `VerifiedSemanticPolicy`, the per-operation shape types (`BinderShape`, `BodyShape`, `ParameterShape`) |
| `common_lisp` | `CommonLispOperator`, `common_lisp_operator_head_eq`, `common_lisp_symbol_reference_eq`, `normalize_common_lisp_operator_head`, the binding/reader form predicates |
| `definition` | `DefinitionCategory`, `DefinitionShape`, `definition_shape`, `macro_expander_body_range` |
| `view_query` | `for_each_subview`, `list_head`, `atom_text`, `atom_child`, `is_paren_list` |
| `expression_equality` | `expressions_structurally_equal`, `render_expression` |
| `form_shape` | `FormShape`, `duplicate_shape` |
| `graph` | `tarjan_scc`, `string_edge_cycles` |
| `leading_trivia` | `first_newline_or`, `strip_leading_blank_lines` |

`#[non_exhaustive]` is deliberately not used on any public enum here. Every
member of this workspace is `publish = false` and consumed only internally, and
`non_exhaustive` would defeat the exhaustiveness checking that the migration is
specifically trying to gain (§9.4).

Note that the root `paredit-cli` crate re-exports these modules as
`domain::sexpr`, `domain::dialect` and so on, so the published library API is
unchanged by the move.

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| supporting a new reader macro or literal syntax | the reader and its dialect policy are here |
| adding a dialect, or changing file-extension detection | `Dialect` and `from_extension` are here — the `lispIncludes` list in `flake.nix` is checked against it by a contract test |
| teaching the parser about a new binding or definition form | `common_lisp` and `definition` classify forms |
| fixing a span, path or offset that points at the wrong bytes | all positional types are here |
| making an edit preserve delimiters or whitespace correctly | balanced edits and `leading_trivia` are here |

| You are… | and it does **not** belong here because… |
| --- | --- |
| adding a lint rule | rules live in a `feature/lint-*` package; this package must not know a rule exists |
| adding a report or a refactoring | those are feature packages built on top of this one |
| adding a CLI flag or changing JSON output | that is the composition root |
| making this package depend on a `paredit-feature-*` crate | that inverts the dependency direction; a contract test rejects it |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
