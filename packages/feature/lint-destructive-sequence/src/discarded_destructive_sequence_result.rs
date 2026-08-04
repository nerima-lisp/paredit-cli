//! `discarded-destructive-sequence-result`: a destructive call whose result is
//! thrown away, on a variable the same body goes on to read.
//!
//! CLHS 17.1 says a destructive sequence function "may be destroyed and used to
//! construct the result", and that the consequences are undefined if the
//! original is used afterwards. The classic spelling of the bug is:
//!
//! ```text
//! (defun report (xs)
//!   (sort xs #'<)     ; the sorted list is returned, and dropped
//!   (print xs))       ; xs is now whatever is left of the old structure
//! ```
//!
//! # What SBCL 2.6.0 actually does — the reason this head list is short
//!
//! SBCL already catches most of this family, and a rule that repeats a
//! diagnostic the compiler gives for free is not worth the false positives.
//! Every one of the 22 CLHS destructive sequence functions was compiled in the
//! shape above and the compiler's own output recorded:
//!
//! ```text
//! HEAD                 warnings-p failure-p  signalled
//! nreverse             T          NIL        STYLE-WARNING: return value of NREVERSE should not be discarded
//! nreconc              T          NIL        STYLE-WARNING  (likewise)
//! delete               T          NIL        STYLE-WARNING
//! delete-if            T          NIL        STYLE-WARNING
//! delete-if-not        T          NIL        STYLE-WARNING
//! delete-duplicates    T          NIL        STYLE-WARNING
//! nunion               T          NIL        STYLE-WARNING
//! nintersection        T          NIL        STYLE-WARNING
//! nset-difference      T          NIL        STYLE-WARNING
//! nset-exclusive-or    T          NIL        STYLE-WARNING
//! merge                T          NIL        STYLE-WARNING
//!
//! sort                 NIL        NIL        (none)        <-- untyped argument
//! stable-sort          NIL        NIL        (none)        <-- untyped argument
//! nconc                NIL        NIL        (none)
//! nbutlast             NIL        NIL        (none)
//! nsubst               NIL        NIL        (none)
//! nsublis              NIL        NIL        (none)
//! nsubstitute          NIL        NIL        (none)
//! nstring-downcase     NIL        NIL        (none)
//! nstring-upcase       NIL        NIL        (none)
//! nstring-capitalize   NIL        NIL        (none)
//! replace              NIL        NIL        (none)
//! ```
//!
//! The eleven that warn are **not** in this rule's head list. They are the
//! compiler's job, it does it, and duplicating it would only add noise.
//!
//! `sort` and `stable-sort` are the interesting pair, because SBCL's silence
//! there is **conditional on type inference**:
//!
//! ```text
//! (defun f (xs) (sort xs #'<) xs)                        => warn=NIL  silent
//! (defun f (xs) (declare (list xs)) (sort xs #'<) xs)    => warn=T    STYLE-WARNING
//! (defun f (xs) (declare (vector xs)) (sort xs #'<) xs)  => warn=NIL  silent
//! (defun f () (let ((xs (list 3 1 2))) (sort xs #'<) xs)) => warn=T   STYLE-WARNING
//! ```
//!
//! SBCL warns only once it can prove the sequence is a list, at which point it
//! picks the `STABLE-SORT-LIST` transform and that transform carries the
//! `should not be discarded` declaration. An **untyped parameter — the common
//! case in real code — gets nothing**. That gap is what this rule is for.
//!
//! # Why the other silent heads are still excluded
//!
//! Silence is necessary but not sufficient: the call also has to be able to
//! *break* something. Measured, again on SBCL 2.6.0:
//!
//! ```text
//! (let ((s (copy-seq "hello"))) (nstring-upcase s) s)  => "HELLO"   correct
//! (let ((v (vector 5 4 3 2 1))) (sort v #'<) v)        => #(1 2 3 4 5)  correct
//! ```
//!
//! `nstring-downcase`, `nstring-upcase`, `nstring-capitalize`, `replace` and
//! `nsubstitute` rewrite elements **in place** and never return a different
//! object, so discarding their result is harmless and reporting it would be a
//! pure false positive. They are excluded.
//!
//! What is left is the six heads that are both silent under SBCL and able to
//! return an object that is not the argument:
//!
//! | head | destroyed argument | how the identity changes |
//! |---|---|---|
//! | `sort`, `stable-sort` | 1 | a list's head cons moves; only the return value names the sorted list |
//! | `nconc` | 1 | a `nil` first argument is not modified at all, and the result is the second |
//! | `nbutlast` | 1 | a list no longer than the count returns `nil`, modifying nothing |
//! | `nsublis` | 2 | a tree that matches at the root returns the replacement |
//! | `nsubst` | 3 | likewise |
//!
//! # What the aftermath actually looks like
//!
//! Recorded rather than assumed, because the folklore ("the variable ends up
//! holding the last cons") is wrong:
//!
//! ```text
//! (let ((xs (list 3 1 2)))     (sort xs #'<)  xs) => (1 2 3)    accidentally right
//! (let ((xs (list 5 4 3 2 1))) (sort xs #'<)  xs) => (4 5)      two-element interior tail
//! (let ((xs (list 1 2 3)))     (nreverse xs)  xs) => (1)        the *first* cons
//! (let ((xs (list 1 2 3 4 5))) (nreverse xs)  xs) => (1)
//! ```
//!
//! `(1 2 3)` in the first line is the reason this bug survives review: on a
//! short list it often looks like it worked.
//!
//! # The three conditions, and why all three
//!
//! A finding needs every one of:
//!
//! 1. **The destroyed argument is a bare symbol.** A literal is
//!    `paredit-feature-lint-sequence`'s `destructive-literal`, which already
//!    ships; a nested call destroys a temporary nobody can observe.
//! 2. **The value is discarded** — a non-final form in a known implicit-progn
//!    body. See `support::BODY_FORMS`; `(setf xs (sort xs #'<))` cannot reach
//!    this because no argument of a plain call is ever a discarded statement.
//! 3. **A later form in the same body reads that symbol.** Without this the
//!    call is merely dead, and "dead" has a large innocent population — a
//!    function whose whole purpose is the side effect, a result deliberately
//!    ignored. With it, there is a variable that still names the wreckage and
//!    code that goes on to read it. That is the bug, spelled out.
//!
//! Report-only. The repair is usually `(setf xs (sort xs #'<))`, but whether the
//! caller wanted the destructive version at all, or a `sort` on a copy, is a
//! decision about the program.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_is};

