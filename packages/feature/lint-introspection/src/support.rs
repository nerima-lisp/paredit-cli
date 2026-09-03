//! What the rules here share: which parts of a file are *code*, and which
//! spellings each dialect actually has.
//!
//! # Why evaluation context is the whole problem for this package
//!
//! Every rule here anchors on a head that macro *bodies* are full of —
//! `intern`, `funcall`, `setf`, `fset`. A macro's output is written as a
//! backquoted template, so the single most common place these spellings occur
//! is inside `` `(…) `` — where they are a list being *built*, not a call being
//! made. The lint engine's dispatch walks into quoted data like any other
//! subtree and [`RuleContext`] carries no parent pointer, so a head-matched node
//! cannot tell on its own whether it is code. Without [`is_unevaluated_at`],
//! every rule in this package would fire on template text.
//!
//! [`QuoteState`] and [`for_each_evaluated_subview`] are copied from
//! `paredit-feature-lint-testing`'s `support.rs` (itself a copy of
//! `paredit-feature-lint-condition-system`'s), tests included, deliberately as a
//! copy rather than as a dependency: a lint feature package depending on another
//! lint feature package would be a new feature→feature edge for a hundred lines
//! of traversal.
//!
//! The two counters are not one depth number. A comma inside `'(…)` is a comma
//! character in a literal list, so `hard` never clears; a comma inside `` `(…) ``
//! escapes back to code, so `quasi` counts up and down. And the verdict is read
//! *at* the target, not at an ancestor: `` `(defun ,(name-for x) () …) `` is data
//! at the `defun` and code at the `,`.
//!
//! # Cost
//!
//! Nothing here runs per visited node. Every rule declares `HeadFilter::Heads`
//! and calls [`is_unevaluated_at`] at most once per candidate finding, *after*
//! the whole structural shape has already matched — which, in the
//! `clean/forms/*` benchmarks that lint files with no findings, is never.
//!
//! [`is_unevaluated_at`] descends from the *one* top-level form containing the
//! target, located by binary search over `root_children`, so a file of N
//! findings costs N·log(top-level forms) and never N².
//! `SyntaxTree::root_view` is never called: it materializes a `Vec` of children
//! and a `Vec` of reader prefixes for every node in the file, so asking it about
//! one node costs the whole document.
//!
//! [`RuleContext`]: paredit_core_lint_engine::engine::RuleContext

use paredit_core_syntax::dialect::Dialect;
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

/// The top-level form containing `target`, materialized on its own.
///
/// The reason this is not `tree.root_view()` followed by a search: `root_view`
/// builds an `ExpressionView` — a `Vec` of children and a `Vec` of reader
/// prefixes — for *every node in the file*, so asking it about one node costs
/// the whole document, uncached, on every call. Selecting the one root child
/// instead costs a binary search over the top level — each step a node-id lookup
/// and a span read, neither of which allocates — plus that one form's own
/// subtree.
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
/// data does not settle it: `` `(a ,(intern x pkg)) `` has a quasiquoted ancestor
/// and an evaluated target. Being inside a hard `'` does settle it, and that is
/// already modelled by `hard` never clearing.
///
/// The root's own span is never consulted. A file with one top-level form has a
/// root whose span equals that form's, and comparing them would call every such
/// form evaluated before looking at its prefixes at all. A span inside no
/// top-level form at all — one a caller synthesized rather than took from the
/// tree — is evaluated, because nothing quotes it.
///
/// Every rule here calls this at most once per candidate finding, *after* the
/// structural shape has already matched — never per visited node.
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
// Atoms
// ---------------------------------------------------------------------------

/// A string literal, which the reader keeps as one atom including its quotes.
///
/// This is what keeps every rule here out of string contents: `"(intern x p)"`
/// is this atom and has no children, so no walk can reach a form inside it.
#[must_use]
pub fn is_string_literal(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with('"'))
}

