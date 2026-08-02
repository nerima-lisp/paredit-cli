//! A `cond` or `if` chain that dispatches on string equality against a set of
//! short, identifier-shaped literals.
//!
//! ```lisp
//! (cond ((string= mode "read")   …)
//!       ((string= mode "write")  …)
//!       ((string= mode "append") …)
//!       ((string= mode "update") …))
//! ```
//!
//! The string set here is an enumeration written as text. Nothing checks it: a
//! misspelt `"raed"` compiles, reads fine, and silently falls through. Written
//! as keywords the same dispatch becomes a `case`, which is a single lookup, and
//! a typo becomes a clause that is visibly never taken.
//!
//! # What has to be true before this reports
//!
//! All four, deliberately:
//!
//! 1. **One subject.** Every counted branch compares the *same* expression,
//!    matched by its exact source text. `(cond ((string= a "x") …)
//!    ((string= b "y") …))` is three unrelated comparisons, not a dispatch, and
//!    is never reported however many there are.
//! 2. **Identifier-shaped literals.** A literal with a space, a format
//!    directive, or any punctuation outside `- _ . / : +` does not count as a
//!    branch. Comparing against `"no such file"` is a string comparison; the
//!    rule is about strings standing in for symbols.
//! 3. **Distinct literals.** A repeated literal is a different defect —
//!    a duplicate test — and is not this rule's subject.
//! 4. **At least `min-branches` of them**, four by default. Three string
//!    comparisons in a row are ordinary; the default is set where the set stops
//!    reading as comparisons and starts reading as an enumeration.
//!
//! # What this rule does not attempt
//!
//! - It cannot see where the value came from. A dispatch on strings that
//!   genuinely arrive as strings — command-line arguments, HTTP headers, file
//!   extensions — is still reported, because the remedy (intern once at the
//!   boundary, dispatch on the keyword) is the same one. That is a judgement,
//!   which is why the rule is tagged `pedantic` and its threshold is tunable.
//! - It reads `string=` and `string-equal` only. `equal`, `equalp` and `eql`
//!   are not string comparisons in general, and treating them as such would
//!   report every `(equal x "…")` in the file.
//! - For an `if` chain it reports the *outermost* `if` of the chain, once.
//! - Scope is Common Lisp and Emacs Lisp. Both spell `cond`, `if`, `string=`
//!   and `string-equal` the same way and give them the same meaning. Clojure's
//!   `=` on strings and Scheme's `string=?` are different enough spellings that
//!   including them would be a guess.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_in};
use serde_json::{Value, json};

use crate::support::{
    calls_any, descend_to, descent_is_unevaluated, root_child_containing, string_literal_contents,
};

/// How many same-subject string branches a form may carry before it reads as an
/// enumeration.
///
/// Four, not three. Three string comparisons in a row are ordinary code; the
/// default is set where a false positive costs more than a missed finding.
pub const DEFAULT_MIN_BRANCHES: usize = 4;

/// The dialects this rule models.
pub const MODELLED_DIALECTS: [Dialect; 2] = [Dialect::CommonLisp, Dialect::EmacsLisp];

/// The two string equality predicates, and only those. `equal`/`equalp`/`eql`
/// are not string comparisons in general.
const STRING_EQUALITY: &[&str] = &["string=", "string-equal"];

/// One reported dispatch.
#[derive(Debug, Clone)]
pub struct StringlyTypedDispatchItem {
    /// The span of the whole `cond`, or of the outermost `if` of the chain.
    pub span: ByteSpan,
    /// `"cond"` or `"if"`.
    pub form: &'static str,
    /// The source text of the expression every branch compares.
    pub subject: String,
    /// How many branches compare it against a distinct identifier-shaped
    /// literal.
    pub branch_count: usize,
    /// The count this run required.
    pub threshold: usize,
}

impl Finding for StringlyTypedDispatchItem {
    fn kind(&self) -> &'static str {
        "stringly-typed-dispatch"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("branch_count={}", self.branch_count)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("form", json!(self.form)),
            ("subject", json!(self.subject)),
            ("branch_count", json!(self.branch_count)),
            ("threshold", json!(self.threshold)),
        ]
    }

    fn message(&self) -> String {
        message(self.form, &self.subject, self.branch_count)
    }
}

/// The one sentence both the report and the lint rule print.
#[must_use]
pub fn message(form: &str, subject: &str, branch_count: usize) -> String {
    format!(
        "{form} dispatches on {branch_count} string literals compared against `{subject}`; the \
         set reads as an enumeration, which a keyword and `case` express without a typo being \
         silent"
    )
}