use crate::support::{discarded_range, is_bare_symbol, is_unevaluated_at, subtree_mentions};

pub const META: RuleMeta = RuleMeta::new(
    "discarded-destructive-sequence-result",
    RuleCategory::DeadCode,
    Severity::Warning,
    "a destructive sequence call whose result is discarded, on a variable the body then reads",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "A destructive sequence function may reuse its argument's storage to build the result, so \
         only the **return value** names the finished sequence. Discarding it leaves the variable \
         pointing into the middle of the old structure: SBCL 2.6.0 leaves `xs` as `(4 5)` after a \
         discarded `(sort xs #'<)` on `(5 4 3 2 1)`. The heads here are the ones SBCL does *not* \
         warn about — it already reports `nreverse`, `delete` and nine others.",
    )
    .with_example(
        "(defun report (xs)\n  (sort xs #'<)\n  (print xs))",
        "(defun report (xs)\n  (setf xs (sort xs #'<))\n  (print xs))",
    )
    .with_caveat(
        "Reported only when all three hold: the destroyed argument is a bare variable, the call \
         sits in a non-final position of a known implicit-progn body, and a later form in that \
         same body reads the variable. `tagbody`, `loop`, `cond` clauses, `unwind-protect` \
         cleanups and `prog1` are treated as ambiguous and never reported. `sort` on a vector is \
         in-place in practice, so a vector-valued variable is a false positive this rule cannot \
         rule out without type inference.",
    ),
);

