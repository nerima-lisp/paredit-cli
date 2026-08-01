//! The evaluated-code walk shared by seven of this package's eight rules —
//! every one except `commented-repl-transcript`, which reads comments rather
//! than forms.
//!
//! Five of the seven — `leftover-print-debug`, `leftover-trace-call`,
//! `leftover-break-call`, `leftover-inspect-call` and
//! `leftover-format-debug-marker` — also consult the [`RemovalSafety`] each
//! visited node carries. The other two, `leftover-step-call` and
//! `leftover-time-benchmark-call`, unwrap rather than remove and so are safe
//! in any position; they use the walk only to stay out of quoted data.
//!
//! Each of the five removes a whole call form as its fix — `(print x)`
//! disappears entirely, unlike a rewrite that replaces one span with another.
//! Whether that is safe depends on where the call sits, and is the same
//! question for all five:
//!
//! - A **bare top-level form** (a direct child of the file) is always safe:
//!   top-level forms are evaluated for effect only, so removing one changes
//!   nothing else.
//! - A **non-last form of an implicit-progn body** (a `defun`/`let`/`let*`/
//!   `progn`/`when`/`unless`/`cond`-clause/… body form that is not the body's
//!   own last form) is safe: only the body's last form determines the body's
//!   value.
//! - The **last form of such a body** is not: removing it changes the body's
//!   return value, so the rule must still report the finding but attach no
//!   fix.
//! - Anywhere else — a function-call argument, a binding's init form, an
//!   `if`/`when` test, a single-expression `if` branch, code reached through
//!   a quasiquote's `,`/`,@` escape — removal could break the form's
//!   structure or change its value, so it is never proposed. It is still
//!   visited and still reported; it just carries no fix.
//! - Anything inside a `quote`/quasiquote region *without* an escaping
//!   `,`/`,@` is data, not code, and is not visited at all: a `(print x)`
//!   shape appearing there is never a debug leftover to begin with.
//!
//! # A structural trap this walk exists to avoid: bindings look like calls
//!
//! `(step x)` is exactly the same three tokens whether it is a call to
//! `step` unwrapping to `x`, or a two-parameter lambda list, or a `let`
//! binding of `step` to `x`. Naively visiting every list's children
//! generically would report `(defun process (step x) ...)`'s lambda list as
//! a leftover `step` call — `step` is a common loop-variable name, so this is
//! not a hypothetical. [`binding_position`] names, for `defun`/`defmacro`/
//! `lambda`/`let`/`let*`/`flet`/`labels`/`macrolet`, exactly which child is a
//! lambda-list or bindings-list, and [`visit_bindings`] walks it with rules
//! that never treat a name or parameter position as a candidate — only a
//! `let`/`let*` binding's own init form, which is genuinely evaluated code.
//!
//! # A second trap: the matched head may not be the operator it names
//!
//! [`binding_position`] keeps a *binding-list* position from being misread as
//! a call, but says nothing about the *body* of the same form. Under
//! `(flet ((time (thunk) …)) (time fn))` the inner `(time fn)` is a call to
//! the file's own local function, not to `cl:time` — and unwrapping it to
//! `fn` returns a closure unevaluated instead of calling it. Matching on
//! symbol text alone cannot tell the two apart.
//!
//! [`OperatorScope`] answers that question by resolving the head atom against
//! this file's binding table, which already models `flet`/`labels`/`macrolet`
//! scope and each dialect's namespace and name-equality rules. A head that
//! resolves to something the file binds keeps its *finding* — an operator
//! named after a debug entry point is still worth a look — but is never
//! offered a *fix*.
//!
//! # Scope this walk deliberately does not cover
//!
//! - **Operator shadowing in a dialect the binding table does not model.**
//!   [`OperatorScope`] is only as precise as
//!   [`paredit_core_semantics::semantics::binding`], which analyses Common
//!   Lisp, Emacs Lisp, Scheme and Racket and returns an empty table for every
//!   other dialect. `leftover-print-debug` is the only rule of the seven that
//!   reaches further (Clojure, Janet, Fennel, Hy), and there a head shadowed
//!   by a local-function binder is not detected.
//! - **`do`/`do*`/`prog`/`prog*`/`multiple-value-bind`/`destructuring-bind`.**
//!   These have the same call-shaped-binding ambiguity as the forms above but
//!   are not in [`binding_position`]'s table, so a variable there named after
//!   a target debug head can still produce a false-positive *report*. Never a
//!   false-positive *fix*: none of these heads is in [`body_start`], so
//!   nothing inside one is ever [`RemovalSafety::Safe`].
//! - **`flet`/`labels`/`macrolet` local function bodies.** Each binding
//!   `(name lambda-list body...)` has its own implicit-progn body, but this
//!   walk (matching the precedent in
//!   `paredit-feature-lint-control-flow::redundant_body_progn`) only
//!   recognizes the *enclosing* form's own trailing body, not each binding's.
//!   A call inside a local function body therefore falls through to
//!   [`RemovalSafety::ReportOnly`] rather than [`RemovalSafety::Safe`] — a
//!   conservative under-approximation, never an unsafe removal.
//! - **`&optional`/`&key` lambda-list default-value forms**, e.g. the `(foo)`
//!   in `(x (foo))` within `(defun f (&optional (x (foo))) ...)`. A lambda
//!   list is not descended into at all (see [`BindingShape::LambdaList`]), so
//!   a leftover debug call there is silently missed — a false negative, not a
//!   false positive.
//! - **Vector/set reader literals** (`#(...)`, `#{...}`). Whether their
//!   elements are evaluated is dialect-dependent (data in Common Lisp, code in
//!   Clojure), so this walk does not change quoting depth for them either way
//!   — meaning a call inside one is *not* skipped as quoted data. This can
//!   only produce a false positive on an uncommon Common Lisp vector literal
//!   containing what reads as a call shape; it never mis-flags Clojure, where
//!   `#{(println 1)}` really does call `println`.

use std::cell::OnceCell;

use paredit_core_lint_engine::engine::RuleContext;
use paredit_core_semantics::semantics::NodeKey;
use paredit_core_semantics::semantics::binding::{BindingTable, build_binding_table};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_span;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_in};