/// Whether a literal reads as a member of an enumeration rather than as text.
///
/// No whitespace, nothing longer than a symbol would be, and no punctuation
/// beyond what appears in real keys (`text/plain`, `on-error`, `x.y`, `a:b`).
#[must_use]
pub fn is_identifier_like(literal: &str) -> bool {
    !literal.is_empty()
        && literal.len() <= 32
        && literal.chars().any(char::is_alphanumeric)
        && literal.chars().all(|character| {
            character.is_alphanumeric() || matches!(character, '-' | '_' | '.' | '/' | ':' | '+')
        })
}

/// One branch of a dispatch: the subject compared, and the literal it is
/// compared against.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Branch {
    subject: String,
    literal: String,
}

/// Reads `(string= SUBJECT "literal")` in either argument order.
///
/// `None` for anything else, including a comparison of two literals, a
/// comparison of two variables, a `string=` with `:start`/`:end` keyword
/// arguments, and a literal that is not identifier-shaped.
fn string_equality_branch(test: &ExpressionView, source: &str) -> Option<Branch> {
    if !is_paren_list(test) || test.children.len() != 3 {
        return None;
    }
    if !list_head(test).is_some_and(|head| symbol_in(head, STRING_EQUALITY)) {
        return None;
    }
    let (left, right) = (&test.children[1], &test.children[2]);
    let (subject, literal) = match (
        string_literal_contents(left),
        string_literal_contents(right),
    ) {
        (None, Some(literal)) => (left, literal),
        (Some(literal), None) => (right, literal),
        _ => return None,
    };
    if !is_identifier_like(literal) {
        return None;
    }
    Some(Branch {
        subject: subject.span.slice(source).to_owned(),
        literal: literal.to_owned(),
    })
}

/// The largest set of branches in `branches` that share one subject and use
/// distinct literals.
fn dominant_subject(branches: &[Branch]) -> Option<(String, usize)> {
    let mut best: Option<(String, usize)> = None;
    for candidate in branches {
        let mut literals: Vec<&str> = Vec::new();
        for branch in branches {
            if branch.subject == candidate.subject && !literals.contains(&branch.literal.as_str()) {
                literals.push(&branch.literal);
            }
        }
        let count = literals.len();
        if best
            .as_ref()
            .is_none_or(|(_, best_count)| count > *best_count)
        {
            best = Some((candidate.subject.clone(), count));
        }
    }
    best
}

/// The branches of a `(cond …)`: each clause's test, when it is a string
/// comparison.
fn cond_branches(view: &ExpressionView, source: &str) -> Vec<Branch> {
    view.children
        .iter()
        .skip(1)
        .filter(|clause| is_paren_list(clause))
        .filter_map(|clause| clause.children.first())
        .filter_map(|test| string_equality_branch(test, source))
        .collect()
}

/// The branches of an `(if TEST THEN ELSE)` chain, following the else branch
/// for as long as it is another two-armed `if` comparing *the same subject*
/// against a string.
///
/// Three things stop the chain, and each of them matters:
///
/// - a node that is not an `if` with exactly four children — a `cond`-style
///   multi-form Emacs Lisp else is not a chain link, and a Common Lisp `if`
///   cannot have one;
/// - a test that is not a string comparison at all;
/// - a test on a *different* subject, which starts a second, unrelated
///   dispatch. Following through one would make every link of the outer chain
///   inherit the inner chain's branches and report it again at each level.
fn if_chain_branches(view: &ExpressionView, source: &str) -> Vec<Branch> {
    let mut branches: Vec<Branch> = Vec::new();
    let mut current = view;
    loop {
        if current.children.len() != 4 || !calls_any(current, &["if"]) {
            return branches;
        }
        let Some(branch) = string_equality_branch(&current.children[1], source) else {
            return branches;
        };
        if branches
            .first()
            .is_some_and(|first| first.subject != branch.subject)
        {
            return branches;
        }
        branches.push(branch);
        current = &current.children[3];
    }
}

/// What the descent to one form says about it.
#[derive(Debug, Clone)]
struct FormContext {
    unevaluated: bool,
    /// The enclosing `if`'s test, when this form is that `if`'s else branch.
    enclosing_if_test: Option<Branch>,
}

