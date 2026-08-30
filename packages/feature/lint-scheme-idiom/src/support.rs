//! Shared helpers for this package's rules.
//!
//! Three things every rule here needs, and each of them is somewhere this
//! project has already been bitten:
//!
//! - **Brackets are lists.** Scheme and Racket write binding lists, `cond`
//!   clauses and `case` clauses with `[...]` as readily as `(...)`, and the
//!   Racket style guide prefers the bracket. `is_paren_list` answers `false`
//!   for one, so a rule built on it would silently see none of the idiomatic
//!   Racket spelling.
//! - **A vector literal is not a call.** `#(begin 1)` parses as a `List` with
//!   delimiter `Paren` and a `HashLiteral` reader prefix, which means the lint
//!   engine's own dispatcher — `list_head` via `is_paren_list` — hands it to
//!   every rule registered for the head `begin`. It is a self-evaluating
//!   vector constant (R7RS 4.1.2), not a form.
//! - **Quoting is two counters, not a depth.** `` `(a ,(f)) `` has code inside
//!   data; `'`, once entered, never clears. The model here is the one in
//!   `packages/feature/lint-condition-system/src/support.rs`; a single `i32`
//!   depth counter is wrong and has shipped as a false-positive source before.

use paredit_core_syntax::sexpr::{
    ByteSpan, Delimiter, ExpressionKind, ExpressionView, Path as SexprPath, ReaderPrefix,
    Selection, SyntaxTree,
};

/// Whether `view` is a list written with `(...)` or `[...]` and is a *form*
/// rather than a `#(...)` vector constant.
#[must_use]
pub fn is_scheme_list(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::List
        && matches!(view.delimiter, Some(Delimiter::Paren | Delimiter::Bracket))
        && !view.reader_prefixes.contains(&ReaderPrefix::HashLiteral)
}

