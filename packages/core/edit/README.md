# paredit-core-edit

Refactor plans, previews, mutation safety, and binding-form edits.

## Responsibilities

The shared machinery every refactoring is built from — the vocabulary of
"what would change, how risky is it, and is it safe to touch":

- **Plans and risk.** `RefactorOperation`, `RefactorPlanSummary`,
  `RefactorRiskLevel` and the gate that turns a plan into a pass or a refusal.
- **Preview and execution.** Deciding what a preview reports, and applying a
  planned change to source text.
- **Mutation safety.** The refusals that protect correctness — most notably
  reader conditionals and overlapping read-time forms, where a textually
  reasonable edit changes what the reader sees.
- **Span editing primitives.** Checked span replacement and top-level form
  insertion, so no feature open-codes byte surgery.
- **Binding-form composition.** The shared shape logic behind `let`, `let*`,
  `flet`, `progn` and control-form conversion.

### What this package does not own

- **No user-facing refactoring.** `inline-function`, `extract-function`,
  `rename` and the rest are feature packages that *use* these primitives. This
  package must not know their names.
- **No impact reporting.** `impact_report` is a feature; the `From
  <ImpactRiskLevel> for RefactorRiskLevel` conversion deliberately lives on the
  feature side so the dependency runs feature → core, not the reverse.
- **No CLI.** `--write`, `--preview` and exit codes are the composition root's.
- **No scope analysis.** It asks `core/semantics` what a name refers to.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-syntax` | 33 references: spans, trees and delimiter-preserving edits. |
| `paredit-core-semantics` | 4 references: safety decisions need to know what a name binds. |
| `thiserror` | Every refusal is an `EditRefusal`. |
| `thiserror` | `ReaderConditionalSafetyError` and its siblings — the pattern §9.2 generalises. |
| `proptest` (dev) | Properties over generated binding forms. |

## Public API

| Module | Principal items |
| --- | --- |
| `refactor_plan` | `RefactorOperation`, `RefactorPlanSummary`, `RefactorRiskLevel`, `RawRefactorRisk`, `RefactorPlanGate`, `RefactorPlanTargetKind` |
| `refactor_preview` | `decide_refactor_preview`, `RefactorPreviewDecisionStatus` |
| `refactor_execute` | Plan application |
| `mutation_safety` | `reject_common_lisp_reader_conditionals`, `reject_overlapping_common_lisp_reader_time_forms`, `ReaderConditionalSafetyError` |
| `extract_shared` | `replace_span`, `replace_span_checked`, `insert_top_level_form`, `TopLevelInsert` |
| `let_composition`, `let_star_composition`, `flet_composition` | The `*Request` types behind merge/split of binding forms |
| `let_binding`, `local_function_binding`, `progn`, `convert_control` | Shape helpers used through the modules above |

The root crate re-exports `refactor_execute`, `refactor_plan` and
`refactor_preview` as `pub`, and the other nine as `pub(crate)`, mirroring
their original declarations.

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| adding a refusal that protects correctness across refactorings | `mutation_safety` is the one place a safety rule is stated once |
| changing how risk is computed or how a plan gate decides | `refactor_plan` owns that vocabulary |
| fixing an edit that corrupts spans or drops trivia | the span primitives are here |
| adding shared shape logic two refactorings both need | that is what the composition modules are for |

| You are… | and it does **not** belong here because… |
| --- | --- |
| implementing a specific refactoring command | that is a feature package built on these primitives |
| reporting on the impact of a change | `impact_report` is a feature; core must not name it |
| deciding an exit code | that is the composition root |

Adding a dependency to `Cargo.toml` means adding a row to the table above.

## Refusals

The 107 refusal messages in this package normalise to 64 shapes, and 59% of
the messages are the **same 20 shapes** repeated across seven edit families —
`convert-*`, `merge-nested-*`, `split-let*`, `flatten-progn`,
`eliminate-empty-binding-form` — differing only in the operation name pasted
into the string.

So the error types are organised by **reason**, not by operation:

| Type | The edit refuses because |
| --- | --- |
| `DialectRefusal` | It is not defined for this dialect |
| `DocumentRefusal` | The input, or its own output, does not parse |
| `ConservativeRefusal` | The form carries comments, reader prefixes, or declarations it would have to move blindly |
| `ShapeRefusal` | The selected form is not the shape it operates on |
| `BindingRefusal` | The binding list, or a binding in it, is not rewritable — including the capture cases |
| `LocalFunctionRefusal` | An `flet`/`labels` definition is not rewritable |
| `InsertionRefusal` | The extracted form has nowhere to go |

Every variant carries `operation`, so "this edit family is conservative about
comments" is one match instead of six message prefixes. That is not new
plumbing: the code already threaded `operation: &str` through shared helpers
like `require_supported_dialect`. The parameter was there; only the type was
missing.

Shapes that differ only in wording (`rejects declarations` versus
`conservatively rejects declarations`) stay separate variants. Unifying them
would change CLI output, which is a behaviour change wearing a type change's
clothes.
