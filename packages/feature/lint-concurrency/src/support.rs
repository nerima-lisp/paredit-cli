//! What the concurrency rules share: which parts of a file are *code*, where a
//! node sits in the tree, and the threading vocabulary every rule matches on.
//!
//! # Evaluation context
//!
//! The quote machinery below (`QuoteState` — private — plus
//! [`for_each_evaluated_subview`],
//! [`for_each_evaluated_subview_where`], [`is_unevaluated_at`]) is a deliberate
//! copy of `paredit-feature-lint-condition-system`'s `support.rs`, semantics
//! included. It is copied rather than depended on because a feature package
//! must not depend on another feature package, and it is copied *exactly*
//! because the two things a hand-rolled version keeps getting wrong are subtle:
//!
//! - `'` and `` ` `` are not one counter. A comma inside `'(…)` is a comma
//!   character in a literal list — it does not escape back to code — so `hard`
//!   is a latch that never clears, while `quasi` counts up and down. A single
//!   `i32` depth would call `'(a ,x)` code.
//! - The verdict is read *at* the node, not one level up. A node one level
//!   inside a quote is still data even though it carries no reader prefix of
//!   its own, so checking a node's own `reader_prefixes` is not enough.
//!
//! In Clojure the unquote is `~`, and `,` is plain whitespace the reader drops
//! — so `` `(a ,x) `` is `` `(a x) ``, all data. The same two counters give the
//! right answer for both dialects because they are driven by [`ReaderPrefix`],
//! which the dialect-aware parser has already resolved.
//!
//! # Cost
//!
//! Nothing here is called per visited node. Every rule in this package is
//! `HeadFilter::Heads`, and each one calls into this module only *after* its
//! head has matched and it already has a candidate — the `clean/forms/*`
//! benchmarks lint files with zero findings, so the per-file cost of a rule
//! that matches nothing is exactly what they measure.
//!
//! Nor is anything here quadratic in the number of candidates. The two
//! span-directed lookups ([`is_unevaluated_at`], [`with_ancestor_chain`]) are
//! called once per candidate, so each one is allowed to cost the *enclosing
//! top-level form* and never the file: both start by binary-searching the top
//! level for the one root child containing the span, and materialize only that
//! form. Starting them from `tree.root_view()` instead — which builds an
//! `ExpressionView` for every node in the document — is what made a file of T
//! candidates cost T×T, and no `--rule` or `--exclude` could avoid it, since
//! `inspect lint` runs every rule and filters afterwards.
//!
//! # A known gap in head normalization
//!
//! [`paredit_core_syntax::view_query::unqualified`] splits on `:`, which is the
//! Common Lisp package marker. Clojure's `/` namespace separator is *not*
//! stripped, so a fully qualified `clojure.core/swap!` does not normalize to
//! `swap!` and neither the engine's head index nor [`symbol_is`] matches it.
//! That is an engine-wide property, not something this package can fix locally;
//! the Clojure rules here therefore match the bare and `:`-qualified spellings,
//! which is how core functions are written in practice. A missed
//! `clojure.core/`-qualified call is a false negative, which is the direction
//! this package errs in throughout.

use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path, ReaderPrefix, SyntaxTree};
use paredit_core_syntax::view_query::{
    is_paren_list, list_head, symbol_in, symbol_is, unqualified,
};

/// How much of the surrounding reader syntax says "this is data".
///
/// Two independent counters, because `'` and `` ` `` are not the same thing. A
/// comma inside `'(…)` is a comma character in a literal list, so `hard` never
/// clears; a comma inside `` `(…) `` escapes back to code, so `quasi` counts up
/// and down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QuoteState {
    hard: bool,
    quasi: u32,
}

impl QuoteState {
    const EVALUATED: Self = Self {
        hard: false,
        quasi: 0,
    };

    const fn is_data(self) -> bool {
        self.hard || self.quasi > 0
    }