/// An atom's text, or `None` for a non-atom.
#[must_use]
pub fn scheme_atom(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

/// The head symbol of a Scheme form, compared byte for byte by the caller.
///
/// Deliberately not [`paredit_core_syntax::view_query::list_head`], which
/// rejects brackets, and deliberately not compared with
/// [`paredit_core_syntax::view_query::symbol_is`], which folds ASCII case and
/// strips everything before a `:`. Scheme is case sensitive (R7RS 2.1 — and
/// `packages/core/syntax/src/scheme/operator/table.rs` matches exactly for that
/// same reason), and `:` is an ordinary identifier constituent in both
/// languages: Racket's `hash:ref` and SRFI-style `srfi:fold` are single names
/// whose tails must not be mistaken for the bare operator.
#[must_use]
pub fn scheme_head(view: &ExpressionView) -> Option<&str> {
    if !is_scheme_list(view) {
        return None;
    }
    scheme_atom(view.children.first()?)
}

/// Whether `view` is a form whose head is exactly `name`.
#[must_use]
pub fn heads_with(view: &ExpressionView, name: &str) -> bool {
    scheme_head(view) == Some(name)
}

/// Whether the atom at `view` is exactly the symbol `name`.
#[must_use]
pub fn is_symbol(view: &ExpressionView, name: &str) -> bool {
    scheme_atom(view) == Some(name)
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
    /// `HashLiteral` is data-entering here and nowhere else in this workspace:
    /// a Scheme vector constant is self-evaluating (R7RS 4.1.2), so its
    /// elements are never code. `#'` and `#.` stay neutral — neither turns
    /// code into data.
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

/// The long-hand `(quote …)`/`(quasiquote …)`, which hand-written Scheme and
/// macro output both spell out.
fn quoting_form(view: &ExpressionView) -> Option<QuoteKind> {
    match scheme_head(view)? {
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
/// A named `let` in a `syntax-rules` template is the clearest case: in
/// `(syntax-rules () ((_ loop ((pat expr var) ...) () . body) (let loop ((var
/// expr) ...) …)))` both the loop name and the body are pattern variables the
/// macro's caller supplies, so nothing about the template's text says what the
/// expansion will contain. Guile's own `ice-9/match.upstream.scm` and
/// `srfi/srfi-71.scm` are exactly this shape, and a rule reading the template
/// as code calls both of them dead loops.
///
/// Suppressing inside a macro definition can only lose findings, never invent
/// them, which is the direction a rule of this kind must err in.
const MACRO_DEFINITION_HEADS: [&str; 9] = [
    "define-syntax",
    "define-syntax-rule",
    "define-syntax-parameter",
    "syntax-rules",
    "syntax-case",
    "let-syntax",
    "letrec-syntax",
    "define-macro",
    "defmacro",
];

/// What a node's ancestors say about it: whether it is data, and what encloses
/// it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodeContext {
    /// The node is unevaluated data rather than code.
    pub unevaluated: bool,
    /// How deep in quasiquotation the node sits, ignoring hard quotes.
    ///
    /// Zero means "not inside a `` ` `` at all", which is what makes an
    /// `,x` child detectable as a matcher pattern rather than a splice: an
    /// unquote outside any quasiquote is not evaluable Scheme.
    pub quasi_depth: u32,
    /// The head symbol of the form that directly encloses the node, or `None`
    /// when the node is top level or its parent has no symbol head.
    pub parent_head: Option<String>,
    /// Some ancestor of the node is a macro-defining form, so the node is part
    /// of a template rather than of code. See `MACRO_DEFINITION_HEADS`.
    pub in_macro_template: bool,
}

/// Both ancestor questions this package asks, answered in one descent.
///
/// # Cost
///
/// This is the expensive call in the package and the shape of it is
/// deliberate, because the obvious spelling is quadratic. `SyntaxTree` gives a
/// feature crate no parent link and no borrowed node arena — the only way in
/// is `root_view()`, which *materializes the whole document* as fresh
/// `ExpressionView`s (a `Vec` per node, a `String` per atom). Calling that once
/// per candidate costs O(file) per candidate and so O(file²) per file:
/// measured at 359 µs per rule invocation on a 50 KB corpus against 160 ns for
/// the shipped `redundant-progn` control, and *rising* with file size — 786 µs
/// at 100 KB, a 2.19x per-call increase for a 2x file.
///
/// So the descent is rooted at the enclosing *top-level form* instead, found
/// by binary search over the root's children through `select_path`, which
/// reads a span without building any view. Nothing outside a top-level form
/// can quote it or be its parent, so the answer is identical; the cost becomes
/// O(log ⟨top-level forms⟩) plus one form's materialization, which does not
/// grow with the file.
///
/// Callers must still treat it as the last thing they do. Every rule here
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

/// The top-level form whose span contains `target`.
///
/// The root's children are disjoint and in source order, so the first one
/// whose span ends after `target` begins is the only candidate.
fn enclosing_top_level_form(tree: &SyntaxTree, target: ByteSpan) -> Option<ExpressionView> {
    let count = tree.root_children().len();
    let span_of = |index: usize| {
        tree.select_path(&SexprPath::root_child(index))
            .ok()
            .map(Selection::span)
    };

    let (mut low, mut high) = (0, count);
    while low < high {
        let middle = low + (high - low) / 2;
        let span = span_of(middle)?;
        if span.end().get() <= target.start().get() {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    if low >= count {
        return None;
    }
    let selection = tree.select_path(&SexprPath::root_child(low)).ok()?;
    span_contains(selection.span(), target).then(|| selection.view())
}

/// Walks from `form` down to `target`, accumulating both answers.
///
/// The verdict is read at the target and nowhere shallower — `` `(a ,(begin
/// e)) `` has a quasiquoted ancestor and an evaluated target — which is
/// exactly what a single depth counter gets wrong.
fn context_within(form: &ExpressionView, target: ByteSpan) -> NodeContext {
    // The starting form's own prefixes count: `'(a (begin x))` is quoted by a
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
        // the operands are. Skipping position 0 keeps `(quote …)` from
        // declaring its own operator quoted.
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
        let head = scheme_head(view);
        if head.is_some_and(|head| MACRO_DEFINITION_HEADS.contains(&head)) {
            in_macro_template = true;
        }
        parent_head = head.map(str::to_owned);
        view = child;
    }
}

/// Whether the node at `target` is unevaluated data rather than code.
#[must_use]
pub fn is_unevaluated_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    node_context(tree, target).unevaluated
}

/// The head symbol of the form that directly encloses the node at `target`.
///
/// `RuleContext` carries no parent link, so a `Heads`-filtered rule that needs
/// to know what it is nested inside has to look — and one of them does: R7RS
/// 5.6.1 makes `begin` a *library declaration* inside `define-library`, where
/// the ordinary unwrapping is not merely unnecessary but produces a form that
/// is not a legal declaration at all.
#[must_use]
pub fn parent_head_of(tree: &SyntaxTree, target: ByteSpan) -> Option<String> {
    node_context(tree, target).parent_head
}

/// Which of the two types R7RS leaves `eq?` unspecified against was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralKind {
    Number,
    Character,
}

impl LiteralKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Number => "number",
            Self::Character => "character",
        }
    }
}