/// Whether a matched call's head names the dialect's own operator, or
/// something this file binds under the same name.
///
/// Two variants because the two callers of a rule's `examine` reach the
/// binding table differently, but both defer the (expensive, whole-file)
/// build until [`head_is_locally_bound`] is first actually called — which
/// only happens once `examine` has already found a genuine syntactic
/// candidate. A file with nothing to flag must not pay for a binding table at
/// all: `Shared` holding `&RuleContext` rather than an already-fetched
/// `&BindingTable` is exactly what makes that true, since
/// [`RuleContext::binding_table`] is itself `OnceCell`-backed and building it
/// is what a naive "resolve it before scanning" construction would have
/// forced unconditionally on every file the lint suite touches, clean or not
/// — this shape shipped once, regressed `clean/forms/*` in the benchmark
/// suite by ~49%, and is the reason this doc paragraph exists.
///
/// [`head_is_locally_bound`]: OperatorScope::head_is_locally_bound
#[derive(Debug)]
pub enum OperatorScope<'a> {
    /// The lint suite's per-file context, shared with every other rule;
    /// `.binding_table()` is called only on first actual use.
    Shared(&'a RuleContext<'a>),
    /// A standalone command's own table, built on first use.
    Standalone {
        dialect: Dialect,
        tree: &'a SyntaxTree,
        table: OnceCell<BindingTable>,
    },
}

impl<'a> OperatorScope<'a> {
    #[must_use]
    pub const fn shared(context: &'a RuleContext<'a>) -> Self {
        Self::Shared(context)
    }

    #[must_use]
    pub const fn standalone(dialect: Dialect, tree: &'a SyntaxTree) -> Self {
        Self::Standalone {
            dialect,
            tree,
            table: OnceCell::new(),
        }
    }

    fn table(&self) -> &BindingTable {
        match self {
            Self::Shared(context) => context.binding_table(),
            Self::Standalone {
                dialect,
                tree,
                table,
            } => table.get_or_init(|| build_binding_table(*dialect, tree, tree.source())),
        }
    }

    /// Whether `call`'s head symbol resolves to a binding this file
    /// introduces — a `flet`/`labels`/`macrolet` local function or macro, or
    /// in a Lisp-1 a local variable holding a procedure.
    ///
    /// The namespace rule is not restated here: the table resolves a head
    /// atom in the function namespace for a Lisp-2 and in the single
    /// namespace for a Lisp-1, so a mere `(let ((print 1)) (print x))` in
    /// Common Lisp is correctly *not* a shadowed call. A miss means the name
    /// is free, which for these rules means it is the dialect's own operator.
    #[must_use]
    pub fn head_is_locally_bound(&self, call: &ExpressionView) -> bool {
        match call.children.first().and_then(atom_symbol_span) {
            Some(span) => self.symbol_span_is_locally_bound(span),
            None => false,
        }
    }

    /// [`OperatorScope::head_is_locally_bound`], for a caller that already
    /// has the head atom's own symbol span — [`EvaluatedCandidate::head_symbol_span`],
    /// precomputed once per node by the shared walk rather than re-derived
    /// from a live view on every rule's own check. The resolution itself
    /// (and the binding-table build it may trigger on first use) still only
    /// happens here, when a rule actually calls this — never during the
    /// shared walk, which must stay ignorant of the binding table entirely
    /// so an unrelated candidate can never force it.
    #[must_use]
    pub fn symbol_span_is_locally_bound(&self, span: ByteSpan) -> bool {
        self.table().resolve(NodeKey::atom(span)).is_some()
    }
}

/// Whether a flagged call form may be removed outright.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovalSafety {
    /// A bare top-level form, or a non-last form of an implicit-progn/`cond`-
    /// clause body: removing it cannot change anything else's meaning.
    Safe,
    /// Anywhere else, including the last form of such a body: still worth
    /// reporting, but no fix is offered.
    ReportOnly,
}

/// Where a visited node sits among its siblings, for [`removal_span`] to turn
/// into the exact byte range a removal fix replaces with nothing.
#[derive(Debug, Clone, Copy)]
pub struct FormPosition<'a> {
    pub safety: RemovalSafety,
    siblings: &'a [ExpressionView],
    index: usize,
}

/// The exact byte span a removal fix should replace with `""`.
///
/// Absorbing one adjacent separator (whitespace/newline) is what keeps a
/// removal from leaving a double space or a blank line behind: extending
/// forward to the next sibling when one follows, or back to the previous
/// sibling when this is the last of the group. A lone sibling (no others to
/// absorb toward) removes just its own span, which can leave a blank body —
/// harmless and still valid source.
#[must_use]
pub fn removal_span(position: FormPosition<'_>, own_span: ByteSpan) -> ByteSpan {
    let FormPosition {
        siblings, index, ..
    } = position;
    if let Some(next) = siblings.get(index + 1) {
        ByteSpan::new(own_span.start(), next.span.start())
    } else if index > 0 {
        siblings.get(index - 1).map_or(own_span, |previous| {
            ByteSpan::new(previous.span.end(), own_span.end())
        })
    } else {
        own_span
    }
}

/// What a group of siblings a walk is currently descending into means for
/// each child's removal safety.
#[derive(Debug, Clone, Copy)]
enum BodyContext {
    /// Direct children of the file itself: always [`RemovalSafety::Safe`].
    TopLevel,
    /// Children at or after this index are an implicit-progn body; the last
    /// one is [`RemovalSafety::ReportOnly`], every earlier one is
    /// [`RemovalSafety::Safe`]. Children before this index (a lambda list, a
    /// lambda-form name, …) are [`RemovalSafety::ReportOnly`].
    Body(usize),
    /// No recognized body shape: every child is [`RemovalSafety::ReportOnly`].
    None,
}

/// The child index at which a form's implicit-progn body begins, keyed by its
/// own head — `None` when the head is not one of the recognized
/// implicit-progn forms.
///
/// Mirrors `paredit-feature-lint-control-flow::redundant_body_progn`'s table,
/// plus `progn` itself: that rule's siblings `nested-progn`/`redundant-progn`
/// own the *wrapper* of a `progn`, but a call form sitting directly in one is
/// a body-of-one exactly like the others.
fn body_start(head: &str) -> Option<usize> {
    let after_one = ["progn"];
    let after_two = [
        "when", "unless", "dolist", "dotimes", "block", "lambda", "let", "let*", "flet", "labels",
        "macrolet", "catch",
    ];
    let after_three = ["defun", "defmacro"];
    if after_one.iter().any(|name| head.eq_ignore_ascii_case(name)) {
        Some(1)
    } else if after_two.iter().any(|name| head.eq_ignore_ascii_case(name)) {
        Some(2)
    } else if after_three
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        Some(3)
    } else {
        None
    }
}