    /// The state inside a node, given the state outside it and the node's own
    /// reader prefixes.
    ///
    /// `#'`, `#.`, `#+`, metadata and the rest are deliberately neutral: none
    /// of them turns code into data. Note that Clojure's `@` deref also reads
    /// as [`ReaderPrefix::Function`], and is likewise neutral — `@x` is code.
    fn after_prefixes(mut self, view: &ExpressionView) -> Self {
        for prefix in &view.reader_prefixes {
            match prefix {
                ReaderPrefix::Quote => self.hard = true,
                ReaderPrefix::Quasiquote => self.quasi += 1,
                ReaderPrefix::Unquote | ReaderPrefix::UnquoteSplicing => {
                    self.quasi = self.quasi.saturating_sub(1);
                }
                _ => {}
            }
        }
        self
    }

    const fn quoted(mut self) -> Self {
        self.hard = true;
        self
    }
}

/// The long-hand `(quote …)`, which the reader also produces for `'…` but which
/// hand-written code and macro output both spell out.
fn is_quote_form(view: &ExpressionView) -> bool {
    list_head(view).is_some_and(|head| symbol_is(head, "quote"))
}

const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start().get() <= inner.start().get() && inner.end().get() <= outer.end().get()
}

/// Calls `visit` on every node of `root` that is reachable as evaluated code,
/// in the same pre-order the lint engine's own walk produces.
///
/// Quoted subtrees are still *descended* — `` `(a ,(f)) `` has code inside data
/// — but their data nodes are never visited.
pub fn for_each_evaluated_subview(root: &ExpressionView, visit: impl FnMut(&ExpressionView)) {
    for_each_evaluated_subview_where(root, |_| true, visit);
}

/// [`for_each_evaluated_subview`], with a say in where the walk stops.
///
/// `descend` is asked about each visited node *before* its children are queued;
/// answering `false` visits that node and nothing under it. Every rule here
/// that must not look through a boundary uses it: `unsynchronized-shared-mutation`
/// stops at a lock scope, and `recursive-lock-reentry-risk` stops at a nested
/// thread spawn, because a lock retaken on another thread is not a reentry.
///
/// A pruned node's subtree is skipped *including* any quoted data in it, which
/// is correct here and would not be if this were used to find data.
pub fn for_each_evaluated_subview_where(
    root: &ExpressionView,
    mut descend: impl FnMut(&ExpressionView) -> bool,
    mut visit: impl FnMut(&ExpressionView),
) {
    let mut stack = vec![(root, QuoteState::EVALUATED)];
    while let Some((view, outer)) = stack.pop() {
        let state = outer.after_prefixes(view);
        if !state.is_data() {
            visit(view);
            if !descend(view) {
                continue;
            }
        }
        let inside = if is_quote_form(view) {
            state.quoted()
        } else {
            state
        };
        for child in view.children.iter().rev() {
            stack.push((child, inside));
        }
    }
}

/// The one child of `view` whose span covers `target`, found without reading
/// the others.
///
/// A node's children are in document order and do not overlap, so the only
/// child that can contain `target` is the last one beginning at or before it —
/// which a binary search finds in `log₂ k` comparisons instead of `k`.
fn child_containing(view: &ExpressionView, target: ByteSpan) -> Option<&ExpressionView> {
    let after = view
        .children
        .partition_point(|child| child.span.start().get() <= target.start().get());
    let child = view.children.get(after.checked_sub(1)?)?;
    span_contains(child.span, target).then_some(child)
}

