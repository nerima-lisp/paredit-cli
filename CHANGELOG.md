# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

The five shallowest dialects — LFE, Fennel, Janet, Hy, and Carp — gain real
scope analysis, and the dialect capability matrix stops answering "unknown"
for 99% of its cells.

Emacs Lisp gains the same treatment one layer deeper: its own operator
model, a binding table, nine lint rules, and a per-file report.

### Added

- **Three new namespaces — `query`, `fix`, `migrate`.** The existing three
  split by what a change costs to undo (`inspect` reads, `edit` transforms one
  form, `refactor` plans and applies). These split by what the caller is
  trying to do, over a file set rather than one form. Nothing is removed and
  no older spelling stops working.

  `paredit query` promotes the `--query` pattern language out of the selector
  position, where it could only name a form in one named file. `query find`
  searches a whole workspace and reports each match with its captures, path,
  and stable selector id, through the shared report envelope (so `--output
  sarif`, `junit`, `csv` and the rest come with it) and with the full input
  surface (`--since origin/main`, `--from-git`, `--include`). `query count`
  tallies several patterns side by side over one file set, which is what makes
  it a migration's progress bar rather than a number. `query replace` is the
  one genuinely new capability: a `--rewrite` template whose `?name`
  placeholders are filled with the **verbatim source bytes** the pattern's
  `?name` captured, so a captured `1.0d0` stays a double float and a captured
  string keeps its escapes — nothing is re-serialized, because re-serializing
  is how a rewrite turns `1.0d0` into `1`.

  `paredit fix` is the write side of `inspect lint`, under a name that says it
  writes. `fix apply`, `fix check`, `fix plan` and `fix list` were `--fix`,
  `--fix --check`, `--fix-plan` and `--list-rules`; each builds the arguments
  its old spelling produced and calls the same engine, so the two paths are
  identical by construction rather than by testing. `inspect lint` also gains
  `--fixable`, which narrows `--list-rules` to the rules that carry a fix.

  `paredit migrate` runs a *recipe*: an ordered list of query/rewrite steps
  scoped to the dialects the rewrite is correct for. Order matters —
  `(if (not p) a nil)` becomes `(unless p a)` only if the negated step runs
  first — and so does scope: `(incf x)` → `(cl-incf x)` modernizes Emacs Lisp
  and breaks Common Lisp, where `incf` is the correct spelling. Two recipes
  ship (`elisp-cl-lib`, `nil-conditionals`), as embedded Lisp source parsed by
  the same reader a project's own `.paredit/migrations/*.lisp` goes through.

  Both writing commands refuse two situations that leave source which still
  parses and is still wrong, and which the reparse guard therefore cannot
  catch: a match nested inside one already rewritten, and a rewrite that would
  delete a comment no capture carries over. Both are counted in every output
  format, including as zeroes.

- Five clone-detection commands built on the existing tree-edit-distance
  scorer, which `similarity` and `duplicates` had been under-using.
  `inspect clone-classes` groups near-duplicate forms into classes, labels each
  on the standard Type-1/2/3 taxonomy, and ranks them by the lines extracting
  one would save — five copies of a helper are one class to act on rather than
  ten pairs to read. `inspect clone-sequences` finds duplicated runs of adjacent
  sibling forms, the sub-form clones that no whole-form report can see because
  the duplication does not line up with a form boundary. `inspect clone-external`
  compares the project against a reference corpus across head symbols, so a
  local `join-strings` matches a library `str:join`. `inspect clone-threshold`
  recommends a `--threshold` from the project's own similarity distribution,
  reporting the histogram, an Otsu split, the widest distribution gap and
  percentiles so the recommendation can be judged rather than taken.
  `inspect clone-genealogy` orders each class by the commit that introduced its
  members, separating the original from the copies.
- `inspect similarity` labels every reported pair with its clone type, the
  number of atoms renamed, and whether that renaming is a bijection. An
  inconsistent Type-2 renaming is the shape a copy-paste bug takes when one
  occurrence of a variable was missed.
