//! Reading a node's *position*: whether it is code at all, and whether the
//! value it produces is thrown away.
//!
//! # Cost, first
//!
//! Everything here except [`is_unevaluated_at`] is **local**: it reads the node
//! the dispatcher handed over and that node's own children, and never the file.
//! That is deliberate, and it is the second design — see [`is_unevaluated_at`]
//! for the 3.9-second measurement that rejected the first one.
//!
//! [`is_unevaluated_at`] is the exception. It reaches `SyntaxTree::root_view()`,
//! which materializes every node in the file, so a rule must call it **only
//! after** its local checks have already produced a finding. A sibling package
//! measured **450,843 ns/call against 28 ns/call** purely from reaching the root
//! before the head check, and the `clean/forms/*` benchmarks lint files with
//! zero findings — so the cost of a rule that matches nothing is exactly what
//! they measure.
//!
//! # The quote model is two counters, not a depth
//!
//! [`QuoteState`] is copied from `paredit-feature-lint-condition-system::support`,
//! as the other lint packages do with it; a consolidation into `packages/core`
//! is in flight, and when it lands this module should be deleted.
//!
//! `'` and `` ` `` are not the same thing. A comma inside `'(…)` is a comma
//! character in a literal list, so `hard` never clears; a comma inside `` `(…) ``
//! escapes back to code, so `quasi` counts up and down. A single `i32` depth
//! counter is wrong and has shipped in this workspace as a false-positive
//! source twice. This package genuinely needs the distinction: a macro template
//! that writes `` `(sort ,x #'<) `` is building code, not discarding a value,
//! and one that writes `` `(progn ,(sort xs #'<) xs) `` is discarding one.

use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_is};

/// How much of the surrounding reader syntax says "this is data".
///
/// Two independent counters; see the module documentation for why one is not
/// enough.
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
    list_head(view).is_some_and(|head| symbol_is(head, "quote"))
}

const fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start().get() <= inner.start().get() && inner.end().get() <= outer.end().get()
}

/// Whether the node at `target` is unevaluated data rather than code.
///
/// Descends from the root through the one child at each level whose span
/// contains `target`, so the *descent* is the node's depth rather than the
/// file's size.
///
/// The verdict is read *at* the target and nowhere shallower. An ancestor being
/// data does not settle it: `` `(a ,(sort xs #'<)) `` has a quasiquoted ancestor
/// and an evaluated target.
///
/// # This is the expensive one
///
/// The caller must obtain a root [`ExpressionView`], and `SyntaxTree::root_view`
/// **materializes the whole tree**: one `ExpressionView` per node, each with its
/// own `Vec` of children and of reader prefixes. That is O(file) with
/// allocations, *per call*, so a rule that calls it per candidate is quadratic
/// in the file.
///
/// This package learned that by measuring rather than by reading: an earlier
/// design anchored on the destructive call and walked *up*, and cost **3.9
/// seconds** on a 200-function fixture with **zero findings**, against the
/// shipped control's 224 µs in the same run. The correct `(setf xs (sort xs #'<))`
/// idiom passes any cheap head-and-argument test, so every correct call in the
/// file paid a full tree materialization.
///
/// The rule therefore anchors on the enclosing body form and reads its own
/// children, and calls this **only once it already has a finding** — which on
/// correct code is never.
#[must_use]
pub fn is_unevaluated_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    let root = tree.root_view();
    let mut view: &ExpressionView = &root;
    let mut state = QuoteState::EVALUATED;

    loop {
        let quoting = is_quote_form(view);
        let Some(child) = view
            .children
            .iter()
            .find(|child| span_contains(child.span, target))
        else {
            return state.is_data();
        };
        state = state.after_prefixes(child);
        if quoting {
            state = state.quoted();
        }
        view = child;
        if view.span == target {
            return state.is_data();
        }
    }
}

/// Calls `visit` on every node of `root` reachable as evaluated code.
///
/// Quoted subtrees are still *descended* — `` `(a ,(f)) `` has code inside data
/// — but their data nodes are never visited. Used by the tests and the corpus
/// harness; the rule itself is driven by the head index.
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

// ---------------------------------------------------------------------------
// Position: is this form's value discarded?
// ---------------------------------------------------------------------------

