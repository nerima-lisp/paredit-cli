//! What every rule here shares: which parts of a file are *code*, how to reach
//! a node's ancestors without materializing the document, and how to read a
//! lambda list.
//!
//! # Quote semantics
//!
//! `QuoteState`, [`for_each_evaluated_subview`],
//! [`for_each_evaluated_subview_where`] and [`is_unevaluated_at`] are copied
//! from `paredit-feature-lint-condition-system`'s `support.rs` — tests included
//! — deliberately as a copy rather than as a dependency: a lint feature package
//! depending on another lint feature package would be a new feature→feature
//! edge for a hundred lines of traversal.
//!
//! The two counters are not one depth number. A comma inside `'(…)` is a comma
//! character in a literal list, so `hard` never clears; a comma inside `` `(…) ``
//! escapes back to code, so `quasi` counts up and down. A node one level *inside*
//! a quote carries no reader prefix of its own and is still data, so a node-local
//! `reader_prefixes` check is not enough either.
//!
//! # Cost
//!
//! Nothing here runs per visited node, and nothing here is quadratic in the
//! number of definitions.
//!
//! Every rule in this package declares `HeadFilter::Heads`, so all of the work
//! below is paid only once a definition or dispatch head has already matched —
//! which, in the `clean/forms/*` benchmarks that lint files with no findings, is
//! either never or is followed immediately by a structural check that fails.
//!
//! [`is_unevaluated_at`] and [`descend_to`] both reach a node through
//! [`root_child_containing`], which binary-searches `tree.root_children()` and
//! materializes exactly one top-level form. The alternative,
//! `SyntaxTree::root_view`, builds an `ExpressionView` — a `Vec` of children and
//! a `Vec` of reader prefixes — for *every node in the file*, so asking it about
//! one node costs the whole document, and asking it once per matched definition
//! costs the document squared.

use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path, ReaderPrefix, SyntaxTree};
use paredit_core_syntax::view_query::{
    atom_text, is_paren_list, list_head, symbol_in, unqualified,
};

// ---------------------------------------------------------------------------
// Evaluation context
// ---------------------------------------------------------------------------

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
    /// of them turns code into data.
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
    list_head(view).is_some_and(|head| symbol_in(head, &["quote"]))
}

/// Whether `outer` covers every byte of `inner`. Equal spans contain each
/// other, so a caller that means "strictly inside" compares the spans too.
#[must_use]
pub const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
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
/// answering `false` visits that node and nothing under it. That is how
/// `positional-argument-count-exceeds-readability` stops at a nested definition:
/// the dispatcher will hand that definition to the rule separately, and walking
/// into it here would report every call in it twice.
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

/// Where a node sits: its parent, and its index among that parent's children.
/// `None` for the root of a walk, which has neither.
pub type ParentSlot<'a> = Option<(&'a ExpressionView, usize)>;

/// One queued node of a positioned walk: where it sits, what it is, and the
/// quote state outside it.
type PendingVisit<'a> = (ParentSlot<'a>, &'a ExpressionView, QuoteState);

/// [`for_each_evaluated_subview`], telling `visit` *where* each node sits.
///
/// `visit(parent, node)` receives the node's parent and its index among that
/// parent's children, and returns whether to descend into it. Position is the
/// one thing the plain walk cannot supply, and three separate questions here
/// need it, because a Lisp list means entirely different things in different
/// slots:
///
/// - `(cond (ready 1 2))` — the clause reads exactly like a call to `ready`;
/// - `(let ((f (lambda …))) …)` — child 1 is a *named* binding, not a body;
/// - `(defun f () (lambda …))` — the definition head is a boundary its
///   enclosing chain does not cross.
///
/// A node that is *data* is neither visited nor pruned: its subtree is still
/// descended, because `` `(let ((a ,(lambda () 1)))) `` has code inside it.
pub fn for_each_evaluated_positioned<'a>(
    root: &'a ExpressionView,
    mut visit: impl FnMut(ParentSlot<'a>, &'a ExpressionView) -> bool,
) {
    let mut stack: Vec<PendingVisit<'a>> = Vec::with_capacity(WALK_STACK_HINT);
    stack.push((None, root, QuoteState::EVALUATED));
    while let Some((parent, view, outer)) = stack.pop() {
        let state = outer.after_prefixes(view);
        if !state.is_data() && !visit(parent, view) {
            continue;
        }
        let inside = if is_quote_form(view) {
            state.quoted()
        } else {
            state
        };
        for (index, child) in view.children.iter().enumerate().rev() {
            stack.push((Some((view, index)), child, inside));
        }
    }
}