/// The child index at which `head`'s clauses begin, for the `cond`-family
/// forms whose children are themselves `(test body...)` lists rather than
/// body forms directly. `None` for every other head.
///
/// Index 0 is always the head symbol itself (`view.children[0]` is the
/// `cond`/`case` atom, exactly as it is for [`body_start`]): `cond`'s clauses
/// start at index 1, and `case`/`typecase`/…'s at index 2 — one further in,
/// past the keyform they test.
fn clause_start(head: &str) -> Option<usize> {
    if head.eq_ignore_ascii_case("cond") {
        Some(1)
    } else if [
        "case",
        "ccase",
        "ecase",
        "typecase",
        "ctypecase",
        "etypecase",
    ]
    .iter()
    .any(|name| head.eq_ignore_ascii_case(name))
    {
        Some(2)
    } else {
        None
    }
}

/// The net change in "quoted data" depth `view`'s own reader prefixes cause.
///
/// `quote`/quasiquote push one level of "this and everything under it is data
/// unless escaped"; `unquote`/`unquote-splicing` pop one level back to code.
/// Everything else (`#'`, `#.`, Clojure metadata, reader conditionals, …) is
/// left neutral — see the module doc's "scope this walk does not cover".
fn quote_delta(view: &ExpressionView) -> i32 {
    view.reader_prefixes
        .iter()
        .map(|prefix| match prefix {
            ReaderPrefix::Quote | ReaderPrefix::Quasiquote => 1,
            ReaderPrefix::Unquote | ReaderPrefix::UnquoteSplicing => -1,
            _ => 0,
        })
        .sum()
}

/// Walks every node reachable as *evaluated code* from `root` (a file's
/// `tree.root_view()`), calling `on_node` once for each with the removal
/// safety its position implies.
///
/// A node inside unescaped quoted data is never visited at all — not "visited
/// but marked unsafe" — because it is not a call to flag in the first place.
pub fn walk_evaluated_forms<'a>(
    root: &'a ExpressionView,
    mut on_node: impl FnMut(&'a ExpressionView, FormPosition<'a>),
) {
    visit_children(&root.children, 0, BodyContext::TopLevel, &mut on_node);
}

/// One call-shaped node (`(head ...)`) reachable as evaluated code, with
/// everything any of the seven rules that share this walk need already
/// worked out — so the walk itself runs at most once per file (see
/// [`evaluated_candidates`]), and each rule's own `check()` only filters and
/// reads.
///
/// Fully owned (an [`ExpressionView`] *shallow clone*, not a borrow into the
/// tree that produced it) rather than a reference, and deliberately so: that
/// is what lets [`evaluated_candidates`] hand the whole `Vec` to
/// [`RuleContext::scratch_cache`], whose slot has no borrowed-data escape
/// hatch (a `'static` bound).
///
/// "Shallow" is load-bearing. [`ExpressionView::clone`] is recursive — it
/// copies a node's *entire* subtree, grandchildren and all. None of the
/// seven rules ever reads past a candidate's immediate children (confirmed
/// by grep across every `leftover_*::domain` module: the deepest access is
/// `view.children.get(1)` / `.get(2)` read as atoms, never
/// `.children[1].children`), so [`shallow_clone`] copies exactly one level —
/// the matched node's own fields plus its immediate children's own fields —
/// and replaces every grandchild with an empty `children` vec. A first
/// attempt at this cache used a plain recursive `.clone()` here and
/// benchmarked *worse* than the seven-independent-walks baseline it was
/// meant to fix (+53%/+11% on `clean/forms/64`/`clean/forms/1024`, measured
/// locally): a `(defun clean-fn-N (a b) "doc" (+ a (* b 2)))`-shaped
/// benchmark fixture has no findings for any of these rules, so the entire
/// per-candidate cost *is* the clone, and cloning every evaluated call's
/// whole subtree — not just the ones a rule's head match cares about — cost
/// far more than the seven redundant walks it replaced. Shallow cloning
/// fixed that: see `evaluated_candidates`'s doc for the corrected
/// measurement.
#[derive(Debug, Clone)]
pub struct EvaluatedCandidate {
    /// The matched node, one level deep: every rule already reads
    /// `list_head`, `.children` (as `.get(1)`/`.get(2)`/`[1]`, never
    /// deeper), `.reader_prefixes`, and atom `.text` off a view exactly like
    /// this one, so nothing downstream has to change shape to use a cached
    /// candidate instead of a live one — it only has to stay shallow.
    pub view: ExpressionView,
    pub safety: RemovalSafety,
    /// Precomputed once, during the one shared walk, while sibling context
    /// was still available. `None` exactly when `safety` is
    /// [`RemovalSafety::ReportOnly`].
    pub removal_span: Option<ByteSpan>,
    /// The head atom's own symbol span (past reader prefixes) — see
    /// [`OperatorScope::symbol_span_is_locally_bound`]. Precomputed for the
    /// same sibling-context reason as `removal_span`, but deliberately *not*
    /// resolved against the binding table here: that stays each rule's own
    /// job, gated behind its own head match, so a candidate no rule cares
    /// about can never force the (still expensive, still lazy) binding-table
    /// build. The shared walk stays wholly ignorant of the binding table.
    pub head_symbol_span: Option<ByteSpan>,
}

/// A one-level-deep copy of `view`: its own fields verbatim, and each
/// immediate child's own fields verbatim but with *that* child's `children`
/// replaced by an empty `Vec` — a grandchild is never materialized.
///
/// Every field [`ExpressionView`] has is `pub`, so this is a plain field-by-
/// field construction, not a workaround for anything private. It exists
/// because [`ExpressionView`]'s own [`Clone`] impl is whole-subtree-deep by
/// design (it backs real tree edits elsewhere, which need every descendant),
/// and reusing it here would silently reintroduce the O(subtree size)
/// per-candidate cost this module's doc comment measures and rejects.
fn shallow_clone(view: &ExpressionView) -> ExpressionView {
    ExpressionView {
        kind: view.kind,
        delimiter: view.delimiter,
        reader_prefixes: view.reader_prefixes.clone(),
        span: view.span,
        content_span: view.content_span,
        text: view.text.clone(),
        children: view.children.iter().map(shallow_child).collect(),
        symbol_offset: view.symbol_offset,
    }
}

