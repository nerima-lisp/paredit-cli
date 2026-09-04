//! What the contract-annotation rules share: which parts of a file are *code*
//! rather than data, how to reach the top-level form enclosing a node without
//! materializing the document, and the small syntactic queries every rule here
//! repeats.
//!
//! # Evaluation context
//!
//! The quote machinery below (`QuoteState` — private — plus
//! [`for_each_evaluated_subview`] and [`is_unevaluated_at`]) is a deliberate
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
//! The three dialects this package models spell the unquote two different ways.
//! Racket and Common Lisp use `,`; Clojure uses `~`, and treats `,` as plain
//! whitespace the reader drops — so `` `(a ,x) `` is `` `(a x) ``, all data.
//! The same two counters give the right answer for all three because they are
//! driven by [`ReaderPrefix`], which the dialect-aware parser has already
//! resolved.
//!
//! # Cost
//!
//! Nothing here is called per visited node. Every rule in this package is
//! `HeadFilter::Heads`, and each one calls into this module only *after* its
//! head has matched and it already has a candidate — the `clean/forms/*`
//! benchmarks lint files with zero findings, so the per-file cost of a rule
//! that matches nothing is exactly what they measure.
//!
//! Nor is anything here linear in the file. The span-directed lookups are
//! called once per candidate, so none of them may cost the document:
//!
//! - [`is_unevaluated_at`] and [`preceding_top_level_form`] binary-search the
//!   top level for the one root child involved and materialize only that form.
//!   Starting them from `tree.root_view()` instead — which builds an
//!   `ExpressionView` for every node in the document — is what made a file of T
//!   candidates cost T×T, and no `--rule` or `--exclude` could avoid it, since
//!   `inspect lint` runs every rule and filters afterwards.
//! - [`with_leading_declarations`] does not materialize *anything* on the way
//!   down. It walks by [`Path`], reading only spans and head symbols, because
//!   even the enclosing top-level form is too much to pay per candidate when
//!   thousands of candidates share one: materializing it made a single function
//!   with 2000 `check-type` calls take 4.6 seconds inside one rule, against 47
//!   milliseconds for the path walk.

use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, Path, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{
    atom_text, is_paren_list, list_head, symbol_is, unqualified,
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
    /// as [`ReaderPrefix::Function`], and is likewise neutral — `@x` is code;
    /// and that Clojure's `#(…)` anonymous-function literal reads as
    /// [`ReaderPrefix::HashLiteral`], which is code too.
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

#[must_use]
pub const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start().get() <= inner.start().get() && inner.end().get() <= outer.end().get()
}