/// A `:keyword` atom, which is one of the two ways a package is named
/// literally.
///
/// Clojure keywords are spelled the same way, and a leading `::` is still a
/// keyword.
#[must_use]
pub fn is_keyword_atom(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with(':') && text.len() > 1)
}

/// Whether an atom is the given symbol, ignoring case and package qualifier.
#[must_use]
pub fn atom_is(view: &ExpressionView, expected: &str) -> bool {
    atom_symbol_text(view).is_some_and(|text| unqualified(text).eq_ignore_ascii_case(expected))
}

/// `'foo` or `(quote foo)`, either spelling, as a *shape* question only.
///
/// Used to recognise a package or function name the source shows outright.
#[must_use]
pub fn is_quoted_form(view: &ExpressionView) -> bool {
    view.reader_prefixes.contains(&ReaderPrefix::Quote) || is_quote_form(view)
}

/// Whether a form names something the source shows outright: a string literal,
/// a keyword, or a quoted symbol.
///
/// The negation is what "dynamically named" means for a probe or an `intern`:
/// a name only the run time knows.
#[must_use]
pub fn is_source_visible_name(view: &ExpressionView) -> bool {
    is_string_literal(view) || is_keyword_atom(view) || is_quoted_form(view)
}

/// Whether a node is a `(head …)` call to one of `heads`.
///
/// A `(…)` list whose first element is one of the given symbols, read past any
/// package qualifier and case. An atom is never a call.
#[must_use]
pub fn calls_any(view: &ExpressionView, heads: &[&str]) -> bool {
    is_paren_list(view) && list_head(view).is_some_and(|head| symbol_in(head, heads))
}

/// The operator a call names, normalized to the spelling messages are written
/// in.
///
/// Allocates, so it is called only once a finding is certain.
#[must_use]
pub fn call_operator(view: &ExpressionView) -> Option<String> {
    list_head(view).map(|head| unqualified(head).to_ascii_lowercase())
}

// ---------------------------------------------------------------------------
// Introspection vocabulary
// ---------------------------------------------------------------------------

/// The probes that answer "not found" with `nil` rather than by signalling.
///
/// Narrow on purpose, because the distinction is the whole rule and the obvious
/// guesses are wrong:
///
/// - **`find-class` is excluded.** CLHS gives it
///   `find-class symbol &optional errorp environment` with `errorp` defaulting
///   to *true*, so the ordinary `(find-class 'foo)` signals rather than
///   returning `nil`. There is no unchecked sentinel to use. (It also returns a
///   class, which is not something `funcall` accepts, so the shape this package
///   detects cannot occur for it.)
/// - **`symbol-function` is excluded.** CLHS says it signals `undefined-function`
///   when the symbol has no function definition — again no `nil` to leak. Emacs
///   Lisp's answer for a void function cell has changed across releases, so it
///   is left out rather than modelled from memory.
/// - **`fboundp` is excluded.** It *is* the check. A rule about a check being
///   skipped cannot anchor on the check.
///
/// What is left is the set whose CLHS/manual text says outright that the
/// not-found answer is `nil`: `find-symbol` returns `nil, nil`, `macro-function`
/// returns `nil` for a symbol that is not a macro name, Emacs Lisp's
/// `intern-soft` returns `nil` for a name no obarray holds, and Clojure's
/// `resolve`/`ns-resolve` return `nil` for a symbol that resolves to nothing.
#[must_use]
pub const fn nil_returning_probes(dialect: Dialect) -> &'static [&'static str] {
    match dialect {
        Dialect::CommonLisp => &["find-symbol", "macro-function"],
        Dialect::EmacsLisp => &["intern-soft"],
        Dialect::Clojure => &["resolve", "ns-resolve"],
        _ => &[],
    }
}