fn form_context_at(tree: &SyntaxTree, target: ByteSpan, source: &str) -> Option<FormContext> {
    let top_level = root_child_containing(tree, target)?;
    let steps = descend_to(&top_level, target);
    if steps.last()?.view.span != target {
        return None;
    }
    let enclosing_if_test = steps
        .len()
        .checked_sub(2)
        .map(|index| &steps[index])
        .filter(|parent| parent.next_child == Some(3) && calls_any(parent.view, &["if"]))
        .and_then(|parent| parent.view.children.get(1))
        .and_then(|test| string_equality_branch(test, source));

    Some(FormContext {
        unevaluated: descent_is_unevaluated(&steps),
        enclosing_if_test,
    })
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// `cond` and `if` through the single dispatch pass instead of walking the tree
/// again.
pub fn examine_dispatch(
    tree: &SyntaxTree,
    view: &ExpressionView,
    min_branches: usize,
    dispatch_form_count: &mut usize,
    violations: &mut Vec<StringlyTypedDispatchItem>,
) {
    let source = tree.source();
    let Some(head) = list_head(view).filter(|_| is_paren_list(view)) else {
        return;
    };
    let form = if symbol_in(head, &["cond"]) {
        "cond"
    } else if symbol_in(head, &["if"]) {
        "if"
    } else {
        return;
    };
    *dispatch_form_count += 1;

    let branches = if form == "cond" {
        cond_branches(view, source)
    } else {
        if_chain_branches(view, source)
    };
    let Some((subject, branch_count)) = dominant_subject(&branches) else {
        return;
    };
    if branch_count < min_branches {
        return;
    }

    // Only now, once the form is otherwise reportable, is the descent worth
    // paying for. It answers two questions at once: whether this is quoted
    // data, and — for an `if` — whether an enclosing `if` already reports the
    // same chain.
    let Some(context) = form_context_at(tree, view.span, source) else {
        return;
    };
    if context.unevaluated {
        return;
    }
    if form == "if"
        && context
            .enclosing_if_test
            .is_some_and(|test| test.subject == subject)
    {
        return;
    }

    violations.push(StringlyTypedDispatchItem {
        span: view.span,
        form,
        subject,
        branch_count,
        threshold: min_branches,
    });
}

/// Collects every stringly-typed dispatch in one file, with the number of
/// `cond`/`if` forms scanned as the denominator beside them.
pub fn build_stringly_typed_dispatch_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<StringlyTypedDispatchItem>> {
    build_report_with_threshold(path, dialect, tree, DEFAULT_MIN_BRANCHES)
}

/// [`build_stringly_typed_dispatch_report`] at a caller-chosen threshold.
pub fn build_report_with_threshold(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
    min_branches: usize,
) -> LintResult<FileFindings<StringlyTypedDispatchItem>> {
    if !MODELLED_DIALECTS.contains(&dialect) {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("dispatch_form_count", json!(0))],
        ));
    }

    let mut dispatch_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        paredit_core_syntax::view_query::for_each_subview(&view, |subview| {
            examine_dispatch(
                tree,
                subview,
                min_branches,
                &mut dispatch_form_count,
                &mut violations,
            );
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("dispatch_form_count", json!(dispatch_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report_in(input: &str, dialect: Dialect) -> FileFindings<StringlyTypedDispatchItem> {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse input");
        build_stringly_typed_dispatch_report(Path::new("test.lisp"), dialect, &tree)
            .expect("build report")
    }

    fn findings(input: &str) -> Vec<StringlyTypedDispatchItem> {
        report_in(input, Dialect::CommonLisp).findings
    }

    const FOUR_WAY_COND: &str = "(cond ((string= mode \"read\") 1)\n      ((string= mode \"write\") 2)\n      ((string= mode \"append\") 3)\n      ((string= mode \"update\") 4)\n      (t nil))";

    // -- positives -----------------------------------------------------------

    #[test]
    fn flags_a_four_way_string_cond() {
        let items = findings(FOUR_WAY_COND);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].form, "cond");
        assert_eq!(items[0].subject, "mode");
        assert_eq!(items[0].branch_count, 4);
    }

    #[test]
    fn flags_a_four_way_string_if_chain_at_the_outermost_if() {
        let source = "(if (string= k \"a\") 1 (if (string= k \"b\") 2 (if (string= k \"c\") 3 (if (string= k \"d\") 4 nil))))";
        let items = findings(source);
        assert_eq!(items.len(), 1, "one chain is one finding");
        assert_eq!(items[0].form, "if");
        assert_eq!(items[0].branch_count, 4);
        assert_eq!(items[0].span.slice(source), source);
    }

    /// The outermost-`if` guard, exercised where it actually bites. A chain of
    /// exactly the threshold cannot show it — the second link is one branch
    /// short and would say nothing anyway. A chain one longer can: without the
    /// guard the outer `if` reports five branches and the second reports four,
    /// which is the same dispatch counted twice.
    #[test]
    fn a_chain_longer_than_the_threshold_is_still_one_finding() {
        let source = "(if (string= k \"a\") 1 (if (string= k \"b\") 2 (if (string= k \"c\") 3 (if (string= k \"d\") 4 (if (string= k \"e\") 5 nil)))))";
        let items = findings(source);
        assert_eq!(items.len(), 1, "one chain is one finding, however long");
        assert_eq!(items[0].branch_count, 5);
        assert_eq!(items[0].span.slice(source), source);
    }

    /// The guard is about *this* chain: an inner chain whose subject differs
    /// is a second dispatch and is reported on its own.
    #[test]
    fn an_inner_chain_on_a_different_subject_is_its_own_finding() {
        let inner = "(if (string= j \"p\") 1 (if (string= j \"q\") 2 (if (string= j \"r\") 3 (if (string= j \"s\") 4 nil))))";
        let source = format!(
            "(if (string= k \"a\") 1 (if (string= k \"b\") 2 (if (string= k \"c\") 3 (if (string= k \"d\") 4 {inner}))))"
        );
        let items = findings(&source);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].subject, "k");
        assert_eq!(items[1].subject, "j");
    }

    #[test]
    fn string_equal_counts_as_well_as_string_eq() {
        assert_eq!(
            findings(
                "(cond ((string-equal m \"a\") 1) ((string-equal m \"b\") 2) ((string-equal m \"c\") 3) ((string-equal m \"d\") 4))"
            )
            .len(),
            1
        );
    }

    #[test]
    fn the_literal_may_be_on_either_side() {
        assert_eq!(
            findings(
                "(cond ((string= \"a\" m) 1) ((string= m \"b\") 2) ((string= \"c\" m) 3) ((string= m \"d\") 4))"
            )
            .len(),
            1
        );
    }

    #[test]
    fn a_compound_subject_is_matched_by_its_source_text() {
        let items = findings(
            "(cond ((string= (mime-type req) \"text/html\") 1)\n      ((string= (mime-type req) \"text/css\") 2)\n      ((string= (mime-type req) \"text/plain\") 3)\n      ((string= (mime-type req) \"application/json\") 4))",
        );
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].subject, "(mime-type req)");
    }

    // -- near-miss negatives -------------------------------------------------

    #[test]
    fn three_branches_are_below_the_default_threshold() {
        assert!(
            findings("(cond ((string= m \"a\") 1) ((string= m \"b\") 2) ((string= m \"c\") 3))")
                .is_empty()
        );
    }

    /// Genuinely three unrelated string comparisons.
    #[test]
    fn branches_comparing_different_subjects_are_not_a_dispatch() {
        assert!(
            findings(
                "(cond ((string= a \"x\") 1) ((string= b \"y\") 2) ((string= c \"z\") 3) ((string= d \"w\") 4))"
            )
            .is_empty()
        );
    }

    /// Genuinely a string comparison: the literals are text, not names.
    #[test]
    fn branches_comparing_against_prose_are_not_a_dispatch() {
        assert!(
            findings(
                "(cond ((string= msg \"no such file\") 1)\n      ((string= msg \"permission denied\") 2)\n      ((string= msg \"is a directory\") 3)\n      ((string= msg \"too many open files\") 4))"
            )
            .is_empty()
        );
    }

    #[test]
    fn a_repeated_literal_does_not_count_twice() {
        assert!(
            findings(
                "(cond ((string= m \"a\") 1) ((string= m \"a\") 2) ((string= m \"a\") 3) ((string= m \"a\") 4))"
            )
            .is_empty()
        );
    }

    #[test]
    fn comparing_two_literals_or_two_variables_is_not_a_branch() {
        assert!(
            findings(
                "(cond ((string= \"a\" \"b\") 1) ((string= x y) 2) ((string= \"c\" \"d\") 3) ((string= p q) 4))"
            )
            .is_empty()
        );
    }

    #[test]
    fn a_string_comparison_with_keyword_arguments_is_not_a_branch() {
        assert!(
            findings(
                "(cond ((string= m \"a\" :start1 0) 1) ((string= m \"b\" :start1 0) 2) ((string= m \"c\" :start1 0) 3) ((string= m \"d\" :start1 0) 4))"
            )
            .is_empty()
        );
    }

    #[test]
    fn equal_and_eql_are_not_read_as_string_comparisons() {
        assert!(
            findings(
                "(cond ((equal m \"a\") 1) ((equal m \"b\") 2) ((equal m \"c\") 3) ((equal m \"d\") 4))"
            )
            .is_empty()
        );
    }

    #[test]
    fn a_cond_on_something_other_than_strings_is_untouched() {
        assert!(
            findings("(cond ((null x) 1) ((zerop x) 2) ((plusp x) 3) ((minusp x) 4) (t 5))")
                .is_empty()
        );
    }

    #[test]
    fn an_if_chain_whose_else_is_a_multi_form_body_is_not_a_chain() {
        // Five children: not a two-armed `if`, so the chain stops.
        assert!(findings("(if (string= k \"a\") 1 (progn 2) (progn 3))").is_empty());
    }

    /// A realistic, correct file.
    #[test]
    fn idiomatic_code_is_silent() {
        let source = "(defun describe-mode (mode)\n  (case mode\n    (:read \"reading\")\n    (:write \"writing\")\n    (:append \"appending\")\n    (t \"unknown\")))\n\n\
             (defun trimmed-equal-p (a b)\n  (string= (string-trim \" \" a) (string-trim \" \" b)))\n\n\
             (defun classify (line)\n  (cond ((zerop (length line)) :blank)\n        ((char= (char line 0) #\\;) :comment)\n        (t :code)))\n";
        assert!(findings(source).is_empty());
    }

    // -- the five quote shapes ----------------------------------------------

    #[test]
    fn a_hard_quoted_dispatch_is_data() {
        assert!(findings(&format!("'{FOUR_WAY_COND}")).is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_data() {
        assert!(findings(&format!("(quote {FOUR_WAY_COND})")).is_empty());
    }

    #[test]
    fn a_quasiquoted_dispatch_without_an_unquote_is_data() {
        assert!(findings(&format!("`{FOUR_WAY_COND}")).is_empty());
    }

    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(findings(&format!("'(x ,{FOUR_WAY_COND})")).is_empty());
    }

    #[test]
    fn an_unquoted_dispatch_inside_a_quasiquote_is_code_again() {
        assert_eq!(findings(&format!("`(x ,{FOUR_WAY_COND})")).len(), 1);
    }

    #[test]
    fn a_dispatch_spelled_only_inside_a_string_is_never_a_form() {
        assert!(findings("(format nil \"(cond ((string= m \\\"a\\\") 1))\")").is_empty());
    }

    // -- thresholds, dialects, denominators ----------------------------------

    #[test]
    fn the_threshold_moves_what_is_reported() {
        let source = "(cond ((string= m \"a\") 1) ((string= m \"b\") 2) ((string= m \"c\") 3))";
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        let strict =
            build_report_with_threshold(Path::new("t.lisp"), Dialect::CommonLisp, &tree, 3)
                .expect("report");
        assert_eq!(strict.findings.len(), 1);
        let lenient =
            build_report_with_threshold(Path::new("t.lisp"), Dialect::CommonLisp, &tree, 4)
                .expect("report");
        assert!(lenient.findings.is_empty());
    }

    #[test]
    fn emacs_lisp_is_modelled() {
        let report = report_in(FOUR_WAY_COND, Dialect::EmacsLisp);
        assert!(report.dialect_modelled);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_dialect_this_rule_does_not_model_is_reported_as_unmodelled() {
        let report = report_in("(cond (= x \"a\") 1)", Dialect::Clojure);
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn the_summary_counts_every_cond_and_if_scanned() {
        let report = report_in(
            "(if a 1 2)\n(cond (x 1))\n(if b 3 4)\n",
            Dialect::CommonLisp,
        );
        assert_eq!(report.summary, vec![("dispatch_form_count", json!(3))]);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn a_finding_carries_its_line_and_its_branch_count() {
        let report = report_in(
            &format!("(defun f (mode)\n  {FOUR_WAY_COND})\n"),
            Dialect::CommonLisp,
        );
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "stringly-typed-dispatch");
        assert_eq!(finding.text_columns(), vec!["branch_count=4".to_owned()]);
        assert!(finding.message().contains("4 string literals"));
    }

    #[test]
    fn identifier_like_accepts_names_and_rejects_prose() {
        assert!(is_identifier_like("read"));
        assert!(is_identifier_like("text/plain"));
        assert!(is_identifier_like("on-error"));
        assert!(is_identifier_like("x.y"));
        assert!(is_identifier_like("ns:name"));
        assert!(is_identifier_like("utf-8"));
        assert!(!is_identifier_like(""));
        assert!(!is_identifier_like("no such file"));
        assert!(!is_identifier_like("~a items"));
        assert!(!is_identifier_like("---"));
        assert!(!is_identifier_like(
            "a-very-long-string-that-is-clearly-prose-and-not-a-name"
        ));
    }
}