/// Calls `visit` on every node of `root` that is reachable as evaluated code,
/// in the same pre-order the lint engine's own walk produces.
///
/// Quoted subtrees are still *descended* — `` `(a ,(f)) `` has code inside data
/// — but their data nodes are never visited.
///
/// Iterative rather than recursive: a deeply nested document must not depend on
/// stack depth.
pub fn for_each_evaluated_subview(root: &ExpressionView, mut visit: impl FnMut(&ExpressionView)) {
    let mut stack = vec![(root, QuoteState::EVALUATED)];
    while let Some((view, outer)) = stack.pop() {
        let state = outer.after_prefixes(view);
        if !state.is_data() {
            visit(view);
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

/// The index of the top-level form containing `target`.
///
/// The reason this is not `tree.root_view()` followed by a search: `root_view`
/// builds an `ExpressionView` — a `Vec` of children and a `Vec` of reader
/// prefixes — for *every node in the file*, so asking it about one node costs
/// the whole document. The span-directed lookups here are called once per
/// candidate, which made a file of T candidates cost T×T.
///
/// Costs a binary search over the top level — each step a slice index and a
/// span read, neither of which allocates — and materializes nothing at all.
///
/// [`SyntaxTree::root_child_span`] rather than
/// `select_path(&Path::root_child(i))?.span()`, which reads the same and is
/// not: `Path::root_child` builds an owned `Vec<ChildIndex>`, so that spelling
/// costs `log2(forms)` heap allocations per call.
fn root_child_index_containing(tree: &SyntaxTree, target: ByteSpan) -> Option<usize> {
    // Top-level forms are in document order and do not overlap, so the only
    // candidate is the last one beginning at or before `target`.
    let mut low = 0;
    let mut high = tree.root_children().len();
    while low < high {
        let middle = low + (high - low) / 2;
        if tree.root_child_span(middle)?.start().get() <= target.start().get() {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    let index = low.checked_sub(1)?;
    span_contains(tree.root_child_span(index)?, target).then_some(index)
}

/// The top-level form containing `target`, materialized on its own.
fn root_child_containing(tree: &SyntaxTree, target: ByteSpan) -> Option<ExpressionView> {
    let index = root_child_index_containing(tree, target)?;
    Some(tree.select_path(&Path::root_child(index)).ok()?.view())
}

/// The top-level form immediately *before* `target`, when `target` is itself a
/// top-level form.
///
/// `typed-racket-arity-mismatch` needs it: Typed Racket's `(: name Type)`
/// annotation is a separate top-level form from the `define` it describes, and
/// the convention — the one every example in the Typed Racket reference follows
/// — is that it sits directly above it.
///
/// `None` unless `target` is *exactly* a root child. A `define` nested in a
/// `module`, a `let`, or a `parameterize` has no top-level predecessor of its
/// own, and answering with the form before its enclosing top-level one would
/// pair an annotation with a definition it does not describe. That is a false
/// negative for nested definitions, which is the direction this package errs
/// in throughout.
///
/// Deliberately *only* the immediately preceding form, never a scan. An
/// annotation written further away is another false negative; a scan over every
/// top-level form, run once per `define`, is the O(N²) shape that made two
/// shipped rules 98% of a lint run at 480 definitions.
///
/// Comments are trivia rather than nodes, so a `;;`-banner between the two does
/// not break the adjacency.
#[must_use]
pub fn preceding_top_level_form(tree: &SyntaxTree, target: ByteSpan) -> Option<ExpressionView> {
    top_level_form(tree, preceding_top_level_index(tree, target)?)
}

/// The root-child index before the one whose span is exactly `target`.
///
/// Public so a caller that wants both the predecessor's *span* and, only
/// sometimes, its materialized form pays for the binary search once instead of
/// twice. `typed-racket-arity-mismatch` does exactly that: it slices the source
/// at the span to ask "does the form above even open with a `:` head?", and
/// materializes only on a hit.
#[must_use]
pub fn preceding_top_level_index(tree: &SyntaxTree, target: ByteSpan) -> Option<usize> {
    let index = root_child_index_containing(tree, target)?;
    if tree.root_child_span(index)? != target {
        return None;
    }
    index.checked_sub(1)
}

/// One top-level form's span, without materializing it: a slice index and a
/// span read, no `ExpressionView` and so no allocation.
#[must_use]
pub fn top_level_span(tree: &SyntaxTree, index: usize) -> Option<ByteSpan> {
    tree.root_child_span(index)
}

/// One top-level form, materialized on its own — that form's subtree, never the
/// document's.
#[must_use]
pub fn top_level_form(tree: &SyntaxTree, index: usize) -> Option<ExpressionView> {
    Some(tree.select_path(&Path::root_child(index)).ok()?.view())
}

/// Whether the node at `target` is unevaluated data rather than code.
///
/// Descends to `target` through the one child at each level whose span contains
/// it, so the cost is the enclosing top-level form's size, and never the
/// file's.
///
/// The verdict is read *at* the target and nowhere shallower. An ancestor being
/// data does not settle it: `` `(a ,(check-type x integer)) `` has a
/// quasiquoted ancestor and an evaluated target. Being inside a hard `'` does
/// settle it, and that is already modelled by `hard` never clearing.
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

// ---------------------------------------------------------------------------
// Reaching a node's parent without materializing anything
// ---------------------------------------------------------------------------
//
// `RuleContext` carries no parent pointer, so a head-matched node cannot see
// what encloses it. The obvious way to recover that — materialize the enclosing
// top-level form and walk down — costs the *whole form* per candidate, which is
// quadratic in the number of candidates that share one form. A Common Lisp
// function with 2000 `check-type` calls measured at 4.6 seconds inside a single
// rule that way.
//
// So the descent below never builds an `ExpressionView` at all. It walks by
// `ExpressionPath`, reading only `Selection::span` and `Selection::head` — both
// of which are node-id lookups — and materializes exactly the handful of
// `declare` forms it is actually asked about.

/// The span of the node at `path`, without materializing it.
fn span_at(tree: &SyntaxTree, path: &[usize]) -> Option<ByteSpan> {
    tree.select_path(&Path::from_indexes(path.to_vec()))
        .ok()
        .map(|selection| selection.span())
}

/// The span of `path`'s `index`-th child, or `None` if there is no such child.
fn child_span_at(tree: &SyntaxTree, path: &[usize], index: usize) -> Option<ByteSpan> {
    let mut child = Vec::with_capacity(path.len() + 1);
    child.extend_from_slice(path);
    child.push(index);
    span_at(tree, &child)
}

/// How many children the node at `path` has.
///
/// Found by doubling until a probe misses and then bisecting, so a node of `k`
/// children costs `2·log₂ k` span reads rather than `k`. The cap keeps a
/// malformed tree from spinning.
fn child_count_at(tree: &SyntaxTree, path: &[usize]) -> usize {
    const CAP: usize = 1 << 24;
    let mut high = 1;
    while high < CAP && child_span_at(tree, path, high - 1).is_some() {
        high *= 2;
    }
    let mut low = high / 2;
    // `low` children exist, `high` do not.
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        if child_span_at(tree, path, middle - 1).is_some() {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    low
}

/// The index of `path`'s one child whose span contains `target`.
fn child_index_containing(tree: &SyntaxTree, path: &[usize], target: ByteSpan) -> Option<usize> {
    let count = child_count_at(tree, path);
    // Children are in document order and do not overlap, so the only candidate
    // is the last one beginning at or before `target`.
    let mut low = 0;
    let mut high = count;
    while low < high {
        let middle = low + (high - low) / 2;
        if child_span_at(tree, path, middle)?.start().get() <= target.start().get() {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    let index = low.checked_sub(1)?;
    span_contains(child_span_at(tree, path, index)?, target).then_some(index)
}

/// The path indexes of the node whose span is exactly `target`.
///
/// `None` when no node in the tree has that span, which is the honest answer
/// for a span a caller synthesized rather than took from the tree.
fn path_to(tree: &SyntaxTree, target: ByteSpan) -> Option<Vec<usize>> {
    let mut path = vec![root_child_index_containing(tree, target)?];
    loop {
        if span_at(tree, &path)? == target {
            return Some(path);
        }
        path.push(child_index_containing(tree, &path, target)?);
    }
}

/// How many leading forms of a body are examined when looking for declarations.
///
/// Not a heuristic: CLHS requires declarations to appear at the *beginning* of
/// the body they apply to, so a `declare` further in than this is not a
/// declaration at all. The bound is generous — a `defun` spends at most four
/// forms on its head, name, lambda list and docstring before the declarations
/// start — and it is what makes the lookup cost independent of how large the
/// enclosing function is.
const MAX_LEADING_BODY_FORMS: usize = 16;

/// Runs `f` over the `declare` forms at the head of the body that immediately
/// encloses `target`, stopping at the first one for which it answers `Some`.
///
/// Only the *immediate* parent is read. A declaration in an enclosing form —
/// a `declare` on a `defun` with the `check-type` down inside a `when` — is a
/// deliberate false negative: walking the whole ancestor chain per candidate is
/// the shape that makes a rule quadratic inside one large function.
///
/// Costs no `ExpressionView` except for the `declare` forms themselves, and at
/// most [`MAX_LEADING_BODY_FORMS`] of those.
pub fn with_leading_declarations<T>(
    tree: &SyntaxTree,
    target: ByteSpan,
    mut f: impl FnMut(&ExpressionView) -> Option<T>,
) -> Option<T> {
    let mut parent = path_to(tree, target)?;
    // Drop the target's own step. An empty path afterwards means `target` was
    // a top-level form, which has no enclosing body.
    parent.pop();
    if parent.is_empty() {
        return None;
    }

    for index in 0..MAX_LEADING_BODY_FORMS {
        let mut child = Vec::with_capacity(parent.len() + 1);
        child.extend_from_slice(&parent);
        child.push(index);
        let Ok(selection) = tree.select_path(&Path::from_indexes(child)) else {
            break;
        };
        if !selection
            .head()
            .is_some_and(|head| symbol_is(head, "declare"))
        {
            continue;
        }
        if let Some(found) = f(&selection.view()) {
            return Some(found);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Small syntactic queries
// ---------------------------------------------------------------------------

/// Whether `view` is a `[…]` bracket list — Clojure's parameter/condition
/// vector, and Racket's `contract-out` entry and optional-argument spelling.
#[must_use]
pub fn is_bracket_list(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::List
        && view.delimiter == Some(paredit_core_syntax::sexpr::Delimiter::Bracket)
}

/// Whether `view` is a `{…}` brace list — Clojure's map literal, which is how
/// a `:pre`/`:post` condition map is written.
#[must_use]
pub fn is_brace_list(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::List
        && view.delimiter == Some(paredit_core_syntax::sexpr::Delimiter::Brace)
}

/// Whether `view` is a string literal, which the reader keeps as one atom
/// including its quotes. Anything spelled inside one is text, never a form.
#[must_use]
pub fn is_string_literal(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with('"'))
}

/// An atom's text, lowercased and stripped of its package qualifier — the
/// spelling every comparison in this package is written in.
///
/// Case folding is right for Common Lisp, whose reader upcases; it is *not*
/// right for Racket or Clojure, which are case-sensitive. The rules that
/// compare Racket or Clojure names use [`symbol_text`] instead, so that
/// `(: F …)` above `(define (f x) …)` is two different names, as Racket reads
/// it.
#[must_use]
pub fn normalized_symbol(view: &ExpressionView) -> Option<String> {
    atom_text(view).map(|text| unqualified(text).to_ascii_lowercase())
}

/// An atom's text exactly as written, for the case-sensitive dialects.
#[must_use]
pub fn symbol_text(view: &ExpressionView) -> Option<&str> {
    atom_text(view)
}

/// Whether `view` is a `(…)` list whose head names `expected`, case-folded and
/// package-stripped.
#[must_use]
pub fn head_is(view: &ExpressionView, expected: &str) -> bool {
    list_head(view).is_some_and(|head| symbol_is(head, expected))
}

/// Whether `view` is a `(…)` list whose head is exactly `expected`, byte for
/// byte — for Racket and Clojure, whose readers are case-sensitive.
#[must_use]
pub fn head_is_exactly(view: &ExpressionView, expected: &str) -> bool {
    is_paren_list(view)
        && view
            .children
            .first()
            .and_then(atom_text)
            .is_some_and(|head| head == expected)
}

// `is_function_literal` and `is_percent_parameter` used to live here. Both
// existed solely to keep `clojure-pre-referencing-percent` off a `%` that was a
// `#(…)` literal's own parameter, and both went with that rule when it was
// dropped — see this package's README. Nothing else in the package reads `%`,
// so they are not kept "in case": a future rule that needs them can recover
// them from git history along with the tests that pinned their edge cases.

// ---------------------------------------------------------------------------
// Clojure `defn` shape
// ---------------------------------------------------------------------------

/// One arity of a `defn`/`defn-`: its parameter vector, and the condition map
/// that follows it if it has one.
#[derive(Debug, Clone, Copy)]
pub struct ClojureArity<'a> {
    pub params: &'a ExpressionView,
    /// The `{:pre […] :post […]}` map, when one is really there.
    pub conditions: Option<&'a ExpressionView>,
}

/// The `defn` heads this package reads. `fn` is deliberately absent: an
/// anonymous `fn` takes the same condition map, but its parameter vector is
/// optional in a way that makes the "is this brace map a condition map or the
/// body?" question harder, and covering it adds no shape these rules cannot
/// already see on the `defn` that almost always wraps it.
pub const CLOJURE_DEFN_HEADS: [&str; 2] = ["defn", "defn-"];

/// Reads the arities of a Clojure `defn` form.
///
/// Handles both shapes the `fn` macro accepts:
///
/// - single arity — `(defn f docstring? attr-map? [params] conds? body …)`
/// - multi arity — `(defn f docstring? attr-map? ([params] conds? body …) … )`
///
/// # The lone-map trap
///
/// A brace map directly after the parameter vector is a condition map *only if
/// something follows it*. `clojure.core`'s `fn` macro spells this
///
/// ```text
/// conds (when (and (next body) (map? (first body))) (first body))
/// ```
///
/// — so in `(defn f [x] {:pre [(pos? x)]})` the map is the function's **return
/// value**, not a precondition, and the preconditions never run. A rule that
/// missed the `(next body)` guard would report a function that legitimately
/// returns a map. [`ClojureArity::conditions`] is `None` for that shape.
///
/// # A known false negative
///
/// The same macro reads `conds (or conds (meta params))`, so conditions may
/// instead be written as metadata on the parameter vector —
/// `(defn f ^{:pre [(pos? x)]} [x] …)`. That spelling is not read here: the
/// rules in this package would rather miss it than guess at it.
#[must_use]
pub fn clojure_defn_arities(view: &ExpressionView) -> Vec<ClojureArity<'_>> {
    // children[0] is the head and children[1] the name; a docstring and an
    // attribute map may sit between the name and the parameter vector. Both of
    // those are told from a *condition* map by position alone — the attribute
    // map comes before the parameter vector, a condition map after it.
    let mut arities = Vec::new();
    for (index, child) in view.children.iter().enumerate().skip(2) {
        if is_bracket_list(child) {
            // Single arity: everything after the vector is the body.
            arities.push(ClojureArity {
                params: child,
                conditions: leading_condition_map(&view.children[index + 1..]),
            });
            break;
        }
        if let Some(params) = child
            .children
            .first()
            .filter(|first| is_paren_list(child) && is_bracket_list(first))
        {
            arities.push(ClojureArity {
                params,
                conditions: leading_condition_map(&child.children[1..]),
            });
        }
    }
    arities
}

/// The condition map at the head of a body, honouring `fn`'s `(next body)`
/// guard: a lone trailing map is the return value, not a contract.
fn leading_condition_map(body: &[ExpressionView]) -> Option<&ExpressionView> {
    let first = body.first()?;
    (body.len() >= 2 && is_brace_list(first)).then_some(first)
}

/// The value a Clojure map literal associates with the keyword `key`.
///
/// Keys sit at even child indices, so a map whose entries are not pairs — which
/// the reader accepts but Clojure rejects — cannot make a value read as a key.
#[must_use]
pub fn clojure_map_value<'a>(map: &'a ExpressionView, key: &str) -> Option<&'a ExpressionView> {
    (0..map.children.len())
        .step_by(2)
        .find(|index| atom_text(&map.children[*index]) == Some(key))
        .and_then(|index| map.children.get(index + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn parse(source: &str, dialect: Dialect) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(source, dialect).expect("parse")
    }

    fn evaluated_heads_in(source: &str, dialect: Dialect) -> Vec<String> {
        let parsed = parse(source, dialect);
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
        assert!(evaluated_heads("'(check-type (foo))").is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_data_below_its_head() {
        assert_eq!(evaluated_heads("(quote (check-type (foo)))"), vec!["quote"]);
    }

    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(evaluated_heads("'(a ,(check-type (foo)))").is_empty());
    }

    #[test]
    fn an_unquote_inside_a_backquote_is_code_again() {
        assert_eq!(
            evaluated_heads("`(a ,(check-type (foo)))"),
            vec!["check-type", "foo"]
        );
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(evaluated_heads("`(check-type (foo))").is_empty());
    }

    #[test]
    fn a_string_literal_is_one_atom_so_its_contents_are_never_forms() {
        assert_eq!(evaluated_heads("(f \"(check-type (foo))\")"), vec!["f"]);
    }

    /// Racket's unquote is `,`, exactly like Common Lisp's — so the five shapes
    /// transfer unchanged.
    #[test]
    fn racket_unquote_is_a_comma() {
        assert_eq!(
            evaluated_heads_in("`(a ,(define (f x) x))", Dialect::Racket),
            // `(f x)` is the parameter list, and a `(…)` list all the same, so
            // the walk reports its head too.
            vec!["define", "f"]
        );
        assert!(evaluated_heads_in("'(a ,(define (f x) x))", Dialect::Racket).is_empty());
        assert!(evaluated_heads_in("'(define (f x) x)", Dialect::Racket).is_empty());
    }

    /// Clojure spells the unquote `~`, and drops `,` as whitespace. Both halves
    /// matter: the first is why `` `(a ~x) `` reaches code, the second is why
    /// `` `(a ,x) `` does not. Five Clojure quote tests written with `,` would
    /// pass while proving nothing.
    #[test]
    fn clojure_unquote_is_tilde_and_comma_is_whitespace() {
        assert_eq!(
            evaluated_heads_in("`(a ~(defn f [x] x))", Dialect::Clojure),
            vec!["defn"]
        );
        assert!(evaluated_heads_in("`(a ,(defn f [x] x))", Dialect::Clojure).is_empty());
        assert!(evaluated_heads_in("'(defn f [x] x)", Dialect::Clojure).is_empty());
    }

    /// Clojure's `#(…)` reads as a `HashLiteral` prefix, which is *code*, not
    /// data — so the walk descends into one. A reader change that started
    /// treating it as data would silently stop every rule here from seeing a
    /// condition written with an anonymous-function literal in it.
    #[test]
    fn a_clojure_function_literal_is_code() {
        assert_eq!(
            evaluated_heads_in("(f #(pos? %))", Dialect::Clojure),
            vec!["f", "pos?"]
        );
    }

    fn first_head_span(parsed: &SyntaxTree, head: &str) -> ByteSpan {
        let mut span = None;
        paredit_core_syntax::view_query::for_each_subview(&parsed.root_view(), |view| {
            if span.is_none() && list_head(view).is_some_and(|found| found == head) {
                span = Some(view.span);
            }
        });
        span.expect("the head must occur in the source")
    }

    #[test]
    fn a_span_inside_a_quote_reads_as_unevaluated() {
        for source in [
            "'(check-type x integer)",
            "(quote (check-type x integer))",
            "'(a ,(check-type x integer))",
        ] {
            let parsed = parse(source, Dialect::CommonLisp);
            let span = first_head_span(&parsed, "check-type");
            assert!(is_unevaluated_at(&parsed, span), "{source}");
        }
    }

    #[test]
    fn a_span_in_plain_code_or_under_an_unquote_reads_as_evaluated() {
        for source in [
            "(defun f (x) (check-type x integer))",
            "`(a ,(check-type x integer))",
        ] {
            let parsed = parse(source, Dialect::CommonLisp);
            let span = first_head_span(&parsed, "check-type");
            assert!(!is_unevaluated_at(&parsed, span), "{source}");
        }
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
    /// reader prefixes, Clojure metadata siblings, Racket brackets, strings.
    #[test]
    fn the_binary_search_answers_exactly_what_a_linear_scan_would() {
        for (source, dialect) in [
            ("(a (b) (c (d)) e)", Dialect::CommonLisp),
            (
                "'(a ,(b)) `(c ,(d)) #'e #(1 2) (f . g)",
                Dialect::CommonLisp,
            ),
            (
                "(defn f ^:private [x] {:pre [(pos? x)]} #(inc %))",
                Dialect::Clojure,
            ),
            (
                "(: f (-> Integer Boolean)) (define (f [x 1] #:kw k) #t)",
                Dialect::Racket,
            ),
            (
                "(f \"a string ( with parens\" #\\( :key 1/2 -3.5)",
                Dialect::CommonLisp,
            ),
        ] {
            let parsed = parse(source, dialect);
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

    #[test]
    fn the_preceding_top_level_form_is_the_one_directly_above() {
        let parsed = parse(
            "(: f (-> Integer Boolean))\n(define (f x) #t)\n",
            Dialect::Racket,
        );
        let span = parsed.root_view().children[1].span;
        let previous = preceding_top_level_form(&parsed, span).expect("a preceding form");
        assert_eq!(previous.children[1].text.as_deref(), Some("f"));
    }

    /// Comments are trivia, not nodes, so a banner between the annotation and
    /// its definition does not break the adjacency the rule relies on.
    #[test]
    fn a_comment_between_two_forms_does_not_break_adjacency() {
        let parsed = parse(
            "(: f (-> Integer Boolean))\n;; explains f\n(define (f x) #t)\n",
            Dialect::Racket,
        );
        let span = parsed.root_view().children[1].span;
        let previous = preceding_top_level_form(&parsed, span).expect("a preceding form");
        assert!(head_is_exactly(&previous, ":"));
    }

    #[test]
    fn the_first_top_level_form_has_no_predecessor() {
        let parsed = parse("(define (f x) #t)\n", Dialect::Racket);
        let span = parsed.root_view().children[0].span;
        assert!(preceding_top_level_form(&parsed, span).is_none());
    }

    /// A nested `define` has no top-level predecessor of its own. Answering
    /// with the form before its *enclosing* top-level one would pair an
    /// annotation with a definition it does not describe.
    #[test]
    fn a_nested_form_has_no_top_level_predecessor() {
        let parsed = parse(
            "(: f (-> Integer Boolean))\n(module m racket (define (f x) #t))\n",
            Dialect::Racket,
        );
        let module = &parsed.root_view().children[1];
        let nested_define = &module.children[3];
        assert!(head_is_exactly(nested_define, "define"));
        assert!(preceding_top_level_form(&parsed, nested_define.span).is_none());
        // The enclosing top-level form itself still has one.
        assert!(preceding_top_level_form(&parsed, module.span).is_some());
    }

    /// The path descent must agree with the materialized walk about where a
    /// node sits. Checked against `child_containing`, the linear-scan-verified
    /// oracle above, at every node of several shapes.
    #[test]
    fn the_path_descent_finds_the_same_node_the_view_walk_would() {
        for (source, dialect) in [
            (
                "(defun f (x) (let ((y 1)) (check-type x integer)))",
                Dialect::CommonLisp,
            ),
            ("(a (b) (c (d)) e)", Dialect::CommonLisp),
            ("'(a ,(b)) `(c ,(d)) #'e (f . g)", Dialect::CommonLisp),
            ("(defn f [x] {:pre [(pos? x)]} #(inc %))", Dialect::Clojure),
        ] {
            let parsed = parse(source, dialect);
            let root = parsed.root_view();
            // The virtual document root is deliberately excluded: it is not a
            // node any path names, and `path_to` says so.
            let mut targets = Vec::new();
            for child in &root.children {
                paredit_core_syntax::view_query::for_each_subview(child, |view| {
                    targets.push(view.span);
                });
            }
            assert!(targets.len() > 1, "{source} must parse into several nodes");
            for target in targets {
                if root.children.iter().any(|child| child.span == target) {
                    // A top-level form: `path_to` gives it a one-step path.
                    assert_eq!(
                        path_to(&parsed, target).map(|path| path.len()),
                        Some(1),
                        "{source} at {target:?}"
                    );
                    continue;
                }
                let path = path_to(&parsed, target).expect("every real node has a path");
                assert_eq!(
                    span_at(&parsed, &path),
                    Some(target),
                    "{source} at {target:?}"
                );
            }
        }
    }

    #[test]
    fn a_span_that_names_no_node_has_no_path() {
        let parsed = parse("(check-type x integer)", Dialect::CommonLisp);
        let span = ByteSpan::new(
            paredit_core_syntax::sexpr::ByteOffset::new(1),
            paredit_core_syntax::sexpr::ByteOffset::new(5),
        );
        assert!(path_to(&parsed, span).is_none());
        assert!(with_leading_declarations(&parsed, span, |_| Some(())).is_none());
    }

    #[test]
    fn the_child_count_matches_the_materialized_one() {
        let parsed = parse("(a b (c d e) f)", Dialect::CommonLisp);
        assert_eq!(child_count_at(&parsed, &[0]), 4);
        assert_eq!(child_count_at(&parsed, &[0, 2]), 3);
        // An atom has no children.
        assert_eq!(child_count_at(&parsed, &[0, 1]), 0);
    }

    #[test]
    fn leading_declarations_are_read_from_the_immediate_parent_only() {
        let parsed = parse(
            "(defun f (x) (declare (type integer x)) (check-type x integer))",
            Dialect::CommonLisp,
        );
        let span = first_head_span(&parsed, "check-type");
        let found = with_leading_declarations(&parsed, span, |declaration| {
            Some(list_head(declaration)?.to_owned())
        });
        assert_eq!(found.as_deref(), Some("declare"));

        // The `check-type` is one level deeper here, so the `defun`'s own
        // declaration is not its parent's.
        let parsed = parse(
            "(defun f (x) (declare (type integer x)) (when t (check-type x integer)))",
            Dialect::CommonLisp,
        );
        let span = first_head_span(&parsed, "check-type");
        assert!(with_leading_declarations(&parsed, span, |_| Some(())).is_none());
    }

    /// A top-level form has no enclosing body, so it has no leading
    /// declarations to read.
    #[test]
    fn a_top_level_form_has_no_enclosing_declarations() {
        let parsed = parse("(check-type x integer)", Dialect::CommonLisp);
        let span = parsed.root_view().children[0].span;
        assert!(with_leading_declarations(&parsed, span, |_| Some(())).is_none());
    }

    /// CLHS puts declarations at the head of a body, so the lookup stops after
    /// a fixed number of forms — which is what makes its cost independent of
    /// how large the enclosing function is.
    #[test]
    fn a_declaration_past_the_leading_forms_is_not_read() {
        let filler: String = (0..MAX_LEADING_BODY_FORMS + 4)
            .map(|index| format!("(step{index})"))
            .collect::<Vec<_>>()
            .join(" ");
        let parsed = parse(
            &format!("(defun f (x) {filler} (declare (type integer x)) (check-type x integer))"),
            Dialect::CommonLisp,
        );
        let span = first_head_span(&parsed, "check-type");
        assert!(with_leading_declarations(&parsed, span, |_| Some(())).is_none());
    }

    /// The cost regression this module's descent exists to avoid:
    /// the span lookups are called once per candidate, and reading
    /// `tree.root_view()` to start the descent made a file of T candidates cost
    /// T×T.
    ///
    /// The budget is an absolute one, not a ratio, and that is deliberate: this
    /// batch removed several wall-clock *ratio* assertions after one failed CI,
    /// because the ratio of two short durations has no safe threshold. An
    /// absolute bound does, when it is measured. On the equivalent fixture the
    /// descent takes ~21 ms in the `test` profile and the `root_view()` shape
    /// projects to ~34 s, so 10 s sits ~485× above the real cost and ~3.4×
    /// below the regression. Re-measure rather than adjust the constant if the
    /// fixture shrinks or these tests start running in `--release`.
    #[test]
    fn resolving_a_span_does_not_scan_the_top_level() {
        let source: String = (0..4000)
            .map(|index| format!("(defun f{index} (x) (check-type x integer))\n"))
            .collect();
        let parsed = parse(&source, Dialect::CommonLisp);
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
            let _ = preceding_top_level_form(&parsed, span);
            let _ = path_to(&parsed, span);
        }
        let elapsed = started.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "12000 lookups took {elapsed:?}; the descent is scanning the top level again"
        );
    }

    #[test]
    fn bracket_brace_and_string_are_told_apart() {
        let parsed = parse("(defn f [x] {:a \"s\"})", Dialect::Clojure);
        let defn = &parsed.root_view().children[0];
        assert!(is_bracket_list(&defn.children[2]));
        assert!(is_brace_list(&defn.children[3]));
        assert!(is_string_literal(&defn.children[3].children[1]));
        assert!(!is_string_literal(&defn.children[3].children[0]));
    }

    /// Common Lisp folds case; Racket and Clojure do not. A rule that compared
    /// Racket names case-insensitively would call `(: F …)` an annotation for
    /// `(define (f …) …)`, which Racket reads as two different names.
    #[test]
    fn head_matching_is_case_folded_only_where_the_reader_folds() {
        let cl = parse("(CHECK-TYPE x integer)", Dialect::CommonLisp);
        assert!(head_is(&cl.root_view().children[0], "check-type"));

        let racket = parse("(Define (f x) #t)", Dialect::Racket);
        assert!(!head_is_exactly(&racket.root_view().children[0], "define"));
        assert!(head_is_exactly(
            &parse("(define (f x) #t)", Dialect::Racket)
                .root_view()
                .children[0],
            "define"
        ));
    }

    fn defn(source: &str) -> SyntaxTree {
        parse(source, Dialect::Clojure)
    }

    fn arity_shapes(source: &str) -> Vec<(usize, bool)> {
        let parsed = defn(source);
        let form = parsed.root_view().children[0].clone();
        clojure_defn_arities(&form)
            .iter()
            .map(|arity| (arity.params.children.len(), arity.conditions.is_some()))
            .collect()
    }

    #[test]
    fn a_single_arity_defn_reports_one_arity_and_its_condition_map() {
        assert_eq!(
            arity_shapes("(defn f [x] {:pre [(pos? x)]} x)"),
            vec![(1, true)]
        );
        assert_eq!(arity_shapes("(defn f [x y] (+ x y))"), vec![(2, false)]);
    }

    /// A docstring and an attribute map both sit *before* the parameter
    /// vector, so neither can be mistaken for a condition map.
    #[test]
    fn a_docstring_and_an_attribute_map_are_skipped() {
        assert_eq!(
            arity_shapes("(defn f \"doc\" {:added \"1.0\"} [x] {:pre [(pos? x)]} x)"),
            vec![(1, true)]
        );
        assert_eq!(
            arity_shapes("(defn f \"doc\" {:added \"1.0\"} [x] x)"),
            vec![(1, false)]
        );
    }

    /// `clojure.core`'s `fn` takes a brace map as a condition map only when
    /// something follows it. A lone map is the function's return value, and a
    /// rule that read it as a contract would fire on every function that
    /// returns a literal map.
    #[test]
    fn a_lone_trailing_map_is_the_return_value_not_a_condition_map() {
        assert_eq!(
            arity_shapes("(defn f [x] {:pre [(pos? x)]})"),
            vec![(1, false)]
        );
        assert_eq!(arity_shapes("(defn f [x] {:a 1})"), vec![(1, false)]);
        // With a body after it, the very same map *is* a condition map.
        assert_eq!(
            arity_shapes("(defn f [x] {:pre [(pos? x)]} x)"),
            vec![(1, true)]
        );
    }

    #[test]
    fn a_multi_arity_defn_reports_each_arity_separately() {
        assert_eq!(
            arity_shapes("(defn f ([x] {:pre [(pos? x)]} x) ([x y] (+ x y)))"),
            vec![(1, true), (2, false)]
        );
    }

    #[test]
    fn a_map_literal_is_read_by_key() {
        let parsed = defn("(defn f [x] {:pre [(pos? x)] :post [(pos? %)]} x)");
        let form = parsed.root_view().children[0].clone();
        let arities = clojure_defn_arities(&form);
        let conditions = arities[0].conditions.expect("a condition map");
        assert!(clojure_map_value(conditions, ":pre").is_some());
        assert!(clojure_map_value(conditions, ":post").is_some());
        assert!(clojure_map_value(conditions, ":missing").is_none());
        // A value must never read as a key: `:post` sits at an odd index only
        // if the map is malformed.
        assert!(clojure_map_value(conditions, "(pos? x)").is_none());
    }

    #[test]
    fn a_symbol_is_read_both_folded_and_verbatim() {
        let parsed = parse("(f CL:Foo)", Dialect::CommonLisp);
        let argument = &parsed.root_view().children[0].children[1];
        assert_eq!(normalized_symbol(argument).as_deref(), Some("foo"));
        assert_eq!(symbol_text(argument), Some("CL:Foo"));
    }
}