/// The operators that apply a value in function position, per dialect.
///
/// `funcall` is deliberately *not* listed for Clojure: Clojure has no such
/// function, so a `(funcall …)` in a `.clj` file is some project's own
/// definition and means nothing to this package.
#[must_use]
pub const fn apply_operators(dialect: Dialect) -> &'static [&'static str] {
    match dialect {
        Dialect::CommonLisp | Dialect::EmacsLisp => &["funcall", "apply"],
        Dialect::Clojure => &["apply"],
        _ => &[],
    }
}

/// The `HeadFilter::Heads` union for [`apply_operators`], normalized so the
/// dispatcher's head index and the rule agree.
///
/// The index is a pre-filter that may over-approximate; [`apply_operators`]
/// then rejects the pairings that do not exist, so a wider index costs a head
/// comparison and never a finding.
pub const APPLY_HEADS: [paredit_core_lint_engine::model::NormalizedHead; 2] = [
    paredit_core_lint_engine::model::NormalizedHead::new("apply"),
    paredit_core_lint_engine::model::NormalizedHead::new("funcall"),
];

/// Whether a form builds a symbol whose *name* no `grep` will find.
///
/// True for `(intern X)` / `(intern-soft X)` where `X` is not a string literal:
/// the resulting symbol's name exists only at run time. False for
/// `(intern "constant-name")`, whose name is right there in the source, and for
/// a plain quoted symbol.
///
/// Deliberately shallow: it asks about the *immediate* call, not about where
/// `X` came from. `(intern (format nil "~A-p" x))` and `(intern name)` are both
/// names the source does not show, and distinguishing them would need the value
/// table this package never asks for.
#[must_use]
pub fn builds_a_runtime_symbol_name(view: &ExpressionView) -> bool {
    if !calls_any(view, &["intern", "intern-soft"]) {
        return false;
    }
    view.children
        .get(1)
        .is_some_and(|name| !is_string_literal(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::view_query::for_each_subview;

    fn tree(source: &str, dialect: Dialect) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(source, dialect).expect("parse")
    }

    fn evaluated_heads(source: &str) -> Vec<String> {
        let parsed = tree(source, Dialect::CommonLisp);
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
        assert!(evaluated_heads("'(intern name pkg)").is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_data_below_its_head() {
        assert_eq!(evaluated_heads("(quote (intern name pkg))"), vec!["quote"]);
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(evaluated_heads("`(intern name pkg)").is_empty());
    }

    #[test]
    fn an_unquote_inside_a_backquote_is_code_again() {
        assert_eq!(
            evaluated_heads("`(a ,(intern name pkg))"),
            vec!["intern"] // `name` and `pkg` are atoms, not calls
        );
    }

    /// The shape a single `i32` depth counter gets wrong: a comma inside a
    /// hard quote is a comma character in a literal list, not an escape.
    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(evaluated_heads("'(a ,(intern name pkg))").is_empty());
    }

    /// The shape a node-local `reader_prefixes` check gets wrong: the inner
    /// node carries no prefix of its own, yet is still data.
    #[test]
    fn a_node_one_level_inside_a_quote_is_still_data() {
        assert!(evaluated_heads("'(outer (intern name pkg))").is_empty());
    }

    #[test]
    fn a_string_literal_is_one_atom_so_its_contents_are_never_forms() {
        assert_eq!(evaluated_heads("(f \"(intern name pkg)\")"), vec!["f"]);
    }

    // -- the same five shapes, through the span-directed lookup --------------

    fn data_at_first_head(source: &str, head: &str) -> bool {
        let parsed = tree(source, Dialect::CommonLisp);
        let mut span = None;
        // Deliberately the *unfiltered* walk: the point is to find the node
        // even when it is data.
        for_each_subview(&parsed.root_view(), |view| {
            if span.is_none() && list_head(view).is_some_and(|found| found == head) {
                span = Some(view.span);
            }
        });
        is_unevaluated_at(&parsed, span.expect("the head must occur in the source"))
    }

    #[test]
    fn a_span_inside_a_quote_reads_as_data() {
        assert!(data_at_first_head("'(intern name pkg)", "intern"));
    }

    #[test]
    fn a_span_inside_a_quote_form_reads_as_data() {
        assert!(data_at_first_head("(quote (intern name pkg))", "intern"));
    }

    #[test]
    fn a_span_in_plain_code_reads_as_evaluated() {
        assert!(!data_at_first_head(
            "(defun f () (intern name pkg))",
            "intern"
        ));
    }

    #[test]
    fn a_span_under_an_unquote_reads_as_evaluated() {
        assert!(!data_at_first_head("`(a ,(intern name pkg))", "intern"));
    }

    #[test]
    fn a_span_under_a_comma_in_a_hard_quote_reads_as_data() {
        assert!(data_at_first_head("'(a ,(intern name pkg))", "intern"));
    }

    /// The shape this package exists to survive: a macro whose expansion
    /// contains every spelling these rules match. The template is data; only
    /// what the commas escape to is code.
    #[test]
    fn a_backquoted_macro_template_is_data_at_the_template_and_code_at_the_unquote() {
        let source = "(defmacro define-handler (name)\n  \
                      `(setf (symbol-function (intern (format nil \"~A-handler\" ,name)))\n     \
                      (lambda () ,(compute-body name))))";
        // Every node of the template — the `setf`, the `symbol-function` place,
        // the `intern` call, the `format` call — is a list being built.
        for head in ["setf", "symbol-function", "intern", "format"] {
            assert!(
                data_at_first_head(source, head),
                "{head} is template text, not a call"
            );
        }
        // The one form the comma escapes back to code is code.
        assert!(!data_at_first_head(source, "compute-body"));
        // And the `defmacro` itself is code.
        assert!(!data_at_first_head(source, "defmacro"));
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
            ("(funcall (find-symbol \"F\" :app) 1)", Dialect::CommonLisp),
            (
                "'(a ,(b)) `(c ,(d)) #'e #(1 2) (f . g)",
                Dialect::CommonLisp,
            ),
            (
                "(f \"a string ( with parens\" #\\( :key 1/2 -3.5)",
                Dialect::CommonLisp,
            ),
            ("(apply ^:meta (resolve sym) args)", Dialect::Clojure),
            ("(fset (intern (concat \"a\" b)) #'g)", Dialect::EmacsLisp),
        ] {
            let parsed = tree(source, dialect);
            let root = parsed.root_view();
            let mut targets = Vec::new();
            for_each_subview(&root, |view| targets.push(view.span));
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

    /// The cost regression: `is_unevaluated_at` is called once per candidate
    /// finding, and a scan of the root's children would make a file of T
    /// findings cost T×T.
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
            .map(|index| format!("(defun f{index} () (funcall (find-symbol \"G\")))\n"))
            .collect();
        let parsed = tree(&source, Dialect::CommonLisp);
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

    // -- atoms ---------------------------------------------------------------

    fn first_form(source: &str, dialect: Dialect) -> (SyntaxTree, ByteSpan) {
        let parsed = tree(source, dialect);
        let span = parsed.root_view().children[0].span;
        (parsed, span)
    }

    fn view_of(source: &str, dialect: Dialect) -> ExpressionView {
        let (parsed, _) = first_form(source, dialect);
        parsed
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view()
    }

    #[test]
    fn a_string_literal_is_recognised_by_its_opening_quote() {
        assert!(is_string_literal(&view_of("\"text\"", Dialect::CommonLisp)));
        assert!(!is_string_literal(&view_of("text", Dialect::CommonLisp)));
        assert!(!is_string_literal(&view_of("(f)", Dialect::CommonLisp)));
    }

    #[test]
    fn a_keyword_needs_more_than_its_colon() {
        assert!(is_keyword_atom(&view_of(":app", Dialect::CommonLisp)));
        assert!(!is_keyword_atom(&view_of("app", Dialect::CommonLisp)));
    }

    #[test]
    fn a_quoted_form_is_read_through_both_spellings() {
        assert!(is_quoted_form(&view_of("'app", Dialect::CommonLisp)));
        assert!(is_quoted_form(&view_of("(quote app)", Dialect::CommonLisp)));
        assert!(!is_quoted_form(&view_of("app", Dialect::CommonLisp)));
        assert!(!is_quoted_form(&view_of("`app", Dialect::CommonLisp)));
    }

    #[test]
    fn a_call_is_read_past_case_and_package_qualifier() {
        assert!(calls_any(
            &view_of("(CL:INTERN x)", Dialect::CommonLisp),
            &["intern"]
        ));
        assert!(!calls_any(
            &view_of("intern", Dialect::CommonLisp),
            &["intern"]
        ));
    }

    #[test]
    fn atom_is_reads_past_case_and_package_qualifier() {
        assert!(atom_is(
            &view_of("CL:*PACKAGE*", Dialect::CommonLisp),
            "*package*"
        ));
        assert!(!atom_is(
            &view_of("*other*", Dialect::CommonLisp),
            "*package*"
        ));
    }

    // -- the vocabulary tables -----------------------------------------------

    /// The three exclusions the `nil` sentinel question turns on. A future
    /// edit that "completes" this list re-introduces the false positives the
    /// table's own documentation explains.
    #[test]
    fn the_signalling_probes_are_not_listed_as_nil_returning() {
        for dialect in [Dialect::CommonLisp, Dialect::EmacsLisp, Dialect::Clojure] {
            for excluded in ["find-class", "symbol-function", "fboundp"] {
                assert!(
                    !nil_returning_probes(dialect).contains(&excluded),
                    "{excluded} does not answer not-found with nil"
                );
            }
        }
    }

    #[test]
    fn each_dialect_gets_only_its_own_probe_spellings() {
        assert_eq!(
            nil_returning_probes(Dialect::CommonLisp),
            &["find-symbol", "macro-function"]
        );
        assert_eq!(nil_returning_probes(Dialect::EmacsLisp), &["intern-soft"]);
        assert_eq!(
            nil_returning_probes(Dialect::Clojure),
            &["resolve", "ns-resolve"]
        );
        assert!(nil_returning_probes(Dialect::Scheme).is_empty());
    }

    /// Clojure has no `funcall`, so a `(funcall …)` in a `.clj` file is some
    /// project's own function and must not be read as an application operator.
    #[test]
    fn funcall_is_not_a_clojure_application_operator() {
        assert_eq!(apply_operators(Dialect::Clojure), &["apply"]);
        assert_eq!(apply_operators(Dialect::CommonLisp), &["funcall", "apply"]);
        assert_eq!(apply_operators(Dialect::EmacsLisp), &["funcall", "apply"]);
        assert!(apply_operators(Dialect::Racket).is_empty());
    }

    // -- runtime symbol names ------------------------------------------------

    #[test]
    fn an_intern_of_a_computed_string_builds_a_runtime_name() {
        assert!(builds_a_runtime_symbol_name(&view_of(
            "(intern (format nil \"~A-p\" x))",
            Dialect::CommonLisp
        )));
        assert!(builds_a_runtime_symbol_name(&view_of(
            "(intern name)",
            Dialect::CommonLisp
        )));
        assert!(builds_a_runtime_symbol_name(&view_of(
            "(intern-soft (concat \"a\" b))",
            Dialect::EmacsLisp
        )));
    }

    #[test]
    fn an_intern_of_a_string_literal_is_a_name_the_source_shows() {
        assert!(!builds_a_runtime_symbol_name(&view_of(
            "(intern \"constant-name\")",
            Dialect::CommonLisp
        )));
    }

    #[test]
    fn a_plain_quoted_symbol_builds_nothing() {
        assert!(!builds_a_runtime_symbol_name(&view_of(
            "'my-function",
            Dialect::CommonLisp
        )));
        assert!(!builds_a_runtime_symbol_name(&view_of(
            "(intern)",
            Dialect::CommonLisp
        )));
    }
}