/// How much room a walk's stack is given up front.
///
/// These walks are started once per matched definition, and the stack held one
/// element to begin with — so an ordinary definition reallocated it three or
/// four times, growing 1 → 2 → 4 → 8 → 16, and every one of those was a heap
/// allocation and a copy charged to that definition. Sized for the widest
/// frontier an ordinary definition reaches; a wider one still grows, once.
const WALK_STACK_HINT: usize = 32;

/// [`for_each_evaluated_positioned`], restricted to nodes that have children.
///
/// For a caller whose finding is always a *call* — a head with arguments — an
/// atom and an empty list are both dead ends: neither has a head to prune on,
/// neither has arguments to count, and neither has children to descend into.
/// Skipping them is therefore invisible to such a caller, and on ordinary code
/// they are most of the nodes there are. `(defun f (a b) "doc" (+ a (* b 2)))`
/// has fourteen nodes and five with children, so this walk does a third of the
/// stack traffic of the full one.
///
/// Quote state is unaffected: a childless node encloses nothing, so never
/// pushing it cannot change the state any other node is visited under.
pub fn for_each_evaluated_branch_positioned<'a>(
    root: &'a ExpressionView,
    mut visit: impl FnMut(ParentSlot<'a>, &'a ExpressionView) -> bool,
) {
    let mut stack: Vec<PendingVisit<'a>> = Vec::with_capacity(WALK_STACK_HINT);
    stack.push((None, root, QuoteState::EVALUATED));
    while let Some((parent, view, outer)) = stack.pop() {
        let state = outer.after_prefixes(view);
        if !state.is_data() && !visit(parent, view) {
            continue;
        }
        let inside = if is_quote_form(view) {
            state.quoted()
        } else {
            state
        };
        for (index, child) in view.children.iter().enumerate().rev() {
            if !child.children.is_empty() {
                stack.push((Some((view, index)), child, inside));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Reaching a node's ancestors without materializing the document
// ---------------------------------------------------------------------------

/// The one child of `view` whose span covers `target`, with its index, found
/// without reading the others.
///
/// A node's children are in document order and do not overlap, so the only
/// child that can contain `target` is the last one beginning at or before it —
/// which a binary search finds in `log₂ k` comparisons instead of `k`.
fn child_containing(view: &ExpressionView, target: ByteSpan) -> Option<(usize, &ExpressionView)> {
    let after = view
        .children
        .partition_point(|child| child.span.start().get() <= target.start().get());
    let index = after.checked_sub(1)?;
    let child = view.children.get(index)?;
    span_contains(child.span, target).then_some((index, child))
}

/// The index of the top-level form containing `target`, read from spans alone.
///
/// No node is materialized: [`SyntaxTree::root_child_span`] is a slice index
/// and a span read, neither of which allocates. Top-level forms are in document
/// order and do not overlap, so the only candidate is the last one beginning at
/// or before `target`.
///
/// The obvious spelling of that read — `select_path(&Path::root_child(i))` —
/// looks equally free and is not: `Path::root_child` builds an owned
/// `Vec<ChildIndex>`, so the search below cost `log2(forms)` heap allocations
/// *per call*. On the `clean/forms/*` benchmarks — a file of ordinary top-level
/// `defun`s, where the only caller is a pre-check that answers "yes, top level"
/// and stops — that was the single largest cost this package added.
fn root_child_index_containing(tree: &SyntaxTree, target: ByteSpan) -> Option<usize> {
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

/// Whether `target` *is* one of the file's top-level forms.
///
/// The cheap pre-check for a rule that anchors on a definition head and only
/// has something to say about a *nested* one: answering costs a binary search
/// over the top level and allocates nothing, so a file of ordinary top-level
/// `defun`s never materializes anything at all.
#[must_use]
pub fn is_top_level(tree: &SyntaxTree, target: ByteSpan) -> bool {
    root_child_index_containing(tree, target)
        .and_then(|index| tree.root_child_span(index))
        .is_some_and(|span| span == target)
}

/// The *span* of the top-level form containing `target`, without materializing
/// anything.
///
/// The pre-check a rule that needs ancestor context can afford on every match:
/// a byte scan of this slice costs a fraction of building the form's
/// `ExpressionView`, so a rule whose finding needs three of some spelling in
/// one form can rule most matches out before allocating. Mirrors the guard
/// `paredit-feature-lint-condition-system` puts in front of its hierarchy walk.
#[must_use]
pub fn root_child_span_containing(tree: &SyntaxTree, target: ByteSpan) -> Option<ByteSpan> {
    let index = root_child_index_containing(tree, target)?;
    tree.root_child_span(index)
}

/// How many times `needle` occurs in `haystack`, ignoring ASCII case.
///
/// Overlapping windows, so it over-counts nothing a reader would call an
/// occurrence and under-counts nothing either. Used only as a *lower bound*
/// guard: a mention inside a string or a comment answers yes, which is the
/// harmless direction — the real analysis then runs and finds nothing.
#[must_use]
pub fn count_occurrences_ignoring_case(haystack: &str, needle: &str) -> usize {
    let needle = needle.as_bytes();
    if needle.is_empty() || haystack.len() < needle.len() {
        return 0;
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .filter(|window| window.eq_ignore_ascii_case(needle))
        .count()
}

/// The top-level form containing `target`, materialized on its own.
///
/// The reason this is not `tree.root_view()` followed by a search: `root_view`
/// builds an `ExpressionView` for *every node in the file*, so asking it about
/// one node costs the whole document, and a rule that asks once per matched
/// definition then costs the document squared. Selecting the one root child
/// instead costs a binary search over the top level plus that one form's own
/// subtree.
#[must_use]
pub fn root_child_containing(tree: &SyntaxTree, target: ByteSpan) -> Option<ExpressionView> {
    let index = root_child_index_containing(tree, target)?;
    Some(tree.select_path(&Path::root_child(index)).ok()?.view())
}

/// One step of a descent: a node, and the index of the child the descent takes
/// next. The final step's index is `None`, because the target has no next.
#[derive(Debug, Clone, Copy)]
pub struct DescentStep<'a> {
    pub view: &'a ExpressionView,
    pub next_child: Option<usize>,
}

/// The chain of nodes from `root` down to the node whose span is `target`,
/// inclusive of both ends.
///
/// Each level costs one binary search, so the whole descent is the target's
/// *depth*, never the file's size. A span that names no node stops at the
/// innermost node containing it, which is the honest answer for a span a caller
/// synthesized rather than took from the tree.
#[must_use]
pub fn descend_to<'a>(root: &'a ExpressionView, target: ByteSpan) -> Vec<DescentStep<'a>> {
    let mut steps = Vec::new();
    let mut view = root;
    loop {
        if view.span == target {
            steps.push(DescentStep {
                view,
                next_child: None,
            });
            return steps;
        }
        let Some((index, child)) = child_containing(view, target) else {
            steps.push(DescentStep {
                view,
                next_child: None,
            });
            return steps;
        };
        steps.push(DescentStep {
            view,
            next_child: Some(index),
        });
        view = child;
    }
}

/// Whether the node at `target` is unevaluated data rather than code.
///
/// Descends to `target` through the one child at each level whose span contains
/// it, so the cost is the enclosing top-level form's size, and never the file's.
///
/// The verdict is read *at* the target and nowhere shallower. An ancestor being
/// data does not settle it: `` `(a ,(lambda (x) x)) `` has a quasiquoted ancestor
/// and an evaluated target. Being inside a hard `'` does settle it, and that is
/// already modelled by `hard` never clearing.
///
/// Every rule here calls this at most once per candidate finding, *after* the
/// structural checks have already passed — never per visited node.
#[must_use]
pub fn is_unevaluated_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    let Some(top_level) = root_child_containing(tree, target) else {
        return false;
    };
    quote_state_along(&descend_to(&top_level, target)).is_data()
}

/// The quote state at the end of a descent, which is what
/// [`is_unevaluated_at`] reads and what the two ancestor-walking rules fold
/// into their own single descent rather than paying for a second one.
fn quote_state_along(steps: &[DescentStep<'_>]) -> QuoteState {
    let Some(first) = steps.first() else {
        return QuoteState::EVALUATED;
    };
    // The root carries no reader prefix and is not a `(quote …)` form, so the
    // state entering the top-level form is whatever that form's own prefixes
    // say.
    let mut state = QuoteState::EVALUATED.after_prefixes(first.view);
    for pair in steps.windows(2) {
        let (parent, child) = (&pair[0], &pair[1]);
        state = state.after_prefixes(child.view);
        if is_quote_form(parent.view) {
            state = state.quoted();
        }
    }
    state
}

/// [`is_unevaluated_at`], answered from a descent the caller already has.
#[must_use]
pub fn descent_is_unevaluated(steps: &[DescentStep<'_>]) -> bool {
    quote_state_along(steps).is_data()
}

// ---------------------------------------------------------------------------
// Atoms
// ---------------------------------------------------------------------------

/// An atom's symbol text, past any reader prefix, lowercased and stripped of
/// its package qualifier — the spelling every comparison here is written in.
#[must_use]
pub fn normalized_symbol(view: &ExpressionView) -> Option<String> {
    atom_symbol_text(view)
        .filter(|text| !text.is_empty())
        .map(|text| unqualified(text).to_ascii_lowercase())
}

/// Whether a node is a `(head …)` call to one of `heads`.
#[must_use]
pub fn calls_any(view: &ExpressionView, heads: &[&str]) -> bool {
    list_head(view).is_some_and(|head| symbol_in(head, heads))
}

/// A string literal, which the reader keeps as one atom including its quotes.
///
/// This is what keeps every rule here out of string contents: `"(lambda (x) x)"`
/// is this atom and has no children, so no walk can reach a form inside it.
#[must_use]
pub fn is_string_literal(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with('"'))
}

/// The contents of a string literal, without its delimiters.
#[must_use]
pub fn string_literal_contents(view: &ExpressionView) -> Option<&str> {
    let text = atom_text(view)?;
    if !view.reader_prefixes.is_empty() {
        return None;
    }
    text.strip_prefix('"')?.strip_suffix('"')
}

/// A `:keyword` atom — the marker that a call is passing keyword arguments
/// rather than a long positional list.
#[must_use]
pub fn is_keyword_atom(view: &ExpressionView) -> bool {
    atom_symbol_text(view).is_some_and(|text| text.starts_with(':') && text.len() > 1)
}

/// A lambda-list keyword (`&optional`, `&rest`, `&key`, `&aux`, …), which ends
/// the required-parameter prefix of a lambda list.
#[must_use]
pub fn is_lambda_list_keyword(view: &ExpressionView) -> bool {
    atom_symbol_text(view).is_some_and(|text| text.starts_with('&'))
}

// ---------------------------------------------------------------------------
// Lambda lists
// ---------------------------------------------------------------------------

/// Where a definition form keeps its lambda list.
///
/// Spelled out per head rather than guessed, because the three shapes genuinely
/// differ: `defun`/`defmacro`/`defgeneric` put it at child 2 (child 1 being the
/// name, which may itself be a `(setf name)` list), while `defmethod` allows any
/// number of qualifiers between the name and the lambda list, so the lambda list
/// is the first `(…)` at or after child 2.
#[must_use]
pub fn definition_lambda_list<'a>(
    view: &'a ExpressionView,
    head: &str,
) -> Option<&'a ExpressionView> {
    if unqualified(head).eq_ignore_ascii_case("defmethod") {
        return view
            .children
            .iter()
            .skip(2)
            .find(|child| is_paren_list(child));
    }
    let index = if unqualified(head).eq_ignore_ascii_case("lambda") {
        1
    } else {
        2
    };
    view.children
        .get(index)
        .filter(|child| is_paren_list(child))
}

/// The required parameters of a lambda list: everything before the first
/// `&`-keyword.
///
/// A `defmethod` specializer (`(x integer)`) and a `defmacro` destructuring
/// pattern (`(a b)`) are each one required parameter, which is what they are to
/// a caller.
#[must_use]
pub fn required_parameters(lambda_list: &ExpressionView) -> Vec<&ExpressionView> {
    lambda_list
        .children
        .iter()
        .take_while(|child| !is_lambda_list_keyword(child))
        .collect()
}

/// How many required parameters `lambda_list` has, without collecting them.
///
/// [`required_parameters`] builds a `Vec` for its caller to own. A caller that
/// only wants the count — `overly-long-parameter-list` asks for it once per
/// definition, and on all but a handful answers "short enough" and stops — was
/// paying a heap allocation per definition to read a number off it.
#[must_use]
pub fn required_parameter_count(lambda_list: &ExpressionView) -> usize {
    lambda_list
        .children
        .iter()
        .take_while(|child| !is_lambda_list_keyword(child))
        .count()
}

/// The *name* a required parameter binds, or `None` for a destructuring pattern
/// this module declines to read.
///
/// `x` is `x`; a `defmethod` specializer `(x integer)` is `x`; a nested
/// destructuring pattern `(a b)` in a `defmacro` is deliberately `None` rather
/// than `a`, because reading it as `a` would claim a binding shape this package
/// does not model.
#[must_use]
pub fn required_parameter_name(parameter: &ExpressionView, specialized: bool) -> Option<String> {
    if let Some(name) = normalized_symbol(parameter) {
        return Some(name);
    }
    if specialized && is_paren_list(parameter) && parameter.children.len() == 2 {
        return parameter.children.first().and_then(normalized_symbol);
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

    fn evaluated_heads(source: &str) -> Vec<String> {
        let parsed = tree(source);
        let mut heads = Vec::new();
        for_each_evaluated_subview(&parsed.root_view(), |view| {
            if let Some(head) = list_head(view) {
                heads.push(head.to_owned());
            }
        });
        heads
    }

    // -- the five quote shapes every rule here depends on --------------------

    #[test]
    fn an_evaluated_walk_visits_plain_code() {
        assert_eq!(evaluated_heads("(a (b) (c (d)))"), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn a_quoted_list_is_data_and_is_not_visited() {
        assert!(evaluated_heads("'(lambda (x) x)").is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_data_below_its_head() {
        assert_eq!(evaluated_heads("(quote (lambda (x) x))"), vec!["quote"]);
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(evaluated_heads("`(lambda (x) x)").is_empty());
    }

    #[test]
    fn an_unquote_inside_a_backquote_is_code_again() {
        // `(x)` is a list whose head atom is `x`, so it appears too: the walk
        // makes no distinction between a call and a lambda list.
        assert_eq!(
            evaluated_heads("`(a ,(lambda (x) (f x)))"),
            vec!["lambda", "x", "f"]
        );
    }

    /// The shape a single `i32` depth counter gets wrong: a comma inside a
    /// hard quote is a comma character in a literal list, not an escape.
    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(evaluated_heads("'(a ,(lambda (x) x))").is_empty());
    }

    /// The shape a node-local `reader_prefixes` check gets wrong: the inner
    /// node carries no prefix of its own, yet is still data.
    #[test]
    fn a_node_one_level_inside_a_quote_is_still_data() {
        assert!(evaluated_heads("'(outer (inner))").is_empty());
    }

    #[test]
    fn a_string_literal_is_one_atom_so_its_contents_are_never_forms() {
        assert_eq!(evaluated_heads("(f \"(lambda (x) x)\")"), vec!["f"]);
    }

    #[test]
    fn a_positioned_walk_reports_each_nodes_parent_and_index() {
        let parsed = tree("(cond (ready 1) (t 2))");
        let root = parsed.root_view();
        let mut seen = Vec::new();
        for_each_evaluated_positioned(&root, |parent, node| {
            seen.push((
                parent.and_then(|(view, index)| list_head(view).map(|head| (head, index))),
                node.span.slice(parsed.source()),
            ));
            true
        });
        assert!(seen.contains(&(Some(("cond", 1)), "(ready 1)")));
        assert!(seen.contains(&(Some(("cond", 2)), "(t 2)")));
        assert!(seen.contains(&(Some(("ready", 0)), "ready")));
    }

    #[test]
    fn a_positioned_walk_stops_where_visit_says_so_but_still_enters_quoted_data() {
        let parsed = tree("(a (stop (b)) `(c ,(d)))");
        let mut heads = Vec::new();
        for_each_evaluated_positioned(&parsed.root_view(), |_, node| {
            if let Some(head) = list_head(node) {
                heads.push(head.to_owned());
                if head == "stop" {
                    return false;
                }
            }
            true
        });
        // `b` is pruned; `d` is reached through the quasiquoted list, whose own
        // nodes are data and are never visited.
        assert_eq!(heads, vec!["a", "stop", "d"]);
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

    // -- the span-directed verdict, which must agree with the walk -----------

    fn data_at_first_head(source: &str, head: &str) -> bool {
        let parsed = tree(source);
        let mut span = None;
        // Deliberately the *unfiltered* walk: the point is to find the node
        // even when it is data.
        paredit_core_syntax::view_query::for_each_subview(&parsed.root_view(), |view| {
            if span.is_none() && list_head(view).is_some_and(|found| found == head) {
                span = Some(view.span);
            }
        });
        is_unevaluated_at(&parsed, span.expect("the head must occur in the source"))
    }

    #[test]
    fn a_span_inside_a_quote_reads_as_data() {
        assert!(data_at_first_head("'(lambda (x) x)", "lambda"));
    }

    #[test]
    fn a_span_inside_a_quote_form_reads_as_data() {
        assert!(data_at_first_head("(quote (lambda (x) x))", "lambda"));
    }

    #[test]
    fn a_span_in_plain_code_reads_as_evaluated() {
        assert!(!data_at_first_head("(defun f () (lambda (x) x))", "lambda"));
    }

    #[test]
    fn a_span_under_an_unquote_reads_as_evaluated() {
        assert!(!data_at_first_head("`(a ,(lambda (x) x))", "lambda"));
    }

    #[test]
    fn a_span_under_a_comma_in_a_hard_quote_reads_as_data() {
        assert!(data_at_first_head("'(a ,(lambda (x) x))", "lambda"));
    }

    #[test]
    fn a_sharp_quote_does_not_turn_code_into_data() {
        assert!(!data_at_first_head("(mapcar #'(lambda (x) x) l)", "lambda"));
    }

    // -- the descent ---------------------------------------------------------

    /// The linear scan `child_containing` replaced, kept as the oracle it is
    /// tested against.
    fn child_containing_linearly(
        view: &ExpressionView,
        target: ByteSpan,
    ) -> Option<(usize, &ExpressionView)> {
        view.children
            .iter()
            .enumerate()
            .find(|(_, child)| span_contains(child.span, target))
    }

    /// The binary search is only correct if a node's children are ordered and
    /// disjoint. Rather than assert that property directly, this asks for the
    /// same answer as the scan at every level of the descent to every node of a
    /// set of sources chosen for the shapes that could break the ordering.
    #[test]
    fn the_binary_search_answers_exactly_what_a_linear_scan_would() {
        for source in [
            "(a (b) (c (d)) e)",
            "'(a ,(b)) `(c ,(d)) #'e #(1 2) (f . g)",
            "(f \"a string ( with parens\" #\\( :key 1/2 -3.5)",
            "(defun f (a b) (flet ((g (a) a)) (g b)))",
        ] {
            let parsed = tree(source);
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
                        child_containing(view, target).map(|(index, child)| (index, child.span)),
                        child_containing_linearly(view, target)
                            .map(|(index, child)| (index, child.span)),
                        "{source} at {target:?}"
                    );
                    let Some((_, child)) = child_containing_linearly(view, target) else {
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
    fn a_descent_lists_every_node_from_the_top_level_form_to_the_target() {
        let parsed = tree("(defun f () (flet ((g (x) x)) (g 1)))");
        let mut lambda_list = None;
        paredit_core_syntax::view_query::for_each_subview(&parsed.root_view(), |view| {
            if lambda_list.is_none() && view.span.slice(parsed.source()) == "(x)" {
                lambda_list = Some(view.span);
            }
        });
        let target = lambda_list.expect("the (x) lambda list");
        let top = root_child_containing(&parsed, target).expect("a top-level form");
        let steps = descend_to(&top, target);
        let heads: Vec<Option<&str>> = steps.iter().map(|step| list_head(step.view)).collect();
        assert_eq!(
            heads,
            vec![
                Some("defun"), // (defun f () (flet …))
                Some("flet"),  // (flet ((g (x) x)) (g 1))
                None,          // ((g (x) x)) — the binding list, whose head is a list
                Some("g"),     // (g (x) x)
                Some("x"),     // (x) — the target, a list whose head atom is `x`
            ]
        );
        // The last step is the target and takes no further child; every other
        // step names the index it descends into.
        assert_eq!(steps.last().expect("a last step").next_child, None);
        assert_eq!(steps[0].next_child, Some(3));
        assert_eq!(steps[1].next_child, Some(1));
        assert_eq!(steps[3].next_child, Some(1));
    }

    #[test]
    fn a_top_level_form_is_recognized_without_materializing_anything() {
        let parsed = tree("(defun a () 1)\n(defun b () (defun c () 2))\n");
        let spans: Vec<ByteSpan> = parsed
            .root_view()
            .children
            .iter()
            .map(|child| child.span)
            .collect();
        assert_eq!(spans.len(), 2);
        assert!(is_top_level(&parsed, spans[0]));
        assert!(is_top_level(&parsed, spans[1]));

        let mut inner = None;
        paredit_core_syntax::view_query::for_each_subview(&parsed.root_view(), |view| {
            let names_c = view.children.get(1).and_then(normalized_symbol).as_deref() == Some("c");
            if inner.is_none() && names_c && list_head(view).is_some_and(|head| head == "defun") {
                inner = Some(view.span);
            }
        });
        assert!(!is_top_level(&parsed, inner.expect("the nested defun (c)")));
    }

    /// The cost regression: resolving a span must not scan the top level, or a
    /// file of T reported definitions costs T×T.
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
            .map(|index| format!("(defun n{index} (a b) (+ a b))\n"))
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
        }
        let elapsed = started.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "4000 lookups took {elapsed:?}; the descent is scanning the top level again"
        );
    }

    // -- lambda lists --------------------------------------------------------

    #[test]
    fn a_lambda_list_is_found_for_each_definition_shape() {
        for (source, head, expected) in [
            ("(defun f (a b c) 1)", "defun", "(a b c)"),
            (
                "(defmacro m (a &body body) 1)",
                "defmacro",
                "(a &body body)",
            ),
            ("(defgeneric g (a b))", "defgeneric", "(a b)"),
            (
                "(defmethod m ((a integer)) 1)",
                "defmethod",
                "((a integer))",
            ),
            (
                "(defmethod m :before ((a integer)) 1)",
                "defmethod",
                "((a integer))",
            ),
            ("(defun (setf f) (v a) 1)", "defun", "(v a)"),
            ("(lambda (a b) 1)", "lambda", "(a b)"),
        ] {
            let parsed = tree(source);
            let root = parsed.root_view();
            let form = &root.children[0];
            let list = definition_lambda_list(form, head)
                .unwrap_or_else(|| panic!("a lambda list in {source}"));
            assert_eq!(list.span.slice(source), expected, "{source}");
        }
    }

    #[test]
    fn required_parameters_stop_at_the_first_lambda_list_keyword() {
        let parsed = tree("(defun f (a b &optional c &key d) 1)");
        let root = parsed.root_view();
        let list = definition_lambda_list(&root.children[0], "defun").expect("a lambda list");
        let required = required_parameters(list);
        assert_eq!(required.len(), 2);
    }

    #[test]
    fn a_specializer_is_one_parameter_named_by_its_first_element() {
        let parsed = tree("(defmethod m ((a integer) (b string)) 1)");
        let root = parsed.root_view();
        let list = definition_lambda_list(&root.children[0], "defmethod").expect("a lambda list");
        let required = required_parameters(list);
        assert_eq!(required.len(), 2);
        assert_eq!(
            required_parameter_name(required[0], true).as_deref(),
            Some("a")
        );
    }

    #[test]
    fn a_destructuring_pattern_is_not_read_as_a_name() {
        let parsed = tree("(defmacro m ((a b) c) 1)");
        let root = parsed.root_view();
        let list = definition_lambda_list(&root.children[0], "defmacro").expect("a lambda list");
        let required = required_parameters(list);
        assert_eq!(required.len(), 2);
        assert_eq!(required_parameter_name(required[0], false), None);
    }
}