/// The top-level form containing `target`, materialized on its own.
///
/// The reason this is not `tree.root_view()` followed by a search: `root_view`
/// builds an `ExpressionView` — a `Vec` of children and a `Vec` of reader
/// prefixes — for *every node in the file*, so asking it about one node costs
/// the whole document. [`is_unevaluated_at`] and [`with_ancestor_chain`] are
/// called once per candidate, which made a file of T candidates cost T×T; and
/// no `--rule` or `--exclude` could avoid it, since `inspect lint` runs every
/// rule and filters afterwards.
///
/// Selecting the one root child instead costs a binary search over the top
/// level — each step a node-id lookup and a span read, neither of which
/// allocates — plus that one form's own subtree.
fn root_child_containing(tree: &SyntaxTree, target: ByteSpan) -> Option<ExpressionView> {
    let start_of = |index: usize| {
        tree.select_path(&Path::root_child(index))
            .ok()
            .map(|selection| selection.span().start().get())
    };
    // Top-level forms are in document order and do not overlap, so the only
    // candidate is the last one beginning at or before `target`.
    let mut low = 0;
    let mut high = tree.root_children().len();
    while low < high {
        let middle = low + (high - low) / 2;
        if start_of(middle)? <= target.start().get() {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    let selection = tree
        .select_path(&Path::root_child(low.checked_sub(1)?))
        .ok()?;
    span_contains(selection.span(), target).then(|| selection.view())
}

/// Whether the node at `target` is unevaluated data rather than code.
///
/// Descends to `target` through the one child at each level whose span contains
/// it, so the cost is the enclosing top-level form's size, and never the
/// file's.
///
/// The verdict is read *at* the target and nowhere shallower. An ancestor being
/// data does not settle it: `` `(a ,(make-thread …)) `` has a quasiquoted
/// ancestor and an evaluated target. Being inside a hard `'` does settle it,
/// and that is already modelled by `hard` never clearing.
///
/// The root's own span is never consulted. A file with one top-level form has a
/// root whose span equals that form's, and comparing them would call every such
/// form evaluated before looking at its prefixes at all. A span inside no
/// top-level form at all — one a caller synthesized rather than took from the
/// tree — is evaluated, because nothing quotes it.
#[must_use]
pub fn is_unevaluated_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    let Some(top_level) = root_child_containing(tree, target) else {
        return false;
    };
    let mut view: &ExpressionView = &top_level;
    // The root carries no reader prefix and is not a `(quote …)` form, so the
    // state entering the top-level form is whatever that form's own prefixes
    // say.
    let mut state = QuoteState::EVALUATED.after_prefixes(view);

    while view.span != target {
        let quoting = is_quote_form(view);
        // A span that names no node is judged by the innermost node that
        // contains it, which is the honest answer for a span the caller
        // synthesized rather than took from the tree.
        let Some(child) = child_containing(view, target) else {
            return state.is_data();
        };
        state = state.after_prefixes(child);
        if quoting {
            state = state.quoted();
        }
        view = child;
    }
    state.is_data()
}

/// Runs `f` over the chain of nodes enclosing `target`, outermost first: the
/// top-level form containing it, then down to `target`'s immediate parent.
///
/// `RuleContext` carries no parent pointer, so a head-matched node cannot see
/// what encloses it. Two rules need that — `lock-acquired-not-released` asks
/// whether an `unwind-protect` is above it, and
/// `dynamic-var-bound-across-thread-boundary` asks which specials an enclosing
/// `let` rebinds — and both ask only once, after their head has already matched.
///
/// The chain starts at the top-level form rather than at the virtual document
/// root, because materializing the root is what made this whole-file work (see
/// `root_child_containing`). Nothing is lost: the root is
/// [`ExpressionKind::Root`], so `list_head` of it is `None`, and both consumers
/// read the chain only through `list_head` — a `with-…`/`unwind-protect` test
/// and a `let`/`let*` test, neither of which the root could ever satisfy. A
/// `target` that *is* a top-level form gets the empty chain, where it used to
/// get the root alone; the one caller that reads `chain.last()` then declines,
/// exactly as it did when `last()` was the root and failed its `is_paren_list`
/// test.
///
/// `None` when no node in the tree has exactly `target`'s span, which is the
/// honest answer for a synthesized span: an approximate chain would let a rule
/// draw a conclusion about a node that is not there.
///
/// [`ExpressionKind::Root`]: paredit_core_syntax::sexpr::ExpressionKind::Root
pub fn with_ancestor_chain<T>(
    tree: &SyntaxTree,
    target: ByteSpan,
    f: impl FnOnce(&[&ExpressionView]) -> T,
) -> Option<T> {
    let top_level = root_child_containing(tree, target)?;
    let mut chain: Vec<&ExpressionView> = Vec::new();
    let mut view: &ExpressionView = &top_level;
    while view.span != target {
        chain.push(view);
        view = child_containing(view, target)?;
    }
    Some(f(&chain))
}

/// An atom's symbol text, past any reader prefix, lowercased and stripped of
/// its package qualifier — the spelling every comparison here is written in.
#[must_use]
pub fn normalized_symbol(text: &str) -> String {
    unqualified(text).to_ascii_lowercase()
}

/// The symbol an atom names, in the normalized spelling.
///
/// The reader prefix is dropped before the name is read, so Clojure's `@future`
/// and a bare `future` both come back as `future`. That is what the rules here
/// want: they ask whether a name is *mentioned*, not how it was spelled.
#[must_use]
pub fn symbol_name(view: &ExpressionView) -> Option<String> {
    atom_symbol_text(view)
        .filter(|text| !text.is_empty())
        .map(normalized_symbol)
}

/// Whether `view` is a `(...)` list whose head names one of `heads`.
#[must_use]
pub fn head_is(view: &ExpressionView, heads: &[&str]) -> bool {
    list_head(view).is_some_and(|head| symbol_in(head, heads))
}

/// Whether `view` is a `with-…` form.
///
/// The naming convention means "this macro owns something for the duration of
/// its body and gives it back afterwards", and every project wraps its own lock
/// in one — `(with-registry-lock (push entry *registry*))` expanding to
/// `bt:with-lock-held`. A rule that only knew the library's own spelling would
/// report the wrapper's every use.
///
/// Over-approximates deliberately: `with-open-file` is not a lock, and treating
/// it as one only ever makes a rule quieter.
#[must_use]
pub fn is_with_macro(view: &ExpressionView) -> bool {
    list_head(view).is_some_and(|head| unqualified(head).to_ascii_lowercase().starts_with("with-"))
}

/// Whether `view` establishes mutual exclusion around its body — a known lock
/// scope, or any `with-…` macro that might be a project-local one.
#[must_use]
pub fn is_lock_scope(view: &ExpressionView) -> bool {
    head_is(view, LOCK_SCOPE_HEADS) || is_with_macro(view)
}

// ---------------------------------------------------------------------------
// The threading vocabulary. Every spelling below was checked against the
// library that defines it; see this package's README for the audit.
// ---------------------------------------------------------------------------

/// Forms that start a new thread and run a body on it.
///
/// `make-thread` is spelled the same by `bordeaux-threads` (`bt:make-thread`)
/// and by SBCL (`sb-thread:make-thread`), and the package qualifier is stripped
/// before comparison, so one entry covers both. Clojure's `future` is a macro
/// over a body, not a function over a thunk, which is why the rules that look
/// *inside* a spawn treat the two shapes separately.
pub const THREAD_SPAWN_HEADS: &[&str] = &["make-thread", "future"];

/// Macros that take a lock, run a body, and release it on every exit.
///
/// `with-lock-held` is `bordeaux-threads`; `with-mutex` and
/// `with-recursive-lock` are `sb-thread`; `with-recursive-lock-held` is
/// `bordeaux-threads`'. `with-locked-hash-table` is `sb-ext`'s, and belongs
/// here because it is a lock scope even though what it locks is a table.
/// Clojure's `locking` is a JVM monitor, which is likewise scoped.
pub const LOCK_SCOPE_HEADS: &[&str] = &[
    "with-lock-held",
    "with-recursive-lock-held",
    "with-mutex",
    "with-recursive-lock",
    "with-locked-hash-table",
    "locking",
];

/// The reentrant subset of [`LOCK_SCOPE_HEADS`], which may be taken again by
/// the thread that already holds them.
///
/// Clojure's `locking` is in here because it compiles to a JVM
/// `monitorenter`/`monitorexit` pair, and JVM monitors are reentrant — nesting
/// `(locking o (locking o …))` is safe, and flagging it would be wrong.
pub const REENTRANT_LOCK_SCOPE_HEADS: &[&str] =
    &["with-recursive-lock-held", "with-recursive-lock", "locking"];

/// Taking a lock by hand, with no scope to give it back.
///
/// `acquire-lock` is `bordeaux-threads`; `grab-mutex` is `sb-thread`.
pub const MANUAL_ACQUIRE_HEADS: &[&str] = &["acquire-lock", "grab-mutex"];

/// Giving a hand-taken lock back. `release-lock` is `bordeaux-threads`;
/// `release-mutex` is `sb-thread`.
pub const MANUAL_RELEASE_HEADS: &[&str] = &["release-lock", "release-mutex"];

/// Forms that install an error handler around a body.
///
/// The Common Lisp five plus Clojure's `try`. `unwind-protect` is deliberately
/// absent: it runs cleanup on the way out, but it does not *handle* anything,
/// so an error still leaves the thread.
pub const HANDLER_HEADS: &[&str] = &[
    "handler-case",
    "handler-bind",
    "ignore-errors",
    "restart-case",
    "with-simple-restart",
    "try",
];

/// The Common Lisp operators that write to a place.
///
/// The same list `lint-safety`'s `global-mutation-in-function` uses, so the two
/// rules cannot disagree about what counts as a mutation.
pub const MUTATOR_HEADS: &[&str] = &[
    "setf", "setq", "incf", "decf", "push", "pushnew", "pop", "rotatef", "shiftf", "remf",
];

/// Whether a name follows the `*earmuff*` convention every Common Lisp style
/// guide reserves for a special (dynamically scoped, process-global) variable.
///
/// A convention, not a fact — nothing in the language enforces it. It is the
/// only signal available without a whole-image view of `defvar` forms.
///
/// The length test excludes `*` and `**`, two of the REPL's three value-history
/// variables: they are not names a program shares on purpose, and `*` in
/// particular is a bare multiplication operator's spelling in several dialects.
/// It does **not** exclude `***`, which is three characters and therefore reads
/// as a special — correctly, since `***` *is* a special variable, and one no
/// program should be assigning to on a background thread either. The tests
/// below pin all three answers, because an earlier version of this comment
/// claimed all of `*`/`**`/`***` were excluded and nothing checked the third.
///
/// # Duplicated on purpose, and must not drift
///
/// This is byte-for-byte the same heuristic as
/// `packages/feature/lint-safety/src/global_mutation_in_function.rs`'s
/// `looks_special`, and the two are deciding about the same variables: on
/// `(make-thread (lambda () (setf *counter* …)))` this package's
/// `unsynchronized-shared-mutation` and that package's
/// `global-mutation-in-function` both fire, so a change to one copy's notion of
/// "special-looking" silently makes the pair disagree about one write.
///
/// It is a copy rather than a call because a feature package must not depend on
/// another feature package (§4.2; the dependency would also need an entry in
/// `tests/cli/feature_dependency_contract.rs`). **If you change this, change
/// that one too** — and vice versa; consolidating the two into a core module is
/// filed separately.
#[must_use]
pub fn looks_special(name: &str) -> bool {
    let stripped = unqualified(name);
    stripped.len() > 2 && stripped.starts_with('*') && stripped.ends_with('*')
}

/// The lock a lock-taking form names, when it names one literally.
///
/// `(with-lock-held (*lock*) …)` and `(with-mutex (*m*) …)` put the designator
/// inside a list; `(acquire-lock *lock*)`, `(release-lock *lock*)` and
/// `(locking obj …)` put it directly. `None` for a computed designator —
/// `(with-lock-held ((lock-of x)) …)` names no lock this can compare, and
/// guessing would let two unrelated locks read as the same one.
#[must_use]
pub fn locked_designator(view: &ExpressionView) -> Option<String> {
    let head = list_head(view)?;
    let argument = view.children.get(1)?;
    if symbol_in(head, MANUAL_ACQUIRE_HEADS)
        || symbol_in(head, MANUAL_RELEASE_HEADS)
        || symbol_is(head, "locking")
    {
        return symbol_name(argument);
    }
    if symbol_in(head, LOCK_SCOPE_HEADS) {
        return is_paren_list(argument)
            .then(|| argument.children.first())
            .flatten()
            .and_then(symbol_name);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn tree(source: &str) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse")
    }

    fn evaluated_heads_in(source: &str, dialect: Dialect) -> Vec<String> {
        let parsed = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
        let mut heads = Vec::new();
        for_each_evaluated_subview(&parsed.root_view(), |view| {
            if let Some(head) = list_head(view) {
                heads.push(head.to_owned());
            }
        });
        heads
    }

    fn evaluated_heads(source: &str) -> Vec<String> {
        evaluated_heads_in(source, Dialect::CommonLisp)
    }

    // The five shapes every rule in this package pins for itself. They are
    // pinned here too, on the walk itself, so a rule's own five tests are
    // testing the rule and not re-testing this.

    #[test]
    fn an_evaluated_walk_visits_plain_code() {
        assert_eq!(evaluated_heads("(a (b) (c (d)))"), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn a_quoted_list_is_data_and_is_not_visited() {
        assert!(evaluated_heads("'(make-thread (foo))").is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_data_below_its_head() {
        assert_eq!(
            evaluated_heads("(quote (make-thread (foo)))"),
            vec!["quote"]
        );
    }

    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(evaluated_heads("'(a ,(make-thread (foo)))").is_empty());
    }

    #[test]
    fn an_unquote_inside_a_backquote_is_code_again() {
        assert_eq!(
            evaluated_heads("`(a ,(make-thread (foo)))"),
            vec!["make-thread", "foo"]
        );
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(evaluated_heads("`(make-thread (foo))").is_empty());
    }

    #[test]
    fn a_string_literal_is_one_atom_so_its_contents_are_never_forms() {
        assert_eq!(evaluated_heads("(f \"(make-thread (foo))\")"), vec!["f"]);
    }

    /// Clojure spells the unquote `~`, and drops `,` as whitespace. Both halves
    /// matter: the first is why `` `(a ~x) `` reaches code, the second is why
    /// `` `(a ,x) `` does not.
    #[test]
    fn clojure_unquote_is_tilde_and_comma_is_whitespace() {
        assert_eq!(
            evaluated_heads_in("`(a ~(swap! b (f)))", Dialect::Clojure),
            vec!["swap!", "f"]
        );
        assert!(evaluated_heads_in("`(a ,(swap! b (f)))", Dialect::Clojure).is_empty());
        assert!(evaluated_heads_in("'(swap! b f)", Dialect::Clojure).is_empty());
    }

    #[test]
    fn a_pruned_walk_stops_below_the_node_it_refuses_to_descend() {
        let parsed = tree("(a (stop (b)) (c))");
        let mut heads = Vec::new();
        for_each_evaluated_subview_where(
            &parsed.root_view(),
            |view| list_head(view).is_none_or(|head| head != "stop"),
            |view| {
                if let Some(head) = list_head(view) {
                    heads.push(head.to_owned());
                }
            },
        );
        assert_eq!(heads, vec!["a", "stop", "c"]);
    }

    fn unevaluated_at_first_head(source: &str, head: &str) -> bool {
        let parsed = tree(source);
        let mut span = None;
        paredit_core_syntax::view_query::for_each_subview(&parsed.root_view(), |view| {
            if span.is_none() && list_head(view).is_some_and(|found| found == head) {
                span = Some(view.span);
            }
        });
        is_unevaluated_at(&parsed, span.expect("the head must occur in the source"))
    }

    #[test]
    fn a_span_inside_a_quote_reads_as_unevaluated() {
        assert!(unevaluated_at_first_head("'(make-thread f)", "make-thread"));
        assert!(unevaluated_at_first_head(
            "(quote (make-thread f))",
            "make-thread"
        ));
        assert!(unevaluated_at_first_head(
            "'(a ,(make-thread f))",
            "make-thread"
        ));
    }

    #[test]
    fn a_span_in_plain_code_or_under_an_unquote_reads_as_evaluated() {
        assert!(!unevaluated_at_first_head(
            "(defun f () (make-thread g))",
            "make-thread"
        ));
        assert!(!unevaluated_at_first_head(
            "`(a ,(make-thread f))",
            "make-thread"
        ));
    }

    /// The linear scan `child_containing` replaced, kept as the oracle it is
    /// tested against.
    fn child_containing_linearly(
        view: &ExpressionView,
        target: ByteSpan,
    ) -> Option<&ExpressionView> {
        view.children
            .iter()
            .find(|child| span_contains(child.span, target))
    }

    /// The binary search is only correct if a node's children are ordered and
    /// disjoint. Rather than assert that property directly, this asks for the
    /// same answer as the scan at every level of the descent to every node of a
    /// set of sources chosen for the shapes that could break the ordering —
    /// reader prefixes, Clojure metadata siblings, strings, dotted pairs.
    #[test]
    fn the_binary_search_answers_exactly_what_a_linear_scan_would() {
        for (source, dialect) in [
            ("(a (b) (c (d)) e)", Dialect::CommonLisp),
            (
                "'(a ,(b)) `(c ,(d)) #'e #(1 2) (f . g)",
                Dialect::CommonLisp,
            ),
            (
                "(future ^:focus (swap! a inc)) @(future (locking o 1))",
                Dialect::Clojure,
            ),
            (
                "(f \"a string ( with parens\" #\\( :key 1/2 -3.5)",
                Dialect::CommonLisp,
            ),
            (
                "(bt:make-thread (lambda () (setf *counter* 1)) :name \"w\")",
                Dialect::CommonLisp,
            ),
        ] {
            let parsed = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
            let root = parsed.root_view();
            let mut targets = Vec::new();
            paredit_core_syntax::view_query::for_each_subview(&root, |view| {
                targets.push(view.span);
            });
            assert!(targets.len() > 1, "{source} must parse into several nodes");
            for target in targets {
                let mut view: &ExpressionView = &root;
                loop {
                    assert_eq!(
                        child_containing(view, target).map(|child| child.span),
                        child_containing_linearly(view, target).map(|child| child.span),
                        "{source} at {target:?}"
                    );
                    let Some(child) = child_containing_linearly(view, target) else {
                        break;
                    };
                    if child.span == target {
                        break;
                    }
                    view = child;
                }
            }
        }
    }

    /// The cost regression this module's descent exists to avoid:
    /// `is_unevaluated_at` is called once per candidate, and reading
    /// `tree.root_view()` to start the descent made a file of T candidates cost
    /// T×T — 4000 `make-thread` forms took tens of seconds inside a single rule.
    /// The budget is deliberately hundreds of times the linear cost, so only an
    /// asymptotic regression can trip it.
    #[test]
    fn resolving_a_span_does_not_scan_the_top_level() {
        let source: String = (0..4000)
            .map(|index| format!("(bt:make-thread (lambda () (setf *n{index}* 1)))\n"))
            .collect();
        let parsed = tree(&source);
        let spans: Vec<ByteSpan> = parsed
            .root_view()
            .children
            .iter()
            .map(|child| child.span)
            .collect();
        assert_eq!(spans.len(), 4000);
        let started = std::time::Instant::now();
        for span in spans {
            assert!(!is_unevaluated_at(&parsed, span));
            assert!(with_ancestor_chain(&parsed, span, |chain| chain.len()).is_some());
        }
        let elapsed = started.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "8000 lookups took {elapsed:?}; the descent is scanning the top level again"
        );
    }

    #[test]
    fn an_ancestor_chain_runs_from_the_top_level_form_to_the_targets_parent() {
        let parsed = tree("(defun f () (let ((x 1)) (acquire-lock *l*)))");
        let mut span = None;
        paredit_core_syntax::view_query::for_each_subview(&parsed.root_view(), |view| {
            if list_head(view).is_some_and(|head| head == "acquire-lock") {
                span = Some(view.span);
            }
        });
        let heads = with_ancestor_chain(&parsed, span.expect("span"), |chain| {
            chain
                .iter()
                .map(|view| list_head(view).unwrap_or("<root>").to_owned())
                .collect::<Vec<_>>()
        })
        .expect("chain");
        assert_eq!(heads, vec!["defun", "let"]);
    }

    /// A top-level form has no enclosing *form*, so its chain is empty. It used
    /// to be the virtual document root alone, which carries no head and which
    /// the one caller that reads `chain.last()` rejected anyway — the two
    /// answers are the same answer, and this one costs no whole-file view.
    #[test]
    fn an_ancestor_chain_of_a_top_level_form_is_empty() {
        let parsed = tree("(acquire-lock *l*)");
        let span = parsed.root_view().children[0].span;
        let length = with_ancestor_chain(&parsed, span, |chain| chain.len()).expect("chain");
        assert_eq!(length, 0);
    }

    #[test]
    fn an_ancestor_chain_of_a_span_that_names_no_node_is_none() {
        let parsed = tree("(acquire-lock *l*)");
        let span = ByteSpan::new(
            paredit_core_syntax::sexpr::ByteOffset::new(1),
            paredit_core_syntax::sexpr::ByteOffset::new(5),
        );
        assert!(with_ancestor_chain(&parsed, span, |_| ()).is_none());
    }

    #[test]
    fn earmuffs_name_a_special_and_the_short_repl_history_variables_do_not() {
        assert!(looks_special("*counter*"));
        assert!(looks_special("cl:*print-base*"));
        assert!(!looks_special("*"));
        assert!(!looks_special("**"));
        // `***` is three characters, so the length test lets it through — and
        // that is the wanted answer, not an accident: `***` is a special
        // variable. Pinned because the doc comment used to claim otherwise and
        // no test disagreed. Its siblings `*` and `**` stay excluded above.
        assert!(looks_special("***"));
        assert!(!looks_special("*log"));
        assert!(!looks_special("counter"));
    }

    #[test]
    fn a_lock_designator_is_read_from_both_shapes() {
        let parsed = tree("(bt:with-lock-held (*lock*) (work))");
        assert_eq!(
            locked_designator(&parsed.root_view().children[0]).as_deref(),
            Some("*lock*")
        );
        let parsed = tree("(bt:acquire-lock *lock*)");
        assert_eq!(
            locked_designator(&parsed.root_view().children[0]).as_deref(),
            Some("*lock*")
        );
        // The release side has to read as the same lock, or a rule pairing the
        // two would call every release a release of some other lock.
        let parsed = tree("(bt:release-lock *lock*)");
        assert_eq!(
            locked_designator(&parsed.root_view().children[0]).as_deref(),
            Some("*lock*")
        );
        let parsed = tree("(sb-thread:release-mutex *m*)");
        assert_eq!(
            locked_designator(&parsed.root_view().children[0]).as_deref(),
            Some("*m*")
        );
        let parsed = SyntaxTree::parse_with_dialect("(locking obj (work))", Dialect::Clojure)
            .expect("parse");
        assert_eq!(
            locked_designator(&parsed.root_view().children[0]).as_deref(),
            Some("obj")
        );
    }

    #[test]
    fn a_computed_lock_designator_is_not_guessed_at() {
        let parsed = tree("(with-lock-held ((lock-of x)) (work))");
        assert_eq!(locked_designator(&parsed.root_view().children[0]), None);
        let parsed = tree("(acquire-lock (lock-of x))");
        assert_eq!(locked_designator(&parsed.root_view().children[0]), None);
    }

    #[test]
    fn a_deref_prefix_is_dropped_before_the_name_is_read() {
        let parsed =
            SyntaxTree::parse_with_dialect("(println @result)", Dialect::Clojure).expect("parse");
        let argument = &parsed.root_view().children[0].children[1];
        assert_eq!(symbol_name(argument).as_deref(), Some("result"));
    }
}
