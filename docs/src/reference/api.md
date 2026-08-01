# Command model

A source-facing command belongs to one of six namespaces. The first three
split by *what a change costs to undo*, which is the decision automation has
to make first:

- `paredit inspect` reads and reports without writing.
- `paredit edit` transforms one selected form; stdout by default, `--diff`
  for a unified diff, `--write` to update the file in place.
- `paredit refactor` plans, previews, verifies, and applies semantic changes.

The other three split by *what the caller is trying to do*, over a file set
rather than one form:

- `paredit query` searches, counts, and rewrites by S-expression pattern.
  The pattern language is the same one `--query` selects with; here it is the
  command rather than the selector, and its reach is the workspace.
- `paredit fix` applies the lint auto-fixes. Every leaf was already a flag on
  `inspect lint`; the older spellings still work. What changed is the address:
  a command that writes source now lives under a name that says it writes.
- `paredit migrate` runs a named, ordered, dialect-scoped codemod recipe.
  Where `query replace` takes one pattern from the command line, a recipe is a
  reviewed artifact with several steps in a fixed order and a declared scope.

Two namespaces sit outside both decisions because they report on the tool
rather than on source:

- `paredit config` inspects, validates, and scaffolds the layered
  `paredit.toml`. See [Configuration](configuration.md).
- `paredit completions <shell>` prints shell completion scripts for bash,
  zsh, fish, elvish, and powershell.