/// One immediate child within [`shallow_clone`]: same fields as the node
/// itself, but `children` forced empty since nothing reads a grandchild.
fn shallow_child(view: &ExpressionView) -> ExpressionView {
    ExpressionView {
        kind: view.kind,
        delimiter: view.delimiter,
        reader_prefixes: view.reader_prefixes.clone(),
        span: view.span,
        content_span: view.content_span,
        text: view.text.clone(),
        children: Vec::new(),
        symbol_offset: view.symbol_offset,
    }
}

/// The union of every head symbol any of the seven sharing rules can match,
/// across every dialect — the filter that keeps the shared walk allocation-
/// free on a file with nothing to find.
///
/// # Why this exists
///
/// Without it, [`compute_evaluated_forms`] built an [`EvaluatedCandidate`]
/// for *every* call site in the file, and each one heap-allocates several
/// times (a [`shallow_clone`] copies `reader_prefixes`, `text`, and a
/// `children` vec whose every element copies its own two again). The seven
/// independent walks this shared walk replaced allocated *nothing* on a clean
/// file — each compared the head against its own small static table, missed,
/// and moved on. `scripts/bench-compare.sh`'s `clean/forms/*` benchmarks are
/// exactly that zero-findings case, so they measured the difference directly:
/// CI reported +17.3%/+17.2%, *worse* than the +13.8%/+10.8% the unification
/// was meant to fix. The single walk was a real saving spent several times
/// over on allocation.
///
/// Filtering here restores the allocation-free clean path — no candidate is
/// constructed at all unless its head could match something — while keeping
/// the one-walk-instead-of-seven win. Each rule still runs its own precise
/// head match against its own table on the (now tiny) candidate list, so no
/// rule's behavior changes.
///
/// # Why one cross-dialect union rather than a dialect-selected one
///
/// This is deliberately a superset: it merges every dialect's spellings
/// rather than selecting per dialect. A dialect-selected filter would be
/// marginally tighter, but it would have to exactly reproduce each rule's own
/// dialect gating to stay correct — and those gates do not all live in the
/// same place (the standalone `collect_*` entry points gate on dialect, while
/// the aggregate lint path's `examine` does not). Over-including a head is
/// free (the owning rule's own match rejects it); under-including one would
/// silently stop a rule from ever firing. `single_source_of_truth` below is
/// the drift guard: it fails if any rule's table ever grows a head this
/// union does not cover.
const CANDIDATE_HEADS: &[&str] = &[
    // `leftover-print-debug`, merged across every dialect its `heads_for`
    // models (Common Lisp, Emacs Lisp, Clojure, Scheme, Racket, Janet,
    // Fennel, Hy).
    "princ",
    "print",
    "prin1",
    "pprint",
    "message",
    "println",
    "prn",
    "display",
    "displayln",
    "pp",
    // The six fixed-table rules.
    "trace",
    "untrace",
    "break",
    "inspect",
    "describe",
    "format",
    "step",
    "time",
];

/// Whether `head` could match any of the seven sharing rules, using the same
/// package-qualifier-stripping, case-insensitive comparison the rules
/// themselves use — so `cl:print` and `PRINT` pass the filter exactly as
/// `print` does.
fn is_candidate_head(head: &str) -> bool {
    symbol_in(head, CANDIDATE_HEADS)
}

/// The result of one shared walk: the candidates worth examining, and the
/// scanned-form denominator the standalone `inspect <rule>` commands report.
///
/// The two are deliberately separate. `candidates` is filtered down to heads
/// that could match ([`CANDIDATE_HEADS`]), but `scanned_form_count` counts
/// *every* node the walk visits, atoms included — which is what each rule's
/// `collect_*` reported before the walk was shared, and what its
/// `scanned_form_count` JSON/text field has always meant. Deriving the
/// denominator from `candidates.len()` instead would silently redefine a
/// published output field.
#[derive(Debug, Clone)]
pub struct EvaluatedForms {
    pub candidates: Vec<EvaluatedCandidate>,
    pub scanned_form_count: usize,
}

/// Every call-shaped node in `root` reachable as evaluated code whose head
/// could match one of the seven sharing rules, plus the total number of nodes
/// walked.
///
/// A caller with a [`RuleContext`] should prefer [`evaluated_candidates`],
/// which computes this at most once per file no matter how many of the seven
/// sharing rules ask. This is for the standalone `inspect <rule>` commands,
/// which build their own [`OperatorScope::standalone`] and have no
/// `RuleContext` — and so nothing to share a walk with in the first place;
/// each standalone command already only walks once per invocation.
#[must_use]
pub fn compute_evaluated_forms(root: &ExpressionView) -> EvaluatedForms {
    let mut candidates = Vec::new();
    let mut scanned_form_count = 0;
    walk_evaluated_forms(root, |view, position| {
        // Counted for every visited node, before any filtering: this is the
        // published denominator, not a count of what survived the filter.
        scanned_form_count += 1;
        if !is_paren_list(view) {
            return;
        }
        // `is_candidate_head` runs before anything is built, so a call site
        // no rule could match costs one comparison against a small static
        // table and allocates nothing at all.
        let Some(head) = list_head(view) else {
            return;
        };
        if !is_candidate_head(head) {
            return;
        }
        let removal_span_value = matches!(position.safety, RemovalSafety::Safe)
            .then(|| removal_span(position, view.span));
        let head_symbol_span = view.children.first().and_then(atom_symbol_span);
        candidates.push(EvaluatedCandidate {
            view: shallow_clone(view),
            safety: position.safety,
            removal_span: removal_span_value,
            head_symbol_span,
        });
    });
    EvaluatedForms {
        candidates,
        scanned_form_count,
    }
}