- **Six new ways to choose which files get analysed.** Discovery was a
  directory walk with four booleans and an exact-path exclude list. It could
  not be told to respect `.gitignore`, to take the file set from a build
  definition, or to look only at what a pull request changed. The flags now
  divide into *selectors*, which decide where the candidate list comes from,
  and *filters*, which narrow whatever the selector produced:
  - `--since <git-ref>` — only the files that differ from a ref, which is the
    single biggest lever on CI time. Deleted files drop out, a rename reports
    its destination, and an unresolvable ref is an error rather than an empty
    change set that would let a gate pass without examining anything.
  - `--from-git` — the file set from `git ls-files`.
  - `--from-manifest` — the project's own build definition: ASDF `defsystem`
    `:components` (through `:module` and `:pathname`, in declaration order),
    `deps.edn`, `shadow-cljs.edn`, `project.clj`, and an Emacs Lisp
    `Package-Requires` header. A named component missing from disk is reported
    rather than silently dropped.
  - `--paths-from <FILE|->` — a list you computed yourself, newline- or
    NUL-separated.
  - `--from-archive <ARCHIVE|-> --extract-to <DIR>` — an uncompressed tar, from
    a file or stdin. Compression and transport stay with the shell, which keeps
    an HTTP client, a TLS stack and a decompressor out of a tool trusted with
    your source tree. Extraction refuses rather than sanitises absolute paths,
    `..` components, symlinks, hardlinks, devices, and overwriting.
  - `--cache-dir <DIR>` — reuse a previous scan. Keyed on everything that can
    change the selection and validated against the tree, so a stale entry is a
    miss rather than a wrong answer.
- **Filters:** `--include` / `--exclude-glob` in `gitignore(5)` syntax,
  `.gitignore` and `.pareditignore` honoured by default (`--no-gitignore`,
  `--no-pareditignore`, `--no-ignore`, and three `PAREDIT_NO_*` environment
  variables), and `--follow-symlinks`. Ignore precedence follows git exactly:
  the deeper file wins, the last matching pattern within a file wins, a
  repository boundary cuts the stack, and a root named on the command line is
  never ignore-filtered.
- **`paredit inspect sources`.** Runs selection and stops, reporting which rule
  dropped every file that did not make it. It parses nothing, which makes it
  the cheapest way to answer "did that CI run find nothing, or did my pattern
  find nothing".
- Discovery reports repository boundaries, so a run over several checkouts
  groups its result per repository instead of flattening them.