Four more top-level commands sit outside `inspect`/`edit`/`refactor`
entirely — long-running or interactive processes rather than one-shot
reports or edits: `paredit lsp` and `paredit serve` (see
[Integrations](../guide/integrations.md)), `paredit mcp` (see
[Agent interface](../guide/agents.md)), and `paredit tui`, an interactive tree
browser that prints a `--path` on exit (see
[Browsing interactively](selectors.md#browsing-interactively-paredit-tui)).

Run `paredit <namespace> --help` for the authoritative list on your installed
version, and `paredit <namespace> <command> --help` for each command's
contract, arguments, and output formats. For a machine-readable catalog of
the entire surface in one call, run:

```sh
paredit inspect capabilities --output json
```

Commands that take roots rather than explicit files share one set of input
selectors and filters — `--since`, `--from-git`, `--from-manifest`,
`--paths-from`, `--from-archive`, `--include`, `--exclude-glob`,
`--no-gitignore` and the rest. See [Choosing Files](workspace-inputs.md), and
run `paredit inspect sources` to see exactly which files a given combination
selects.

## Global options

Every command accepts the same top-level flags. `--dry-run`, `--progress`, and
the `--config`/`--no-config`/`--no-config-env` trio are covered in
[Run-wide controls](../guide/agents.md#run-wide-controls); `--timeout-ms` and the
`--max-*` budgets in [Bounding a run](../guide/safety.md#bounding-a-run); and
`--new-file-mode`/`--refuse-symlinked-ancestors` in
[Write permissions and symlinked ancestors](../guide/safety.md#write-permissions-and-symlinked-ancestors).
The remaining three are terminal presentation, not safety, and apply nowhere
else:

| Flag | What it does |
| --- | --- |
| `--color <auto\|always\|never>` | Whether text output may use ANSI color. `auto`, the default, colors if and only if the destination stream is a terminal, `NO_COLOR` is unset, and `CLICOLOR_FORCE` has not already decided the question. The hues it may use, and the rule that none of them ever carries a signal alone, are in [Terminal Color](color-palette.md). |
| `--paginate` | Delegates stdout to `$PAGER` (falling back to `less`) when it is a terminal. Off by default — unlike `--color`, paging changes the interaction itself, so it has to be asked for. |
| `--plain-language` | Adds one line to a text-mode failure paraphrasing its error category — the same `category_description` the `--output json` envelope has always carried. Off by default, so the existing stderr rendering is byte-for-byte unchanged. |

## Inspect

`paredit inspect` never writes source files. Prefer these commands for
discovery, impact analysis, and preflight checks.

| Command | Purpose |
| --- | --- |
| `check` | Validate that input is a balanced S-expression document. |
| `dialect` | Detect Lisp dialect from `--file` extension or explicit `--dialect`. |
| `stats` | Print parse, dialect, and structural metrics for agent planning. |
| `agent-report` | Print a complete JSON report for AI coding agent refactor planning. |
| `change` | Describe what changed between two versions of a file, as prose a pull request can use. |
| `capabilities` | Print a machine-readable catalog of every command, flag, default, and enum value. |
| `outline` | Print top-level forms with paths, spans, and definition hints. |
| `form` | Report one selected form with local structure for refactor planning. |
| `resolve` | Report which forms a selector names, with paths, line/column coordinates, stable selector ids, and pattern captures. Resolves `--query` / `--name` / `--line-column` / `--id` / `--from`+`--to` without acting on them. |
| `find-symbol` | Find exact atom occurrences without touching strings or comments. |
| `symbols` | Report exact atom occurrences across explicit files for rename planning. |
| `calls` | Report list-head call sites across explicit files for arity refactor planning. |
| `signature` | Compare callable definitions and call-site arity across explicit files. |
| `call-graph` | Report internal and optional external call graph edges. |
| `impact` | Report refactoring impact risks for one symbol across explicit files. |
| `workspace` | Discover Lisp sources under roots and report parse/refactor inventory. |
| `sources` | Report which files an analysis would select, and which rule dropped the rest. |
| `dependencies` | Report package, system, load, and qualified-symbol dependencies. |
| `packages` | Report Common Lisp package declarations across explicit files. |
| `definitions` | Report definition-like top-level forms across explicit files. |
| `unused-definitions` | Report definitions with no external exact atom references. |
| `duplicates` | Report repeated structural S-expression shapes across explicit files. |
| `diff` | Compare two documents by their parse rather than their lines: which forms were inserted, deleted, or replaced, and at what path. Whitespace, indentation, and comments are not part of the comparison, so a reformatted file reports no changes and an edited argument reports as that argument instead of as the whole wrapped line. `--max-depth` hides the deep edits and leaves the shape changes; `--fail-on-change` gates on the two documents differing structurally. **The blind spot is stated in every run's output:** an empty structural diff does not mean the files are identical, only that the programs are. |
| `similarity` | Report structurally similar S-expression forms across explicit files. |
| `clone-classes` | Group near-duplicate forms into clone classes, label each on the Type-1/2/3 taxonomy, and rank them by the lines extracting one would save. Where `similarity` reports pairs, this reports the thing there is to extract: five copies of one helper are one class, not ten pairs. |
| `clone-sequences` | Report duplicated runs of adjacent sibling forms — the sub-form clones that no whole-form report can see, because the duplication does not line up with a form boundary. Runs whose enclosing forms are themselves clones are left to `clone-classes`. |
| `clone-external` | Report project forms that duplicate a reference corpus, to find code a dependency already provides. Unlike `similarity` it compares across head symbols, since a local `join-strings` and a library `str:join` disagree on the head and are the point. The reference corpus is scanned *including* generated directories, because that is where dependencies live. |
| `clone-threshold` | Recommend a `--threshold` from the project's own similarity distribution instead of the built-in 0.87. Reports the histogram, an Otsu split, the widest distribution gap, and percentiles, so the recommendation can be judged rather than taken. |
| `clone-genealogy` | Order each clone class by the commit that introduced each member, separating the original from the copies and reporting how long the copying went on. Degrades like `blame`: no repository, no `git`, or an untracked file yields `unknown`, never a fabricated date. |
| `lets` | Report local let bindings and inline safety for refactor planning. |
| `complexity` | Report per-definition nesting depth and size metrics for refactor prioritization. |
| `naming` | Report definition names that deviate from idiomatic kebab-case Lisp naming. |
| `reachability` | Report callable definitions unreachable from any entry point in the internal call graph. |
| `unused-parameters` | Report declared function parameters with no unshadowed reference in their body. |
| `shadowed-bindings` | Report let-family bindings that shadow an enclosing parameter or let binding. |
| `unused-local-callables` | Report flet/labels local callables never called anywhere in their visible scope. |
| `package-boundaries` | Report `package::symbol` references that reach into another package's internal symbols. |
| `call-cycles` | Report strongly connected cycles of two or more definitions in the internal call graph. |
| `package-cycles` | Report defpackage :use/:import-from cycles across two or more packages. |
| `system-cycles` | Report ASDF defsystem :depends-on cycles across two or more systems. |
| `unused-packages` | Report defpackage declarations never used, imported-from, or reached by a qualified symbol. |
| `unused-exports` | Report defpackage :export symbols never reached by a qualified symbol reference. |
| `duplicate-exports` | Report defpackage forms that export the same symbol more than once. |
| `unused-nicknames` | Report defpackage :nicknames never used as a qualifier anywhere. |
| `package-conflicts` | Report distinct defpackage forms that claim the same package name or nickname. |
| `redefinitions` | Report top-level definitions of the same category and name declared more than once. |
| `undefined-packages` | Report in-package forms naming a package no analyzed defpackage declares. |
| `context-at` | Report what kind of text sits at a byte offset — code, a string, a comment, a list delimiter, reader sugar, or the whitespace between forms — together with the enclosing list, the nesting depth, and the stack of open delimiters. The question to ask *before* a character edit rather than after a refused one: `edit delete-forward` and `edit newline` decline every offset this reports as carrying structure, and `--fail-on-structural` turns that into an exit code. |
| `writability` | Report whether a write to `--file` would succeed — right now, without writing anything — by staging a same-size placeholder exactly as a real write would (so a full disk fails this the same way it would fail the real write) and discarding it instead of publishing it. `--file` need not exist yet. The answer `--dry-run` cannot give: `--dry-run` refuses every write unconditionally and says nothing about whether it would have worked. |
| `data-check` | Report schema-free structural sanity issues in an S-expression *data* file: a plist or alist with the same key spelled twice (the later value silently overrides the earlier one), a plist with a trailing keyword and no value, and a top-level list of same-shaped tuples with one entry whose arity does not match its siblings. No schema is read or required — each check fires only on a shape the file's own repetition already implies, conservatively, so a real mismatch is worth trusting. Format-specific detection is a later addition; today only these baseline checks run. |
| `kill-ring` | Diagnose the kill ring file `edit kill`/`edit copy --to-ring`/`edit yank` share, without touching it: invalid JSON, a shape missing the `entries` array or `schema_version` field, or a `schema_version` this build does not recognise are each reported by name, alongside a well-formed ring's entry count. A missing file is not corruption — that is `edit yank`'s own "empty ring" convention. `--repair-reset` discards a corrupted ring and writes a fresh empty one in its place; it never touches a missing or well-formed ring, and never runs unless passed explicitly. |
| `api-surface` | Report every exported symbol with the signature its export commits to — the defining category, the required and maximum arity, and the lambda list as written. `defpackage`'s `:export` is a list of names; what a caller relies on is those names *plus their shapes*, and that pairing exists nowhere in the source. An export nothing defines is reported rather than dropped: that is usually a rename that missed one side. |
| `api-diff` | Compare the current API against a `--baseline` `api-surface` snapshot and answer the SemVer question mechanically. Breaking: an export removed, a minimum arity raised, a maximum lowered, or a defining category changed. Compatible: an export added or a range widened. `--intended-bump` fails the run when the diff requires a larger bump than the release claims. |
| `test-map` | Pair definitions with the tests that name them, by the `test-x` / `x-test` / `x-tests` conventions, and report both sides that have no counterpart — untested definitions and tests nobody can tell what they cover. A list of tests and a list of definitions are each easy to get; neither answers the question. |
| `symbol-index` | Index every symbol to its definition site, its category, and the byte offset of every occurrence. Built for a consumer that will ask thousands of "where is this defined" questions and should not re-parse for each one. Symbols nothing analyzed defines are reported as external, which makes the index also an answer to "what does this file depend on". |
| `keyword-arity` | Check call sites against `&optional`, `&rest`, and `&key` lambda lists. `signature` compares positional counts, which cannot express "accepts one, three, or five arguments and rejects two" — and cannot see that a call passing `:widht` to a function taking `:width` is wrong, because the argument *count* is right. |
| `unreachable-expressions` | Report forms that cannot run because a `return-from`, `go`, `throw`, or `error` precedes them in the same implicit progn. `reachability` answers this between definitions; this answers it inside one, which is where it hides. An exit inside an `if` branch is correctly *not* treated as killing the following form. |
| `external-diagnostics` | Compile each file with an external Lisp implementation (`--implementation sbcl`) and report the implementation's own diagnostics as findings, placed at the definition it named. `--save-baseline` records a run and `--baseline` marks the diagnostics absent from it, so a refactor can be gated on what it *introduced* rather than on the whole set. **Compiling is executing**: `compile-file` runs the file's macros, its `eval-when (:compile-toplevel)` forms, and its `#.` read-time evaluation, which is why `--implementation` has no default. The fasl goes to a temporary directory. |
| `external-systems` | Report which ASDF systems this project depends on but does not define — an SBOM, in effect. Reads both `:depends-on` spellings, including the `(:version …)` and `(:feature …)` forms whose system name is not in first position. Internal dependencies are reported too, so the output is a complete account of the graph. |
| `licenses` | Report each `defsystem`'s declared licence and its copyleft strength (permissive, weak, strong, unknown, undeclared), and flag systems whose licence is superseded by a stronger one in the same file — an MIT system beside a GPL one ships as GPL. An unrecognised licence is reported as unknown, never assumed permissive. Not legal advice. |
| `serial-consistency` | Report components whose `:depends-on` contradicts their system's `:serial t` — a dependency on a *later* sibling, which the serial order cannot satisfy — or merely duplicates it. Also flags a non-serial system whose components declare no dependencies at all, where the file order everyone assumes is not a guarantee. |
| `blame` | Report the last author, date, and commit for each definition, so any other report's finding can be routed to someone. Attribution is per definition rather than per line, taking the most recent line in the span. Degrades like `hotspots`: when git cannot answer it says so rather than emitting an empty author. |
| `duplication-ratio` | Report what fraction of a file is structurally repeated, as an integer per mille, plus each repeated shape with its occurrence count and redundant bytes. `duplicates` says *which* forms repeat; this says whether the tree is 3% repeated or 30%, which is the number a decision gets made on. Matching is exact structural equality with identifiers erased, so the ratio does not move when a similarity threshold does. |
| `cohesion` | Report per-definition coupling — calls to definitions in the same file versus calls out of it — and the file's internal/external ratio. A file whose definitions call each other is a module; one whose definitions each call outward and never to each other is a namespace, and can be split anywhere. Definitions nothing links to are flagged as isolated. |
| `hotspots` | Rank definitions by git change frequency (`--since`) multiplied by complexity. Complexity alone ranks code that is hard; churn alone ranks code that moves; the product ranks code where a refactor pays. The only report that reads outside its input files — when `git log` cannot answer it says so and falls back to complexity, rather than reporting a zero that reads like "never changed". |
| `debt-score` | Report one score per file with the weighted contribution of every input shown, so the number can be argued with. Weighted by how expensive a problem is to live with: deep nesting highest (it makes every other problem harder to fix), then oversized definitions, then missing documentation and parked work. Uncapped, because a cap compresses exactly the files that most need distinguishing. |
| `indentation` | Report body forms indented against the Emacs/SLIME convention. Not `format` under another name: `format` states what *this tool* would print, and this states what an Emacs user's editor would produce on `C-M-q`. Most Lisp is written in Emacs, so a file `format` considers correct can still churn every line the moment someone opens it. |
| `docstrings` | Report definitions with no docstring, and — the useful half — docstrings that name a parameter the lambda list does not have. A stale docstring survives every rename this tool performs, since renaming deliberately does not touch string contents, so nothing else will ever notice it. Parameters the docstring never mentions are reported separately, as the weaker signal. |
| `todo` | Report `TODO`/`FIXME`/`XXX`/`HACK`/`BUG` markers with the top-level definition each one sits inside and any `TODO(name):` attribution. Comments are kept as trivia beside the tree rather than as nodes in it, so this is the only report that can see one. |
| `line-metrics` | Report line length, file length, and lines per definition against thresholds the caller sets (`--max-line-length`, `--max-file-lines`, `--max-definition-lines`). Distinct from `complexity`, which measures how hard a definition is to reason about; this measures how hard a file is to navigate. Width is counted in characters, not bytes. |
| `macro-expansion` | Report what each same-file `defmacro` expands its own call sites into. Template substitution only — it does not evaluate, does not expand nested macros, and reports every call it declined with the reason (a computed expansion, an `&key`/destructuring lambda list, or an argument-count mismatch). |
| `macro-hygiene` | Report the two ways a macro template betrays its caller: binding a literal name inside a quasiquoted template (variable capture, since Common Lisp macros are unhygienic), and unquoting one parameter more than once (multiple evaluation of the caller's argument form). A name bound to `(gensym)` outside the template is recognised and not reported. |
| `loop` | Report each `loop`'s clause structure: the variables `for`/`as`/`with` bind, what it accumulates and into what, which clauses can end it, and whether anything can. `loop` has a grammar rather than an S-expression shape, so nothing else in this tool can see inside one. |
| `format-directives` | Report `format`-family calls with a literal control string, counting the arguments the directives consume against the arguments the call supplies. Iteration (`~{…~}`), conditionals (`~[…~]`), and `~?` make the count indeterminate rather than wrong, and an indeterminate call never counts as a mismatch. |
| `read-conditionals` | Report every `#+`/`#-`, the feature expression it tests, the individual features that expression names, and the code it guards. Also counts features named exactly once in a file, which is usually a misspelling of one named everywhere else. |
| `read-time-eval` | Report every `#.`, separating an inert dispatch (a quoted datum, a literal) from a live one (a call). Read-time evaluation runs while the file is being *read*, which is a build-reproducibility and trust question as much as a correctness one. |
| `circular-literals` | Report `#n=` and `#n#` reader labels, pairing each definition against its references. A `#n#` with no `#n=` is a read error; a `#n=` nothing refers to is usually the residue of a deleted reference. |
| `readtable-case` | Report symbols whose identity changes with `readtable-case` — mixed-case symbols, which read as different symbols under `:upcase` and `:preserve` — and the `|…|` escapes that pin a spelling against every readtable case. |
| `package-locks` | Report definitions and bindings that collide with a `COMMON-LISP` symbol. CLHS leaves the consequences undefined and locked implementations refuse to load, so this is a portability failure a test suite on one implementation cannot find. An explicit `(:shadow …)` is reported but not counted as undefined behaviour. |
| `method-combination` | Report `defmethod` qualifiers (`:before`/`:after`/`:around`/primary) and the auxiliary methods with no primary on the same specializers to run around — a generic function that signals `no-applicable-method` because a form nobody wrote is missing. |
| `class-hierarchy` | Report the CLOS inheritance tree: each class's direct superclasses, depth, own slots, the slots it inherits and from which class, and the slots it *shadows*. Covers `defclass`, `define-condition`, and `defstruct` `:include`. The non-cyclic counterpart of `class-cycles`. |
| `generic-dispatch` | Report `defgeneric` declarations against the `defmethod` forms that implement them: methods with no `defgeneric`, a `defgeneric` with no method, a method whose required arity is not congruent with the declaration, and two methods sharing a name, qualifier, and specializers. |
| `restarts` | Report established restarts against invoked ones, and each side with no counterpart: an `invoke-restart` naming a restart nothing establishes signals `control-error`, and a `restart-case` clause nobody invokes is dead recovery code. `handler-bind`/`handler-case` clauses are listed beside them. |
| `types` | Report what the type layer proved: each binding's declared type (`declare`/`declaim`/`proclaim`/`check-type`) beside the constant the value layer proved it holds, plus every typed expression. A pair whose types share no member is flagged as a contradiction — a declaration no object can satisfy. Common Lisp only; other dialects report `dialect_modelled: false` rather than an empty list. |
| `narrowing` | Report where a branch proves something about a binding that is not true outside it: a type predicate in an `if`/`when`/`unless`/`cond` test, or a `typecase` clause's type specifier. Each site names the binding, the type the branch proves, which branch it holds in, and the span the narrowing is scoped to. |
| `constants` | Report expressions that provably evaluate to a literal, with the value, its kind, and the bytes folding would remove; plus the file's `defconstant` values as the value layer resolved them. A fold is reported once at its outermost form, so nested arithmetic is one opportunity rather than several. |
| `value-propagation` | Report which bindings carry a provable constant and, for those that do not, the first of the four propagation conditions they failed — reassigned, special, opaque scope, no initial form, or a non-constant initial form. The reason is the actionable half: "not constant" and "constant but reassigned" call for different work. |
| `effects` | Classify each definition as `pure`, `effectful`, or `unknown`, propagating effects along the file's own call graph to a fixpoint. `unknown` is a real verdict, not a failure: a body reaching an unregistered head may be reaching a macro, and a macro can expand into anything. Many refactor-safety questions (may this be hoisted? folded? inlined?) reduce to this one. |
| `semantic-coverage` | Report how much of `types`/`narrowing`/`constants`/`value-propagation` actually resolves on real source: variable-binding and constant-folding rates, broken down per dialect (Common Lisp only today; every other dialect's rate is the measure of section A's progress), plus the unresolved bindings ranked by cause — the highest-count unknown head is the next operator worth registering in the transparency table. `--fail-under` gates CI on a corpus-wide resolution-rate floor; repeatable `--fail-under-dialect DIALECT=PERCENT` (e.g. `--fail-under-dialect common-lisp=90`) gates a specific dialect's own rate instead, and combines with `--fail-under` — either one failing fails the run. A dialect with no discovered files fails loudly rather than passing trivially, the same as an empty corpus does for `--fail-under`. |
| `class-cycles` | Report CLOS defclass/define-condition superclass inheritance cycles across two or more classes. |
| `struct-cycles` | Report defstruct :include cycles across two or more structs. |
| `system-conflicts` | Report distinct asdf:defsystem forms that claim the same system name. |
| `elisp-file` | Report Emacs Lisp per-file facts: the `lexical-binding` header, the provided and required features, and the `;;;###autoload` cookies with the definitions they attach to. |
| `duplicate-slots` | Report defclass/define-condition/defstruct forms declaring the same slot name more than once. |
| `duplicate-methods` | Report defmethod forms with the same name, qualifier, and specializers declared more than once. |
| `duplicate-parameters` | Report callable definitions whose lambda list names the same parameter more than once. |
| `redundant-quote` | Report self-evaluating literals (numbers, strings, characters, keywords) that are quoted redundantly. |
| `redundant-progn` | Report progn forms that are redundant — empty (`(progn)` is `nil`) or wrapping a single form (`(progn X)` is just `X`). |
| `redundant-prog1` | Report a `(prog1 x)` wrapping a single form; `prog1` returns its first form's value, so with one form it is just `x`. Auto-fixable (unwraps to `x`). A multi-form `(prog1 x y)` (which sequences side effects and cannot become `progn`) and an empty `(prog1)` are left alone. |
| `negated-when-unless` | Report when/unless forms whose test is a `(not X)`/`(null X)` negation; flipping the macro (`when`↔`unless`) and dropping the negation reads more directly. |
| `nested-progn` | Report a multi-form `progn` nested directly inside another `progn`; its forms splice into the outer one, so `(progn a (progn b c) d)` is just `(progn a b c d)`. |
| `redundant-body-progn` | Report a multi-form `progn` used as the body of a form that already has an implicit progn (`when`, `unless`, `let`, `defun`, `lambda`, …), so `(when c (progn a b))` is just `(when c a b)`. |
| `empty-let` | Report a `let` whose binding list is empty (`(let () body)` or `(let nil body)`); with no bindings, it is just `(progn body)`. Auto-fixable (rewrites `(let ()` as `(progn`). A body that leads with `(declare …)` is left alone (invalid in `progn`), as is `(let* () …)` — the province of `redundant-let-star`. |
| `redundant-if-nil` | Report a three-argument `if` whose else branch is a literal `nil`; a two-argument `if` already returns `nil`, so `(if c x nil)` is just `(if c x)`. |
| `redundant-let-star` | Report a `let*` whose binding list holds zero or one binding: with no earlier binding to depend on, the sequential scope is unused, so `(let* ((x e)) body)` is just `(let ((x e)) body)`. Auto-fixable (rewrites the head to `let`). |
| `single-clause-cond` | Report a `cond` with exactly one clause that has a body and a non-`t` test; with nothing to fall through to, `(cond (test a b))` is just `(when test a b)`. Auto-fixable (rewrites to `when`). A `t`/`otherwise` catch-all clause (a `progn`) and a test-only clause (returns the test value) are left alone. |
| `cond-t-clause` | Report a `cond` with exactly one clause whose test is the literal `t` and which has a body; since `t` always holds and there is nothing to fall through to, `(cond (t a b))` is just `(progn a b)`. Auto-fixable (rewrites to `progn`). The `t`-clause complement of `single-clause-cond`. A multi-clause `cond` (a trailing `(t …)` else is idiomatic), a test-only `(cond (t))` (returns `t`), and a non-`t` test are left alone; `otherwise` is not special in `cond` and is not treated as a catch-all. |
| `explicit-step-delta` | Report an `incf`/`decf` whose delta operand is the literal `1`; since `1` is the default step, `(incf x 1)` is just `(incf x)` (and `(decf x 1)` is `(decf x)`). Auto-fixable (drops the delta). Only the bare integer `1` is matched — a float `1.0` can coerce the place's type, so it is left alone. |
| `negated-step-delta` | Report an `incf`/`decf` whose delta is a *negative* numeric literal; adding a negative delta is subtracting, so `(incf x -1)` is `(decf x 1)` and `(decf n -5)` is `(incf n 5)`. Auto-fixable (flips the operator and drops the sign); a resulting `(decf x 1)` is then reduced to `(decf x)` by `explicit-step-delta`. A variable delta and a positive literal are left alone. |
| `explicit-nil-return` | Report a `return` or `return-from` whose result form is the literal `nil`; `nil` is the default result, so `(return nil)` is just `(return)` and `(return-from foo nil)` is `(return-from foo)`. Auto-fixable (drops the `nil`). `(return-from nil)` is left alone — there the `nil` is the block name, not a result. |
| `funcall-lambda` | Report a `funcall` whose first argument is a literal `(lambda …)` form; a lambda form is directly applicable, so `(funcall (lambda (x) …) a)` is just `((lambda (x) …) a)`. Auto-fixable (drops `funcall`). The `#'symbol` case belongs to `redundant-funcall`; a `#'(lambda …)` first argument is left alone (its reader prefix cannot sit in operator position). |
| `if-to-or` | Report a three-argument `if` whose test and then-branch are the same bare atom; `(if x x y)` returns `x` or the else, which is `(or x y)` — evaluated once instead of twice. Auto-fixable (rewrites to `or`). Only an atom test/then pair is matched (a compound test would be evaluated twice); a literal `t`/`nil` test belongs to `constant-if-test`. |
| `if-not` | Report a three-argument `if` whose then-branch is the literal `nil` and else-branch is the literal `t`; `(if test nil t)` yields `nil` when `test` holds and `t` otherwise, which is exactly `(not test)`. Auto-fixable (rewrites to `(not test)`, copying the test verbatim). The dual `(if test t nil)` is a boolean coercion with no clearer builtin and is left alone, as is `(if test nil nil)` (`identical-if-branches`) and any non-literal branch. |
| `if-to-unless` | Report a three-argument `(if c nil e)` whose then-branch is the literal `nil`; it yields `nil` when `c` holds and `e` otherwise, which is exactly `(unless c e)`. Auto-fixable (rewrites to `(unless c e)`). To avoid overlap, `else = t` is left to `if-not`, `else = nil` to `identical-if-branches`, and a constant `t`/`nil` test to `constant-if-test`. |
| `prog2-to-progn` | Report a two-form `(prog2 a b)`; `prog2` returns its second form's value, which for exactly two forms is also the last, so it equals `(progn a b)`. Auto-fixable (rewrites the `prog2` operator to `progn`). A three-or-more-form `prog2` (which returns the *second* form, not the last) and a one-form `prog2` are left alone. |
| `handler-case-no-clauses` | Report a `(handler-case expr)` with no handler clauses; it establishes no handlers and just returns `expr`. Auto-fixable (unwraps to `expr`). A `handler-case` with any clause is left alone. |
| `unwind-protect-no-cleanup` | Report an `(unwind-protect x)` with no cleanup forms; with nothing to run on exit it just returns `x`. Auto-fixable (unwraps to `x`). An `unwind-protect` with any cleanup form is left alone. |
| `one-step-arithmetic` | Report a two-argument `+`/`-` of the literal `1`, which has a unary shorthand: `(+ x 1)` and `(+ 1 x)` are `(1+ x)`, and `(- x 1)` is `(1- x)`. Auto-fixable (rewrites to `1+`/`1-`). `(- 1 x)` has no shorthand and is left alone; only the bare integer `1` matches (a float `1.0` would coerce the result type). |
| `case-nil-key` | Report a `case`/`ecase`/`ccase` clause whose key is the bare atom `nil`. `case` keys designate a list of objects, and `nil` designates the *empty* list, so `(case x (nil …))` can never match — to match the value `nil` you must write `((nil) …)`. Report-only (an **error**-severity likely bug). `((nil) …)` and a quoted `'nil` (see `quoted-case-key`) are left alone. |
| `typecase-nil-key` | Report a `typecase`/`etypecase`/`ctypecase` clause whose head is the bare atom `nil`. In `typecase` a clause head is a *type specifier*, and `nil` is the empty type — no object is of it — so `(typecase x (nil …))` is a dead clause. To match the `nil` value use the `null` type. Report-only (**error**-severity). The `t` catch-all, the `null` type, and a quoted `'nil` are left alone. The typecase-family analog of `case-nil-key`. |
| `sharp-quoted-lambda` | Report a `(lambda …)` form with a redundant `#'` prefix. `lambda` already expands to `(function (lambda …))`, so `#'(lambda (x) …)` is exactly `(lambda (x) …)` in every position. Auto-fixable (strips the `#'`). A `#'foo` symbol reference and a bare `(lambda …)` are left alone. |
| `redundant-eql-test` | Report an explicit `:test #'eql` on an operator whose `:test` already defaults to `eql` (`member`, `assoc`, `find`, `position`, `count`, `remove`, the set operations, …), so `(find x list :test #'eql)` is just `(find x list)`. Auto-fixable (deletes the `:test #'eql` pair). Recognizes `#'eql`, `'eql`, and `(function eql)`; a custom test, `:test-not`, and non-eql-defaulting operators are left alone. |
| `redundant-start-zero` | Report an explicit `:start 0` on a bounded-sequence operator whose `:start` defaults to `0` (`find`, `position`, `count`, `remove`, `substitute`, the `-if`/`-if-not` variants, `fill`, `reduce`, `parse-integer`, the string-case functions, …), so `(find x seq :start 0)` is `(find x seq)`. Auto-fixable (deletes the `:start 0` pair). Two-sequence operators (`search`/`mismatch`/`replace`, which use `:start1`/`:start2`) and a non-zero start are left alone. |
| `redundant-end-nil` | Report an explicit `:end nil` (`:end` defaults to `nil` = end of sequence) on the same bounded-sequence operators, so `(find x seq :end nil)` is `(find x seq)`. Auto-fixable (deletes the `:end nil` pair). A non-`nil` end is left alone. |
| `redundant-from-end-nil` | Report an explicit `:from-end nil` (the default) on a `:from-end`-taking operator (`find`/`position`/`count`/`remove`/`delete`/`substitute` families, `remove-duplicates`, `reduce`, `search`, `mismatch`), so `(find x seq :from-end nil)` is `(find x seq)`. Auto-fixable. A `:from-end t` is left alone. |
| `redundant-count-nil` | Report an explicit `:count nil` (the default, meaning unlimited) on `remove`/`delete`/`substitute`/`nsubstitute` and their `-if`/`-if-not` variants, so `(remove x seq :count nil)` is `(remove x seq)`. Auto-fixable. A numeric `:count` is left alone. |
| `make-hash-table-test` | Report a `make-hash-table` with an explicit `:test 'eql`; the `:test` argument defaults to `eql`, so `(make-hash-table :test 'eql)` is `(make-hash-table)`. Auto-fixable (deletes the `:test 'eql` pair, keeping any other keyword arguments). The `make-hash-table` sibling of `redundant-eql-test`; a custom test (`'equal`/`'equalp`/`'eq`) is left alone. |
| `gethash-default` | Report a three-argument `(gethash key table nil)` whose default value is the literal `nil`; `gethash`'s default already defaults to `nil` (and the second, present-p, value is unchanged), so it is `(gethash key table)`. Auto-fixable (deletes the ` nil`). A non-`nil` default is left alone. |
| `typep-predicate` | Report a `(typep x 'TYPE)` whose type is a CL type name with a dedicated total predicate (`null`, `symbol`, `atom`, `cons`, `list`, `integer`, `string`, `hash-table`, `function`, …), which is exactly `(PRED x)` — e.g. `(typep x 'string)` → `(stringp x)`. Auto-fixable. A compound type spec, a type without a dedicated predicate (`fixnum`), the always-true `t`, and the three-argument `(typep x type env)` form are left alone. |
| `coerce-to-t` | Report a `(coerce x t)`; coercing to type `t` returns the object unchanged, so it is just `x`. Auto-fixable (unwraps to `x`). A real coercion (`(coerce x 'list)`, `(coerce x 'double-float)`) is left alone. |
| `redundant-identity-key` | Report an explicit `:key #'identity` (or `:key nil`) on an operator that accepts `:key` (`find`, `position`, `remove`, `sort`, `merge`, `reduce`, the `-if` variants, …); `:key` defaults to `nil` (the element itself), so `(sort xs #'< :key #'identity)` is just `(sort xs #'<)`. Auto-fixable (deletes the pair). Recognizes `#'identity`, `'identity`, `(function identity)`, and `nil`; a custom key and non-`:key` operators (e.g. `tree-equal`) are left alone. |
| `single-value-bind` | Report a `multiple-value-bind` that binds exactly one variable; `let` captures the form's primary value, which is all a one-variable bind uses, so `(multiple-value-bind (x) f body)` is just `(let ((x f)) body)`. Auto-fixable (rewrites to `let`). Two-or-more variables (which capture secondary values) and an empty variable list (a `progn`) are left alone. |
| `nested-boolean` | Report an `and`/`or` nested directly inside a same-operator `and`/`or`; both operators are associative with identical short-circuiting, so `(or a (or b c) d)` is just `(or a b c d)` (and likewise for `and`). Auto-fixable (splices the inner operands in). The inner form must have two or more operands — the single-operand `(or x)` collapse belongs to `single-operand-boolean`. |
| `nested-when` | Report a `when` whose only body form is another `when`; the two tests combine, so `(when a (when b body))` is `(when (and a b) body)`. Auto-fixable (merges the tests with `and`). An outer `when` with additional body forms after the inner `when` is left alone — those forms are not guarded by the inner test. |
| `nested-unless` | Report an `unless` whose only body form is another `unless`; the body runs only when both tests are nil, so `(unless a (unless b body))` is `(unless (or a b) body)`. Auto-fixable (merges the tests with `or`). The `or`-combining mirror of `nested-when`; an outer `unless` with extra body forms is left alone. |
| `empty-body` | Report a `when`/`unless`/`dolist`/`dotimes` form that has its test/spec but no body (`(when ready)`, `(dolist (x items))`); the test/spec runs and then nothing happens — usually a forgotten body. |
| `verbose-negation` | Report negation written the long way: `(- 0 x)` (zero minus x), `(* x -1)` and `(* -1 x)` (times minus one) are all `(- x)`. Auto-fixable (rewrites to unary `(- x)`). Only bare integer `0`/`-1` are matched (a float `-1.0` would coerce the result type); the *trailing* `(- x 0)` is `identity-arithmetic`'s job. |
| `identity-arithmetic` | Report an arithmetic form with a redundant integer identity operand (`(+ x 0)`, `(* x 1)`, `(- x 0)`, `(/ x 1)`); adding 0, multiplying by 1, etc. does nothing. Float `0.0`/`1.0` (which coerce) and the leading operand of `-`/`/` are not flagged. |
| `redundant-divisor` | Report a two-argument `floor`/`ceiling`/`truncate`/`round` (or the float variants `ffloor`/`fceiling`/`ftruncate`/`fround`) whose divisor is the literal integer `1`; the divisor defaults to `1`, so `(floor x 1)` is exactly `(floor x)` — same quotient and remainder. Auto-fixable (drops the divisor, preserving operator casing). A float `1.0` (which changes the remainder type), a non-`1` divisor, and `mod`/`rem` (no defaultable divisor) are left alone. |
| `single-operand-list-op` | Report a single-argument `append`, `nconc`, or `list*`, each of which returns its argument unchanged, so `(append x)` is just `x`. Auto-fixable (replaces the form with the argument). Scoped to these three because their single-argument identity is unconditional; numeric ops like `(max x)` (which require a real and would signal a type error) are excluded. Two-or-zero-argument forms are left alone. |
| `single-operand-arithmetic` | Report a single-operand `+`/`*` form (`(+ x)`, `(* x)`); a one-argument `+`/`*` returns its operand verbatim, so the wrapper is pure noise. Auto-fixable (unwraps to the operand). Unary `(- x)` (negation) and `(/ x)` (reciprocal) are meaningful and not flagged; the zero-operand identities `(+)`/`(*)` are left alone. |
| `redundant-funcall` | Report a `funcall` of a sharp-quoted symbol (`(funcall #'foo a b)`); it resolves `foo` through the same lexical function namespace as a direct `(foo a b)`, so the `funcall`/`#'` is ceremony. Auto-fixable (rewrites to the direct call). A variable argument (`(funcall fn …)`), a sharp-quoted lambda (`#'(lambda …)`), and an ordinary-quoted symbol (`'foo`) are not flagged. |
| `redundant-the` | Report a `(the t form)` type declaration; the type `t` matches every object, so the assertion is vacuous and the form is exactly `form` (`the` passes all values through, so this holds for multiple values too). Auto-fixable (replaces the form with its inner form). A specific type (`(the fixnum x)`), a compound type, and a wrong-arity `(the t)`/`(the t a b)` are not flagged. |
| `nil-comparison` | Report an `eq`/`eql`/`equal`/`equalp` comparison against a bare `nil` (`(eq x nil)`, `(equal nil y)`); each is exactly the idiomatic `(null x)` nil test. Auto-fixable (rewrites to `(null X)`). Numeric `=` (a type error on nil), a degenerate `(eq nil nil)`, and a quoted `'nil` are not flagged. |
| `one-armed-if` | Report a two-argument `if` with no else branch (`(if test then)`); the mainstream style guides recommend `(when test then)` for a conditional with a single arm. Auto-fixable (swaps the `if` head for `when`; a `progn` then-branch is then spliced by `redundant-body-progn`). A two-armed `(if test a b)`, an argument-short `(if test)`, and a reader-conditional operand are not flagged. |
| `t-comparison` | Report an `eq`/`eql`/`equal`/`equalp` comparison against the literal `t` (`(eq x t)`); it matches only the symbol `T`, so as a boolean test it silently fails for any other true value (a generalized-boolean mistake). Report-only — the right rewrite (drop the comparison, or keep an intentional symbol test) depends on intent. The symmetric partner of `nil-comparison`. Numeric `=`, a degenerate `(eq t t)`, and a quoted `'t` are not flagged. |
| `manual-incf` | Report a `setf`/`setq` that manually increments or decrements a variable (`(setf x (1+ x))`, `(setq n (+ n 2))`, `(setf i (- i 1))`), which is exactly what `incf`/`decf` express. Auto-fixable (rewrites to `(incf x)` / `(incf n 2)` / `(decf i 1)`). Only bare-variable places are matched (so `incf`'s single evaluation of a compound place never changes side-effect timing); a compound place, a non-commuting `(- d v)`, a different variable, and a multi-pair `setf` are not flagged. |
| `manual-push` | Report a `setf`/`setq` that manually conses an element onto a variable (`(setf stack (cons item stack))`), which is exactly what `push` expresses. Auto-fixable (rewrites to `(push item stack)`). Because `cons` is `(cons element list)`, only a form whose *second* `cons` operand is the place counts; consing the place as the element (`(cons x other)`), a compound place, and a multi-pair `setf` are not flagged. |
| `cons-to-list` | Report a `cons` whose tail is `nil`/`()` or a `list` literal, which is really a `list` construction: `(cons a nil)` is `(list a)`, `(cons a (list b c))` is `(list a b c)`. Auto-fixable (rewrites to `list`; a spelled-out cons chain like `(cons a (cons b nil))` converges to `(list a b)` one layer per fixpoint pass). A cons onto a variable or an improper pair `(cons a b)` is a genuine cons and not flagged. |
| `double-reverse` | Report a `(reverse (reverse x))`; `reverse` returns a fresh sequence of the same kind with the order flipped, so reversing twice yields a fresh sequence equal to `x` — exactly `(copy-seq x)`, a wasteful obfuscated copy. Auto-fixable (rewrites to `(copy-seq x)`, copying the inner argument verbatim). The destructive `nreverse` (on either level) and a mixed `reverse`/`nreverse` nesting are left alone — they cannot be reasoned about as a plain copy. |
| `append-list-to-cons` | Report a two-argument `(append (list x) rest)` whose first argument is a one-element `(list x)`; `append` conses the copied singleton onto the shared tail, which is exactly `(cons x rest)` — same single fresh cons, same sharing of `rest`, same left-to-right evaluation. Auto-fixable (rewrites to `(cons x rest)`). A multi-element first list (`(append (list x y) rest)` is `(list* x y rest)`), a non-`list` first argument, and a different argument count are left alone. |
| `list-star-to-cons` | Report a two-argument `(list* a b)`; by definition `list*` of exactly two arguments builds one cons, so it is exactly `(cons a b)`. Auto-fixable (rewrites to `(cons a b)`). A single-argument `(list* x)` (which is `x`, `single-operand-list-op`'s concern) and a three-or-more-argument `list*` (nested conses) are left alone. |
| `values-list-of-list` | Report a `(values-list (list a b))` whose sole argument is a `list` construction; building a fresh list only to spread it is exactly `(values a b)` — same values, order, and evaluation. Auto-fixable (rewrites to `(values a b)`; an empty `(list)` becomes `(values)`). A quoted list (`'(a b)`, whose elements are data, not forms), a variable argument, and a non-`list` constructor are left alone. |
| `multiple-value-list-of-values` | Report a `(multiple-value-list (values a b))`; collecting the values of a literal `(values …)` into a list is exactly `(list a b)` — the inverse of `values-list-of-list`. Auto-fixable (rewrites to `(list a b)`; an empty `(values)` becomes `(list)`). A variable or non-`values` argument is left alone. |
| `append-nil` | Report a two-argument `(append x nil)`; `append` copies all but its last argument, so with a `nil` tail the result is a fresh top-level copy of `x` — exactly `(copy-list x)`. Both `append` (non-last arg must be a proper list) and `copy-list` reject a non-list identically, so the type-check domain is preserved. Auto-fixable. A non-`nil` tail and a differing argument count are left alone. (`(nconc x nil)` is deliberately not handled — rewriting to bare `x` would drop `nconc`'s list type-check.) |
| `manual-pushnew` | Report a `setf`/`setq` that manually `adjoin`s an element onto a variable (`(setf set (adjoin item set))`, `(setf seen (adjoin k seen :test #'equal))`), which is exactly what `pushnew` expresses. Auto-fixable (rewrites to `(pushnew item set)`, passing any `:test`/`:key` keyword arguments through unchanged). Only a form whose *second* `adjoin` operand is the place counts; a compound place and a multi-pair `setf` are not flagged. |
| `nested-cxr` | Report nested `car`/`cdr`-family accessors that the standard combines into one (`(car (cdr x))` is `(cadr x)`, `(cdr (cdr x))` is `(cddr x)`). Auto-fixable (collapses to the combined `cXr`; deeper nestings converge one layer per fixpoint pass, so `(car (cdr (cdr x)))` becomes `(caddr x)`). Only combinations that stay within the four-letter standard accessors are flagged; `first`/`rest` spellings and a nesting that would exceed `cddddr` are not. |
| `redundant-identity` | Report an `(identity x)` call, which returns its argument unchanged — so it is exactly `x`. Auto-fixable (replaces the form with its argument). Composes with `redundant-funcall` (`(funcall #'identity x)` → `(identity x)` → `x`). A `#'identity` function reference (e.g. a `:key` argument) and an argument-mismatched `(identity)`/`(identity a b)` are not flagged. |
| `nthcdr-zero` | Report `(nthcdr 0 list)`, which returns the list unchanged, so it is just `list`. Auto-fixable (replaces the whole form with the list operand). Only the bare integer `0` count is matched; a non-zero index, a float `0.0`, and a variable count are left alone. |
| `subseq-zero` | Report a two-argument `(subseq seq 0)`; a subseq from index 0 with no end is a fresh copy of the whole sequence, exactly `(copy-seq seq)`. Auto-fixable (rewrites to `(copy-seq seq)`). A present end argument (`(subseq seq 0 n)`, a genuine slice), a non-zero start, a float `0.0`, and a variable start are left alone. |
| `car-nthcdr` | Report a `(car (nthcdr n x))`; the standard defines `nth` as the `car` of the `nthcdr`, so this is exactly `(nth n x)` — same element and nil-on-overrun. Auto-fixable (rewrites to `(nth n x)`). A `cdr`/other outer accessor and a wrong `nthcdr` arity are left alone. |
| `car-reverse` | Report a `(car (reverse x))` / `(first (reverse x))`; the first element of the reversed list is the last of the original, but `reverse` builds a whole fresh copy to read one element. `(car (last x))` yields the same element without the O(n) allocation. Auto-fixable (rewrites to `(car (last x))`, keeping the outer accessor). The destructive `nreverse` (which mutates `x`) is left alone. |
| `nthcdr-small-index` | Report `(nthcdr n list)` with a literal count `1`–`4`, for which the standard defines a named `cdr` accessor: `(nthcdr 1 x)` is `(cdr x)`, `(nthcdr 2 x)` is `(cddr x)`, `(nthcdr 3 x)` is `(cdddr x)`, `(nthcdr 4 x)` is `(cddddr x)`. Auto-fixable (rewrites to the accessor, copying the list operand). The count `0` belongs to `nthcdr-zero`; `5`+ (no `cdddddr`), a float, and a variable count are left alone. |
| `nth-constant-index` | Report an `nth` with a small literal index that has a named ordinal accessor (`(nth 0 x)` is `(first x)`, … `(nth 9 x)` is `(tenth x)`). Auto-fixable (rewrites to the ordinal). Only literal indices 0–9 are flagged (there is no `eleventh`); a variable index and index 10+ are not. |
| `redundant-apply` | Report an `apply` of a sharp-quoted symbol to a literal list (`(apply #'foo (list a b))`); spreading a literal `(list …)` is exactly the direct call `(foo a b)`. Auto-fixable (rewrites to the direct call; an empty `(list)` yields a zero-argument call). The sibling of `redundant-funcall`. A variable list argument (`(apply #'foo args)`), leading fixed arguments, an ordinary-quoted `'foo`, and a sharp-quoted lambda are not flagged. |
| `sign-comparison` | Report a two-argument `=`/`>`/`<` comparison against the literal `0`, which has a dedicated predicate: `(= x 0)` is `(zerop x)`, `(> x 0)` is `(plusp x)`, `(< x 0)` is `(minusp x)`. Auto-fixable (rewrites to the predicate, accounting for which side the `0` is on — `(> 0 x)` becomes `(minusp x)`). `>=`/`<=`/`/=` (no single-word predicate), a `0.0` spelling, a three-argument comparison, and `(= 0 0)` are not flagged. |
| `negated-comparison` | Report a `not`/`null` wrapping a two-argument numeric comparison, which has an exact complement: `(not (= a b))` is `(/= a b)`, `(not (< a b))` is `(>= a b)`, `(not (> a b))` is `(<= a b)` (and their inverses). Auto-fixable (rewrites to the complementary operator). Only the two-operand shape is flagged — `(not (= a b c))` (all-equal vs pairwise-distinct) and a reader-conditional operand are not. |
| `de-morgan` | Report an `and`/`or` whose operands are *all* single-argument negations, which collapses to one outer negation: `(and (not a) (not b))` is `(not (or a b))`, `(or (not a) (not b))` is `(not (and a b))`. Auto-fixable (rewrites via De Morgan — exact down to short-circuit order). Requires at least two operands, all `not`/`null`; a mix of negated and non-negated operands is not flagged. |
| `redundant-boolean-identity` | Report an `and`/`or` containing its identity element, which contributes nothing: `t` in an `and` (`(and a t b)` is `(and a b)`) or `nil` in an `or` (`(or a nil b)` is `(or a b)`). Auto-fixable (drops the identity operand; collapses to the bare `t`/`nil` if all operands were removed). The complement of `dead-boolean-operand` (which handles the *dominant* `nil`-in-`and`/`t`-in-`or`). A trailing `t` in `and` (its return value) and a single-operand form are not flagged. |
| `constant-if-test` | Report an `if` whose test is the literal constant `t` or `nil`, so one branch is dead code: `(if t a b)` is `a`, `(if nil a b)` is `b`, `(if nil a)` is `nil`. Auto-fixable (`dead-code` category — replaces the form with the live branch). A truthy non-`t` literal (`(if 5 a b)`) and a variable test are not flagged. |
| `constant-when-test` | Report a `when`/`unless` whose test is the literal constant `t` or `nil`, so the body is statically decided: `(when t body…)` and `(unless nil body…)` always run (they are `(progn body…)`), while `(when nil body…)` and `(unless t body…)` never run (they are `nil`, dead code). Auto-fixable (`dead-code` category — splices the always-true form to `progn` and collapses the dead form to `nil`; the discarded body is never evaluated, so no side effects are lost). A truthy non-`t` literal and a variable test are not flagged. The `if` sibling is `constant-if-test`. |
| `negated-if` | Report a three-argument `if` whose test is a `(not X)`/`(null X)` negation (`(if (not ready) a b)`); negating the test just flips the branches, so it is exactly `(if X B A)`. Auto-fixable (drops the negation and swaps the then/else branches). A one-armed `(if (not c) a)` (no else to swap — the `when`/`unless` idiom's job) and a reader-conditional branch are not flagged. |
| `duplicate-setf-places` | Report a `setf`/`setq`/`psetf`/`psetq` that assigns the same variable more than once in one form (`(setf a 1 a 2)`, `(setq x 1 y 2 x 3)`); the earlier assignment is dead — almost always a copy-paste slip or a typo. Report-only (error severity, `duplicate` category). Only symbol places are compared by name (case-insensitively); a compound `setf` place and a malformed odd-arity form are not flagged. |
| `single-operand-boolean` | Report single-operand `and`/`or` forms; a one-argument `and`/`or` returns its operand unchanged, so `(and X)` and `(or X)` are just `X`. |
| `single-arg-comparison` | Report numeric comparisons (`<` `>` `<=` `>=` `=` `/=`) called with a single argument; these are vacuously true (`(< x)` is always `t`), usually a missing operand. |
| `format-missing-destination` | Report `format` calls whose first argument is a string literal; the first argument is the destination (`nil`/`t`/stream), so `(format "~a" x)` is missing it. |
| `format-to-string` | Report a `(format nil "~A" x)` / `(format nil "~S" x)` whose control string is exactly one directive; with a `nil` destination `format` returns the string, and a lone `~A`/`~S` is `princ`/`prin1` semantics, so these are exactly `(princ-to-string x)` / `(prin1-to-string x)`. Auto-fixable (rewrites to the string function). Any surrounding text or extra directive, a non-`nil` destination (`t`/stream return `nil`, not the string), and an argument count other than one are left alone. |
| `format-newline` | Report a `(format t "~%")`; with the `t` destination `format` writes to `*standard-output*` and returns `nil`, and `~%` emits one newline — exactly `(terpri)`. Auto-fixable (rewrites to `(terpri)`). Only the `t` destination is matched (an arbitrary destination could be a fill-pointer string, not a valid `terpri` argument); `~&` (`fresh-line`, whose return value differs) and any call carrying format arguments are left alone. |
| `literal-place` | Report `incf`/`decf`/`push`/`pop`/`pushnew`/`setf`/`psetf` whose place is a self-evaluating literal (`(incf 5)`, `(push x 3)`, `(setf 5 x)`); a literal is not a modifiable place, so the form fails at macroexpansion. |
| `zero-divisor` | Report a division-family form with a literal `0` in a divisor position — `(/ x 0)`, `(/ 0)` (reciprocal), `(mod x 0)`, `(rem x 0)`, `(floor x 0)`, … — which always signals `division-by-zero` at run time. Report-only (there is no meaningful rewrite). A `0` numerator (`(/ 0 x)`), a float `0.0`, and a single-argument quotient with no divisor are not flagged. |
| `duplicate-keyword` | Report a `make-*` constructor call (`make-instance`, `make-hash-table`, `make-array`, `make-string`, `make-condition`, `make-pathname`, `make-string-output-stream`) that passes the same keyword argument twice, e.g. `(make-instance 'c :x 1 :x 2)`; the leftmost value wins and the rest are silently ignored. Report-only. Scope is gated to operators with a fixed positional prefix so a positional argument that is itself a keyword is never mistaken for a keyword-argument name. |
| `defpackage-quoted` | Report a quoted or sharp-quoted designator inside a `defpackage` clause that takes symbol/package designators (`:export`, `:shadow`, `:intern`, `:import-from`, `:shadowing-import-from`, `:use`, `:nicknames`), e.g. `(:export 'foo)`; `defpackage` does not evaluate its options, so the quote is almost always a bug. Report-only. |
| `step-zero` | Report an `(incf place 0)` / `(decf place 0)` whose step is the literal `0` — a no-op that evaluates the place but changes nothing, usually a forgotten step. Report-only (dropping it could discard the place's side effects). A float `0.0`, a non-zero step, and the default-step `(incf x)` are not flagged. |
| `unreachable-cond-clause` | Report cond forms with clauses after a t catch-all clause that can never run. |
| `malformed-let-binding` | Report let/let* bindings that are neither a symbol nor a (var value) pair. |
| `if-arity` | Report if forms with the wrong number of arguments (Common Lisp if takes 2 or 3). |
| `malformed-cond-clause` | Report cond clauses that are not a non-empty list (a bare atom or empty clause). |
| `malformed-case-clause` | Report case/typecase-family clauses that are not a non-empty list (a bare atom or empty clause). |
| `unreachable-case-clause` | Report case/typecase clauses after a t/otherwise catch-all clause that can never run. |
| `malformed-iteration-spec` | Report dolist/dotimes specs that are not a (var form [result]) list. |
| `duplicate-lambda-list-keyword` | Report lambda lists that repeat a lambda-list keyword (&optional, &rest, &key, ...). |
| `lambda-list-keyword-order` | Report lambda lists whose keywords are out of the canonical &optional/&rest/&key/&aux order. |
| `modify-macro-arity` | Report incf/decf/push/pop calls with the wrong number of arguments. |
| `binds-constant` | Report let/let*/do/do* bindings whose variable is a constant (nil, t, or a keyword). |
| `quoted-case-key` | Report case/ecase/ccase clauses with a quoted key ('a matches quote and a, not a). |
| `the-arity` | Report the special forms without exactly two arguments (a type and a form). |
| `equality-arity` | Report eq/eql/equal/equalp calls without exactly two arguments. |
| `accessor-arity` | Report nth/elt/gethash/getf/... accessors with the wrong number of arguments. |
| `setq-non-variable` | Report setq/psetq places that are not variables (a list, literal, or constant). |
| `eval-when-situation` | Report eval-when forms with an invalid situation (not :compile-toplevel/:load-toplevel/:execute). |
| `exhaustive-case-otherwise` | Report ecase/ccase/etypecase/ctypecase forms with a forbidden t/otherwise clause. |
| `duplicate-case-keys` | Report case/ecase/ccase forms with the same key in more than one clause. |
| `self-assignments` | Report setq/setf/psetq/psetf pairs that assign a place to itself. |
| `identical-if-branches` | Report if forms whose then and else branches are structurally identical. |
| `duplicate-cond-tests` | Report cond forms with the same test expression in more than one clause. |
| `duplicate-let-bindings` | Report parallel let forms that bind the same variable more than once. |
| `duplicate-boolean-operands` | Report and/or forms that list the same operand more than once. |
| `eql-string-comparison` | Report eq/eql calls that compare against a string literal (never reliably eql). |
| `self-comparison` | Report comparison calls whose two operands are structurally identical (always true/false). |
| `dead-boolean-operand` | Report and/or forms whose non-final constant operand makes later operands dead. |
| `eq-number-comparison` | Report eq calls that compare against a number literal (eq on numbers is unreliable). |
| `eq-char-comparison` | Report eq calls that compare against a character literal (`(eq c #\a)`); `eq` on characters is unreliable — use `eql` or `char=`. |
| `char-op-string` | Report character functions (`char=`, `char<`, `char-code`, `char-upcase`, `alpha-char-p`, …) applied to a string literal (`(char= "a" c)`); these require a character, so a string literal is a type error. |
| `string-case-fold` | Report a `string=` whose two operands are each case-folded the same way — `(string= (string-downcase a) (string-downcase b))` (or both `string-upcase`); folding both sides then comparing case-sensitively is a case-insensitive comparison, exactly `(string-equal a b)`. Auto-fixable. A mixed pair (one downcase, one upcase) and a `:start`/`:end`-keyworded comparison are left alone. |
| `char-case-fold` | Report a `char=` whose two operands are each case-folded the same way — `(char= (char-downcase a) (char-downcase b))` (or both `char-upcase`), which is exactly `(char-equal a b)`. Auto-fixable. A mixed pair and a three-or-more-argument comparison are left alone. |
| `nested-string-case` | Report a nested pair of `string-upcase`/`string-downcase`/`string-capitalize` — `(string-upcase (string-downcase s))`; the outer case operation fully determines the result (case ops change case but not letter identity or word boundaries), so the inner one is dead work and the form is `(string-upcase s)`. Auto-fixable (keeps the outer op). The destructive `nstring-*` variants are excluded (dropping them would drop their mutation). |
| `code-char-char-code` | Report a `(code-char (char-code c))`; `char-code` maps a character to its code and `code-char` maps it back, so the round-trip is just `c`. Auto-fixable (unwraps to `c`). The reverse `(char-code (code-char n))` is not flagged (`code-char` can return `nil` for an unsupported code). |
| `last-default-count` | Report a `(last x 1)` whose explicit trailing count restates `last`'s default of `1`, so it is exactly `(last x)`. Auto-fixable (drops the ` 1`). A non-`1` count (`(last x 2)`) is meaningful and left alone. |
| `butlast-default-count` | Report a `(butlast x 1)` (or destructive `(nbutlast x 1)`) whose explicit trailing count restates the default of `1`, so it is exactly `(butlast x)`. Auto-fixable (drops the ` 1`); `nbutlast`'s mutation is preserved. A non-`1` count is left alone. |
| `make-list-default-element` | Report a `(make-list n :initial-element nil)` whose keyword restates `make-list`'s default of `nil`, so it is exactly `(make-list n)`. Auto-fixable (drops the ` :initial-element nil` pair). A non-`nil` element (`:initial-element 0`) is meaningful and left alone. |
| `parse-integer-default-radix` | Report a `(parse-integer s :radix 10)` whose keyword restates `parse-integer`'s default of `10`, so it is exactly `(parse-integer s)`. Auto-fixable (drops the ` :radix 10` pair, preserving any other keywords). A non-`10` radix is left alone. |
| `getf-default-nil` | Report a `(getf p k nil)` whose explicit default restates `getf`'s default of `nil`, so it is exactly `(getf p k)`. Auto-fixable (drops the trailing ` nil`). A non-`nil` default is meaningful and left alone. |
| `make-array-default-keyword` | Report a `make-array` call with an explicit `:adjustable nil` or `:fill-pointer nil`, restating the default. Auto-fixable (drops the redundant keyword pair, preserving other keywords). A non-`nil` value, and `:initial-element nil` (which is *not* redundant for `make-array`), are left alone. |
| `nested-char-case` | Report a nested pair of `char-upcase`/`char-downcase` — `(char-upcase (char-downcase c))`; the outer case operation fully determines the result, so the inner one is dead work and the form is `(char-upcase c)`. Auto-fixable (keeps the outer op). |
| `list-star-nil` | Report a `(list* a … nil)` whose final argument is the literal `nil`; with a `nil` tail `list*` builds a proper list, exactly what `list` builds, so the form is `(list a …)`. Auto-fixable (rewrites the head to `list` and drops the trailing ` nil`). A non-`nil` tail is a genuine `list*` and is left alone. |
| `destructive-literal` | Report destructive calls (`nreverse`, `sort`, `delete`, `nsubstitute`, `nunion`, `nconc`, `rplaca`, …) whose modified argument is a quoted list literal (`(sort '(3 1 2) #'<)`, `(delete x '(1 2))`); modifying a constant is undefined behavior. Each function's sequence argument position is known, so a literal *item* or a last-`nconc` argument is not flagged. |
| `eql-list-comparison` | Report eq/eql calls that compare against a quoted list literal (never reliably eql). |
| `eql-search-literal` | Report `member`/`assoc`/`find`/`position`/`count`/`remove`/`delete`/`adjoin`/`pushnew` (item first) and `substitute`/`nsubstitute`/`subst`/`nsubst` (item second) searching for a string or quoted-list literal with no `:test`; the default `eql` never matches a string/list literal — add `:test #'equal`. |
| `setf-arity` | Report setq/setf/psetq/psetf forms with an odd argument count (a place missing its value). |
| `lint` | Run every within-file logic-bug lint at once and report all findings, tagged by rule and category. Each finding is self-describing — it carries its `severity`, `category`, and a `fixable` flag inline (so an agent can triage and decide whether to run `--fix` without cross-referencing `--list-rules`). `--list-rules` prints the rule catalog with categories, descriptions, a `severity` (`error` for likely/certain bugs, `warning` for redundant/non-idiomatic style), and a `fixable` flag marking the rules `--fix` can repair — and it honors the same `--rule`/`--exclude`/`--category` selectors, so `--list-rules --category dead-code` lists just that group; `--rule`/`--exclude` select rules; `--category` selects a whole group (see `--list-rules` for the current set); `--sarif` emits a SARIF 2.1.0 log for CI code scanning (with stable fingerprints and one-click `fixes` for every rule `--list-rules` marks fixable); `--github` emits GitHub Actions `::error::` annotations for inline PR review; `--fix` applies those auto-fixes in place, iterating to a fixpoint (so nested redundancies collapse fully) and reporting the per-file/per-rule counts; add `--diff` to preview the changes as a unified diff without writing, or `--check` to write nothing and exit 3 when any auto-fix is still pending (a CI gate that stays green only when fixable lint has been cleaned up — distinct from `--fail-on-finding`, which also gates on report-only findings). `--check` and `--diff` combine (show the diff and fail). `--fix-plan` instead emits the machine-readable fix plan — each fixable finding's exact byte-region replacements as JSON (or tab-separated text) — without writing, so an editor or agent can preview or apply fixes one at a time (honoring the same suppressions and `--baseline` as `--fix`). Findings can be silenced in source with an inline `; paredit:ignore [rule…]` comment: on its own line it suppresses the next line, trailing after code it suppresses that line, and with no rule names it suppresses every rule — honored uniformly across the report, SARIF, GitHub, and `--fix` outputs. `--fail-on <error\|warning>` gates only on findings at or above a severity (so CI can block on bugs while still reporting style warnings), and SARIF `level` reflects each finding's severity. `--stats` prints a lint-debt rollup instead of individual findings — finding counts by severity, by category, and by rule, plus files-scanned/files-with-findings — honoring the same `--rule`/`--category`/`--baseline` filters. `--report-unused-suppressions` instead reports any `; paredit:ignore` that silences no finding (a stale ignore or a typo'd rule name) and exits 3 if any are found, keeping the ignore list honest in CI. A directive may also carry `-until <date>`; `--report-expired-suppressions` reports any past its date (used or not) and exits 3 if any are found, and `--report-suppressions` lists every directive, used or not, with its scope, rules, reason, and expiry, and always exits 0. `--suppress-path <path>` (repeatable) silences every finding under a path as if it carried `paredit:ignore-file`, for generated/vendored code that cannot hold an inline directive. For adopting the linter on an existing codebase, `--write-baseline <file>` snapshots today's findings and `--baseline <file>` then suppresses those known findings (matched by rule and trimmed-line content, so they survive line shifts) — reporting and gating only on new findings, across the default, `--sarif`, and `--github` outputs. `--fixable` narrows `--list-rules` to just the rules that carry an auto-fix — `paredit fix list` is this pair under a name that says so. |

Most reports accept `--output json` for machine-readable results. Reports whose
output is a list of located findings accept the interchange formats as well —
`sarif`, `junit`, `code-climate`, `csv`, `tsv`, `html`, `markdown`, and
`github` — see [Report output formats](../guide/integrations.md#report-output-formats).
The same reports also accept `--verbosity <quiet|normal|detailed>` for
`--output text`: `quiet` prints only the summary and gate lines, `normal`
(the default) is unchanged from before this flag existed, and `detailed`
adds each finding's full field set as indented lines under its row.
`--output json` always carries full detail regardless of `--verbosity`.

### Choosing and tuning lint rules

With 165 rules, `inspect lint` needs more than an on/off switch per rule. The
flags below are about the rule *set* rather than about any one rule, and all of
them work with `--list-rules` as well as with a scan — so a run can be
inspected before it is made.

| Flag | What it does |
| --- | --- |
| `--preset <minimal\|recommended\|pedantic\|all>` | How wide a net to cast. `minimal` is error-severity rules only; `recommended` (the default) is every stable, non-opinionated rule; `pedantic` adds the naming and documentation conventions; `all` adds the experimental ones. `--list-presets` prints the ladder with the size of each rung. |
| `--experimental` | Adds the experimental rules to whichever preset is in force, rather than jumping to `all`. |
| `--tag <TAG>` | Runs only rules carrying *every* named tag. Tags are orthogonal to categories: `experimental`, `pedantic`, `destructive`, `semantic`, `style`, `cross-file`. `--list-tags` prints each with the rules that carry it. |
| `--deny <RULE\|CATEGORY>` / `--warn <RULE\|CATEGORY>` | Reports the named rules (or every rule in the named categories) at error or warning severity, whatever they ship as. This changes the `--fail-on` gate, the SARIF `level`, and whether `--github` emits `::error` or `::warning` — not only the printed word. Use `--exclude` to silence a rule entirely. |
| `--rule-arg <RULE.KEY=VALUE>` | Retunes one rule's threshold. The rule must declare the key, so a typo fails the run before any file is read; `--explain <rule>` lists the knobs a rule has. |
| `--explain <RULE>` | Prints everything known about one rule: its category, severity, tags, dialects, why it fires, a before/after example, what it deliberately leaves alone, and its tunable settings. |
| `--docs` | Emits the whole rule reference as Markdown, one section per rule grouped by category, generated from the same metadata the report reads. |
| `--timings` | Reports what each rule cost and how often it ran, slowest first, instead of the findings. Measurement is not free, so it is opt-in. |
| `--no-destructive-fixes` | With `--fix`, holds back the fixes tagged `destructive` — the few whose rewrite can change runtime behaviour rather than only spelling. |

Every finding also carries a content-derived `id` (in the JSON report, the fix
plan, and as a SARIF `pareditFindingId` fingerprint). It is derived from the
rule and a whitespace-normalized prefix of the reported form, so it survives
reformatting and unrelated edits above it — which is what makes it usable as a
key for baselines and suppression tooling.

### Rules a project writes for itself

The 165 shipped rules are the ones everybody gets. A rule like "in *this*
codebase, `defentity` must always be given a `:table`" is the majority of what
a mature project wants and none of what a linter can ship, so a project writes
those itself, in Lisp, in `.paredit/rules/*.lisp`:

```lisp
(defrule entity-needs-table
  :category malformed          ; optional; defaults to suspicious
  :severity error              ; optional; defaults to warning
  :description "a defentity with no :table option"
  :pattern (defentity ?name ...)
  :message "defentity needs a :table"
  :fix (defentity ?name :table "TODO"))   ; optional

(deftest entity-needs-table
  (:matches  "(defentity user)")
  (:no-match "(defentity user :table \"users\")")
  (:fix "(defentity user)" "(defentity user :table \"TODO\")"))

(deprecate legacy-connect :use connect :reason "removed in 3.0")
```

The directory is read automatically when it exists; `--custom-rules <DIR>`
points elsewhere. A rule file that does not load fails the run — a project that
has written a rule and sees a green build has been told the rule passed.

Three spellings are special in a `:pattern`:

| Spelling | Matches |
| --- | --- |
| `?name` | one form, and binds it; a repeated `?name` must match the *same* form, which is how `(setf ?p ?p)` says "self-assignment" |
| `?_` | one form, binding nothing, so two `?_` need not agree |
| `...` | the rest of the enclosing list, however many forms |

Everything else matches itself: a symbol case- and package-insensitively (so
`(cl:print x)` matches `(print ?x)`), a string or number exactly.

A `:fix` is a template in the same language; each `?name` is replaced by the
source it bound, verbatim, so the parts the fix does not change keep their
formatting. A template naming a variable the pattern does not bind is rejected
when the file loads.

`--test-rules` runs every `deftest` and exits 3 on a failure. `:no-match` is
the clause that earns its keep: it is what catches a pattern that grew broader
than its author meant.

Custom findings are then indistinguishable from shipped ones — same suppression
comments, same baseline, same stable ids, same `--fail-on` gate, same SARIF and
GitHub output, same `--fix`. `--list-rules` lists them in a separate
`custom_rules` block so the two are still tellable apart.

### Suppressing findings in source

Three scopes, each spelled as a comment:

| Directive | Scope |
| --- | --- |
| `; paredit:ignore [rule…]` | One line: its own if code precedes the comment, otherwise the next. |
| `;; paredit:ignore-next-form [rule…]` | Every line of the next top-level form, however many it spans. |
| `;; paredit:ignore-file [rule…]` | Every line of the file. |

With no rule names a directive silences every rule in its scope. Anything after
`--` is a free-text reason; `--require-suppression-reason` turns a missing one
into a reported problem, so a project can insist that every silenced finding
says why.

Any of the three directives may also carry `-until YYYY-MM-DD` right after the
token — `paredit:ignore-until`, `paredit:ignore-next-form-until`,
`paredit:ignore-file-until` — still followed by the same optional rule names
and reason. A directive past its date keeps suppressing (expiry is a prompt to
renew or delete it, not a silent deadline); a missing or malformed date makes
the whole comment not a directive at all, so a typo shows up as the finding
reappearing rather than as a suppression that silently never expires.

`--report-unused-suppressions` reports the directives that silence nothing (a
stale ignore or a typo'd rule name) and exits 3 if any are found.
`--remove-unused-suppressions` is its write side: it deletes those directives
in place and *narrows* the partly stale ones, keeping the rule names that are
still doing work — so cleaning up a typo cannot silently un-suppress a finding
somebody meant to ignore. `--report-expired-suppressions` is the `-until`
counterpart: it reports directives whose date has passed, whether or not they
are still silencing something, and exits 3 if any are found — a CI gate
against a suppression outliving the reason it was written for.
`--report-suppressions` lists every directive, used or not, with its scope,
rules, reason, and expiry — a full inventory, one step past
`--report-unused-suppressions`'s stale-only view; it always exits 0.

`--suppress-path <path>` (repeatable) silences every lint finding under a
path, as if the whole file carried `paredit:ignore-file`, without editing the
file — for generated code and vendored dependencies that get overwritten and
so cannot carry an inline directive. Scoped to `inspect lint` alone; other
commands still see these files. The `lint.suppress-paths` key in
`paredit.toml` sets it from configuration, resolved relative to the file that
sets it, the same way `paths.exclude` is (see [Configuration](configuration.md)) —
but unlike `paths.exclude`, it hides only lint findings, not the file itself.

## Edit

`paredit edit` makes one structural transformation on the form selected by
`--path` or `--at` (see [Selecting forms](selectors.md)). By default the
rewritten document is printed to standard output and the file is untouched.
Mutating commands also accept:

- `--diff` — print a unified diff against the input instead of the whole
  rewritten document.
- `--write` — persist the result back to `--file`. The write is refused if
  the rewritten document no longer parses, and file writes are staged with
  automatic rollback.

| Command | Purpose |
| --- | --- |
| `format` | Print a canonical, indentation-based rendering. |
| `repair-unclosed-lists` | Append matching delimiters for parser-detected unclosed lists; refuse all other parse errors. |
| `select` | Print the S-expression selected by `--path` or `--at`. |
| `replace` | Replace the selected S-expression with replacement text. |
| `kill` | Remove the selected S-expression. `--to-ring` pushes it onto the kill ring first. |
| `copy` | Print the selected S-expression together with the own-line comment block written above it. `--to-ring` pushes it onto the kill ring. |
| `duplicate` | Write a second copy of the selected S-expression immediately after it, carrying its own-line comment block along and following the same layout rule `yank` does — a form on its own line gets one, a form sharing a line stays inline. `copy --to-ring` then `yank --placement after` reaches the same result in two calls, at the cost of whatever was on the kill ring; this touches neither. |
| `normalize-quotes` | Rewrite the selected quote between its two spellings — `'x` / `(quote x)` and `#'f` / `(function f)` — with `--style shorthand` (the default) or `--style longhand`. The reader expands one into the other before anything else sees it, so which a file uses is a style choice that tends to drift: a macro emitting `(quote ...)` sits beside hand-written `'...` and neither `format` nor any lint rule reconciles them, because both are correct. A form already in the requested spelling is left alone rather than refused, so this is safe to run over a whole file; a form that is not a quote at all *is* refused, because that is a selector that missed. Quasiquote and unquote are deliberately absent — `` ` ``, `,` and `,@` have no portable list spelling to normalize to. |
| `yank` | Paste a kill ring entry `--placement before\|after\|replace` the selection. `--index` picks the entry, newest first. |
| `wrap` | Wrap the selected S-expression. `--delimiter paren\|bracket\|brace\|doublequote` chooses the delimiter; `doublequote` produces a string literal and escapes the selection's own quotes and backslashes. `--prefix quote\|quasiquote\|unquote\|unquote-splicing\|sharp-quote` attaches reader sugar instead. |
| `unwrap-prefix` | Remove the selected expression's outermost reader prefix, or every one of them with `--all-prefixes`. |
| `splice` | Remove one list pair while keeping its children. |
| `split` | Split the enclosing list in two immediately before the selected expression. |
| `join` | Join the selected list with its next sibling list, or two adjacent string literals, into one form. |
| `splice-killing-backward` | Splice the enclosing list, keeping the selection and following siblings while removing preceding ones. |
| `splice-killing-forward` | Splice the enclosing list, keeping the preceding siblings while removing the selection and following ones. |
| `convolute` | Reverse the nesting of the two lists enclosing the selected list. |
| `raise` | Replace the selected expression's parent list with the selection. `--levels N` climbs N enclosing lists in one step. |
| `transpose-forward` | Exchange the selected expression with its next sibling while keeping trivia in place. |
| `transpose-backward` | Exchange the selected expression with its previous sibling while keeping trivia in place. |
| `slurp-forward` | Pull the next sibling into the selected list. |
| `slurp-backward` | Pull the previous sibling into the selected list. |
| `barf-forward` | Push the last child out of the selected list. |
| `barf-backward` | Push the first child out of the selected list. |
| `transpose` | Exchange the selection with any other expression in the same list, adjacent or not. The partner is addressed by `--with-path`, `--with-at`, or `--with-select` — its own flag names, because the primary selector already claims `--path`, `--at` and `--select`. |
| `navigate` | Print the `--path` that `--direction forward\|backward\|up\|down` lands on. Text output is the bare path, so it composes directly into the next command. |
| `delete-forward` | Delete the character at `--at`, refusing anything that would unbalance the document. An empty `()` or `""` is deleted as a pair. |
| `delete-backward` | Delete the character before `--at`, under the same rules. |
| `newline` | Insert a newline at `--at` and reindent the definition it lands in. `--no-reindent` inserts only. |
| `reindent-defun` | Reindent the selected definition to the Emacs convention without rewrapping its lines. |
| `split-string` | Split the string literal containing `--at` into two adjacent literals. The inverse of `join` on two strings. |
| `escape-string` | Escape the selected string literal's contents one level, so it can be embedded in another string. |
| `unescape-string` | Reverse one level of escaping. Collapses `\\` and `\"` only, and refuses any other sequence rather than guessing what the dialect meant by it. |

For example, preview then apply a wrap of the third child of the first
top-level form:

```sh
paredit edit wrap --file source.lisp --path 0.2 --diff
paredit edit wrap --file source.lisp --path 0.2 --write
```

### The kill ring

`kill --to-ring`, `copy --to-ring` and `yank` share a ring file. Its path comes
from `--ring`, then `$PAREDIT_KILL_RING`, then `.paredit/kill-ring.json` —
repository-relative by default, so two checkouts do not share a clipboard by
accident, and the ring stays a file you can read, diff, or delete. `--ring-size`
caps how many entries it keeps; entry `--index 0` is always the most recent.

`kill --to-ring` stores exactly what it removed. `copy --to-ring` stores the
comment block as well, because that is what `copy` prints.

### Character edits and where they are safe

`delete-forward`, `delete-backward` and `newline` address a byte offset rather
than a form, and refuse any offset where the edit would change structure: a
delimiter with something inside it, the whitespace holding two symbols apart, a
comment's opening token, the inside of a string or symbol. Run
[`inspect context-at`](#inspect) first to learn which offsets those are without
attempting the edit — it reports the kind of text at an offset and whether a
character edit there is inert.

## Refactor

`paredit refactor` contains the reviewable workflow commands and the semantic
refactorings they gate. See [Refactor workflow](../guide/workflows.md) for the
plan/preview/verify/apply lifecycle.

### Workflow commands

| Command | Purpose |
| --- | --- |
| `plan` | Produce an ordered, gated refactoring plan for AI coding agents. |
| `verify` | Verify pre/post refactoring invariants for agents and CI gates. |
| `preview` | Preview exact refactoring rewrites without modifying files. |
| `check` | Validate a refactor preview manifest without writing files. |
| `status` | Summarize a preview manifest into agent-safe next actions. |
| `apply` | Apply a previously generated preview manifest with hash guards. `--undo-out` records a reverse-edit journal; `--verify-command` runs a check afterwards and restores every written file when it fails. |
| `undo` | Restore the pre-refactor content recorded by `apply --undo-out`. Refuses unless every file is still byte-for-byte what the write produced, so a journal cannot be applied twice or over an intervening edit. |
| `diff` | Render a verified diff from a preview manifest without writing files. |
| `step` | Walk a preview manifest one edit at a time, taking only the steps you accept. `refactor apply` is all-or-nothing, which is right for applying and wrong for reviewing: a reader who disagrees with one of forty edits would otherwise have to discard the manifest. Steps are numbered in source order and each carries its line, the text it replaces, and the source line it sits on. `--accept`/`--skip` take a selector (`all`, `3`, `1,4`, `2-5`); `--interactive` reads one `y`/`n`/`a`/`q` decision per step from stdin. `--diff` previews, `--write` applies, `--fail-on-partial` gates a script that took a subset by accident. Both hash guards still apply, and a subset that would not parse is refused before anything is written. |
| `patch` | Carry the difference between two versions of one file (`--from`/`--to`, neither written) onto a third (`--apply-to`), matching each change by structure rather than by position — so it lands whatever the target's formatting and wherever in the file the form sits. Each change is reported as `applied`, `not-found` (the target never had the form), `ambiguous` (several sites match; `--all` applies to all of them), or `unportable` (a top-level insertion, which names no existing form to anchor on). Plans by default; `--diff` previews, `--write` applies, `--fail-on-unapplied` gates on a partial port. A patch that would produce source this tool cannot parse is refused before anything is written. |
| `workspace-plan` | Discover Lisp sources under roots and build a gated refactor plan. |
| `workspace-preview` | Discover sources and preview exact refactoring rewrites. |
| `workspace-execute` | Execute a workspace refactor with preview gates and post-write verification. |

### Definition and file layout

| Command | Purpose |
| --- | --- |
| `remove-definition` | Plan or remove a top-level definition from one file. |
| `remove-unused-definitions` | Plan or remove unused top-level definitions across files. |
| `add-ignore-declaration` | Insert `(declare (ignore …))` for every parameter `inspect unused-parameters` reports as unused. The write side that report never had: it could name the problem and nothing in the tool could fix it, so acting on it meant hand-editing every definition it listed. The declaration goes after the lambda list, past a docstring and past any declarations already there — several `declare` forms may head one body, so an existing one is followed rather than merged into. One declaration per definition names every unused parameter of it. A parameter already declared ignored never reaches this, because the report counts its appearance in the declaration as a reference. Common Lisp and Emacs Lisp only; other dialects plan nothing rather than refusing. |
| `fold-constants` | Replace every expression `inspect constants` proves constant with the literal it evaluates to — `(+ 1 2)` becomes `3`. The write side of that report, which its own documentation already described as "the input a `fold-constants` edit would take". It takes that report's spans and reader spellings rather than folding anything itself, so the two cannot disagree. Quoted forms are safe by construction rather than by a guard: the value layer refuses to evaluate through `'` and `` ` ``, so `'(+ 1 2)` is never reported foldable and never reaches the edit. `--min-saved-bytes` folds only the profitable ones, since folding a short form to a longer string literal is a loss. |
| `move-definition` | Plan or move a top-level definition between files. |
| `split-file` | Plan or split multiple top-level definitions into another file. |
| `sort-definitions` | Plan or sort contiguous top-level definition blocks in one file. |
| `move-form` | Plan or move any top-level form between files. |
| `insert-top-level` | Insert exactly one top-level S-expression before, after, or at the end of a file. |
| `replacement-plan` | Convert duplicate groups into reviewed replace-forms batches. |
| `replace-forms` | Plan or replace multiple reviewed forms in one file. |

### Packages

| Command | Purpose |
| --- | --- |
| `add-export` | Plan or add a symbol to a Common Lisp `defpackage` `:export` option. |
| `sort-package-exports` | Plan or sort `defpackage` `:export` symbol designators. |
| `sort-package-options` | Plan or sort `defpackage` option forms. |
| `merge-package-options` | Plan or merge duplicate `defpackage` option forms. |
| `rename-package` | Plan or rename package designators and qualified prefixes. |

### Renames

| Command | Purpose |
| --- | --- |
| `rename-at` | Rename whatever symbol occupies a byte offset, dispatching to the owning namespace and scope. |
| `rename-symbol` | Rename exact atom occurrences without touching strings or comments. |
| `rename-in-form` | Rename exact atom occurrences inside one selected form. |
| `rename-binding` | Rename one local binding and only the references in its lexical scope. |
| `rename-symbols` | Plan or apply an exact atom rename across explicit files. |
| `rename-function` | Plan or apply a Common Lisp callable definition and designator rename. |
| `rename-macrolet` | Plan or apply a `macrolet`/`compiler-macrolet` binding and call-site rename. |
| `rename-symbol-macro` | Plan or apply a `define-symbol-macro` binding and value-reference rename. |
| `rename-local-function` | Plan or apply a `flet`/`labels` local function binding and call-site rename. |

### Calls and functions

| Command | Purpose |
| --- | --- |
| `replace-function-calls` | Plan or replace callable call-site heads across explicit files. |
| `wrap-function-calls` | Plan or wrap callable call sites in another function or macro call. |
| `unwrap-function-calls` | Plan or remove a unary wrapper around callable call sites. |
| `unwrap-call` | Replace one selected wrapper call with one selected argument. |
| `thread-expression` | Convert a nested call chain into a thread-first or thread-last pipeline. |
| `unthread-expression` | Convert a threading pipeline back into nested calls. |
| `extract-function` | Extract the selected expression into a top-level function with inferred parameters. |
| `extract-local-function` | Extract the selected expression into a Common Lisp `flet` or `labels` binding. |
| `extract-constant` | Extract the selected expression into a top-level constant. |
| `inline-function` | Inline one selected function call using a selected function definition. |
| `inline-lambda` | Replace a safe, immediately invoked Common Lisp lambda with a parallel `let`. |
| `inline-local-function` | Inline the sole direct call in a safe, single-binding Common Lisp `flet` form. |
| `inline-symbol-macro` | Expand a conservative single-binding Common Lisp `symbol-macrolet` form. |
| `inline-literal-constant` | Inline an immutable self-evaluating Common Lisp `defconstant` value. |
| `convert-labels-to-flet` | Convert a non-recursive Common Lisp `labels` form into `flet`. |
| `convert-flet-to-labels` | Convert a Common Lisp `flet` form into `labels` when definition references cannot be captured. |
| `rename-block` | Rename a selected Common Lisp `block` and matching `return-from` references. |
| `rename-tag` | Rename one tag in a selected Common Lisp `tagbody` and matching `go` references. |
| `remove-unused-block` | Remove a selected Common Lisp `block` with no matching `return-from`. |
| `remove-unused-tag` | Remove an unreferenced tag from a selected Common Lisp `tagbody`. |

### Parameters and bindings

| Command | Purpose |
| --- | --- |
| `add-function-parameter` | Add a parameter to a selected function and explicit call sites. |
| `move-function-parameter` | Move one positional parameter in a function and its call sites. |
| `swap-function-parameters` | Swap two positional parameters in a function and its call sites. |
| `reorder-function-parameters` | Reorder all positional parameters in a function and its call sites. |
| `remove-function-parameter` | Remove one positional parameter from a function and its call sites. |
| `introduce-let` | Replace the selected expression with a local binding in the enclosing list. |
| `inline-let` | Inline a single local let binding into its body. |
| `convert-let-to-let-star` | Convert a Common Lisp or Emacs Lisp `let` to `let*` when later initializers do not reference earlier bindings. |
| `convert-let-star-to-let` | Convert a Common Lisp `let*` to `let` when later initializers do not reference earlier bindings. `--allow-partial` splits off the longest independent prefix into an outer `let` wrapping a nested `let*` instead of refusing. |
| `convert-do-star-to-do` | Convert a Common Lisp `do*` to `do` when later initializers and step expressions do not reference earlier bindings. |
| `convert-prog-star-to-prog` | Convert a Common Lisp `prog*` to `prog` when later initializers do not reference earlier bindings. |
| `merge-nested-let-star` | Merge a directly nested Common Lisp or Emacs Lisp `let*` into one sequential binding form. |
| `split-let-star` | Split a Common Lisp or Emacs Lisp `let*` into nested sequential binding forms at `--binding-index`. |
| `merge-nested-let` | Merge directly nested Common Lisp or Emacs Lisp parallel `let` forms when inner initializers are independent. |
| `merge-nested-flet` | Merge directly nested Common Lisp `flet` forms when inner definitions do not reference outer local functions. |
| `split-let` | Split a Common Lisp or Emacs Lisp parallel `let` at `--binding-index` without capturing initializer references. |
| `eliminate-empty-binding-form` | Remove an empty Common Lisp or Emacs Lisp `let` or `let*` from a known expression position. |
| `flatten-progn` | Flatten directly nested Common Lisp or Emacs Lisp `progn` forms in a safe expression context. |
| `convert-if-to-cond` | Convert a Common Lisp or Emacs Lisp `(if test then [else])` form to `cond`. |
| `convert-cond-to-if` | Convert simple Common Lisp or Emacs Lisp `cond` clauses to nested `if` forms. |
| `convert-when-to-if` | Convert a Common Lisp or Emacs Lisp `when` form to `if`. |
| `convert-unless-to-if` | Convert a Common Lisp or Emacs Lisp `unless` form to `if`. |
| `convert-if-to-when` | Convert a Common Lisp or Emacs Lisp `if` without a meaningful else to `when`. |
| `convert-if-to-unless` | Convert a Common Lisp or Emacs Lisp `if` with a literal `nil` then branch to `unless`. |
| `remove-unused-binding` | Plan or remove one unused local let binding. |

## Query

`paredit query` promotes the `--query` pattern language from a *selector* —
one of eight ways to name the form another command should act on — to a
capability of its own. The difference is reach and direction: a selector names
a form in one named file, and these ask about a whole workspace and can
rewrite what they find.

| Command | Purpose | Its own flags |
| --- | --- | --- |
| `find` | Report every form in the workspace whose shape matches `--query`, with its captures, path, and stable selector id. | `--preview-bytes N` bounds the source shown per match; `--fail-on-match` and `--fail-on-no-match` are the two CI gates — a shape that must not appear, and one that must. |
| `count` | Count matches per pattern and per file, for several `--query` patterns side by side. | `--per-file` breaks the totals down by file, and `--include-empty` keeps the files no pattern reached (off by default: over a repository they are the overwhelming majority). `--fail-on-match` gates. |
| `replace` | Rewrite every match with a `--rewrite` template. Prints the plan by default; `--diff` previews, `--write` applies. | `--check` writes nothing and exits 3 if any replacement is pending. `--allow-comment-loss` and `--include-quoted` waive the guards below. |

All three take the full workspace input surface (`--since`, `--from-git`,
`--include`, …), so `query find --query '(eq ?x ?x)' --fail-on-match --since
origin/main .` is a CI gate on a shape a branch introduced.

```sh
# Where does this shape appear?
paredit query find --query '(defun ?name ...)' src/

# How is a migration progressing?
paredit query count --query '(if ?t ?a nil)' --query '(when ?t ?a)' src/

# Rewrite it. Nothing is written without --write.
paredit query replace --query '(if ?t ?a nil)' --rewrite '(when ?t ?a)' --diff src/
paredit query replace --query '(old-name ?args...)' --rewrite '(new-name ?args...)' --write src/
```

### What `query replace` refuses

The rewrite is a splice of verbatim source: `?name` in the template is
replaced by exactly the bytes the pattern's `?name` matched, so a captured
`1.0d0` stays a double float and a captured string keeps its escapes. Three
situations are refused rather than rewritten, because all three would leave
source that still parses and is still wrong — which is exactly what the
reparse guard cannot see:

| skipped as | why |
| --- | --- |
| `overlapping` | An enclosing match was rewritten. Run the command again to reach the nested one. |
| `comment-loss` | A comment inside the match is carried by no capture the template uses, so the rewrite would delete it. `--allow-comment-loss` overrides. |
| `quoted` | The match is inside quoted data. `'(a (if x y nil) b)` is a *list literal*: it has the shape the pattern matches, and rewriting it changes the program's data rather than its code. `--include-quoted` overrides. |

All three are counted in every output format, including when the count is
zero, so "37 matched, 35 rewritten" is never something to discover by reading
a diff.

A rewrite reflows the matched form onto the template's own layout. Running
`paredit edit format --write` afterwards is usual.

## Fix

`paredit fix` is the write side of `inspect lint`, under a name that says it
writes. It reimplements nothing: each leaf builds the arguments its
`inspect lint` spelling would have produced and runs the same engine.

| Command | Purpose | Was |
| --- | --- | --- |
| `apply` | Apply every available auto-fix in place and report what changed. | `inspect lint --fix` |
| `check` | Write nothing; exit 3 if any auto-fix is still pending. | `inspect lint --fix --check` |
| `plan` | Emit the machine-readable fix plan without writing. | `inspect lint --fix-plan` |
| `list` | List the rules that carry an auto-fix. | `inspect lint --list-rules --fixable` |

Unlike every other writing command here, `fix apply` writes in place with **no
`--write`**. That is inherited from `inspect lint --fix`, and inheriting it
exactly is what makes the two spellings the same bytes. Use `--diff` to
preview, `fix check` to gate, and the global `--dry-run` to refuse the write.

All four take the rule-selection flags (`--rule`, `--category`, `--exclude`,
`--tag`, `--preset`, `--experimental`, `--custom-rules`) and
`--no-destructive-fixes`. The flags that shape a *report* rather than a fix
run — `--emit`, `--baseline`, `--stats`, `--timings`, `--fail-on` — stay on
`inspect lint`, which is where they mean something.

```sh
paredit fix list
paredit fix apply --diff src/
paredit fix apply --rule redundant-progn --no-destructive-fixes src/
paredit fix check src/          # exit 3 when fixable lint is outstanding
```

## Migrate

`paredit migrate` runs a *recipe*: a named list of `--query`/`--rewrite` steps
in a fixed order, scoped to the dialects the rewrite is correct for. Both
properties are why this is not just repeated `query replace` invocations:

- **Order.** `(if (not p) a nil)` should become `(unless p a)`, not
  `(when (not p) a)`. Which one it becomes depends entirely on which step runs
  first, and the recipe fixes that once.
- **Scope.** `(incf x)` → `(cl-incf x)` modernizes Emacs Lisp and *breaks*
  Common Lisp, where `incf` is the correct spelling. A recipe skips every file
  outside its `:dialects` and reports how many, so a run that changed nothing
  says why.

| Command | Purpose |
| --- | --- |
| `list` | List the recipes this run can reach, with each one's step count, dialect scope, and origin. |
| `explain` | Print one recipe's steps and notes before running it. |
| `run` | Apply a recipe's steps in order. Prints the plan by default; `--diff` previews, `--write` applies, `--check` exits 3 when the migration is not yet applied. |

All three take `--recipes <DIR>` to load a project's own recipes from
somewhere other than `.paredit/migrations`. `explain` and `run` resolve the
same catalogue, so what `explain` prints is what `run` will do.

Shipped recipes:

| Recipe | Dialects | What it does |
| --- | --- | --- |
| `elisp-cl-lib` | `emacs-lisp` | cl.el's unprefixed names to their cl-lib `cl-` spellings. Deliberately excludes `flet`/`labels` (cl.el's were dynamic, cl-lib's are lexical) and `first`…`tenth` (which may be locally defined). |
| `nil-conditionals` | `common-lisp`, `emacs-lisp` | One-armed `if` with a `nil` else-branch to `when` and `unless`. |

A project writes its own in `.paredit/migrations/*.lisp` — beside
`.paredit/rules`, where the custom lint rules live — in the same Lisp form the
built-ins use, and a project recipe of the same name shadows a built-in:

```lisp
(defmigration nil-conditionals
  :description "one-armed `if' with a nil else-branch to `when' and `unless'"
  :dialects (common-lisp emacs-lisp)
  :steps ((:query (if (not ?test) ?then nil)
           :rewrite (unless ?test ?then)
           :note "first, so the general step below cannot claim a negated test")
          (:query (if ?test ?then nil)
           :rewrite (when ?test ?then))))
```

```sh
paredit migrate list
paredit migrate explain elisp-cl-lib
paredit migrate run elisp-cl-lib --diff lisp/
paredit migrate run nil-conditionals --check .   # exit 3 when not yet applied
```

`migrate run` skips the same three situations `query replace` does, for the
same reasons, and reports them the same way — the quote guard most of all, since
`nil-conditionals` over a file holding `'(a (if x y nil))` would otherwise
rewrite a data literal.

## Config

`paredit config` reads `paredit.toml`, never source. It answers what this build
is going to do and why, which is a different question from any `inspect`
report. See [Configuration](configuration.md) for the file format, the layer
order, and `extends`.

| Command | Purpose |
| --- | --- |
| `check` | Validate every discovered file and exit 3 if any key is unusable. |
| `show` | Print the effective configuration with the file and line that set each key. |
| `schema` | Print every recognised key with its type, default, and environment variable. |
| `init` | Write a documented starter `paredit.toml` generated from the schema. |

All four accept `--config <FILE>`, `--no-config`, `--no-config-env`, and
`--from <DIR>` to control which layers are consulted.

```sh
paredit config check
paredit config show --changed-only --output text
paredit config show --key lint.preset
paredit config init --dry-run
```

## Generate

`paredit generate` produces new Common Lisp source from analysis this tool
already does elsewhere, rather than transforming a form that already exists —
the direction `edit` and `refactor` do not cover. Common Lisp only: every
generator refuses a non-Common-Lisp dialect.

| Command | Purpose |
| --- | --- |
| `defpackage` | Generate a `defpackage` form from a file's own definitions (export) and qualified symbol references (`:use`). |
| `defsystem` | Generate an ASDF `defsystem` form from a directory of Lisp sources: one `(:file ...)` per source, `:depends-on` inferred from packages used but not defined in the directory. |
| `tests` | Generate a `deftest` skeleton for every definition `inspect test-map` reports as untested. |
| `accessors` | Add `:accessor` to every `defclass` slot that has neither `:accessor`, `:reader`, nor `:writer`. |
| `defgeneric` | Generate a `defgeneric` for a name whose `defmethod` forms have no declaration, from the methods' congruent required arity. |
| `docstring` | Insert a docstring template at the position Common Lisp expects it: after the lambda list, at the fixed value slot, before a `defstruct`'s slots, or as a `(:documentation ...)` option. |

`defpackage`, `defgeneric`, `accessors`, and `docstring` print a plan by
default; `--write` applies it to `--file` and `--diff` previews it as a
unified diff. `defsystem` and `tests` operate on a directory or a list of
files and write with `--write` to `<directory>/<name>.asd` or `--into <FILE>`
respectively.

```sh
paredit generate defpackage --file src/app.lisp --write
paredit generate defsystem . --write
paredit generate tests src/app.lisp --into tests/app-tests.lisp --write
paredit generate accessors --file src/point.lisp --select name:point --write
paredit generate defgeneric --file src/app.lisp --write
paredit generate docstring --file src/app.lisp --select name:render --write
```