/// [`compute_evaluated_forms`]'s candidates, computed at most once per file: the
/// first of the seven sharing rules whose `check()` runs for this file does
/// the walk, cached in `context`'s own [`RuleContext::scratch_cache`], and
/// the other six read the same result back rather than walking again.
///
/// This is the fix for a real, twice-CI-caught regression. Before it, each
/// of the seven rules ran its own independent full tree walk — seven
/// redundant O(n) passes over every file, clean or not — and
/// `scripts/bench-compare.sh`'s `clean/forms/*` benchmarks (a file with
/// nothing to flag, so the walk is the *entire* per-file cost these rules
/// add) measured it at +10.8%/+13.8%, over the 10% gate.
///
/// Sharing across seven otherwise-independent `LintRule::check()` calls is
/// safe here specifically because of what [`RuleContext::scratch_cache`]'s
/// own doc explains: a `RuleContext` is constructed fresh once per file's
/// pass and never reused or looked up by identity, so unlike a thread-local
/// or global cache keyed by a pointer or a path — both of which a stack
/// address or a re-linted-in-place file can alias across two *different*
/// files or passes — there is no key to get wrong.
///
/// The sharing itself was not the whole fix, though. Two later corrections
/// were needed, both caught by CI rather than by local benchmarking (this
/// project's dev sandbox reports wildly unstable bench numbers under parallel
/// load, so only CI's `bench-compare` job can settle a question like this):
///
/// 1. A first version stored each candidate with a plain `.clone()`, which
///    for an [`ExpressionView`] is a *whole-subtree* deep copy. See
///    [`EvaluatedCandidate`]'s doc and [`shallow_clone`].
/// 2. Even shallow, cloning at *every* call site still allocated far more
///    than the seven allocation-free walks it replaced — CI measured
///    +17.3%/+17.2%, worse than before the unification. See
///    [`CANDIDATE_HEADS`], the head-union filter that fixed it.
///
/// With both corrections the clean path allocates nothing: no candidate is
/// constructed unless its head could match a rule, so a file with nothing to
/// find pays one walk and one static-table comparison per call site, instead
/// of seven walks (before) or one walk plus an allocation per call site
/// (the regressing version).
#[must_use]
pub fn evaluated_candidates<'a>(
    context: &'a RuleContext<'a>,
    root: &ExpressionView,
) -> &'a [EvaluatedCandidate] {
    &context
        .scratch_cache(|| compute_evaluated_forms(root))
        .candidates
}

fn visit_children<'a>(
    siblings: &'a [ExpressionView],
    quote_depth: i32,
    context: BodyContext,
    on_node: &mut impl FnMut(&'a ExpressionView, FormPosition<'a>),
) {
    let last_index = siblings.len().saturating_sub(1);
    for (index, child) in siblings.iter().enumerate() {
        let child_depth = (quote_depth + quote_delta(child)).max(0);

        let safety = match context {
            BodyContext::TopLevel => RemovalSafety::Safe,
            BodyContext::Body(start) if index >= start => {
                if index < last_index {
                    RemovalSafety::Safe
                } else {
                    RemovalSafety::ReportOnly
                }
            }
            BodyContext::Body(_) | BodyContext::None => RemovalSafety::ReportOnly,
        };

        if child_depth == 0 {
            on_node(
                child,
                FormPosition {
                    safety,
                    siblings,
                    index,
                },
            );
        }

        recurse_into(child, child_depth, on_node);
    }
}

/// Descends into `view`'s own children (if it has any), choosing the
/// [`BodyContext`] its head implies.
fn recurse_into<'a>(
    view: &'a ExpressionView,
    quote_depth: i32,
    on_node: &mut impl FnMut(&'a ExpressionView, FormPosition<'a>),
) {
    if view.kind != ExpressionKind::List {
        return;
    }
    let head = list_head(view);

    // `(quote x)` is `'x` spelled out: `quote_delta` only reads *reader*
    // prefixes, which the reader desugars `'x` into, so the explicit list
    // form needs its own check here — otherwise `(quote (foo))` would walk
    // `(foo)` as ordinary evaluated code instead of the data it is.
    if head.is_some_and(|h| h.eq_ignore_ascii_case("quote")) && view.children.len() == 2 {
        let quoted = &view.children[1];
        let quoted_depth = (quote_depth + 1 + quote_delta(quoted)).max(0);
        if quoted_depth == 0 {
            on_node(
                quoted,
                FormPosition {
                    safety: RemovalSafety::ReportOnly,
                    siblings: &view.children,
                    index: 1,
                },
            );
        }
        recurse_into(quoted, quoted_depth, on_node);
        return;
    }

    if let Some(start) = head.and_then(clause_start) {
        // Children before `start` (the keyform of `case`/`typecase`/…, none
        // for `cond`) are ordinary evaluated positions, not clauses.
        let keyform_end = start.min(view.children.len());
        visit_children(
            &view.children[..keyform_end],
            quote_depth,
            BodyContext::None,
            on_node,
        );
        // Children at or after `start` are clause lists (`(test body...)`):
        // not themselves candidates (a clause's leading element is a key or
        // type designator, not a call, even when it happens to share a
        // debug-print function's name), but their own children from index 1
        // are the clause's implicit-progn body.
        for child in view.children.iter().skip(start) {
            let child_depth = (quote_depth + quote_delta(child)).max(0);
            if child_depth == 0 {
                visit_children(&child.children, child_depth, BodyContext::Body(1), on_node);
            }
        }
        return;
    }

    if let Some((binding_idx, shape)) = head.and_then(binding_position) {
        // A lambda-list or bindings-list child is syntactically
        // indistinguishable from a call — `(step x)` reads the same as a
        // two-parameter lambda list and as a call unwrapping `x` after a
        // `step`. It must never reach the generic per-child dispatch below,
        // which would call `on_node` on it directly; [`visit_bindings`]
        // walks it with its own shape-aware rules instead.
        let body_context = head
            .and_then(body_start)
            .map_or(BodyContext::None, BodyContext::Body);
        let last_index = view.children.len().saturating_sub(1);
        for (index, child) in view.children.iter().enumerate() {
            let child_depth = (quote_depth + quote_delta(child)).max(0);
            if index == binding_idx {
                if child_depth == 0 {
                    visit_bindings(child, child_depth, shape, on_node);
                }
                continue;
            }
            let safety = match body_context {
                BodyContext::Body(start) if index >= start => {
                    if index < last_index {
                        RemovalSafety::Safe
                    } else {
                        RemovalSafety::ReportOnly
                    }
                }
                BodyContext::Body(_) | BodyContext::None | BodyContext::TopLevel => {
                    RemovalSafety::ReportOnly
                }
            };
            if child_depth == 0 {
                on_node(
                    child,
                    FormPosition {
                        safety,
                        siblings: &view.children,
                        index,
                    },
                );
            }
            recurse_into(child, child_depth, on_node);
        }
        return;
    }

    let context = head
        .and_then(body_start)
        .map_or(BodyContext::None, BodyContext::Body);
    visit_children(&view.children, quote_depth, context, on_node);
}

