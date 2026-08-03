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
The remaining four are terminal presentation, not safety, and apply nowhere
else:

| Flag | What it does |
| --- | --- |
| `--color <auto\|always\|never>` | Whether text output may use ANSI color. `auto`, the default, colors if and only if the destination stream is a terminal, `NO_COLOR` and `TERM=dumb` are both unset, and `CLICOLOR_FORCE`/`FORCE_COLOR` have not already decided the question in the other direction (the off signals win over the on signals). The hues it may use, and the rule that none of them ever carries a signal alone, are in [Terminal Color](color-palette.md). |
| `--paginate` | Delegates stdout to `$PAGER` (falling back to `less`) when it is a terminal. Off by default — unlike `--color`, paging changes the interaction itself, so it has to be asked for. |
| `--plain-language` | Adds one line to a text-mode failure paraphrasing its error category — the same `category_description` the `--output json` envelope has always carried. Off by default, so the existing stderr rendering is byte-for-byte unchanged. |
| `--explain-error` | On a failure, prints the full [Error Codes](errors.md) section for that code inline — the same text `doc_url` links to, extracted from the same markdown embedded in the binary at compile time. No network access. Named `--explain-error` rather than `--explain` because `inspect lint --explain <RULE>` and `migrate explain` already claim that spelling for unrelated things. |

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
| `use-widening` | Report each defpackage `:use` clause as a namespace-widening risk — a whole-package import, versus the narrower explicit symbol list `:import-from` gives. Does not flag `:import-from`; the point is to make the wider alternative visible where a narrower one exists. |
| `package-conflicts` | Report distinct defpackage forms that claim the same package name or nickname. |
| `redefinitions` | Report top-level definitions of the same category and name declared more than once. |
| `undefined-packages` | Report in-package forms naming a package no analyzed defpackage declares. |
| `context-at` | Report what kind of text sits at a byte offset — code, a string, a comment, a list delimiter, reader sugar, or the whitespace between forms — together with the enclosing list, the nesting depth, and the stack of open delimiters. The question to ask *before* a character edit rather than after a refused one: `edit delete-forward` and `edit newline` decline every offset this reports as carrying structure, and `--fail-on-structural` turns that into an exit code. |
| `writability` | Report whether a write to `--file` would succeed — right now, without writing anything — by staging a same-size placeholder exactly as a real write would (so a full disk fails this the same way it would fail the real write) and discarding it instead of publishing it. `--file` need not exist yet. The answer `--dry-run` cannot give: `--dry-run` refuses every write unconditionally and says nothing about whether it would have worked. |
| `data-check` | Report schema-free structural sanity issues in an S-expression *data* file: a plist or alist with the same key spelled twice (the later value silently overrides the earlier one), a plist with a trailing keyword and no value, and a top-level list of same-shaped tuples with one entry whose arity does not match its siblings. No schema is read or required — each check fires only on a shape the file's own repetition already implies, conservatively, so a real mismatch is worth trusting. These baseline checks always run; `--format` (auto-detected per file, or overridden explicitly) adds convention-specific checks on top: Emacs `custom-set-variables` entry shape, EDN's ban on code-only Clojure reader macros, `.dir-locals.el`'s alist-of-alist shape (plus a presence-only flag for an `eval` key), and routing `.rktd` Racket data files into this report at all (`#lang` alone cannot mark a Racket file as data, since every named language, `typed/racket` included, is still executable code). `.paredit/rules`/`.paredit/migrations` are deliberately not a format here — `inspect check --paredit-config` already validates them, with checks (`RulesetError`s, cross-file collision detection) this shape-only report could not add to. |
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
| `license-headers` | Report files missing a leading license-header comment block, and files whose header text disagrees with the majority header text across the analyzed fileset — a stale or hand-edited copy nobody noticed. Separate from `licenses`, which checks the `defsystem` `:license` keyword rather than file contents. `--fail-on-missing-header` gates only on missing headers; an inconsistent-but-present header is a review-worthy finding, not a hard failure. |
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
| `macro-hygiene` | Report the five ways a macro template betrays its caller: binding a literal name inside a quasiquoted template (variable capture, since these macros are unhygienic), unquoting one parameter more than once or referencing a non-side-effect-free `symbol-macrolet` expansion more than once (multiple evaluation of the caller's form), unquoting parameters in an order the lambda list does not write (parameter reordering), nesting quasiquotes three or more levels deep, and — Emacs Lisp only — a macro with no leading `(declare (indent …))`/`(declare (debug …))`. A name bound to `(gensym)` outside the template is recognised and not reported. Also covers Emacs Lisp, Clojure, Janet, Hy, Carp, Fennel and LFE; Scheme and Racket are excluded because `syntax-rules` makes hygiene a language guarantee. `--fail-on-risk` exits 3 if *any* of the five is reported — it is all-or-nothing, and per-risk gating is `inspect lint`'s job, since each of the five also ships as its own lint rule. |
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
| `magic-numbers` | Report numeric literals inside a function or method body that fall outside an idiomatic allow-list (`0`, `1`, `-1`, `2`), suggesting a named `defconstant`/`defparameter` extraction. Does not flag the literal directly bound by a constant-definition form — that is the definition, not a use. |
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
| `self-recursive-tail-call` | Report a function's own name called in tail position of its body (through `if`/`cond`/`case`/`when`/`unless`/`and`/`or`/`let`/`progn`, transparently), annotated with whether the target dialect guarantees tail-call optimization there: guaranteed for Scheme, Racket, LFE, and Fennel; not performed for Emacs Lisp, Hy, and Clojure (use `recur`); implementation-defined for Common Lisp; not modeled for Janet and Carp. Report-only; a call buried in another call's arguments is not in tail position and is not reported. |
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
| `giant-conditional-form` | Report a `let`/`let*`/`cond`/`case`-family (`case`, `ecase`, `ccase`, `typecase`, `etypecase`, `ctypecase`) form carrying more bindings or clauses than a configurable threshold (`max-clauses`, default 8), a candidate for splitting into smaller, named pieces. Report-only; the split itself is a design decision. |
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
| `package-level-shadowing` | Report an inner `let`/`let*` binding or a `defun`/`defmacro`'s own lambda-list parameter that reuses the name of a top-level `defun`/`defvar`/`defparameter`/`defconstant`/`defmacro` in the same file, making the outer definition unreachable for the rest of the binding's scope. The wider case `inspect shadowed-bindings` (lexical shadowing between nested scopes) does not cover. Report-only. |
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
| `leftover-print-debug` | Report a bare debug-print call left in committed source — `princ`/`print`/`prin1`/`pprint` (Common Lisp), `message` (Emacs Lisp), `println`/`prn` (Clojure), `display` (Scheme), `displayln` (Racket), `print`/`pp` (Janet), `print` (Fennel, Hy). Auto-fixable: removes the call outright when it is a bare top-level form or a non-last form of its enclosing implicit-progn/`cond`-clause body; the last form of such a body is reported with no fix, since removing it would change the body's return value. |
| `leftover-trace-call` | Report `trace`/`untrace` used as a statement, left in committed source. Common Lisp and Emacs Lisp only. Auto-fixable under the same position rule as `leftover-print-debug`. |
| `leftover-break-call` | Report a Common Lisp `(break ...)` left in committed source. Auto-fixable under the same position rule as `leftover-print-debug`. Does not extend to Racket's differently-semantic `break`. |
| `leftover-inspect-call` | Report a Common Lisp `(inspect x)` or `(describe x)` left in committed source. Auto-fixable under the same position rule as `leftover-print-debug`. |
| `leftover-time-benchmark-call` | Report a Common Lisp `(time form)` wrapper left in committed source. Auto-fixable unconditionally, in every position including tail: per the CLHS, `time` returns exactly `form`'s own value(s), so unwrapping to `form` never changes what the enclosing code returns. |
| `leftover-step-call` | Report a Common Lisp `(step form)` wrapper left in committed source. Auto-fixable unconditionally, for the same CLHS-return-value reason as `leftover-time-benchmark-call`. |
| `commented-repl-transcript` | Report a `;`-comment block heuristically shaped like a pasted REPL session — two or more prompt-shaped lines, or at least one prompt-shaped and one `=>`-result-shaped line. Report-only: comments live outside the node tree a rewrite walks, so a removal here cannot be proven safe. |
| `leftover-format-debug-marker` | Report a `(format t "...")` or `(format *standard-output* "...")` whose literal control string case-insensitively contains a `DEBUG`/`DBG` marker as a distinguishable word/prefix (`"~&DEBUG: ~a"`, and also `"DEBUGGING"`: only the character before the marker is checked). Auto-fixable under the same position rule as `leftover-print-debug`. Disjoint from `leftover-print-debug`, which never matches `format`. |
| `around-method-missing-call-next-method` | Report an `:around` method whose body never calls `call-next-method`, so the rest of the method combination — the primary method included — never runs. Common Lisp only, report-only: an `:around` that deliberately short-circuits (a cache hit, a refusing guard) is a real idiom, so what is reported is that the choice was made, not that it was wrong. |
| `defclass-required-slot-no-initform-or-initarg` | Report a `defclass` slot declared with neither `:initform` nor `:initarg` that a method in the same file reads; nothing can initialize such a slot, so the read hits an unbound slot. Common Lisp only, report-only. |
| `defclass-slot-shadowing` | Report a subclass slot that redeclares a slot one of its same-file superclasses already declares **at the same `:allocation`**; a redeclaration that changes `:allocation` is exempt. Per CLHS 7.5.3 the redeclaration does not replace the inherited one: `:initarg` and `:reader`/`:writer`/`:accessor` sets are unioned, `:type` is conjoined to `(and T1 … Tn)`, and `:initform`/`:documentation` come from the most specific declaration that supplies one. What is reported is that one slot has two declarations in play whose options merge by four different rules — not that anything is silently replaced. Same file only. Common Lisp only, report-only. |
| `duplicate-defmethod-signature` | Report two `defmethod`s with the same generic-function name, qualifiers and specializers; the later definition replaces the earlier with no diagnostic. Error severity, Common Lisp only, report-only. Distinct from `inspect duplicate-methods`, which describes a whole file's method graph rather than flagging one form. |
| `generic-function-no-methods` | Report a `defgeneric` that nothing in the same file ever specializes — no `defmethod`, no `(:method …)` option, no `add-method` call — and that does not name a `:generic-function-class`. Object-system category, not dead code: the form executes fine and installs a generic function, and what is wrong is generic/method disagreement. Warning severity, which is the engine's floor, because a protocol declared in one file and specialized in others is ordinary ASDF practice and indistinguishable from the defect within one file; the finding states what this file establishes rather than asserting breakage. Common Lisp only, report-only. |
| `method-qualifier-typo` | Report a `defmethod` qualifier outside the standard `:before`, `:after` and `:around`. A misspelled qualifier is not a syntax error, and it does not merely define a method that never runs: under standard method combination it signals an error when the generic is first called, far from the `defmethod` that caused it. Warning rather than error severity, because a `define-method-combination` or a `(:method-combination …)` option in *another* file can license the qualifier; the rule is silenced for a whole file carrying either, including inside `eval-when`/`progn`/`locally`/`macrolet`/`symbol-macrolet`. Common Lisp only, report-only. |
| `print-object-without-print-unreadable-object` | Report a `print-object` method that writes to the stream directly rather than through `print-unreadable-object`. Common Lisp only, report-only. |
| `slot-value-bypasses-accessor` | Report a `(slot-value o 'x)` **read** where the same file declares a `:reader` or `:accessor` for slot `x`; the accessor is the protocol the class published, and unlike `slot-value` it can be specialized. Write position is never reported — `(setf (slot-value o 'x) v)` has no accessor to prefer when the slot carries only a `:reader`, and it is how a computed slot is filled in `initialize-instance :after`. A slot with only a `:writer` or with no accessor at all is exempt, as is `slot-value` inside a hand-written method on the accessor generic itself, where going through the accessor would recurse. Only a constant slot name is read. Common Lisp only, report-only. |
| `cerror-missing-continue-format` | Report a `cerror` whose continue-format-control argument is missing or `nil`, which defeats the continuability that distinguishes `cerror` from `error`. Common Lisp only, report-only. |
| `define-condition-empty-superclass-list` | Report a `define-condition` with an empty `()` supertype list; it defaults to `condition`, not `error`, so `ignore-errors` and a `handler-case` `error` clause will not catch it. Common Lisp only, report-only. |
| `define-condition-missing-report-for-error-type` | Report an `error` subtype with no `:report` option anywhere in its same-file ancestry. Common Lisp only, report-only: what the report *should say* is not something a rewrite can answer. |
| `handler-bind-handler-returns-bare-value` | Report a `handler-bind` handler ending in a bare value; `handler-bind` runs a handler for effect and throws the value away, then declines the condition — `handler-case` is what returns a value. Common Lisp only, report-only. |
| `ignore-errors-wraps-non-error-signal` | Report an `ignore-errors` around a `signal` of a same-file condition that is not an `error` subtype, which it therefore cannot catch. Common Lisp only, report-only. |
| `restart-case-clause-without-report` | Report a `restart-case` clause with no `:report` option. Common Lisp only, report-only. |
| `signal-on-error-condition-returns-silently` | Report a `signal` of a same-file `error` subtype; when unhandled, `signal` returns `nil` rather than entering the debugger, so an error-severity condition is raised and then dropped. Common Lisp only, report-only. |
| `dolist-result-form-references-loop-variable` | Report a `dolist` result form reading the loop variable, which the spec binds to `nil` there rather than to the last element. Common Lisp only, report-only. |
| `dotimes-bound-mutation-has-no-effect` | Report an assignment to the `dotimes` count variable inside the body, which cannot change the number of iterations. Common Lisp only, report-only. |
| `loop-clause-order-violation` | Report a `loop` variable clause placed after a main clause, or a `named` clause that is not first. Error severity, Common Lisp only, report-only. |
| `loop-for-across-statically-known-list` | Report a `loop` `for … across` clause over a value that is provably a list rather than a vector. Error severity, Common Lisp only, report-only. |
| `loop-into-accumulator-kind-conflict` | Report two `loop` accumulation clauses building incompatible kinds — a list and a number — into the same `into` variable. Error severity, Common Lisp only, report-only. Complements `inspect loop`'s `conflicting-accumulation` finding, which covers only the *implicit* result (two accumulation verbs with no `into`) and drops any verb naming an `into` target. |
| `loop-unreachable-finally-clause` | Report a `loop` epilogue form placed after a `finally` clause that already returns, so it can never run. Error severity, Common Lisp only, report-only. |
| `disabled-test-left-in` | Report a test switched off in place by a marker its framework actually honours, rather than removed: Clojure `^:kaocha/skip` or `^:kaocha/pending` metadata on the definition, or — as a *direct* body form, since anything deeper sits inside a conditional and is therefore itself conditional — an Emacs Lisp `(ert-skip …)`, `(skip-unless nil)` or `(skip-when t)`, or a FiveAM `(skip …)`. A conditional skip such as `(skip-unless (executable-find "git"))` is a test that runs wherever it can and is not flagged. Bare `^:skip` and `^:pending` are deliberately **not** matched — they mean whatever a project's Kaocha or Leiningen config says they mean, which the rule cannot see — and neither are `:expected-result :failed` (the test still runs) nor `:disabled t` (no framework here spells it). `:tags` plays no part. `DeadCode` category: an unconditionally skipped test is code that cannot run. Warning severity — skipping a test while a fix is in flight is legitimate, and what the rule is for is keeping "for a while" visible. Common Lisp, Emacs Lisp and Clojure, report-only. |
| `duplicate-test-name` | Report two top-level test definitions in one file sharing a name; the later replaces the earlier, so the first never runs and the suite still reports green. Error severity, Common Lisp, Emacs Lisp and Clojure, report-only. |
| `empty-test-body` | Report a test definition with no body at all, which every framework counts as a passing test having checked nothing. Common Lisp, Emacs Lisp and Clojure, report-only. |
| `sleep-in-test` | Report a wall-clock sleep (`sleep`, `sleep-for`, `sit-for`, `Thread/sleep`) *sequenced* in a test body, which makes the result depend on machine load rather than on the code under test. A sleep below an assertion form is not flagged — in `(is (= :timeout (deref (future (Thread/sleep 5000)) 10 :timeout)))` the sleep is the workload being asserted about, so every timeout/debounce/rate-limiter suite would otherwise report on itself — and neither is a `sit-for` used as the interval of a `while`/`cl-loop` poll loop, since it returns early on input. A literal zero duration is never flagged. Common Lisp, Emacs Lisp and Clojure, report-only: what the sleep should wait on instead is not something a rewrite can infer. |
| `test-asserts-constant` | Report an assertion whose truth is settled by the source itself — `(is t)`, `(is (= 1 1))`, `(should t)` — so it can never fail. Common Lisp, Emacs Lisp and Clojure, report-only. |
| `test-without-assertion` | Report a test definition whose body runs code but contains no assertion form. Framework-aware rather than name-based: `deftest` is `rt`'s positional test in Common Lisp, which contains no assertion form *by design*, and `clojure.test`'s body-of-assertions test in Clojure — only the latter is reported. Common Lisp, Emacs Lisp and Clojure, report-only. |
| `atom-swap-with-side-effect` | Report a `swap!`/`swap-vals!`/`alter`/`commute` whose inline update function performs a side effect. The update function is retried on contention, so every effect in it happens more than once. **Clojure only**, report-only. |
| `dynamic-var-bound-across-thread-boundary` | Report a `make-thread` thunk reading a special variable that an enclosing `let` rebinds; the dynamic binding is per-thread, so the new thread sees the global value, not the rebound one. Common Lisp only, report-only. |
| `future-promise-never-realized` | Report a `future`, `future-call`, `promise` or `delay` — bound by `let`, `loop`, `binding`, `if-let` or `when-let` — whose symbol the body never mentions at all, so its value is discarded and any error inside it is never surfaced. **Clojure only**, report-only. |
| `lock-acquired-not-released` | Report a manual `acquire-lock`/`grab-mutex` with no `unwind-protect` and no `with-…` scope to release it on a non-local exit. Error severity, for the same reason as `unclosed-stream`: the leak does not degrade the program, it stops it. Common Lisp only, report-only — the repair is usually `with-lock-held`, which reshapes the whole body. |
| `recursive-lock-reentry-risk` | Report the same non-recursive lock taken again inside its own scope. This is a **heuristic**, and the finding says so: a thread that already holds a `bordeaux-threads` lock and asks for it again blocks on itself forever, but the inner form may sit in a `lambda` some other thread runs later, or behind a test that is never true on this path. The rule reports a *risk*, not a proven deadlock, and its message is phrased that way on purpose. Common Lisp only, report-only. |
| `thread-spawned-without-error-handler` | Report a `make-thread` thunk that inlines two or more forms with no handler anywhere in it, so an error on that thread is lost rather than reported. A single-call thunk is deliberately ignored: that call is usually a function that handles its own errors, and nothing at the spawn site distinguishes the two. Common Lisp only, report-only. |
| `unsynchronized-shared-mutation` | Report an `*earmuffed*` global written inside a `make-thread` thunk with no lock scope around the write, which races. Common Lisp only, report-only. |
| `asdf-perform-without-call-next-method` | Report a *primary* `perform` method on `load-op`/`compile-op` and a standard component class whose body never calls `call-next-method`, so the built-in compile or load step it was meant to extend is replaced rather than extended. Common Lisp only, report-only. |
| `asdf-self-referential-depends-on` | Report a `:depends-on`/`:defsystem-depends-on`/`:weakly-depends-on` entry naming the enclosing system itself. Error severity, Common Lisp only, report-only. |
| `asdf-system-missing-version` | Report a *primary* `defsystem` (a name with no `/`) that declares no `:version`, which is what `asdf:component-version` publishes and what a dependant's version floor reads. **Tagged `pedantic`**, so `recommended` and `minimal` exclude it and `--preset pedantic` turns it on: measured over a local Quicklisp checkout it fires on roughly 10% of primary systems, every one of them correct code by a maintainer who omitted `:version` deliberately. Common Lisp only, report-only. |
| `defpackage-without-in-package` | Report a file that declares a package and defines symbols but never enters it with `in-package`, so every definition lands in whatever package was current instead. Common Lisp only, report-only. |
| `block-name-shadows-outer-block` | Report a `block` (or a `loop … named`) reusing the name of a block that lexically encloses it, so a `return-from` in between reaches the inner one and the outer block becomes unreachable from that point. Common Lisp only, report-only. |
| `dotimes-dolist-index-var-mutated` | Report an assignment to a `dotimes`/`dolist` iteration variable inside the body. The spec leaves the effect on the iteration undefined, so an implementation may honour the write, ignore it, or rebind afresh each step. Common Lisp only, report-only. |
| `go-to-undefined-tag` | Report a `go` naming a tag no enclosing `tagbody` establishes, which is a compile-time error rather than a jump. Common Lisp only, report-only. |
| `multiple-value-bind-all-ignored` | Report a `multiple-value-bind` whose body references none of the variables it binds, so the extra values are computed and dropped and the form says nothing a bare call would not. Common Lisp only, report-only. |
| `return-from-unmatched-block` | Report a `return-from` naming a block that does not lexically enclose it. `defun` and `defmethod` establish a block named for the function, and those are honoured; a name matched by no enclosing block at all is the error this reports. Common Lisp only, report-only. |
| `return-outside-implicit-nil-block` | Report a `return` with no enclosing form establishing the implicit `nil` block that `return` is defined as returning from — `loop`, `do`, `dotimes`, `dolist` and a literal `(block nil …)`. Common Lisp only, report-only. |
| `tagbody-unreachable-tag` | Report a `tagbody` label that no `go` anywhere in the form targets, so the statements under it are reachable only by falling through and the label itself is dead. Common Lisp only, report-only. |
| `case-key-eql-pitfall` | Report a `case`/`ecase`/`ccase` clause keyed on a string or float literal. `case` matches with `eql`, which is not dependable for either — two equal strings are rarely `eql`, and a float literal's identity is implementation-dependent — so the clause silently never fires. Common Lisp only, report-only. |
| `cond-to-case-candidate` | Report a `cond` whose every test compares one variable against a literal, which `case` states directly and dispatches on rather than testing in sequence. Common Lisp only, report-only. |
| `nested-cond-flattenable` | Report a `cond` whose final `t` clause holds nothing but another `cond`; the inner clause list splices into the outer one with no change in meaning. Common Lisp only, report-only. |
| `when-unless-implicit-nil-misused` | Report a `when`/`unless` value passed to an operator that requires a number. Both yield `nil` when the branch is not taken, and `nil` is not a number, so the call is a type error on exactly the path the conditional exists to guard. Error severity, Common Lisp only, report-only. |
| `deeply-nested-anonymous-lambda` | Report anonymous lambdas nested more than `max-nesting` deep (default 2) with no intervening named binding, so every step of the chain is spelled inline and none of it can be named in a backtrace. **Tagged `pedantic`** and `style`: the depth that reads badly is a house convention, not a defect. Common Lisp and Emacs Lisp, report-only. |
| `nested-function-parameter-shadows-enclosing-parameter` | Report an `flet`/`labels` or nested-`defun` parameter reusing an enclosing function's parameter name, so the inner binding shadows the outer one and a reader has to track which is in scope. Tagged `style`. Common Lisp only, report-only. |
| `overly-long-parameter-list` | Report a definition declaring more than `max-required` required parameters (default 7). **Tagged `pedantic`** and `style`: the threshold is a convention a codebase either adopted or did not, and a long list is often the honest shape of the operation. Common Lisp only, report-only. |
| `positional-argument-count-exceeds-readability` | Report a call passing more than `max-positional-literals` unlabelled literal arguments of mixed kinds (default 4), where the call site gives a reader no way to tell which literal means what. **Tagged `pedantic`** and `style`. Common Lisp only, report-only. |
| `stringly-typed-dispatch` | Report a `cond`/`if` chain dispatching on `string=`/`string-equal` against an enumeration of literals — a set of cases a symbol or keyword would let the compiler check. Fires at `min-branches` branches (default 4). **Tagged `pedantic`** and `style`. Common Lisp and Emacs Lisp, report-only. |
| `intern-dynamic-package-target` | Report an `intern` whose package argument is a computed expression, so the package the symbol lands in is not statically knowable and no search can follow it. Common Lisp only, report-only. |
| `introspection-probe-unchecked` | Report a lookup that answers `nil` when it finds nothing — `find-symbol`, `find-package`, `find-class`, `get`, `resolve` — whose result is handed straight to `funcall`/`apply` with no opportunity to check it, so a miss becomes a call on `nil`. Common Lisp, Emacs Lisp and Clojure, report-only. |
| `symbol-function-fset-dynamic-name` | Report a function definition installed through `setf symbol-function`/`fset` under a name built by `intern`, which no textual search can connect to its callers. Common Lisp and Emacs Lisp, report-only. |
| `destructuring-bind-unused-whole` | Report a `destructuring-bind` that binds a `&whole` variable and never references it, so the whole form is destructured a second time into a name nobody reads. Common Lisp only, report-only. |
| `loop-collect-into-immediately-returned` | Report a `loop` whose only `collect … into acc` is handed back unchanged by `finally (return acc)` — three clauses saying what a bare `collect` already says. Common Lisp only, report-only. |
| `flet-single-use-inlinable` | Report an `flet`/`labels` defining one local function whose only use is a tail call that *is* the whole body, so the name buys nothing a reader could not get from the body itself. Common Lisp only, report-only. |
| `multiple-value-setq-arity-mismatch` | Report a `multiple-value-setq` whose variable list is a different length from its literal `(values …)` right-hand side, which silently `nil`s a variable or drops a value. Common Lisp only, report-only. |
| `with-open-file-redundant-direction-default` | Report an `open`/`with-open-file` passing an explicit `:direction :input`, which is already the default — `(open p :direction :input)` is `(open p)`. Common Lisp only, report-only. |
| `ftype-values-arity-mismatch` | Report a declaimed `ftype` whose `(values …)` return arity is larger than the arity its `defun`'s final literal `(values …)` returns. A violated `ftype` is undefined behaviour at low safety, so this is error severity. Common Lisp only, report-only. |
| `with-accessors-empty-binding-list` | Report a `with-slots`/`with-accessors` with an empty binding list: `(with-slots () o body)` is `(progn o body)` written the long way. Common Lisp only, report-only. |
| `quoted-form-contains-stray-unquote` | Report a quoted form containing a `,` or `,@` with no enclosing backquote. SBCL refuses to *read* the file at all — "Comma not inside a backquote" — so this is error severity rather than a style opinion. Common Lisp only, report-only. |
| `hash-table-iteration-order-assumed` | Report an element read by position out of a hash table's iteration. CLHS 18.1 leaves the order of `maphash` and `loop … being the hash-keys` unspecified, so `(first (loop for k being the hash-keys of table collect k))` gives "some key", not "the first key". A sorted result is never reported. Common Lisp only, report-only. |
| `set-membership-via-linear-scan` | Report a `member` against a literal list of more than `min-elements` distinct plain symbols (default 8) — past that size the list has stopped being an argument and become a set, and both `case` and a hash table answer in constant time. Common Lisp only, report-only. |
| `nested-get-chain` | Report nested two-operand `get`s reading one path, which is `get-in`: `(get (get m :a) :b)` is `(get-in m [:a :b])`. A chain carrying a not-found argument anywhere is left alone, because `get-in`'s applies to the whole path while an inner `get`'s applies to one step. **Clojure only**, report-only. |
| `redundant-into-empty-collection` | Report an `into` onto an empty vector or set, which is a direct conversion: `(into [] coll)` is `(vec coll)`. `'()` and `{}` are left alone — neither has an order-preserving one-call equivalent — as is the transducer arity. **Clojure only**, report-only. |
| `mixed-float-precision-arithmetic` | Report a single-float literal beside a double-float literal in one arithmetic form. CLHS 12.1.4.4 widens the single first, and widening preserves its rounding error rather than removing it: SBCL evaluates `(* 3.14 1.0d0)` to `3.140000104904175d0`. Only reported when widening actually changes the value. Common Lisp only, report-only. |
| `division-result-precision-loss` | Report an Emacs Lisp integer division whose quotient truncates to zero, discarding the value — `(/ 1 3)` is `0`, not one third. **Emacs Lisp only**: the identical Common Lisp form is the exact ratio `1/3` and loses nothing. Report-only. |
| `epsilon-less-float-loop-bound` | Report a `do` loop stepping an inexact float that terminates on `=` or `eql` rather than an ordered comparison. Repeated addition accumulates error and steps past the bound rather than landing on it, so the end test may never hold. An exactly representable step (0.5, 0.25, 0.125) is left alone. Common Lisp only, report-only. |
| `redundant-precision-coercion` | Report a float coercion immediately discarded by a `truncate`/`floor`/`ceiling`/`round` around it. The conversion can be amplified into a full unit of error before it is thrown away: SBCL gives `(truncate (float 123456789123456789))` as `123456790519087104`. Deliberately not fixable — the two forms are different functions. Common Lisp only, report-only. |
| `format-unknown-directive` | Report a `~` directive in a literal `format` control string that CLHS 22.3 does not define. Common Lisp only, report-only. |
| `format-percent-ampersand-adjacent-redundancy` | Report a `~%~&` in a literal `format` control string: `~&` outputs a newline only when not already at the start of a line, and the `~%` just put it there. Common Lisp only, report-only. |
| `format-nested-directive-unbalanced` | Report an unbalanced `~[` / `~{` / `~<` / `~(` bracketing construct in a literal `format` control string, which `format` signals on at run time. Error severity, Common Lisp only, report-only. |
| `package-circular-in-package-chain` | Report a top-level `in-package` that re-enters a package the file had already left, so one package ends up with two disjoint regions and a symbol spelled the same way in between is a different symbol. Ambient packages (`CL-USER`, `KEYWORD`, …) are exempt. Common Lisp only, report-only. |
| `with-open-returns-lazy-seq` | Report a `with-open` whose value is a lazy sequence over the resource it closes, so realizing it at the call site throws `IOException: Stream closed` — and only on inputs large enough not to have been buffered. Error severity, Clojure only, report-only (the repair is a choice between `doall`, `into`, `reduce` and restructuring, each with a different memory profile). |
| `def-inside-function-body` | Report a `def`/`defn`/`defonce` inside a `defn`/`defn-`/`defmethod` body, which interns a namespace Var at call time rather than at load time: the Var does not exist until the function runs, and concurrent callers race on it. Error severity, Clojure only, report-only. |
| `single-key-nested-path` | Report an `assoc-in`/`update-in`/`get-in` whose path vector holds exactly one key, which the direct `assoc`/`update`/`get` says without building and destructuring a path. Clojure only, report-only. |
| `apply-with-literal-collection` | Report an `apply` whose argument sequence is a literal, so the call can be written directly and the short-lived vector is never built. Clojure only, report-only. |
| `scheme-begin-single-form` | Report a `begin` wrapping a single expression, which is just that expression. Library declaration bodies (`define-library`, `library`) are exempt. Scheme and Racket, **fixable** — the inner form's source is copied verbatim, so its spacing, comments and reader prefixes survive. |
| `scheme-let-star-independent-bindings` | Report a `let*` of two or more bindings whose initializers are all literals or free references, so no binding can see another and the sequential scope buys nothing. Scheme and Racket, **fixable** (only the head symbol is rewritten). |
| `scheme-memq-assq-literal-key` | Report a `memq` or `assq` searching for a number or character literal, which R7RS 6.4 leaves unspecified: `(memq 101 '(100 101 102))` ⟹ *unspecified* while `(memv 101 …)` ⟹ `(101 102)`. Scheme only — Racket specifies both cases, so a finding there would complain about code the language promises will work. **Fixable** (`memq`→`memv`, `assq`→`assv`, which cannot break a working search). |
| `scheme-named-let-never-recurs` | Report a named `let` whose loop name is never mentioned in its body, so it can never iterate and is an ordinary `let` wearing a loop's clothes. Scheme and Racket, **fixable** where the name can simply be deleted. |
| `eval-when-execute-only` | Report a **top-level** `eval-when` naming `:execute` but neither `:compile-toplevel` nor `:load-toplevel`, wrapping a definition: `compile-file` discards the body entirely, so the file loads from source and the compiled fasl is missing the definition. Error severity, Common Lisp only, report-only (which situations were meant is not recoverable from the source). |
| `eval-when-body-never-runs` | Report a **non**-top-level `eval-when` naming only situations the standard ignores there, so its body runs in no phase at all. CLHS 3.2.3.1 makes such a form equivalent to `nil`, and SBCL emits no diagnostic. Error severity, Common Lisp only, report-only (whether `:execute` was meant, or the form wanted hoisting, or deleting, is a judgement). |
| `defconstant-non-eql-value` | Report a `defconstant` whose initform allocates (a string, list, vector or structure literal), so the compile-time and load-time values are not `eql` and a fresh image that compiles and loads the file signals `DEFCONSTANT-UNEQL`. Error severity, Common Lisp only, report-only (`defparameter`, or `define-constant` with which `:test`, is the author's call). |
| `lint` | Run every within-file logic-bug lint at once and report all findings, tagged by rule and category. Each finding is self-describing — it carries its `severity`, `category`, and a `fixable` flag inline (so an agent can triage and decide whether to run `--fix` without cross-referencing `--list-rules`). `--list-rules` prints the rule catalog with categories, descriptions, a `severity` (`error` for likely/certain bugs, `warning` for redundant/non-idiomatic style), and a `fixable` flag marking the rules `--fix` can repair — and it honors the same `--rule`/`--exclude`/`--category` selectors, so `--list-rules --category dead-code` lists just that group; `--rule`/`--exclude` select rules; `--category` selects a whole group (see `--list-rules` for the current set); `--sarif` emits a SARIF 2.1.0 log for CI code scanning (with stable fingerprints and one-click `fixes` for every rule `--list-rules` marks fixable); `--github` emits GitHub Actions `::error::` annotations for inline PR review; `--fix` applies those auto-fixes in place, iterating to a fixpoint (so nested redundancies collapse fully) and reporting the per-file/per-rule counts; add `--diff` to preview the changes as a unified diff without writing, or `--check` to write nothing and exit 3 when any auto-fix is still pending (a CI gate that stays green only when fixable lint has been cleaned up — distinct from `--fail-on-finding`, which also gates on report-only findings). `--check` and `--diff` combine (show the diff and fail). `--fix-plan` instead emits the machine-readable fix plan — each fixable finding's exact byte-region replacements as JSON (or tab-separated text) — without writing, so an editor or agent can preview or apply fixes one at a time (honoring the same suppressions and `--baseline` as `--fix`). Findings can be silenced in source with an inline `; paredit:ignore [rule…]` comment: on its own line it suppresses the next line, trailing after code it suppresses that line, and with no rule names it suppresses every rule — honored uniformly across the report, SARIF, GitHub, and `--fix` outputs. `--fail-on <error\|warning>` gates only on findings at or above a severity (so CI can block on bugs while still reporting style warnings), and SARIF `level` reflects each finding's severity. `--stats` prints a lint-debt rollup instead of individual findings — finding counts by severity, by category, and by rule, plus files-scanned/files-with-findings — honoring the same `--rule`/`--category`/`--baseline` filters. `--suggest-severity` instead prints advisory severity suggestions: for each rule that fired, its findings-per-file density across the scanned workspace (`very high`/`high`/`moderate`/`low`/`very low`), and — only when that disagrees with the rule's current severity — the severity it suggests instead (a currently-`error` rule firing on nearly every file is likely too noisy to gate a build on; a currently-`warning` rule that never fired at all may be rare enough to be worth failing over). This is guidance only: it never writes `paredit.toml`, never changes a rule's declared severity, and never affects this or any later run's exit code. `--report-unused-suppressions` instead reports any `; paredit:ignore` that silences no finding (a stale ignore or a typo'd rule name) and exits 3 if any are found, keeping the ignore list honest in CI. A directive may also carry `-until <date>`; `--report-expired-suppressions` reports any past its date (used or not) and exits 3 if any are found, and `--report-suppressions` lists every directive, used or not, with its scope, rules, reason, and expiry, and always exits 0. `--suppress-path <path>` (repeatable) silences every finding under a path as if it carried `paredit:ignore-file`, for generated/vendored code that cannot hold an inline directive. For adopting the linter on an existing codebase, `--write-baseline <file>` snapshots today's findings and `--baseline <file>` then suppresses those known findings (matched by rule and trimmed-line content, so they survive line shifts) — reporting and gating only on new findings, across the default, `--sarif`, and `--github` outputs. `--fixable` narrows `--list-rules` to just the rules that carry an auto-fix — `paredit fix list` is this pair under a name that says so. |

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

With 316 rules, `inspect lint` needs more than an on/off switch per rule. The
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

The 316 shipped rules are the ones everybody gets. A rule like "in *this*
codebase, `defentity` must always be given a `:table`" is the majority of what
a mature project wants and none of what a linter can ship, so a project writes
those itself, in Lisp, in `.paredit/rules/*.lisp`:

```lisp
(defrule entity-needs-table
  :category malformed          ; optional; defaults to suspicious
  :severity error              ; optional; defaults to warning
  :description "a defentity with no :table option"
  :dialects (common-lisp)       ; optional; defaults to every dialect
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

`:dialects` is a guard, not a hint, the same as `defmigration`'s own clause:
naming dialects skips every file outside them entirely (reported as a skip
count on stderr), rather than matching them and finding nothing. Omitting it,
as every rule written before this clause existed does, keeps the rule scoped
to every dialect — unchanged.

A rule set may also register a named, reusable pattern fragment:

```lisp
(defpattern bare-print (print ?x))

(defrule no-print-in-handler
  :pattern (handler-case (:fragment bare-print) ...)
  :message "do not print from inside a handler")
```

`(:fragment name)` stands for that fragment's own pattern, substituted in
whole before any rule matches — including inside another `defpattern`, so
fragments may build on each other. Referencing an undefined fragment, or a
cycle between fragments, fails the rule file at load time.

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
`custom_rules` block so the two are still tellable apart. A custom rule's name
must be unique, both against the shipped catalogue and against every other
custom rule loaded in the same run (even across files) — a rule file where two
rules share a name fails to load rather than leaving it to whichever code path
happens to pick one of them.

`--timings` reports the loaded custom rules' own per-rule cost as a separate
section alongside the shipped suite's, measured the same way: serially, since a
per-rule cost that changes with `--jobs` is not a per-rule cost.

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
| `canonicalize` | Sort an alist- or plist-shaped data file's keys and flatten its whitespace to a single space between elements. Refuses a file with no confidently alist- or plist-shaped list anywhere in it, and never reorders or rewrites inside a reader-prefixed subtree (`#+feature (...)`, a quoted or quasiquoted form) — this is deliberately not `format`, which renders code; a data file is not read as nested code blocks. |
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
| `apply` | Apply a previously generated preview manifest with hash guards. `--undo-out` records a reverse-edit journal; `--verify-command` runs a check afterwards and restores every written file when it fails. `--compact` prints only the one-line change headline instead of the full field-by-field report. `--group-by-impact-area` (requires `--write`) writes changed files one impact-area (declared package) group at a time instead of all at once, continuing to the next group when one group's write fails. |
| `undo` | Restore the pre-refactor content recorded by `apply --undo-out`. Refuses unless every file is still byte-for-byte what the write produced, so a journal cannot be applied twice or over an intervening edit. |
| `diff` | Render a verified diff from a preview manifest without writing files. |
| `step` | Walk a preview manifest one edit at a time, taking only the steps you accept. `refactor apply` is all-or-nothing, which is right for applying and wrong for reviewing: a reader who disagrees with one of forty edits would otherwise have to discard the manifest. Steps are numbered in source order and each carries its line, the text it replaces, and the source line it sits on. `--accept`/`--skip` take a selector (`all`, `3`, `1,4`, `2-5`); `--interactive` reads one `y`/`n`/`a`/`q` decision per step from stdin. `--diff` previews, `--write` applies, `--fail-on-partial` gates a script that took a subset by accident. Both hash guards still apply, and a subset that would not parse is refused before anything is written. |
| `create-checkpoint` | Record the current content of `--name`'s files as a named checkpoint under `.paredit/checkpoints/` (`$PAREDIT_CHECKPOINTS_DIR` overrides the directory, the same convention `.paredit/kill-ring.json` uses), so a later invocation — a separate agent turn — can name the point it wants to get back to instead of keeping an `--undo-out` path around. Refuses a name already in use unless `--force` is given. |
| `list-checkpoints` | List every registered checkpoint: name, creation time, and the files it covers. |
| `restore-checkpoint` | Report whether `--name`'s checkpoint can be restored, and with `--write`, confirm it. A file is restorable only when it is still byte-for-byte what the checkpoint recorded; anything else — a later `refactor apply --write`, or a person editing the file directly — refuses, since the two are indistinguishable from a content hash alone and silently overwriting either would be the wrong default. |
| `delete-checkpoint` | Remove `--name`'s checkpoint from the registry. Does not garbage-collect checkpoints that no longer resolve to any file. |
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

`fix apply` also takes two flags only it makes sense for, mirroring
`refactor apply`'s own `--compact`/`--group-by-impact-area`: `--compact`
prints only the one-line change headline instead of the full field-by-field
report, and `--group-by-impact-area` writes changed files one impact-area
(declared package) group at a time instead of all at once, continuing to the
next group when one group's write fails. Neither is available on `check`,
`plan`, or `list`, which never write. `--compact` conflicts with `--diff`,
since `--diff` writes nothing for `--compact` to summarize.

In JSON, `fix apply` (and `refactor apply`) always carry a `headline` field
— the same one-line summary `--compact` prints on its own — and an
`impact_area_groups` array, populated only under `--group-by-impact-area`,
with one entry per group naming the group (`group`), how many files it
covers (`file_count`), whether its write succeeded (`written`), and the
failure reason when it did not. `fix plan` and `fix apply` also carry
`next_commands` (see
[What to run next](../guide/agents.md#what-to-run-next)) when their contents
justify one — a plan with fixes available points at `fix apply`.

All four take the rule-selection flags (`--rule`, `--category`, `--exclude`,
`--tag`, `--preset`, `--experimental`, `--custom-rules`) and
`--no-destructive-fixes`. The flags that shape a *report* rather than a fix
run — `--emit`, `--baseline`, `--stats`, `--timings`, `--fail-on` — stay on
`inspect lint`, which is where they mean something.

```sh
paredit fix list
paredit fix apply --diff src/
paredit fix apply --compact src/
paredit fix apply --group-by-impact-area src/
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

## Schema

`paredit schema check` validates one S-expression data file (an *instance*)
against a small schema language of its own, `defschema`, written as ordinary
Lisp forms rather than an embedded foreign syntax:

```lisp
(defschema config
  (fields
    (:name (:type string))
    (:port (:type integer :min 1 :max 65535))
    (:mode (:type string :one-of ("dev" "staging" "prod")))
    (:label (:type string :matches "^[a-z][a-z0-9-]*$" :optional t))))
```

| Command | Purpose | Its own flags |
| --- | --- | --- |
| `check` | Validate an instance file against a `defschema` schema. | `--schema <FILE>` names the schema file. `--schema-name <NAME>` picks which `defschema` to validate against when the file defines more than one; optional when it defines exactly one. `--fail-on-violation` exits 3 if the instance has any finding. |

Nothing here is ever evaluated, on either side of the check. `:type` accepts
exactly five names (`string`, `integer`, `boolean`, `symbol`, `list`), and a
refinement is one of exactly four (`:min`/`:max` on `integer`, `:one-of`/
`:matches` on `string`), each meaningful only for the type it was written
for — an unrecognized `:type` or refinement is a parse error, never code
this tool tries to run. `:matches` is a small hand-rolled glob (`*` = any
run of characters, `?` = one character), **not a regular expression**.

An instance may be alist-shaped (`((key . value) ...)`) or plist-shaped
(`(:key value ...)`); both validate identically. A field present in the
instance but not declared by the schema is reported too, as `unknown-field`,
at a lower severity than a genuine type or refinement violation. A field is
required unless it carries `:optional t`.

```sh
paredit schema check instance.lisp --schema .paredit/schemas/config.lisp
paredit schema check instance.lisp --schema schemas.lisp --schema-name config
paredit schema check instance.lisp --schema schemas.lisp --output text --fail-on-violation
```

## Config

`paredit config` reads `paredit.toml`, never source. It answers what this build
is going to do and why, which is a different question from any `inspect`
report. See [Configuration](configuration.md) for the file format, the layer
order, and `extends`.

| Command | Purpose |
| --- | --- |
| `check` | Validate every discovered file and exit 3 if any key is unusable. Also flags when a custom rule under `lint.custom-rules` declares its own `:severity` that disagrees with what `lint.warn`/`lint.deny` implies for that rule's name. |
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
