//! Shared helpers for this package's rules.
//!
//! Three things every rule here needs, each of them somewhere this project has
//! already been bitten:
//!
//! - **Brackets are lists.** Racket's own style guide writes `match` clauses,
//!   `cond` clauses and binding lists with `[...]`, and `contract-out` entries
//!   are `[name contract]` pairs. `is_paren_list` answers `false` for every one
//!   of those, so a rule built on it would see none of the idiomatic spelling.
//! - **A vector literal is not a call.** `#(match x)` parses as a `List` with
//!   delimiter `Paren` and a `HashLiteral` reader prefix, so the lint engine's
//!   head index hands it to every rule registered for the head `match`. It is a
//!   self-evaluating constant, not a form.
//! - **Quoting is two counters, not a depth.** `` `(a ,(f)) `` has code inside
//!   data, and `'`, once entered, never clears. The model here is the one in
//!   `packages/feature/lint-condition-system/src/support.rs`; a single `i32`
//!   depth counter is wrong and has shipped as a false-positive source before.
//!
//! The head index ASCII-lowercases every head it stores, but **Racket is case
//! sensitive**. So the index is a pre-filter that over-approximates, and every
//! rule here re-compares the head byte for byte through [`racket_head`].

use paredit_core_syntax::sexpr::{
    ByteSpan, Delimiter, ExpressionKind, ExpressionView, Path as SexprPath, ReaderPrefix,
    SyntaxTree,
};

/// Whether `view` is a list written with `(...)` or `[...]` and is a *form*
/// rather than a `#(...)` vector constant.
#[must_use]
pub fn is_racket_list(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::List
        && matches!(view.delimiter, Some(Delimiter::Paren | Delimiter::Bracket))
        && !view.reader_prefixes.contains(&ReaderPrefix::HashLiteral)
}