/// The six heads, each paired with the index of the argument it may destroy.
///
/// Short on purpose: see the module docstring for the SBCL sweep that removed
/// the other sixteen.
const DESTRUCTIVE: [(&str, usize); 6] = [
    ("sort", 1),
    ("stable-sort", 1),
    ("nconc", 1),
    ("nbutlast", 1),
    ("nsublis", 2),
    ("nsubst", 3),
];

/// The heads the rule **anchors on**: the implicit-progn operators, not the
/// destructive functions.
///
/// # Why the anchor is the body and not the call
///
/// "Discarded" is a property of a form's *position*, so deciding it needs the
/// parent — and `RuleContext` carries no parent pointer. Recovering one means
/// descending from `SyntaxTree::root_view()`, which **materializes the whole
/// tree**: one `ExpressionView` per node, each with its own `Vec`s. That is
/// O(file) with allocations per call.
///
/// The first version of this rule anchored on `sort`/`nconc`/… and walked up.
/// It measured **3.9 seconds** on a 200-function fixture with **zero findings**,
/// against a shipped control's 224 µs in the same run — quadratic, because the
/// *correct* idiom `(setf xs (sort xs #'<))` passes any cheap head-and-argument
/// test, so every correct call in the file paid a full materialization. See
/// `cost_tests`.
///
/// Anchoring on the body form inverts the direction: the dispatcher hands over
/// the `defun`/`let`/`when`, and the parent-child relation the rule needs is
/// simply that node's own children. No tree access, no allocation, and the
/// per-file cost is linear. `paredit-feature-lint-performance`'s
/// `unnecessary-sort-before-extremum-extraction` chose the same inversion for
/// the same reason.
///
/// The heads are common, so the invocation count is large; the work per
/// invocation is a bounded scan of the node's own children, most of which fail
/// a six-entry table probe immediately.
const HEADS: [NormalizedHead; 20] = [
    NormalizedHead::new("progn"),
    NormalizedHead::new("prog"),
    NormalizedHead::new("prog*"),
    NormalizedHead::new("let"),
    NormalizedHead::new("let*"),
    NormalizedHead::new("flet"),
    NormalizedHead::new("labels"),
    NormalizedHead::new("macrolet"),
    NormalizedHead::new("symbol-macrolet"),
    NormalizedHead::new("lambda"),
    NormalizedHead::new("when"),
    NormalizedHead::new("unless"),
    NormalizedHead::new("dolist"),
    NormalizedHead::new("dotimes"),
    NormalizedHead::new("block"),
    NormalizedHead::new("with-open-file"),
    NormalizedHead::new("with-slots"),
    NormalizedHead::new("defun"),
    NormalizedHead::new("defmethod"),
    NormalizedHead::new("defmacro"),
];

/// The index of the argument `head` may destroy, if `head` is one of the six.
fn destroyed_index(head: &str) -> Option<usize> {
    DESTRUCTIVE
        .iter()
        .find(|(name, _)| symbol_is(head, name))
        .map(|(_, index)| *index)
}

/// One discarded destructive call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscardedResult {
    /// The span of the call itself — the thing to wrap in a `setf`.
    pub span: ByteSpan,
    /// The variable that still names the wreckage.
    pub variable: String,
    /// The destructive operator, for the message.
    pub head: String,
}

/// Is `view` a destructive call on a bare variable? If so, that variable.
///
/// A table probe and two child lookups. No tree access.
fn destroyed_variable(view: &ExpressionView) -> Option<&str> {
    let index = destroyed_index(list_head(view)?)?;
    let argument = view.children.get(index)?;
    if !is_bare_symbol(argument) {
        return None;
    }
    atom_text(argument)
}

/// [`destroyed_variable`], for the corpus audit's funnel.
///
/// The audit needs to count condition 1's survivors independently of the rule's
/// dispatch, so that a zero-finding sweep can say *which* condition did the
/// cutting rather than only that nothing was reported.
#[must_use]
pub fn destroyed_variable_of(view: &ExpressionView) -> Option<&str> {
    destroyed_variable(view)
}