/// The radix and exactness prefixes a Scheme numeric literal may open with
/// (R7RS 7.1.1 ⟨prefix R⟩). Matched case-insensitively on the letter, since
/// `#X10` and `#x10` denote the same number.
const NUMBER_PREFIX_LETTERS: [char; 6] = ['b', 'o', 'd', 'x', 'e', 'i'];

/// The four infinity and NaN spellings, whose second character is a letter and
/// so escapes the sign-then-digit rule.
const NON_FINITE: [&str; 4] = ["+inf.0", "-inf.0", "+nan.0", "-nan.0"];

/// Classifies an atom's text as a number or character literal, or neither.
///
/// Deliberately not a full R7RS 7.1.1 number grammar: this decides whether a
/// token is *reaching for* a number, and a token that opens like one but does
/// not parse is a reader error rather than something a rule should have an
/// opinion about. What it must never do is accept `#t`, `#f` or a symbol, none
/// of which R7RS leaves unspecified.
#[must_use]
pub fn literal_kind(text: &str) -> Option<LiteralKind> {
    if text.starts_with("#\\") {
        return Some(LiteralKind::Character);
    }
    if NON_FINITE.contains(&text) {
        return Some(LiteralKind::Number);
    }
    if let Some(rest) = text.strip_prefix('#') {
        let number = rest
            .chars()
            .next()
            .is_some_and(|letter| NUMBER_PREFIX_LETTERS.contains(&letter.to_ascii_lowercase()));
        return number.then_some(LiteralKind::Number);
    }
    // A sign is optional and says nothing by itself: `-` and `->list` are
    // identifiers, `-5` and `-.5` are numbers.
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    let mut characters = unsigned.chars();
    let number = match characters.next() {
        Some('.') => characters.next().is_some_and(|next| next.is_ascii_digit()),
        Some(first) => first.is_ascii_digit(),
        None => false,
    };
    number.then_some(LiteralKind::Number)
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn tree(input: &str) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(input, Dialect::Scheme).expect("parse")
    }

    /// The span of the first node anywhere in the document whose head is
    /// `head`, in pre-order.
    fn first_form(tree: &SyntaxTree, head: &str) -> ByteSpan {
        fn walk(view: &ExpressionView, head: &str, found: &mut Option<ByteSpan>) {
            if found.is_none() && scheme_head(view) == Some(head) {
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
    fn a_bracket_list_is_a_scheme_list_and_a_vector_is_not() {
        let bracket = tree("[x 1]");
        assert!(is_scheme_list(&bracket.root_view().children[0]));

        let vector = tree("#(begin 1)");
        let node = &vector.root_view().children[0];
        // The engine's own dispatcher would call this a `begin` form.
        assert_eq!(node.delimiter, Some(Delimiter::Paren));
        assert!(!is_scheme_list(node));
        assert_eq!(scheme_head(node), None);
    }

    #[test]
    fn a_head_is_compared_exactly() {
        let upper = tree("(SET! x 1)");
        assert_eq!(scheme_head(&upper.root_view().children[0]), Some("SET!"));
        assert!(!heads_with(&upper.root_view().children[0], "set!"));

        let qualified = tree("(srfi:begin x)");
        assert!(!heads_with(&qualified.root_view().children[0], "begin"));
    }

    #[test]
    fn a_bare_form_is_evaluated() {
        let tree = tree("(begin x)");
        assert!(!is_unevaluated_at(&tree, first_form(&tree, "begin")));
    }

    #[test]
    fn a_quote_prefix_makes_the_form_data() {
        let tree = tree("'(begin x)");
        assert!(is_unevaluated_at(&tree, first_form(&tree, "begin")));
    }

    #[test]
    fn a_nested_form_under_a_quote_prefix_is_data() {
        let tree = tree("'(a (begin x))");
        assert!(is_unevaluated_at(&tree, first_form(&tree, "begin")));
    }

    #[test]
    fn a_long_hand_quote_form_makes_its_operand_data() {
        let tree = tree("(quote (begin x))");
        assert!(is_unevaluated_at(&tree, first_form(&tree, "begin")));
    }

    #[test]
    fn an_unquote_inside_a_quasiquote_is_code_again() {
        let tree = tree("`(a ,(begin x))");
        assert!(!is_unevaluated_at(&tree, first_form(&tree, "begin")));
    }

    #[test]
    fn a_quasiquote_without_an_unquote_is_data() {
        let tree = tree("`(a (begin x))");
        assert!(is_unevaluated_at(&tree, first_form(&tree, "begin")));
    }

    #[test]
    fn one_unquote_does_not_escape_two_quasiquotes() {
        let tree = tree("``(a ,(begin x))");
        assert!(is_unevaluated_at(&tree, first_form(&tree, "begin")));
    }

    #[test]
    fn two_unquotes_escape_two_quasiquotes() {
        let tree = tree("``(a ,,(begin x))");
        assert!(!is_unevaluated_at(&tree, first_form(&tree, "begin")));
    }

    #[test]
    fn an_unquote_does_not_escape_a_hard_quote() {
        // `'` has no escape: R7RS `(quote (a ,(b)))` is the list containing an
        // `unquote` form, not a call to `b`.
        let tree = tree("'(a ,(begin x))");
        assert!(is_unevaluated_at(&tree, first_form(&tree, "begin")));
    }

    #[test]
    fn a_form_inside_a_vector_constant_is_data() {
        let tree = tree("#((begin x))");
        assert!(is_unevaluated_at(&tree, first_form(&tree, "begin")));
    }

    #[test]
    fn a_long_hand_quasiquote_and_unquote_pair_reaches_code() {
        let tree = tree("(quasiquote (a (unquote (begin x))))");
        assert!(!is_unevaluated_at(&tree, first_form(&tree, "begin")));
    }

    /// In `(quote x)` the symbol `quote` is the operator, not the datum. Only
    /// the operands become data, and a descent that quoted position 0 as well
    /// would call the operator itself unevaluated.
    #[test]
    fn the_operator_of_a_quoting_form_is_not_itself_data() {
        let tree = tree("(quote (begin x))");
        let form = &tree.root_view().children[0];
        let operator = form.children[0].span;
        let operand = form.children[1].span;
        assert!(!is_unevaluated_at(&tree, operator));
        assert!(is_unevaluated_at(&tree, operand));
    }

    /// A span that names no top-level form is judged on its own, not by
    /// whichever form the search happened to land beside. Without the
    /// containment check the binary search returns the *next* form, and this
    /// gap would inherit that form's quoting.
    #[test]
    fn a_span_between_two_top_level_forms_is_not_judged_by_its_neighbour() {
        let source = "(a)\n'(b)\n";
        let tree = tree(source);
        let gap = ByteSpan::new(
            paredit_core_syntax::sexpr::ByteOffset::new(3),
            paredit_core_syntax::sexpr::ByteOffset::new(4),
        );
        assert!(!is_unevaluated_at(&tree, gap));
        assert_eq!(parent_head_of(&tree, gap), None);
    }

    #[test]
    fn a_top_level_form_has_no_parent_head() {
        let tree = tree("(begin x)");
        assert_eq!(parent_head_of(&tree, first_form(&tree, "begin")), None);
    }

    #[test]
    fn a_nested_form_reports_its_immediate_enclosing_head() {
        let tree = tree("(define-library (foo) (begin (define x 1)))");
        assert_eq!(
            parent_head_of(&tree, first_form(&tree, "begin")).as_deref(),
            Some("define-library")
        );
        // The *immediate* parent, not the outermost one.
        assert_eq!(
            parent_head_of(&tree, first_form(&tree, "define")).as_deref(),
            Some("begin")
        );
    }

    /// A bracketed `cond` clause is a list whose own head is a list, so it has
    /// no symbol head to report — the answer is `None`, not the `cond` above
    /// it. Callers ask about the *immediate* parent and must not read a `None`
    /// as "top level".
    #[test]
    fn a_parent_with_no_symbol_head_reports_none_rather_than_its_grandparent() {
        let tree = tree("(cond [(p) (begin x)])");
        assert_eq!(parent_head_of(&tree, first_form(&tree, "begin")), None);
    }

    /// A parent written with brackets is found by head like any other.
    #[test]
    fn a_bracketed_parent_still_reports_its_head() {
        let tree = tree("(let () [begin x])");
        let span = first_form(&tree, "begin");
        assert_eq!(parent_head_of(&tree, span).as_deref(), Some("let"));
    }

    #[test]
    fn a_form_outside_any_macro_definition_is_not_a_template() {
        let tree = tree("(define (f) (let loop ((i 0)) i))");
        let context = node_context(&tree, first_form(&tree, "let"));
        assert!(!context.in_macro_template);
    }

    /// The shape that made Guile's `match.upstream.scm` and `srfi-71.scm` read
    /// as dead loops: the named `let` is a `syntax-rules` template, and both
    /// its name and its body are supplied by the macro's caller.
    #[test]
    fn a_form_inside_a_syntax_rules_template_is_a_template() {
        let tree = tree(
            "(define-syntax r5rs-let (syntax-rules () ((_ tag ((v x) ...) body ...) (let tag ((v x) ...) body ...))))",
        );
        let context = node_context(&tree, first_form(&tree, "let"));
        assert!(context.in_macro_template);
    }

    #[test]
    fn every_macro_defining_head_marks_a_template() {
        for head in [
            "define-syntax",
            "define-syntax-rule",
            "syntax-rules",
            "syntax-case",
            "let-syntax",
            "letrec-syntax",
            "define-macro",
            "defmacro",
        ] {
            let tree = tree(&format!("({head} whatever (begin x))"));
            let context = node_context(&tree, first_form(&tree, "begin"));
            assert!(context.in_macro_template, "{head} must mark a template");
        }
    }

    #[test]
    fn quasi_depth_counts_only_quasiquotation() {
        let bare = tree("(begin x)");
        assert_eq!(
            node_context(&bare, first_form(&bare, "begin")).quasi_depth,
            0
        );

        let one = tree("`(a (begin x))");
        assert_eq!(node_context(&one, first_form(&one, "begin")).quasi_depth, 1);

        let two = tree("``(a (begin x))");
        assert_eq!(node_context(&two, first_form(&two, "begin")).quasi_depth, 2);

        // A hard quote is data but is not quasiquotation.
        let hard = tree("'(begin x)");
        let context = node_context(&hard, first_form(&hard, "begin"));
        assert!(context.unevaluated);
        assert_eq!(context.quasi_depth, 0);
    }

    #[test]
    fn the_literal_classifier_accepts_every_numeric_spelling_r7rs_admits() {
        for literal in [
            "5", "-5", "+5", "5.0", ".5", "-.5", "1/2", "#xff", "#XFF", "#b101", "#o17", "#d9",
            "#e1.0", "#i1", "+inf.0", "-inf.0", "+nan.0", "-nan.0", "1e10", "1+",
        ] {
            assert_eq!(
                literal_kind(literal),
                Some(LiteralKind::Number),
                "{literal} was not recognized as a number"
            );
        }
        assert_eq!(literal_kind("#\\a"), Some(LiteralKind::Character));
        assert_eq!(literal_kind("#\\space"), Some(LiteralKind::Character));
    }

    #[test]
    fn the_literal_classifier_rejects_everything_r7rs_guarantees() {
        for text in [
            "#t", "#f", "#true", "#false", "#u8(1)", "#", "", "-", "+", "...", "->list", ".",
            "sym", "'sym",
        ] {
            assert_eq!(literal_kind(text), None, "{text} must not be a literal");
        }
    }
}