/// An atom's text, or `None` for a non-atom.
#[must_use]
pub fn racket_atom(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

/// The head symbol of a Racket form, compared byte for byte by the caller.
///
/// Deliberately not [`paredit_core_syntax::view_query::list_head`], which
/// rejects brackets, and deliberately not compared with
/// [`paredit_core_syntax::view_query::symbol_is`], which folds ASCII case and
/// strips everything before a `:`. Racket is case sensitive, and `:` is an
/// ordinary identifier constituent: `hash:ref` is one name whose tail must not
/// be mistaken for the bare operator.
#[must_use]
pub fn racket_head(view: &ExpressionView) -> Option<&str> {
    if !is_racket_list(view) {
        return None;
    }
    racket_atom(view.children.first()?)
}

/// Whether `view` is a form whose head is exactly `name`.
#[must_use]
pub fn heads_with(view: &ExpressionView, name: &str) -> bool {
    racket_head(view) == Some(name)
}

/// The reader state at a node: inside a hard quote, or how deep in
/// quasiquotation.
///
/// `hard` never clears, because `'` has no escape. `quasi` is a count so that
/// `` ``(a ,,(f)) `` reaches code again at the right level rather than at the
/// first comma.
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
    /// `HashLiteral` is data-entering: a `#(...)` vector constant is
    /// self-evaluating, so its elements are never code. `#'` (syntax) and `#`.`
    /// stay neutral — a `#'(match …)` is a syntax object whose content is still
    /// the program text a reader cares about, and neither turns code into data
    /// in the sense these rules mean.
    fn after_prefixes(mut self, view: &ExpressionView) -> Self {
        for prefix in &view.reader_prefixes {
            match prefix {
                ReaderPrefix::Quote | ReaderPrefix::HashLiteral => self.hard = true,
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

/// The long-hand `(quote …)`/`(quasiquote …)`, which macro output spells out.
fn quoting_form(view: &ExpressionView) -> Option<QuoteKind> {
    match racket_head(view)? {
        "quote" => Some(QuoteKind::Hard),
        "quasiquote" => Some(QuoteKind::Quasi),
        "unquote" | "unquote-splicing" => Some(QuoteKind::Unquote),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy)]
enum QuoteKind {
    Hard,
    Quasi,
    Unquote,
}

const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start().get() <= inner.start().get() && inner.end().get() <= outer.end().get()
}

/// The operators whose operands are macro *templates* rather than code.
///
/// A `syntax-rules` template is written in the surface syntax of the language
/// but is not a program: its pattern variables are filled in by the macro's
/// caller, so nothing about the template's text says what the expansion will
/// contain. A `match` template whose clauses are `(_ pat body ...)` ellipsis
/// forms is the shape that matters here — reading it as code would call a
/// perfectly ordinary macro's template an unreachable clause.
///
/// Suppressing inside a macro definition can only lose findings, never invent
/// them, which is the direction a rule of this kind must err in.
const MACRO_DEFINITION_HEADS: [&str; 10] = [
    "define-syntax",
    "define-syntax-rule",
    "define-syntax-parameter",
    "define-simple-macro",
    "syntax-rules",
    "syntax-case",
    "syntax-parse",
    "let-syntax",
    "letrec-syntax",
    "define-macro",
];

/// What a node's ancestors say about it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodeContext {
    /// The node is unevaluated data rather than code.
    pub unevaluated: bool,
    /// How deep in quasiquotation the node sits, ignoring hard quotes.
    pub quasi_depth: u32,
    /// The head symbol of the form that directly encloses the node, or `None`
    /// when the node is top level or its parent has no symbol head.
    pub parent_head: Option<String>,
    /// Some ancestor of the node is a macro-defining form, so the node is part
    /// of a template rather than of code. See [`MACRO_DEFINITION_HEADS`].
    pub in_macro_template: bool,
}

/// Both ancestor questions this package asks, answered in one descent.
///
/// # Cost
///
/// This is the expensive call in the package and the shape of it is
/// deliberate, because the obvious spelling is quadratic. `SyntaxTree` gives a
/// feature crate no parent link and no borrowed node arena — the only way in is
/// `root_view()`, which *materializes the whole document* as fresh
/// `ExpressionView`s (a `Vec` per node, a `String` per atom). Calling that once
/// per candidate costs O(file) per candidate and so O(file²) per file. A
/// sibling package measured 450843 ns/call against 28 ns/call purely from
/// calling this before its node-local checks instead of after them.
///
/// So the descent is rooted at the enclosing *top-level form*, found by binary
/// search over the root's children through
/// [`SyntaxTree::root_child_span`] — a slice index and a field read, with **no
/// heap allocation at all**. (`Path::root_child` owns a `Vec<ChildIndex>` and
/// allocates once per call, so a `log2(forms)` binary search built on it pays
/// `log2(forms)` allocations to answer one question.) Nothing outside a
/// top-level form can quote it or be its parent, so the answer is identical.
///
/// Callers must still treat this as the last thing they do. Every rule here
/// settles its node-local questions first and asks this only about a node it
/// would otherwise report, so a clean file — the overwhelmingly common case —
/// never pays for it at all.
#[must_use]
pub fn node_context(tree: &SyntaxTree, target: ByteSpan) -> NodeContext {
    let Some(form) = enclosing_top_level_form(tree, target) else {
        return NodeContext::default();
    };
    context_within(&form, target)
}

/// The index of the top-level form whose span contains `target`.
///
/// The root's children are disjoint and in source order, so the first one whose
/// span ends after `target` begins is the only candidate. Allocation-free: the
/// search reads spans only.
#[must_use]
pub fn enclosing_top_level_index(tree: &SyntaxTree, target: ByteSpan) -> Option<usize> {
    let count = tree.root_children().len();
    let (mut low, mut high) = (0, count);
    while low < high {
        let middle = low + (high - low) / 2;
        let span = tree.root_child_span(middle)?;
        if span.end().get() <= target.start().get() {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    if low >= count {
        return None;
    }
    span_contains(tree.root_child_span(low)?, target).then_some(low)
}

/// The top-level form whose span contains `target`, materialized.
fn enclosing_top_level_form(tree: &SyntaxTree, target: ByteSpan) -> Option<ExpressionView> {
    let index = enclosing_top_level_index(tree, target)?;
    Some(tree.select_path(&SexprPath::root_child(index)).ok()?.view())
}

/// Walks from `form` down to `target`, accumulating every answer.
///
/// The verdict is read at the target and nowhere shallower — `` `(a ,(match e))
/// `` has a quasiquoted ancestor and an evaluated target — which is exactly
/// what a single depth counter gets wrong.
fn context_within(form: &ExpressionView, target: ByteSpan) -> NodeContext {
    // The starting form's own prefixes count: `'(a (match x))` is quoted by a
    // prefix on the top-level form itself, which no descent step would see.
    let mut state = QuoteState::EVALUATED.after_prefixes(form);
    let mut parent_head = None;
    let mut in_macro_template = false;
    let mut view = form;

    loop {
        if view.span == target {
            return NodeContext {
                unevaluated: state.is_data(),
                quasi_depth: state.quasi,
                parent_head,
                in_macro_template,
            };
        }
        let quoting = quoting_form(view);
        // A span naming no node is judged by the innermost node containing it,
        // which is the honest answer for a span a caller synthesized.
        let Some((position, child)) = view
            .children
            .iter()
            .enumerate()
            .find(|(_, child)| span_contains(child.span, target))
        else {
            return NodeContext {
                unevaluated: state.is_data(),
                quasi_depth: state.quasi,
                parent_head,
                in_macro_template,
            };
        };
        state = state.after_prefixes(child);
        // The head of `(quote x)` is `quote` itself, which is not data; only
        // the operands are.
        let is_operand = position != 0;
        state = match quoting {
            Some(QuoteKind::Hard) if is_operand => state.quoted(),
            Some(QuoteKind::Quasi) if is_operand => QuoteState {
                quasi: state.quasi + 1,
                ..state
            },
            Some(QuoteKind::Unquote) if is_operand => QuoteState {
                quasi: state.quasi.saturating_sub(1),
                ..state
            },
            _ => state,
        };
        let head = racket_head(view);
        if head.is_some_and(|head| MACRO_DEFINITION_HEADS.contains(&head)) {
            in_macro_template = true;
        }
        parent_head = head.map(str::to_owned);
        view = child;
    }
}

/// Whether the node at `target` is unevaluated data, or part of a macro
/// template, rather than code this package should have an opinion about.
///
/// The two are one question for every rule here, and answering them in one
/// descent is what keeps the cost to a single ancestor walk.
#[must_use]
pub fn is_inert_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    let context = node_context(tree, target);
    context.unevaluated || context.in_macro_template
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn tree(input: &str) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(input, Dialect::Racket).expect("parse")
    }

    /// The span of the first node anywhere in the document whose head is
    /// `head`, in pre-order.
    fn first_form(tree: &SyntaxTree, head: &str) -> ByteSpan {
        fn walk(view: &ExpressionView, head: &str, found: &mut Option<ByteSpan>) {
            if found.is_none() && racket_head(view) == Some(head) {
                *found = Some(view.span);
            }
            for child in &view.children {
                walk(child, head, found);
            }
        }
        let mut found = None;
        walk(&tree.root_view(), head, &mut found);
        found.expect("a form with that head")
    }

    #[test]
    fn a_bracket_list_is_a_racket_list_and_a_vector_is_not() {
        let bracket = tree("[x 1]");
        assert!(is_racket_list(&bracket.root_view().children[0]));

        let vector = tree("#(match x)");
        let node = &vector.root_view().children[0];
        // The engine's own dispatcher would call this a `match` form.
        assert_eq!(node.delimiter, Some(Delimiter::Paren));
        assert!(!is_racket_list(node));
        assert_eq!(racket_head(node), None);
    }

    /// The head index ASCII-lowercases, so it would offer `(MATCH x)` to a rule
    /// registered for `match`. Racket is case sensitive and it is a different
    /// name.
    #[test]
    fn a_head_is_compared_exactly() {
        let upper = tree("(MATCH x)");
        assert_eq!(racket_head(&upper.root_view().children[0]), Some("MATCH"));
        assert!(!heads_with(&upper.root_view().children[0], "match"));

        let qualified = tree("(racket:match x)");
        assert!(!heads_with(&qualified.root_view().children[0], "match"));
    }

    #[test]
    fn a_bare_form_is_evaluated() {
        let tree = tree("(match x)");
        assert!(!is_inert_at(&tree, first_form(&tree, "match")));
    }

    #[test]
    fn a_quote_prefix_makes_the_form_data() {
        let tree = tree("'(match x)");
        assert!(is_inert_at(&tree, first_form(&tree, "match")));
    }

    #[test]
    fn a_nested_form_under_a_quote_prefix_is_data() {
        let tree = tree("'(a (match x))");
        assert!(is_inert_at(&tree, first_form(&tree, "match")));
    }

    #[test]
    fn a_long_hand_quote_form_makes_its_operand_data() {
        let tree = tree("(quote (match x))");
        assert!(is_inert_at(&tree, first_form(&tree, "match")));
    }

    #[test]
    fn an_unquote_inside_a_quasiquote_is_code_again() {
        let tree = tree("`(a ,(match x))");
        assert!(!is_inert_at(&tree, first_form(&tree, "match")));
    }

    #[test]
    fn a_quasiquote_without_an_unquote_is_data() {
        let tree = tree("`(a (match x))");
        assert!(is_inert_at(&tree, first_form(&tree, "match")));
    }

    #[test]
    fn one_unquote_does_not_escape_two_quasiquotes() {
        let tree = tree("``(a ,(match x))");
        assert!(is_inert_at(&tree, first_form(&tree, "match")));
    }

    #[test]
    fn two_unquotes_escape_two_quasiquotes() {
        let tree = tree("``(a ,,(match x))");
        assert!(!is_inert_at(&tree, first_form(&tree, "match")));
    }

    /// `'` has no escape: `(quote (a ,(b)))` is the list containing an
    /// `unquote` form, not a call to `b`.
    #[test]
    fn an_unquote_does_not_escape_a_hard_quote() {
        let tree = tree("'(a ,(match x))");
        assert!(is_inert_at(&tree, first_form(&tree, "match")));
    }

    #[test]
    fn a_form_inside_a_vector_constant_is_data() {
        let tree = tree("#((match x))");
        assert!(is_inert_at(&tree, first_form(&tree, "match")));
    }

    #[test]
    fn the_operator_of_a_quoting_form_is_not_itself_data() {
        let tree = tree("(quote (match x))");
        let form = &tree.root_view().children[0];
        let operator = form.children[0].span;
        let operand = form.children[1].span;
        assert!(!node_context(&tree, operator).unevaluated);
        assert!(node_context(&tree, operand).unevaluated);
    }

    /// Without the containment check the binary search returns the *next* form,
    /// and a gap between two top-level forms would inherit that form's quoting.
    #[test]
    fn a_span_between_two_top_level_forms_is_not_judged_by_its_neighbour() {
        let tree = tree("(a)\n'(b)\n");
        let gap = ByteSpan::new(
            paredit_core_syntax::sexpr::ByteOffset::new(3),
            paredit_core_syntax::sexpr::ByteOffset::new(4),
        );
        assert!(!is_inert_at(&tree, gap));
        assert_eq!(node_context(&tree, gap).parent_head, None);
    }

    #[test]
    fn a_top_level_form_has_no_parent_head() {
        let tree = tree("(match x)");
        assert_eq!(
            node_context(&tree, first_form(&tree, "match")).parent_head,
            None
        );
    }

    #[test]
    fn a_nested_form_reports_its_immediate_enclosing_head() {
        let tree = tree("(define (f) (when p (match x)))");
        assert_eq!(
            node_context(&tree, first_form(&tree, "match"))
                .parent_head
                .as_deref(),
            Some("when")
        );
    }

    #[test]
    fn a_form_outside_any_macro_definition_is_not_a_template() {
        let tree = tree("(define (f x) (match x [_ 1]))");
        assert!(!node_context(&tree, first_form(&tree, "match")).in_macro_template);
    }

    #[test]
    fn a_form_inside_a_syntax_rules_template_is_a_template() {
        let tree = tree(
            "(define-syntax my-match (syntax-rules () ((_ e cl ...) (match e cl ... [_ 'fallback]))))",
        );
        assert!(node_context(&tree, first_form(&tree, "match")).in_macro_template);
    }

    #[test]
    fn every_macro_defining_head_marks_a_template() {
        for head in MACRO_DEFINITION_HEADS {
            let tree = tree(&format!("({head} whatever (match x))"));
            assert!(
                node_context(&tree, first_form(&tree, "match")).in_macro_template,
                "{head} must mark a template"
            );
        }
    }

    #[test]
    fn quasi_depth_counts_only_quasiquotation() {
        let bare = tree("(match x)");
        assert_eq!(
            node_context(&bare, first_form(&bare, "match")).quasi_depth,
            0
        );

        let one = tree("`(a (match x))");
        assert_eq!(node_context(&one, first_form(&one, "match")).quasi_depth, 1);

        let two = tree("``(a (match x))");
        assert_eq!(node_context(&two, first_form(&two, "match")).quasi_depth, 2);

        // A hard quote is data but is not quasiquotation.
        let hard = tree("'(match x)");
        let context = node_context(&hard, first_form(&hard, "match"));
        assert!(context.unevaluated);
        assert_eq!(context.quasi_depth, 0);
    }

    #[test]
    fn the_top_level_search_finds_each_form_and_declines_past_the_end() {
        let tree = tree("(a)\n(b)\n(c)\n");
        for (index, span) in (0..3).filter_map(|i| tree.root_child_span(i).map(|s| (i, s))) {
            assert_eq!(enclosing_top_level_index(&tree, span), Some(index));
        }
        let past = ByteSpan::new(
            paredit_core_syntax::sexpr::ByteOffset::new(99),
            paredit_core_syntax::sexpr::ByteOffset::new(100),
        );
        assert_eq!(enclosing_top_level_index(&tree, past), None);
    }
}