/// Every discarded destructive call in **`view`'s own body**.
///
/// `view` is an implicit-progn form the dispatcher matched. Entirely local: it
/// reads `view.children` and nothing else, so the cost is the node's own arity
/// and the file is never touched.
#[must_use]
pub fn examine(view: &ExpressionView) -> Vec<DiscardedResult> {
    // Condition 2, as a range: the body's statement positions, excluding the
    // last child, which is the body's value. `discarded_range` is the single
    // implementation of that predicate — `support::value_is_discarded` is
    // defined in terms of it rather than repeating it.
    let Some(range) = discarded_range(view) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for index in range {
        let Some(child) = view.children.get(index) else {
            break;
        };
        // A quoted statement in an otherwise evaluated body is data.
        if !child.reader_prefixes.is_empty() {
            continue;
        }
        let Some(variable) = destroyed_variable(child) else {
            continue;
        };
        // Condition 3: a later form in this same body reads the variable.
        let read_later = view
            .children
            .iter()
            .skip(index + 1)
            .any(|sibling| subtree_mentions(sibling, variable));
        if !read_later {
            continue;
        }
        found.push(DiscardedResult {
            span: child.span,
            variable: variable.to_owned(),
            head: list_head(child).unwrap_or_default().to_owned(),
        });
    }
    found
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// Stated rather than inherited: every destructive head here is a Common
    /// Lisp standard function, and `sort`/`nconc` mean different things in the
    /// Clojure and Scheme dialects this suite also lints.
    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::COMMON_LISP_ONLY
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        let found = examine(view);
        if found.is_empty() {
            return Ok(());
        }
        // `is_unevaluated_at` reaches `root_view()`, which materializes the
        // whole tree. It runs only once there is a finding to suppress — which
        // on correct code is never. See the module docstring on `HEADS`.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in found {
            sink.report(
                item.span,
                format!(
                    "`{head}` may reuse `{variable}`'s storage to build its result, so discarding \
                     the result leaves `{variable}` pointing into the old structure; bind the \
                     result instead, as in (setf {variable} ({head} {variable} …))",
                    head = item.head,
                    variable = item.variable,
                ),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::for_each_evaluated_subview;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    fn count(input: &str) -> usize {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let root = tree.root_view();
        let mut total = 0;
        for_each_evaluated_subview(&root, |view| {
            total += examine(view).len();
        });
        total
    }

    // -- the bug, in each of its spellings ---------------------------------

    #[test]
    fn flags_a_discarded_sort_in_a_defun_body() {
        assert_eq!(count("(defun report (xs) (sort xs #'<) (print xs))"), 1);
    }

    #[test]
    fn flags_the_classic_let_shape() {
        assert_eq!(count("(let ((xs (list 3 1 2))) (sort xs #'<) xs)"), 1);
    }

    #[test]
    fn flags_each_of_the_six_heads_once() {
        for (source, label) in [
            ("(defun f (xs) (sort xs #'<) xs)", "sort"),
            ("(defun f (xs) (stable-sort xs #'<) xs)", "stable-sort"),
            ("(defun f (xs ys) (nconc xs ys) xs)", "nconc"),
            ("(defun f (xs) (nbutlast xs) xs)", "nbutlast"),
            ("(defun f (al xs) (nsublis al xs) xs)", "nsublis"),
            ("(defun f (xs) (nsubst 1 2 xs) xs)", "nsubst"),
        ] {
            assert_eq!(count(source), 1, "{label} must fire exactly once");
        }
    }

    #[test]
    fn flags_a_discarded_call_in_every_body_form_it_claims() {
        for source in [
            "(progn (sort xs #'<) (print xs))",
            "(let ((a 1)) (sort xs #'<) (print xs))",
            "(let* ((a 1)) (sort xs #'<) (print xs))",
            "(when (p) (sort xs #'<) (print xs))",
            "(unless (p) (sort xs #'<) (print xs))",
            "(dolist (i is) (sort xs #'<) (print xs))",
            "(dotimes (i 10) (sort xs #'<) (print xs))",
            "(lambda (xs) (sort xs #'<) (print xs))",
            "(block b (sort xs #'<) (print xs))",
            "(defmethod m ((xs list)) (sort xs #'<) (print xs))",
        ] {
            assert_eq!(count(source), 1, "must fire in: {source}");
        }
    }

    // -- the correct idiom, which must never fire ---------------------------

    /// The single most important negative: this is how the code is *supposed*
    /// to be written, and a rule that reports it is unusable.
    #[test]
    fn does_not_flag_the_setf_idiom() {
        assert_eq!(
            count("(defun f (xs) (setf xs (sort xs #'<)) (print xs))"),
            0
        );
        assert_eq!(
            count("(let ((xs (list 3 1 2))) (setf xs (sort xs #'<)) xs)"),
            0
        );
        // Two places in one `setf`: the first `sort` is not the last child, and
        // is still safe, because `setf` is not a body form.
        assert_eq!(
            count("(defun f (xs ys) (setf xs (sort xs #'<) ys (sort ys #'>)) (list xs ys))"),
            0
        );
    }

    #[test]
    fn does_not_flag_a_result_that_is_used() {
        // Last form of the body: the value is the function's result.
        assert_eq!(count("(defun f (xs) (sort xs #'<))"), 0);
        // An argument to another call.
        assert_eq!(count("(defun f (xs) (print (sort xs #'<)) xs)"), 0);
        assert_eq!(count("(defun f (xs) (push (nconc xs ys) acc) xs)"), 0);
        assert_eq!(count("(defun f (xs) (return-from f (sort xs #'<)))"), 0);
        // Bound by a `let`.
        assert_eq!(
            count("(defun f (xs) (let ((s (sort xs #'<))) (print s)))"),
            0
        );
    }

    /// Condition 3. Without a later read the call is merely dead, and "dead"
    /// has too large an innocent population to report.
    #[test]
    fn does_not_flag_a_discarded_call_the_body_never_reads_again() {
        assert_eq!(count("(defun f (xs) (sort xs #'<) nil)"), 0);
        assert_eq!(count("(defun f (xs ys) (sort xs #'<) (print ys))"), 0);
    }

    /// Condition 1. A literal is `destructive-literal`'s finding, not this
    /// rule's, and a temporary is nobody's.
    #[test]
    fn does_not_flag_a_literal_or_a_temporary() {
        assert_eq!(count("(defun f () (sort '(3 1 2) #'<) (print xs))"), 0);
        assert_eq!(
            count("(defun f (xs) (sort (copy-list xs) #'<) (print xs))"),
            0
        );
    }

    /// Condition 1, against the *atom* literals specifically.
    ///
    /// Added because mutation testing showed that removing the `is_bare_symbol`
    /// check in `destroyed_variable` broke no test: a nested call is already
    /// rejected by `atom_text` returning `None`, so only an atom literal
    /// distinguishes the guard. Each of these is mentioned again by a later
    /// form, so condition 3 cannot be what declines them.
    #[test]
    fn does_not_treat_an_atom_literal_as_a_destroyed_variable() {
        for source in [
            "(defun f () (nbutlast 12) (print 12))",
            "(defun f () (nbutlast \"abc\") (print \"abc\"))",
            "(defun f () (nbutlast :key) (print :key))",
            "(defun f () (nbutlast nil) (print nil))",
            "(defun f () (nbutlast t) (print t))",
            "(defun f (xs) (nbutlast #'xs) (print #'xs))",
        ] {
            assert_eq!(count(source), 0, "a literal is not a variable: {source}");
        }
    }

    /// A quoted statement inside an otherwise evaluated body is data.
    ///
    /// Added because mutation testing showed the `reader_prefixes` check in
    /// `examine` killed no test: `list_head` reads straight through a quote
    /// prefix, so without it `'(sort xs #'<)` is indistinguishable from a call.
    #[test]
    fn does_not_flag_a_quoted_statement_inside_an_evaluated_body() {
        assert_eq!(count("(progn '(sort xs #'<) (print xs))"), 0);
        assert_eq!(count("(defun f (xs) `(sort xs #'<) (print xs))"), 0);
        // The control: the same body without the quote does fire.
        assert_eq!(count("(progn (sort xs #'<) (print xs))"), 1);
    }

    /// The heads SBCL already warns about are deliberately absent. This test is
    /// the head list's specification, not an accident of it.
    #[test]
    fn does_not_flag_the_heads_sbcl_already_warns_about() {
        for head in [
            "nreverse",
            "nreconc",
            "delete",
            "delete-if",
            "delete-if-not",
            "delete-duplicates",
            "nunion",
            "nintersection",
            "nset-difference",
            "nset-exclusive-or",
            "merge",
        ] {
            assert_eq!(
                count(&format!("(defun f (xs) ({head} xs) (print xs))")),
                0,
                "{head} is SBCL's job; see the module docstring"
            );
        }
    }

    /// The always-in-place heads, verified against SBCL to preserve identity.
    #[test]
    fn does_not_flag_the_heads_that_never_change_identity() {
        for head in [
            "nstring-downcase",
            "nstring-upcase",
            "nstring-capitalize",
            "replace",
            "nsubstitute",
        ] {
            assert_eq!(
                count(&format!("(defun f (xs) ({head} xs) (print xs))")),
                0,
                "{head} rewrites in place and returns the same object"
            );
        }
    }

    /// The ambiguous set. Not reported, rather than reported and wrong.
    #[test]
    fn does_not_flag_the_ambiguous_positions() {
        assert_eq!(count("(tagbody top (sort xs #'<) (print xs))"), 0);
        assert_eq!(count("(unwind-protect (sort xs #'<) (print xs))"), 0);
        assert_eq!(count("(prog1 (sort xs #'<) (print xs))"), 0);
        assert_eq!(count("(loop for i in is do (sort xs #'<) (print xs))"), 0);
        assert_eq!(count("(cond ((p) (sort xs #'<) (print xs)))"), 0);
    }

    // -- quoting -----------------------------------------------------------

    #[test]
    fn a_matching_form_inside_a_quote_is_data() {
        assert_eq!(count("'(progn (sort xs #'<) (print xs))"), 0);
        assert_eq!(count("(quote (progn (sort xs #'<) (print xs)))"), 0);
    }

    /// The two-counter model's reason for existing: a macro template that
    /// *writes* a discarded sort is building code, not running one.
    #[test]
    fn a_macro_template_is_data() {
        assert_eq!(
            count("(defmacro m (&body body) `(progn (sort xs #'<) (print xs) ,@body))"),
            0
        );
    }

    /// …and a comma re-enters code, where a real discarded call can sit.
    #[test]
    fn an_unquoted_form_inside_a_backquote_is_still_code() {
        assert_eq!(count("`(a ,(progn (sort xs #'<) (print xs)))"), 1);
    }

    #[test]
    fn a_matching_shape_inside_a_string_literal_is_not_a_form() {
        assert_eq!(
            count("(defun f (xs) (format t \"(sort xs #'<) xs\") xs)"),
            0
        );
    }

    // -- the finding itself -------------------------------------------------

    #[test]
    fn the_finding_points_at_the_call_and_names_the_variable() {
        let source = "(defun report (xs) (sort xs #'<) (print xs))";
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        let root = tree.root_view();
        // The rule anchors on the `defun`, not on the `sort`.
        let body_form = &root.children[0];
        let found = examine(body_form);
        assert_eq!(found.len(), 1, "the discarded sort is a finding");
        assert_eq!(
            &source[found[0].span.start().get()..found[0].span.end().get()],
            "(sort xs #'<)",
            "the finding points at the call, not at the enclosing defun"
        );
        assert_eq!(found[0].variable, "xs");
        assert_eq!(found[0].head, "sort");
    }

    /// Two defects in one body are two findings, so the per-body scan does not
    /// stop at the first.
    #[test]
    fn a_body_with_two_defects_reports_both() {
        assert_eq!(
            count("(defun f (xs ys) (sort xs #'<) (nconc ys xs) (list xs ys))"),
            2
        );
    }

    /// A package-qualified spelling is the same function.
    #[test]
    fn a_package_qualified_head_is_still_the_standard_function() {
        assert_eq!(count("(defun f (xs) (cl:sort xs #'<) (print xs))"), 1);
    }
}
