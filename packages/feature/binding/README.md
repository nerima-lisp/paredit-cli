# paredit-feature-binding

Reshaping `let`, `let*`, `flet`, `labels` and `progn` binding forms, and
reporting on what they bind.

## Responsibilities

Everything that reasons about a *binding form's shape* rather than about the
values flowing through it — both the transformations and the reports, because
they share the same reading of a binding list:

- **Introducing and reporting.** `introduce_let` lifts a repeated expression
  into a new binding; `let_report` describes the binding forms in a file;
  `shadowed_binding_report` finds a binding that reuses an enclosing
  parameter's or binding's name. The last of these is why this package reaches
  `paredit-feature-function-parameter`: deciding whether a `let` shadows a
  parameter needs the validated lambda-list parser that names parameters.
- **Splitting and merging.** `split_let`, `split_let_star`,
  `merge_nested_let`, `merge_nested_let_star`, `merge_nested_flet`.
- **Converting between forms.** `convert_let_to_let_star` and its inverse,
  `convert_flet_to_labels` and its inverse, `convert_sequential_binding` (which
  owns both `do*`→`do` and `prog*`→`prog`).
- **Removing and flattening.** `eliminate_empty_binding_form`, `flatten_progn`.

The hard part is uniformly the same: whether a rewrite changes what a name
refers to. Merging `let*` into `let` is only safe when no initializer depends
on an earlier binding; splitting is only safe when nothing captures across the
split point.

### What this package does not own

- **No lint rules.** `shadowed_binding_report`, `empty_let_report`,
  `duplicate_let_binding_report` and friends mention `let` but are *rules*, and
  belong to `feature/lint-*` packages. Grouping a rule with a refactoring
  because both say "let"
  would scatter the rule set for no benefit.
- **No unused-binding removal.** That is `feature/remove-unused`.
- **No scope analysis.** Whether a binding is captured is answered by
  `paredit-core-semantics`.
- **No binding-form shape helpers.** `let_composition`, `flet_composition` and
  `progn` live in `paredit-core-edit`, shared with any other feature that needs
  them.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-syntax` | Binding forms are subtrees, and Common Lisp binding-form classification decides what is a binder at all. |
| `paredit-core-semantics` | Every safety question here is a scope question: does this rewrite change what a name refers to? |
| `paredit-core-edit` | The `let`/`let*`/`flet`/`progn` composition helpers and the shared mutation-safety refusals. |
| `paredit-core-cli` | Input reading, atomic writes, shared argument types. |
| `clap` | Argument parsing, confined to each slice's `cli`. |
| `serde_json` | JSON output. |
| `anyhow` | Fallible planning paths, pending §9.2. |
| `thiserror` | Typed failures. |
| `proptest` (dev) | Round-trip properties: merging then splitting must reproduce the input. |

## Public API

Fourteen slices, thirteen published `(Args, run)` pairs — `convert_sequential_binding`
publishes two commands (`convert_do_star_to_do`, `convert_prog_star_to_prog`)
and one slice publishes none of its own. The count is read from what each
slice's `cli` actually defines rather than derived from the slice name, because
those two facts do not always agree.

`#[non_exhaustive]` is deliberately absent (§9.4).

## Layout

Slice-first, per §3.1 — fourteen slice directories, each with the layers it
actually has:

```text
src/
├── introduce_let/{domain.rs + domain/, usecase.rs, cli.rs}
├── let_report/{domain.rs + domain/, usecase.rs, cli.rs}
├── split_let/ … split_let_star/
├── merge_nested_let/ … merge_nested_let_star/ … merge_nested_flet/
├── convert_let_to_let_star/ … convert_let_star_to_let/
├── convert_flet_to_labels/ … convert_labels_to_flet/
├── convert_sequential_binding/
├── eliminate_empty_binding_form/
└── flatten_progn/
```

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| fixing a merge that changes what an initializer sees | that is the central safety question, and it lives in the slice's `domain` |
| fixing a split that breaks a capture | same |
| adding a conversion between two binding forms | it is a new slice here |
| adding a flag to any of these commands | the slice's `cli` |

| You are… | and it does **not** belong here because… |
| --- | --- |
| adding a rule that *reports* a bad binding | rules are `feature/lint-*`; this package rewrites, it does not judge |
| removing a binding because it is unused | that is `feature/remove-unused` |
| adding a shape helper another feature would also want | that is `core/edit` |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