/// The shape of a form's binding/lambda list, for [`binding_position`].
#[derive(Debug, Clone, Copy)]
enum BindingShape {
    /// A lambda list (`defun`/`defmacro`/`lambda`, and a `flet`/`labels`
    /// binding's own): deliberately never descended into at all.
    /// `&optional`/`&key` default-value forms can contain real evaluated
    /// code (`(x (compute-default))`), but recognizing that shape is out of
    /// this batch's scope — a documented false negative, never a false
    /// positive or an unsafe fix.
    LambdaList,
    /// `let`/`let*`: each binding is a bare name or `(name init)`.
    LetBindings,
    /// `flet`/`labels`/`macrolet`: each binding is
    /// `(name lambda-list body...)`.
    DefBindings,
}

/// The child index holding a form's lambda-list or bindings-list, and its
/// shape, for the heads this walk models. `None` for a head with no such
/// position, or one this walk does not model — `do`/`do*`/`prog`/`prog*`/
/// `multiple-value-bind`/`destructuring-bind` all have the same
/// call-shaped-binding ambiguity and are not covered; a binding or parameter
/// there named after a target debug head (`step` is a common loop-variable
/// name) can still produce a false-positive *report*. Never a false-positive
/// *fix*: none of these heads is in [`body_start`], so nothing inside one is
/// ever [`RemovalSafety::Safe`].
fn binding_position(head: &str) -> Option<(usize, BindingShape)> {
    if head.eq_ignore_ascii_case("defun") || head.eq_ignore_ascii_case("defmacro") {
        Some((2, BindingShape::LambdaList))
    } else if head.eq_ignore_ascii_case("lambda") {
        Some((1, BindingShape::LambdaList))
    } else if head.eq_ignore_ascii_case("let") || head.eq_ignore_ascii_case("let*") {
        Some((1, BindingShape::LetBindings))
    } else if head.eq_ignore_ascii_case("flet")
        || head.eq_ignore_ascii_case("labels")
        || head.eq_ignore_ascii_case("macrolet")
    {
        Some((1, BindingShape::DefBindings))
    } else {
        None
    }
}