/// Operators with an **implicit `progn` body**, and the child index that body
/// starts at.
///
/// This table is the whole rule. A form at index `>= start` that is *not* the
/// last child of one of these operators has its value discarded, by the
/// definition of an implicit progn — no dataflow analysis, no type inference,
/// no guessing.
///
/// # What is deliberately absent, and why
///
/// - **`tagbody`** — every form in it is discarded, but a `tagbody` body is
///   interleaved with go-tags, and a bare symbol there is a label rather than a
///   value. Reporting inside one would need the tag/statement split, and
///   `tagbody` almost never appears in the code this rule is aimed at.
/// - **`loop`** — a `loop`'s `do` clause discards its forms, but `loop` is a
///   flat keyword grammar rather than nested lists, so "index in parent" says
///   nothing about position. `paredit-feature-lint-loop-facility` owns that
///   grammar.
/// - **`cond`** / `case` clauses — a clause is a list whose head is the *test*,
///   not an operator, so it never matches this table. A non-final form in a
///   `cond` clause body is genuinely discarded, but the clause's first child is
///   a test whose value very much is used, and the two are told apart only by
///   index. Left out to keep the table's meaning uniform.
/// - **`unwind-protect`** — the one form here whose **last** child is also
///   discarded, since the value is the protected form's. Including it would
///   break the "last child is used" invariant every other entry relies on.
/// - **`prog1`** / **`prog2`** — their point is that a non-final value *is* the
///   result.
///
/// All five are the *ambiguous* set: not reported, rather than reported and
/// wrong.
pub const BODY_FORMS: [(&str, usize); 20] = [
    ("progn", 1),
    ("prog", 2),
    ("prog*", 2),
    ("let", 2),
    ("let*", 2),
    ("flet", 2),
    ("labels", 2),
    ("macrolet", 2),
    ("symbol-macrolet", 2),
    ("lambda", 2),
    ("when", 2),
    ("unless", 2),
    ("dolist", 2),
    ("dotimes", 2),
    ("block", 2),
    ("with-open-file", 2),
    ("with-slots", 2),
    ("defun", 3),
    ("defmethod", 3),
    ("defmacro", 3),
];

/// The body-start index for `head`, if it is an implicit-progn operator.
#[must_use]
pub fn body_start(head: &str) -> Option<usize> {
    BODY_FORMS
        .iter()
        .find(|(name, _)| symbol_is(head, name))
        .map(|(_, start)| *start)
}

/// Whether the node at `index` in `parent` has its value **discarded**.
///
/// True only for a non-final form in the body of a known implicit-progn
/// operator. Everything else answers `false`, which is the safe direction:
///
/// - the **last** child of any form is its value,
/// - a child at an index *before* the body — a `let`'s binding list, a `when`'s
///   test, a `defun`'s lambda list — is not a body form at all,
/// - a child of anything not in [`BODY_FORMS`] is an **argument to a call**,
///   whose value the call consumes. This is what makes the correct idiom
///   `(setf xs (sort xs #'<))` unreportable: `setf` is not a body form, so no
///   child of it is ever a discarded statement. So is `(push (sort xs #'<) acc)`,
///   `(return-from f (nconc a b))`, and every other consuming position.
///
/// Defined in terms of [`discarded_range`], which is the single implementation
/// the rule also iterates. An earlier version spelled the predicate out twice —
/// once here and once inline in the rule — and mutation testing found this copy
/// killed no test, because nothing outside its own tests called it. Two
/// spellings of one specification is the defect that produces; this is the
/// repair.
#[must_use]
pub fn value_is_discarded(parent: &ExpressionView, index: usize) -> bool {
    discarded_range(parent).is_some_and(|range| range.contains(&index))
}

/// The range of child indices of `view` whose values are **discarded**.
///
/// `None` when `view` is not an implicit-progn operator or has no children at
/// all. The range always excludes the last child, which is the body's value, and
/// is **empty** — rather than `None` — for a form whose body is only that value.
///
/// # On the missing emptiness check
///
/// An earlier version ended `(start < last).then(|| start..last)`, so that a
/// body with no statements answered `None` instead of an empty range. Mutation
/// testing showed that check killed no test, and it cannot: `start..last` with
/// `start >= last` is already empty, so the rule's `for index in range` does
/// nothing and `value_is_discarded`'s `range.contains(&index)` is already false
/// for every index. It was a defensive line that changed no behaviour, and it is
/// gone rather than left to look load-bearing. `a_form_with_no_statements_yields_an_empty_range`
/// pins the behaviour that replaced it.
#[must_use]
pub fn discarded_range(view: &ExpressionView) -> Option<std::ops::Range<usize>> {
    let start = body_start(list_head(view)?)?;
    // The last child of a body form is the body's value, so the range stops one
    // short of it.
    let last = view.children.len().checked_sub(1)?;
    Some(start..last)
}