- **Six new ways to select a form.** `--path` and `--at` were the only two,
  and both cost a round trip to build: an agent had to run `inspect outline`,
  read a path out of it, and hope nothing moved in between. Every command that
  takes a target now also accepts:
  - `--query '(defun ?name ...)'` — an S-expression pattern, written in the
    file's own dialect and read with its own reader. `_` matches one form,
    `?name` binds one, `...` matches a run, `?body...` binds a run. Captures
    may be constrained (`?x:list`, `?x:number`, …), a repeated name is a
    back-reference (`(eq ?x ?x)` finds self-comparisons), and `--capture name`
    selects the bound sub-form rather than the whole match.
  - `--name <symbol>` — the definition of that name, at any nesting depth.
  - `--line-column LINE[:COLUMN]` — a 1-based editor coordinate, columns
    counted in characters. The column defaults to 1.
  - `--id <id>` — a content-addressed id that keeps naming the same form after
    edits elsewhere in the file, where a `--path` would not.
  - `--from` / `--to` — a contiguous range of siblings, each end given as a
    compact selector (`0.2`, `at:120`, `name:foo`, `query:(defun ?n ...)`).
  - `--parent` / `--child N` / `--sibling ±N` — relative moves over any of the
    above, applied up-across-down.
  - `--select <selector>` — any of the above in one flag, using the compact
    grammar. This is how the richer selectors reach the eight commands whose
    own flags already claim these names (`refactor introduce-let --name` is
    the new binding's name, `rename-binding --from`/`--to` are symbols):
    `introduce-let`, `inline-let`, `remove-unused-binding`,
    `thread-expression`, `unthread-expression`, `unwrap-call`,
    `extract-function`, `extract-constant` now take `--path`, `--at`, and
    `--select`. They also report the *resolved* path in their JSON plan where
    they previously reported `null` for `--at`.
- **`--all`.** A selector naming more than one form is now refused by default
  rather than resolving to the first match; `--all` turns the refusal into a
  fan-out. Edits apply right to left with a re-parse between them, and stop
  with a refusal if one edit disturbs a match still to come.
- **`paredit inspect resolve`.** Reports what a selector names — path, byte
  span, start and end line/column, kind, head, stable id, preview, and every
  pattern capture — without acting on it. Never refuses an ambiguous selector,
  since seeing all the matches is how you decide whether to narrow.
  `--fail-on-empty` makes "no match" an exit code for scripts.
- `inspect form` accepts the whole selector surface, so
  `inspect form --name parse-header` turns a name into a path in one call. Its
  JSON now reports the *resolved* path for `--at` and the other coordinate
  selectors, where it previously reported `null`.
- Thirteen `edit` commands and one `inspect` report close the remaining gaps
  against Emacs `paredit.el`, whose keystroke-level operations had no CLI
  equivalent:
  - `edit wrap --delimiter doublequote` wraps a form in a string literal,
    escaping the quotes and backslashes it contains (`paredit-meta-doublequote`).
    `--prefix quote|quasiquote|unquote|unquote-splicing|sharp-quote` attaches
    reader sugar instead of a delimiter pair, and `edit unwrap-prefix` removes
    it — outermost first, or all of it with `--all`.
  - `edit navigate --direction forward|backward|up|down` prints the `--path`
    the move lands on, so an agent composes addresses instead of computing
    them. Text output is the bare path. It refuses at a list's boundary rather
    than silently changing depth, which is right for an address even though
    Emacs moves point out of the list.
  - `edit delete-forward` / `edit delete-backward` delete one character at a
    byte offset and refuse anything structural: a delimiter with something
    inside it, the whitespace holding two symbols apart, a comment's opening
    token. `()` and `""` are deleted as a pair, and a backslash inside a string
    travels with the character it escapes.
  - `edit newline` inserts a break and reindents the definition it landed in,
    refusing an offset inside a string, a comment, a symbol, or reader sugar.
  - `edit reindent-defun` reindents one definition to the Emacs
    `lisp-indent-function` convention *without* rewrapping its lines, which
    `edit format` cannot do — a one-character insertion should not arrive in
    review as a twenty-line diff. `inspect indentation` now measures deviation
    from the same table this produces, rather than a second copy of it.
  - `edit copy` prints a form together with the own-line comment block above
    it, which `edit select` leaves behind. With `--to-ring` it, `edit kill
    --to-ring`, and `edit yank` share a kill ring: a named file, from `--ring`,
    then `$PAREDIT_KILL_RING`, then `.paredit/kill-ring.json`. Repository-
    relative by default, so two checkouts do not share a clipboard by accident.
  - `edit raise --levels N` climbs N enclosing lists in one call, and names how
    deep the selection actually sat when it cannot.
  - `edit transpose --with-path` / `--with-at` / `--with-select` swaps any two
    expressions in the same list, not only adjacent ones. The partner keeps its
    own flag names because the primary selector already claims `--path`,
    `--at` and `--select`.
  - `edit split-string` splits a string literal in two, the inverse of `edit
    join` on two strings. `edit escape-string` / `edit unescape-string` add and
    remove one level of escaping; unescaping collapses `\\` and `\"` only and
    refuses any other sequence, because `"a\nb"` is a newline in Emacs Lisp and
    the letter `n` in Common Lisp.
  - `inspect context-at` reports whether a byte offset is code, a string, a
    comment, a delimiter, reader sugar, or the whitespace between forms, with
    the enclosing list, the nesting depth, and the stack of open delimiters.
    `--fail-on-structural` turns "a character edit here is not safe" into exit
    code 3 — the question to ask before a cursor edit rather than after a
    refused one.
- `inspect capabilities --schema-version 3` reports a `dialect_contract` in
  which every one of the 3250 command/dialect cells is answered. Previously
  2720 of the 2760 cells the matrix then held said `unknown`. Cells gain a fourth status, `silent`: the
  command succeeds and reports nothing because it has no rules for that
  dialect, which is not the same as finding nothing. Roughly 155 of the 276
  commands are silent outside Common Lisp. Each command also reports the
  capability `tier` it needs, and a `dialect_depth` array summarises the
  counts per dialect. Schema versions 1 and 2 keep their three-value
  vocabulary and fold `silent` onto `unsupported`.
- Scope and definition shapes for the binding forms these dialects actually
  use: LFE `flet`/`fletrec`/`match-lambda`/`defrecord`/`defmodule`, Fennel
  `lambda`/`var`/`with-open` and the `each`/`for`/`collect`/`icollect`/
  `accumulate`/`fcollect` comprehensions, Janet `var`/`varfn`/`def-`/named
  `fn`/`each`/`with`/`with-vars`/`with-syms`/`when-let`/`when-with`, Hy
  `for`/`with`/`defclass`, and Carp `let-do`/`defndynamic`/`deftype`/
  `definterface`/`defmodule`. Because the rename, introduce-let and
  extract-function engines are all driven by these shapes, each addition
  reaches every one of them.
- Emacs Lisp is analysed by the semantic layer. `build_binding_table` returned
  an empty table for every dialect but Common Lisp, which left the 170 lint
  rules and the typing and value layers with nothing to work with on a `.el`
  file. The Emacs Lisp binding forms are now walked: the `subr-x` conditional
  binders (`if-let*`, `when-let*`, `and-let*`, `while-let`), `letrec`, `dlet`,
  `named-let`, `pcase-let*`, `seq-let`, `pcase-dolist`,
  `cl-destructuring-bind`, `cl-flet*`, `cl-labels`, `cl-macrolet`,
  `condition-case`, and `with-slots`.
- `inspect elisp-file` reports the per-file Emacs Lisp facts that are not
  forms: the `lexical-binding` header, the provided and required features
  (separating an eager `require` from a deferred `autoload`), and the
  `;;;###autoload` cookies with the definitions they attach to. It gates with
  `--fail-on-missing-lexical-binding`.
- Nine Emacs Lisp lint rules, bringing the suite to 143:
  `elisp-missing-lexical-binding`, `elisp-unreachable-lexical-binding`,
  `elisp-autoload-cookie-without-form`, `elisp-defcustom-missing-type`,
  `elisp-defcustom-missing-group`, `elisp-obsolete-cl-alias`,
  `elisp-quoted-lambda`, `elisp-interactive-in-macro`, and
  `elisp-condition-case-without-handler`. Each declares `Dialect::EmacsLisp`
  only, so a Common Lisp run skips them before walking anything.
- `inspect semantic-coverage`, promoted from a development-only example into
  a real command. It measures how much of `types`/`narrowing`/`constants`/
  `value-propagation` actually resolves on real source — variable-binding and
  constant-folding rates, broken down per dialect — and ranks unresolved
  bindings by cause, so the highest-count unknown head is the next operator
  worth registering in the transparency table. `--fail-under` gates CI on a
  resolution-rate floor; a new bundled corpus and baseline test
  (`tests/semantic_coverage_baseline.rs`) pin today's rate so a future change
  cannot quietly narrow it.
- Every reported failure names the byte position it is about, when it has
  one: `--output json`'s error envelope carries an `offset` field, and the
  text rendering shows a `rustc`-style caret under the source line. A parse
  failure always has one; a handful of `inspect`/`edit` selection failures
  (`--at` past the document, an invalid byte span) do too. A shape refusal
  like "cannot raise a top-level expression" is not about one place in the
  source, so it reports `null` rather than a guess.
- Every error code now links to its own documentation section
  (`docs/src/errors.md`, one page cataloguing all forty), surfaced as
  `doc_url` in the JSON error envelope. A contract test ties the two together
  so a code cannot be added without documenting it.
- An unknown `--rule`, `--deny`/`--warn` selector, `--category`, `--tag`, or
  `--rule-arg` key now offers a "did you mean" suggestion when one registered
  name is a close edit away, the same way `paredit.toml` already does for
  configuration keys.
- A configuration file this tool ignored or rejected at startup is now
  reported as a structured JSON warning (`"status": "warning"`) when the
  command that follows defaults to `--output json`, matching the JSON
  contract errors already keep. Text-mode output is unchanged.
- `inspect check` reports every syntax error in a document, not only the
  first: `SyntaxTree::find_parse_errors` recovers after a failure by
  resuming at the next line that starts a top-level form and keeps scanning,
  so a file with three unrelated problems is now one round trip instead of
  three. The existing singular `error` field is unchanged; `errors` is the
  new, additive array carrying all of them, each with its own byte offset.
- A lint run over many files no longer discards every result when one file
  fails to parse. `paredit inspect lint`, `--sarif`, and `--github` now
  report findings from every file that *did* analyze cleanly, name the ones
  that did not in a new `partial_failures` field (and on stderr), and only
  fail outright when nothing in the request could be analyzed at all.
- **Three extensions to lint suppression.** Any `paredit:ignore` directive may
  now carry `-until YYYY-MM-DD` right after the token
  (`paredit:ignore-until`, `paredit:ignore-next-form-until`,
  `paredit:ignore-file-until`), and `--report-expired-suppressions` reports
  any past its date — used or not — exiting 3 so CI can catch a suppression
  that outlived the reason it was written for; a missing or malformed date
  makes the whole comment not a directive, so a typo shows up as the finding
  reappearing rather than as a suppression that silently never expires.
  `--report-suppressions` lists every directive, used or not, with its scope,
  rules, reason, and expiry — the full inventory, one step past
  `--report-unused-suppressions`'s stale-only view. `--suppress-path <path>`
  (repeatable, also settable as `lint.suppress-paths` in `paredit.toml`)
  silences every finding under a path as if the whole file carried
  `paredit:ignore-file`, for generated code and vendored dependencies that get
  overwritten and so cannot hold an inline directive; scoped to `inspect
  lint` alone, unlike `paths.exclude` which hides a path from every command.

### Fixed

- LFE's clause-style `(defun f ((x) body) ((x y) body))` was read as a single
  parameter list, so the first clause's body was treated as a parameter and
  renaming reported an ambiguity that did not exist. Each clause now scopes
  its own parameters, matching how Clojure's multi-arity `fn` is handled.
- Renaming a lexical binding in LFE rewrote the head of a call, but LFE
  resolves call heads in a separate namespace like Common Lisp and Emacs
  Lisp: `(f 2)` calls the function `f` whatever a surrounding `let` bound.
- Renaming a binding in Fennel or Hy left member accesses behind, producing
  code that no longer compiled: renaming `f` in `(with-open [f ...] (f:read))`
  rewrote the binder and not `f:read`. The leading segment is now rewritten
  in place. Janet's `file/open` and Carp's `Array.length` are deliberately
  untouched — those separators namespace a module, not a lexical binding.
- `refactor rename-at` accepted Common Lisp only, even though the binding
  search underneath it is dialect-neutral. It now resolves lexical value
  bindings in all ten dialects; the Common Lisp specializations layered on
  top (`flet`/`labels`, `macrolet`, `symbol-macrolet`, global callables)
  remain Common Lisp only.
- Every report that reads a lambda list — `inspect unused-parameters`,
  `inspect impact`, `inspect signature` — examined nothing in Fennel while
  succeeding, because the lambda-list resolver knew `defn` but not Fennel's
  `fn`. They now check Fennel definitions like every other bracket dialect's.
- Emacs Lisp head resolution no longer applies Common Lisp reader rules. Every
  dialect capability reached `CommonLispOperator::from_head`, which folds case
  and strips a `cl:` package prefix; in a `.el` file `LET` and `cl:let` are
  ordinary user symbols, and both resolved to the `let` special form. Symbol
  identity in the binding table is likewise exact, so `(let ((x 1)) X)` no
  longer attributes a reference to `x`.
- An Emacs Lisp script's `#!/usr/bin/emacs --script` header is read as a
  comment instead of failing the parse with an unsupported reader dispatch.
  Emacs skips that line the same way; reading it rather than stripping it
  keeps every byte offset after it unchanged.
- `Dialect::EmacsLisp.is_definition_head` recognizes the forms the dialect
  actually has — `defsubst`, `define-inline`, `cl-defsubst`, `cl-defstruct`,
  `defvar-local`, `defvar-keymap`, `defface`, `defalias`, `define-error`,
  `define-globalized-minor-mode`, `ert-deftest` and the rest — and rejects
  Common Lisp spellings such as `defparameter` and `defpackage`.
## [1.2.1] - 2026-07-28

No command, flag, exit code, or JSON field changed in this release: nothing
under `src/` or `packages/` was touched. What changed is how the project
verifies itself. The whole verification gate went from 27 minutes to 8, by
removing work it was doing twice rather than by checking less.

### Changed

- The Rust flake checks are [crane](https://github.com/ipetkov/crane)
  derivations sharing pre-built dependency artifacts, instead of four
  independent `buildRustPackage` derivations that each recompiled the whole
  dependency graph. Consumers of `packages.default` or `overlays.default` get
  the same binary; what changed is that building it no longer runs the test
  suite, which now belongs to `checks.<system>.nextest` alone. `flake.nix`
  gains a `crane` input, and `lib.<system>.ciCheckNames` is exported so CI can
  derive its job matrix from the checks rather than restating them.
- `checks.<system>.msrv` stops at `cargo check --all-targets` instead of
  building and testing the whole workspace under the MSRV toolchain. It
  verifies the same thing — that the declared MSRV still compiles the
  workspace — without a third full run of the test suite.
- The `dev` profile emits line tables instead of full debug information.
  Backtraces still resolve every frame to `file:line`; use
  `RUSTFLAGS=-Cdebuginfo=2` when step-debugging.
- CI runs one job per flake check instead of one `nix flake check` for all of
  them, and derives that job list from the flake so a new check cannot silently
  stop being verified.

## [1.2.0] - 2026-07-28

No command, flag, exit code, or JSON field changed in this release: the
capability catalogue is byte-identical to v1.1.0. What changed is the shape of
the source tree behind it, and — for anyone using the crate as a library — the
type of every error it can return.

### Changed

- The single crate is now a 24-package Cargo workspace: six `paredit-core-*`
  packages (syntax, semantics, edit, lint-engine, workspace, cli) and eighteen
  `paredit-feature-*` packages, with the binary as a thin composition root.
  The `paredit_cli` façade re-exports every package module, so existing import
  paths such as `paredit_cli::domain::sexpr` still resolve.
- The lint engine no longer depends on the rule registry. Rules are supplied
  through a `RuleCatalog`, which breaks the cycle that previously forced the
  engine and the 134 rules into one compilation unit.
- **Breaking for library consumers.** Fallible entry points return typed error
  enums instead of `anyhow::Result` — `SexprError`, `EditRefusal`, `LintError`,
  `CliError`, and per-feature equivalents. A caller that names a return type or
  matches on an error must be updated; `?` into an `anyhow::Result` still
  compiles unchanged. The CLI's own stderr text is unchanged, because `main`
  still converts at the boundary.
- Several types now make invalid states unrepresentable rather than checking
  for them: inline-function's call selection, split-file's destination, and the
  refactor manifest's hash and flag comparisons, which are derived rather than
  stored.
- The compatibility guide no longer lists the Rust library API as a stable
  surface. The crate is `publish = false` with no registry release, and the
  supported interface is the command line — see
  [Releases and compatibility](https://nerima-lisp.github.io/paredit-cli/releases.html).

## [1.1.0] - 2026-07-26

No command, flag, exit code, or JSON field changed in this release: the
capability catalogue is byte-identical to v1.0.0. What changed is what the
existing rules can prove, and a new public analysis layer beneath them.

### Added

- A static semantic analysis layer (`paredit_cli::domain::semantics`) of
  read-only side tables beside the syntax tree: bindings, constant values,
  types, and project-wide symbol identity. Facts are recorded only when
  provable — anything uncertain is absent rather than guessed.
- `defconstant` values now cross file boundaries. A constant defined exactly
  once project-wide resolves in the other files of its package; a file's own
  definition always wins, and a file with no `in-package` is unaffected.
- `lint_report::collect_lint_findings_and_fixes`, which answers both halves of
  a lint run from one dispatch pass.

### Changed

- `char-op-string` now flags any argument the type layer proves cannot be a
  character, not only a string literal — `(char= (length xs) c)` is the same
  guaranteed type error as `(char= "a" c)`.
- `redundant-the` now flags an assertion the form already satisfies, such as
  `(the integer (length xs))`, not only the vacuous `(the t x)`.
- Both changes can only add findings. If you gate CI on the lint exit code,
  expect new true positives on code that was always wrong.
- CI runs on Linux only and shares one Nix setup across workflows. Darwin is
  no longer verified in CI; see the development guide.
- The community and policy files (contributing, code of conduct, support,
  security, releasing) moved onto the documentation site.

### Fixed

- `undefined-package` no longer reports a package's own declared `:nicknames`
  as undefined. `(defpackage :app (:nicknames :a))` followed by
  `(in-package :a)` is correct code and was flagged as a typo.
- A `defconstant` is now found by a reference written in any case. The reader
  folds a symbol's case, so `+limit+` and `+LIMIT+` name one constant; the
  value table had keyed them separately.
- Quoted data no longer stops the value layer from reasoning about the
  surrounding scope. `'(setq x 2)` is a list, not an assignment.
- Declaration specifiers are no longer read as calls, so `(declare (ignore x))`
  no longer makes its enclosing scope unanalysable.
- `check-type`, `assert`, `remf`, and `multiple-value-setq` are now recorded as
  writing to their places, so a value is not propagated through a binding the
  program may replace.

### Performance

- Lint runs about 8% faster on finding-dense input, measured against v1.0.0
  back to back on one machine.

## [1.0.0] - 2026-07-26

### Added

- 49 new lint rules since v0.8.0, covering redundant forms, degenerate
  conditionals, explicit-default keyword arguments, string and character
  comparison simplification, and suspicious arithmetic.
- 276 leaf commands across inspect, edit, refactor, and completions.

### Changed

- First stable release: paredit-cli now follows semantic versioning. Within
  the 1.x series, command paths, flag names and documented defaults, the
  exit-code table, documented JSON fields for a given `schema_version`, the
  `paredit_cli` crate-root API, and the Nix packages, apps, overlay, and lib
  helpers change only in a major release.
- Repository moved to github.com/nerima-lisp/paredit-cli and documentation
  moved to nerima-lisp.github.io/paredit-cli.
- paredit-cli is not published to a package registry; the git tag remains
  the release artifact (install via `nix run`, `nix profile install`, or
  `cargo install --git`).

### Fixed

- `paredit edit ... --write` and `paredit refactor ... --write` failed for
  any file named without a directory component. Editing a file in the
  working directory by bare name now works.
- A symlinked input was refused with the raw `O_NOFOLLOW` errno, which read
  as a link cycle. The refusal now names the policy and the remedy.