/// Walks a lambda-list or bindings-list per its [`BindingShape`], visiting
/// only the sub-positions that are genuinely evaluated code.
fn visit_bindings<'a>(
    bindings_list: &'a ExpressionView,
    quote_depth: i32,
    shape: BindingShape,
    on_node: &mut impl FnMut(&'a ExpressionView, FormPosition<'a>),
) {
    if bindings_list.kind != ExpressionKind::List {
        return;
    }
    match shape {
        BindingShape::LambdaList => {}
        BindingShape::LetBindings => {
            for binding in &bindings_list.children {
                let binding_depth = (quote_depth + quote_delta(binding)).max(0);
                if binding_depth != 0 || binding.kind != ExpressionKind::List {
                    // A bare `name` binding (no init) has nothing to visit;
                    // a quoted binding form is data, not visited either.
                    continue;
                }
                // `(name init)` (or `(name)`): only `init`, at index 1, is
                // evaluated code — `name` never is, even when it is itself
                // shaped like a call.
                if let Some(init) = binding.children.get(1) {
                    let init_depth = (binding_depth + quote_delta(init)).max(0);
                    if init_depth == 0 {
                        on_node(
                            init,
                            FormPosition {
                                safety: RemovalSafety::ReportOnly,
                                siblings: &binding.children,
                                index: 1,
                            },
                        );
                    }
                    recurse_into(init, init_depth, on_node);
                }
            }
        }
        BindingShape::DefBindings => {
            for binding in &bindings_list.children {
                let binding_depth = (quote_depth + quote_delta(binding)).max(0);
                if binding_depth != 0 || binding.kind != ExpressionKind::List {
                    continue;
                }
                // `(name lambda-list body...)`: `name` (index 0) and the
                // local function's own lambda-list (index 1) are never
                // code — see `BindingShape::LambdaList`. The body
                // (index >= 2) is visited, report-only: this walk does not
                // track each local function's *own* body separately from
                // its enclosing form's (see the module doc).
                if binding.children.len() > 2 {
                    visit_children(
                        &binding.children[2..],
                        binding_depth,
                        BodyContext::None,
                        on_node,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    fn root(input: &str) -> ExpressionView {
        SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp)
            .expect("parse")
            .root_view()
    }

    fn heads_with_safety(input: &str) -> Vec<(String, RemovalSafety)> {
        let root = root(input);
        let mut out = Vec::new();
        walk_evaluated_forms(&root, |view, position| {
            if let Some(head) = list_head(view) {
                out.push((head.to_owned(), position.safety));
            }
        });
        out
    }

    #[test]
    fn a_bare_top_level_form_is_always_safe() {
        let out = heads_with_safety("(foo)");
        assert_eq!(out, vec![("foo".to_owned(), RemovalSafety::Safe)]);
    }

    #[test]
    fn the_last_of_several_top_level_forms_is_still_safe() {
        let out = heads_with_safety("(a)\n(b)\n(c)");
        assert_eq!(
            out,
            vec![
                ("a".to_owned(), RemovalSafety::Safe),
                ("b".to_owned(), RemovalSafety::Safe),
                ("c".to_owned(), RemovalSafety::Safe),
            ]
        );
    }

    #[test]
    fn a_non_last_body_form_is_safe() {
        let out = heads_with_safety("(defun f (x) (foo x) (bar x))");
        // defun, foo, bar — foo is not last, bar is.
        assert_eq!(out[1], ("foo".to_owned(), RemovalSafety::Safe));
        assert_eq!(out[2], ("bar".to_owned(), RemovalSafety::ReportOnly));
    }

    #[test]
    fn a_when_bodys_last_form_is_report_only() {
        let out = heads_with_safety("(when c (foo) (bar))");
        assert_eq!(out[1], ("foo".to_owned(), RemovalSafety::Safe));
        assert_eq!(out[2], ("bar".to_owned(), RemovalSafety::ReportOnly));
    }

    #[test]
    fn a_progns_sole_form_is_report_only() {
        let out = heads_with_safety("(progn (foo))");
        assert_eq!(out[1], ("foo".to_owned(), RemovalSafety::ReportOnly));
    }

    #[test]
    fn an_argument_position_is_report_only() {
        let out = heads_with_safety("(+ 1 (foo))");
        assert_eq!(out[1], ("foo".to_owned(), RemovalSafety::ReportOnly));
    }

    #[test]
    fn a_let_binding_init_is_report_only() {
        let out = heads_with_safety("(let ((x (foo))) x)");
        assert_eq!(out[1], ("foo".to_owned(), RemovalSafety::ReportOnly));
    }

    #[test]
    fn an_if_test_and_branch_are_report_only() {
        let out = heads_with_safety("(if (foo) (bar) (baz))");
        // `if` itself is a bare top-level form (Safe); its test and branches
        // are not implicit-progn body positions at all, so every one of them
        // is report-only.
        assert!(
            out.iter()
                .filter(|(head, _)| head != "if")
                .all(|(_, safety)| *safety == RemovalSafety::ReportOnly)
        );
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn a_cond_clause_non_last_form_is_safe() {
        let out = heads_with_safety("(cond (test (foo) (bar)))");
        assert_eq!(out[1], ("foo".to_owned(), RemovalSafety::Safe));
        assert_eq!(out[2], ("bar".to_owned(), RemovalSafety::ReportOnly));
    }

    #[test]
    fn a_case_clause_body_is_recognized_past_the_keyform() {
        let out = heads_with_safety("(case x (:a (foo) (bar)))");
        assert_eq!(out[1], ("foo".to_owned(), RemovalSafety::Safe));
        assert_eq!(out[2], ("bar".to_owned(), RemovalSafety::ReportOnly));
    }

    #[test]
    fn a_case_keyform_position_is_still_visited() {
        // The keyform is evaluated (unlike a clause's key designator), so a
        // call there must still be reported — just never as a body position.
        let out = heads_with_safety("(case (foo) (:a 1))");
        assert_eq!(
            out,
            vec![
                ("case".to_owned(), RemovalSafety::Safe),
                ("foo".to_owned(), RemovalSafety::ReportOnly),
            ]
        );
    }

    #[test]
    fn a_clause_key_designator_sharing_a_debug_heads_name_is_not_a_call() {
        // `print` here is `case`'s key designator, not a call to `print` —
        // this is exactly what `clause_start` exists to keep the two apart.
        let out = heads_with_safety("(case x (print (foo)))");
        assert!(!out.iter().any(|(head, _)| head == "print"));
        assert_eq!(
            out,
            vec![
                ("case".to_owned(), RemovalSafety::Safe),
                ("foo".to_owned(), RemovalSafety::ReportOnly),
            ]
        );
    }

    #[test]
    fn a_lambda_list_parameter_sharing_a_debug_heads_name_is_not_a_call() {
        // `(step x)` reads identically as a two-parameter lambda list and as
        // a call to `step` unwrapping to `x` — `binding_position` is what
        // tells them apart.
        let out = heads_with_safety("(defun process (step x) (+ step x))");
        assert!(!out.iter().any(|(head, _)| head == "step"));
    }

    #[test]
    fn a_let_binding_name_sharing_a_debug_heads_name_is_not_a_call() {
        let out = heads_with_safety("(let ((step (foo))) step)");
        assert!(!out.iter().any(|(head, _)| head == "step"));
        assert!(
            out.iter()
                .any(|(head, safety)| { head == "foo" && *safety == RemovalSafety::ReportOnly })
        );
    }

    #[test]
    fn an_explicit_quote_list_hides_its_argument_the_same_as_the_reader_sugar() {
        let out = heads_with_safety("(quote (foo (bar)))");
        assert!(out.iter().all(|(head, _)| head != "foo" && head != "bar"));
    }

    #[test]
    fn a_quoted_call_shape_is_never_visited() {
        let out = heads_with_safety("'(foo (bar))");
        assert!(out.is_empty());
    }

    #[test]
    fn quote_of_the_symbol_quote_itself_is_still_quoted() {
        let out = heads_with_safety("(quote (foo))");
        // The outer `(quote ...)` form is itself a bare top-level form (and
        // is visited as one — nothing in this walk special-cases `quote` as
        // a head worth hiding), but its argument is data and must not appear.
        assert_eq!(out, vec![("quote".to_owned(), RemovalSafety::Safe)]);
    }

    #[test]
    fn an_unquote_inside_quasiquote_is_visited_but_never_a_body_position() {
        // `,(foo)` is spliced into the backquote's data shape, not a body
        // form of anything — removing it would change that shape, the same
        // way removing a function argument would.
        let out = heads_with_safety("`(a ,(foo) b)");
        assert_eq!(out, vec![("foo".to_owned(), RemovalSafety::ReportOnly)]);
    }

    #[test]
    fn a_quasiquoted_form_with_no_unquote_stays_data() {
        let out = heads_with_safety("`(a (foo) b)");
        assert!(out.is_empty());
    }

    #[test]
    fn removal_span_absorbs_the_trailing_separator_when_a_sibling_follows() {
        let input = "(defun f (x) (foo x) (bar x))";
        let root = root(input);
        let defun = &root.children[0];
        // children: `defun`, `f`, `(x)` (the lambda list), `(foo x)`, `(bar x)`.
        let foo = &defun.children[3];
        let mut span = None;
        walk_evaluated_forms(&root, |view, position| {
            if std::ptr::eq(view, foo) {
                span = Some(removal_span(position, view.span));
            }
        });
        let span = span.expect("foo visited");
        assert_eq!(&input[span.start().get()..span.end().get()], "(foo x) ");
    }

    #[test]
    fn removal_span_absorbs_the_leading_separator_for_the_final_sibling() {
        let input = "(when c (foo) (bar))";
        let root = root(input);
        let when_form = &root.children[0];
        // children: `when`, `c` (the test), `(foo)`, `(bar)`.
        let bar = &when_form.children[3];
        let mut span = None;
        walk_evaluated_forms(&root, |view, position| {
            if std::ptr::eq(view, bar) {
                span = Some(removal_span(position, view.span));
            }
        });
        let span = span.expect("bar visited");
        assert_eq!(&input[span.start().get()..span.end().get()], " (bar)");
    }

    #[test]
    fn compute_evaluated_forms_matches_walk_evaluated_forms() {
        // The candidate list must agree with the raw walk on every node it
        // keeps: same head, same safety, same order — just restricted to
        // heads a rule could match.
        let input = "(defun f (x) (print x) (time x))";
        let root = root(input);
        let forms = compute_evaluated_forms(&root);
        let via_candidates: Vec<(String, RemovalSafety)> = forms
            .candidates
            .iter()
            .filter_map(|c| list_head(&c.view).map(|h| (h.to_owned(), c.safety)))
            .collect();
        let via_walk: Vec<(String, RemovalSafety)> = heads_with_safety(input)
            .into_iter()
            .filter(|(head, _)| is_candidate_head(head))
            .collect();
        assert_eq!(via_candidates, via_walk);
        assert_eq!(
            via_candidates,
            vec![
                ("print".to_owned(), RemovalSafety::Safe),
                ("time".to_owned(), RemovalSafety::ReportOnly),
            ]
        );
    }

    #[test]
    fn compute_evaluated_forms_skips_heads_no_rule_can_match() {
        // The allocation-free clean path: a file whose call sites are all
        // ordinary code yields no candidates at all, however many calls it
        // has. This is what `clean/forms/*` measures.
        let root = root("(defun f (a b) \"doc\" (+ a (* b 2)))");
        let forms = compute_evaluated_forms(&root);
        assert!(
            forms.candidates.is_empty(),
            "no candidate should be built for heads no rule can match, got {:?}",
            forms
                .candidates
                .iter()
                .map(|c| list_head(&c.view))
                .collect::<Vec<_>>()
        );
        assert!(
            forms.scanned_form_count > 0,
            "the denominator still counts every walked node"
        );
    }

    #[test]
    fn compute_evaluated_forms_counts_every_walked_node_not_just_candidates() {
        // `scanned_form_count` is a published output field of the standalone
        // `inspect <rule>` commands and means "nodes walked", which is far
        // more than the number of candidates kept.
        let input = "(defun f (x) (print x) (time x))";
        let root = root(input);
        let forms = compute_evaluated_forms(&root);
        let walked = {
            let mut n = 0;
            walk_evaluated_forms(&root, |_, _| n += 1);
            n
        };
        assert_eq!(forms.scanned_form_count, walked);
        assert!(
            forms.scanned_form_count > forms.candidates.len(),
            "the denominator must not collapse to the candidate count"
        );
    }

    #[test]
    fn compute_evaluated_forms_excludes_quoted_and_binding_positions() {
        // The same false-positive traps `heads_with_safety`'s own tests cover
        // — a quoted shape, and a lambda-list parameter sharing a target
        // head's name — must not appear as candidates at all. Both names here
        // are real candidate heads, so the filter cannot be what hides them.
        let root = root("(defun process (step x) '(step (print)))");
        let forms = compute_evaluated_forms(&root);
        assert!(
            !forms
                .candidates
                .iter()
                .any(|c| list_head(&c.view) == Some("step"))
        );
        assert!(
            !forms
                .candidates
                .iter()
                .any(|c| list_head(&c.view) == Some("print"))
        );
    }

    #[test]
    fn candidate_heads_covers_every_rules_own_table() {
        // The drift guard for `CANDIDATE_HEADS`. A rule that grows a head the
        // union does not cover would silently stop firing in the shared path,
        // with no other test failing — so this asserts the union is a
        // superset of every rule's own table, for every dialect.
        for dialect in Dialect::ALL {
            for head in crate::leftover_print_debug::domain::heads_for(dialect) {
                assert!(
                    is_candidate_head(head),
                    "leftover-print-debug head {head:?} ({dialect:?}) is missing from CANDIDATE_HEADS"
                );
            }
        }
        for head in crate::leftover_trace_call::domain::HEADS {
            assert!(
                is_candidate_head(head),
                "leftover-trace-call head {head:?} is missing from CANDIDATE_HEADS"
            );
        }
        for head in crate::leftover_inspect_call::domain::HEADS {
            assert!(
                is_candidate_head(head),
                "leftover-inspect-call head {head:?} is missing from CANDIDATE_HEADS"
            );
        }
        // The four single-head rules match a literal rather than a table, so
        // there is nothing to import; these mirror the `symbol_is` calls in
        // each rule's `examine`.
        for (head, rule) in [
            ("break", "leftover-break-call"),
            ("format", "leftover-format-debug-marker"),
            ("step", "leftover-step-call"),
            ("time", "leftover-time-benchmark-call"),
        ] {
            assert!(
                is_candidate_head(head),
                "{rule} head {head:?} is missing from CANDIDATE_HEADS"
            );
        }
    }

    #[test]
    fn is_candidate_head_ignores_case_and_package_qualifiers() {
        // The filter must accept everything the rules' own `symbol_is` /
        // `symbol_in` would, or it would drop a real finding before any rule
        // saw it.
        assert!(is_candidate_head("print"));
        assert!(is_candidate_head("PRINT"));
        assert!(is_candidate_head("cl:print"));
        assert!(is_candidate_head("cl::print"));
        assert!(!is_candidate_head("printer"));
        assert!(!is_candidate_head("foo"));
    }

    #[test]
    fn evaluated_candidates_computes_once_and_is_reused_across_calls() {
        use paredit_core_lint_engine::engine::RuleContext;
        use std::path::Path;

        let input = "(defun f (x) (print x) (time x))";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let context = RuleContext::new(Path::new("test.lisp"), Dialect::CommonLisp, &tree, input);
        let root = tree.root_view();

        let first = evaluated_candidates(&context, &root);
        assert!(!first.is_empty(), "the fixture must produce candidates");
        let first_ptr = first.as_ptr();
        let second = evaluated_candidates(&context, &root);
        assert_eq!(
            first.len(),
            second.len(),
            "both calls see the same candidate list"
        );
        assert_eq!(
            first_ptr,
            second.as_ptr(),
            "the second call reused the cached allocation rather than recomputing"
        );
    }
}