/// Whether `view` is a bare symbol — a variable reference, not a literal, a
/// call, or a quoted datum.
///
/// The destroyed argument being a *variable* is what makes a discarded result a
/// bug worth reporting: there is a name that still points at the wreckage. A
/// literal (`(sort '(3 1 2) #'<)`) is already
/// `paredit-feature-lint-sequence`'s `destructive-literal`, and a nested call
/// (`(sort (copy-list xs) #'<)`) destroys a temporary nobody else can see.
#[must_use]
pub fn is_bare_symbol(view: &ExpressionView) -> bool {
    if view.kind != ExpressionKind::Atom || !view.reader_prefixes.is_empty() {
        return false;
    }
    atom_text(view).is_some_and(|text| {
        // Not a literal of any kind the reader leaves as an atom.
        !text.is_empty()
            && !text.starts_with('"')
            && !text.starts_with('#')
            && !text.starts_with(':')
            && !text.starts_with(|c: char| c.is_ascii_digit())
            && !text.eq_ignore_ascii_case("nil")
            && !text.eq_ignore_ascii_case("t")
    })
}

/// Whether `symbol` occurs anywhere in `view`'s subtree.
///
/// Bounded by the subtree the caller passes — a sibling form in the same body —
/// never by the file.
///
/// # Why there is no `is_bare_symbol` guard here
///
/// There was one, and **mutation testing found it killed no test**. It is
/// unreachable rather than untested, and the reason is that
/// [`paredit_core_syntax::view_query::atom_text`] returns an atom's text
/// *including* its reader prefix — `'xs` reads as `"'xs"`, `#'xs` as `"#'xs"`,
/// `"xs"` as `"\"xs\""` — while [`symbol_is`] compares past a package qualifier
/// only, and [`paredit_core_syntax::view_query::unqualified`] returns a keyword
/// like `:xs` unchanged. So none of the shapes the guard excluded could ever
/// have compared equal to a symbol that itself passed [`is_bare_symbol`], which
/// every caller's `symbol` argument has.
///
/// The knowledge lives here and in
/// `a_mention_inside_a_string_literal_is_not_a_reference` — which passes for
/// this reason, not because of a guard — rather than in a line that never runs.
/// That is the trap this repository has hit before.
#[must_use]
pub fn subtree_mentions(view: &ExpressionView, symbol: &str) -> bool {
    if atom_text(view).is_some_and(|text| symbol_is(text, symbol)) {
        return true;
    }
    view.children
        .iter()
        .any(|child| subtree_mentions(child, symbol))
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

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

    #[test]
    fn an_evaluated_walk_visits_plain_code() {
        assert_eq!(evaluated_heads("(a (b) (c (d)))"), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn a_quoted_list_is_data_and_is_not_visited() {
        assert!(evaluated_heads("'(sort xs #'<)").is_empty());
    }

    #[test]
    fn a_backquote_without_an_unquote_is_data() {
        assert!(evaluated_heads("`(sort xs #'<)").is_empty());
    }

    /// The two-counter model's whole point: a comma re-enters code under `` ` ``
    /// and does not under `'`.
    #[test]
    fn an_unquote_inside_a_backquote_is_code_again() {
        assert_eq!(evaluated_heads("`(a ,(sort xs #'<))"), vec!["sort"]);
    }

    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(evaluated_heads("'(a ,(sort xs #'<))").is_empty());
    }

    /// Position, form by form. The table is the rule, so it is tested directly.
    fn discarded_indices(source: &str) -> Vec<usize> {
        let parsed = tree(source);
        let form = &parsed.root_view().children[0];
        (0..form.children.len())
            .filter(|index| value_is_discarded(form, *index))
            .collect()
    }

    #[test]
    fn a_progn_discards_every_form_but_the_last() {
        assert_eq!(discarded_indices("(progn (a) (b) (c))"), vec![1, 2]);
    }

    #[test]
    fn a_let_discards_its_body_but_never_its_bindings() {
        // 0=let 1=bindings 2=(a) 3=(b) — the bindings list is not a body form.
        assert_eq!(discarded_indices("(let ((x 1)) (a) (b))"), vec![2]);
    }

    #[test]
    fn a_defun_body_starts_after_the_lambda_list() {
        // 0=defun 1=name 2=args 3=(a) 4=(b)
        assert_eq!(discarded_indices("(defun f (xs) (a) (b))"), vec![3]);
    }

    #[test]
    fn a_when_discards_its_body_but_never_its_test() {
        assert_eq!(discarded_indices("(when (p) (a) (b))"), vec![2]);
    }

    /// The correct idiom's safety net: `setf` is not a body form, so **no**
    /// child of it is ever a discarded statement, whatever its index.
    #[test]
    fn no_argument_of_a_plain_call_is_ever_discarded() {
        assert!(discarded_indices("(setf xs (sort xs #'<) ys (sort ys #'>))").is_empty());
        assert!(discarded_indices("(list (a) (b) (c))").is_empty());
        assert!(discarded_indices("(push (nconc a b) acc)").is_empty());
    }

    /// A body whose only form is its value has no statement positions, and a
    /// form with no body at all is `None`.
    #[test]
    fn a_form_with_no_statements_yields_an_empty_range() {
        let parsed = tree("(progn (a))");
        let form = &parsed.root_view().children[0];
        let range = discarded_range(form).expect("progn is a body form");
        assert!(range.is_empty(), "the only form is the body's value");
        assert!(!value_is_discarded(form, 1));

        let parsed = tree("(defun f (xs))");
        let form = &parsed.root_view().children[0];
        assert!(
            discarded_range(form)
                .expect("defun is a body form")
                .is_empty(),
            "a defun with an empty body has no discarded positions"
        );

        let parsed = tree("(setf a 1)");
        let form = &parsed.root_view().children[0];
        assert!(discarded_range(form).is_none(), "setf is not a body form");
    }

    /// The ambiguous set answers `false`, deliberately.
    #[test]
    fn the_ambiguous_forms_report_nothing() {
        assert!(discarded_indices("(tagbody top (a) (b) (go top))").is_empty());
        assert!(discarded_indices("(unwind-protect (a) (b) (c))").is_empty());
        assert!(discarded_indices("(prog1 (a) (b) (c))").is_empty());
        assert!(discarded_indices("(loop for x in xs do (a) (b))").is_empty());
    }

    #[test]
    fn a_bare_symbol_is_told_apart_from_every_literal() {
        let parsed = tree("(f xs \"s\" 12 :key nil t #'g 'q)");
        let call = &parsed.root_view().children[0];
        assert!(is_bare_symbol(&call.children[1]), "xs is a variable");
        for index in 2..call.children.len() {
            assert!(
                !is_bare_symbol(&call.children[index]),
                "child {index} is a literal, not a variable"
            );
        }
    }

    /// Each literal-shaped exclusion in `is_bare_symbol`, one at a time.
    ///
    /// The grouped test above passes if *any* exclusion rejects a given child,
    /// so it cannot tell which line does the work — mutation testing showed the
    /// string-literal clause in particular needed its own case.
    #[test]
    fn each_literal_shape_is_excluded_on_its_own() {
        for (source, why) in [
            ("(f \"abc\")", "a string literal"),
            ("(f 12)", "an integer"),
            ("(f :key)", "a keyword"),
            ("(f nil)", "nil"),
            ("(f t)", "t"),
            ("(f #'g)", "a function designator"),
            ("(f 'q)", "a quoted symbol"),
            // These two are why the `#` clause exists and is not covered by the
            // reader-prefix check above: neither carries a `ReaderPrefix`, so
            // only the leading `#` distinguishes them. A CL reader conditional
            // and the form after it fold into a **single atom** whose text
            // carries the prefix — `#+sbcl xs` reads as one atom, not as `xs`.
            ("(f #\\a)", "a character literal"),
            ("(f #+sbcl xs)", "a reader conditional folded into one atom"),
        ] {
            let parsed = tree(source);
            let call = &parsed.root_view().children[0];
            assert!(
                !is_bare_symbol(&call.children[1]),
                "{why} must not read as a variable: {source}"
            );
        }
        // The control, so this cannot pass by rejecting everything.
        let parsed = tree("(f xs)");
        let call = &parsed.root_view().children[0];
        assert!(is_bare_symbol(&call.children[1]));
    }

    #[test]
    fn a_subtree_mention_finds_a_later_use_and_ignores_a_lookalike() {
        let parsed = tree("(progn (print (car xs)) (print ys))");
        let form = &parsed.root_view().children[0];
        assert!(subtree_mentions(&form.children[1], "xs"));
        assert!(!subtree_mentions(&form.children[2], "xs"));
    }

    /// A mention inside a string is not a mention: the reader makes the whole
    /// string one atom, and `is_bare_symbol` rejects it by its delimiter.
    #[test]
    fn a_mention_inside_a_string_literal_is_not_a_reference() {
        let parsed = tree("(progn (print \"xs\"))");
        let form = &parsed.root_view().children[0];
        assert!(!subtree_mentions(&form.children[1], "xs"));
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
        assert!(unevaluated_at_first_head("'(sort xs #'<)", "sort"));
        assert!(unevaluated_at_first_head("(quote (sort xs #'<))", "sort"));
        assert!(unevaluated_at_first_head("`(sort xs #'<)", "sort"));
    }

    #[test]
    fn a_span_in_plain_code_reads_as_evaluated() {
        assert!(!unevaluated_at_first_head(
            "(defun f (xs) (sort xs #'<))",
            "sort"
        ));
    }

    /// The two-counter model again, this time through the span-based reader.
    #[test]
    fn a_span_under_an_unquote_reads_as_evaluated() {
        assert!(!unevaluated_at_first_head("`(a ,(sort xs #'<))", "sort"));
    }

    #[test]
    fn a_span_under_a_comma_inside_a_hard_quote_stays_data() {
        assert!(unevaluated_at_first_head("'(a ,(sort xs #'<))", "sort"));
    }
}
