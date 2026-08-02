//! Common Lisp long-literal-`member` detection: a `(member x '(a b c d …))`
//! against a constant list of many distinct symbols.
//!
//! `member` walks its list, so a membership test against a fixed set of names
//! costs the length of that set on every call. Past a certain size the set is
//! better expressed as something that answers in constant time — a `case`, or a
//! hash table built once — and, just as much to the point, as something that
//! reads as a set rather than as a list to be searched.
//!
//! # The quote polarity, which is inverted here
//!
//! Every other rule in this package uses the quote machinery to *skip* data.
//! This one has to read it: `(member x '(a b c))` has its list quoted because
//! that is the correct way to write the call, and a rule that skipped quoted
//! nodes would never see the list at all. The two questions are asked at
//! different nodes and do not conflict:
//!
//! - the `member` **call** must be evaluated code — a `member` inside `'(…)` is
//!   a list of symbols, not a call, and the rule's `check` asks
//!   [`crate::support::is_unevaluated_at`] about exactly that node;
//! - its **list argument** must be data — read through
//!   [`crate::support::hard_quoted_list_children`], which accepts `'(a b c)` and
//!   `(quote (a b c))` and rejects `` `(a b ,c) `` (not a constant), `''(a b c)`
//!   (the two-element list `(quote (a b c))`), and `(list a b c)` (not a
//!   literal).
//!
//! # What is *not* reported, and why
//!
//! - **A short list.** Three or four names is completely normal and is what
//!   `member` is for. The threshold is [`MIN_ELEMENTS`], a `--rule-arg` knob,
//!   defaulting to eight — high enough that no ordinary two-or-three-way test
//!   is ever reported.
//! - **Anything but plain symbols.** A list containing a string, a number, a
//!   sublist or a reader conditional is left alone: `member` defaults to `eql`,
//!   which does not compare strings or lists usefully — that is
//!   `eql-search-literal`'s finding, not this one — and a `case` cannot be
//!   written over sublists.
//! - **A call with keyword arguments.** `(member x '(…) :test #'string=)` has
//!   semantics `case` does not share, so only the exact three-operand call is
//!   read.
//! - **A computed list.** Only a literal can be counted at all.
//!
//! # Overlap with `linear-search-in-loop`
//!
//! `paredit-feature-lint-performance`'s `linear-search-in-loop` reports a
//! `member` whose collection does not vary with an enclosing loop, for any
//! collection including a variable. This rule reports a `member` over a long
//! *literal*, in or out of a loop. A long literal inside a loop earns both, and
//! that is correct: they recommend different repairs (hoist into a hash table
//! versus rewrite the set itself) and neither subsumes the other.
//!
//! Report-only: `case` returns its clause's value where `member` returns the
//! tail it found, so the rewrite is only meaning-preserving where the result is
//! used as a boolean — a judgement about the caller, not about this form.
//!
//! Scope: Common Lisp only.

use std::collections::BTreeSet;
use std::path::Path;

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::model::RuleSetting;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_in, unqualified};
use serde_json::{Value, json};

use crate::support::{for_each_evaluated_subview, hard_quoted_list_children, is_symbol_atom};

/// The knob: how many distinct symbols a literal `member` list needs before the
/// rule speaks.
///
/// Eight by default. A two-, three- or four-way membership test is the ordinary
/// use of `member` and reporting one would be noise; by eight names the list has
/// stopped being an argument and started being a set. A project that disagrees
/// moves the number rather than turning the rule off.
pub const MIN_ELEMENTS: RuleSetting = RuleSetting::new(
    "min-elements",
    8,
    "how many distinct symbols a literal member list needs before it is reported",
);

#[derive(Debug, Clone)]
pub struct LinearScanItem {
    /// The span of the whole `(member x '(…))` form.
    pub span: ByteSpan,
    /// The span of the quoted list.
    pub list_span: ByteSpan,
    /// How many distinct symbols the list holds.
    pub distinct: usize,
}

impl Finding for LinearScanItem {
    fn kind(&self) -> &'static str {
        "set-membership-linear-scan"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.distinct.to_string()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("distinct", json!(self.distinct)),
            ("list_span", span_json(self.list_span)),
        ]
    }

    fn message(&self) -> String {
        message_for(self.distinct)
    }
}

/// The one sentence both the report and the lint rule phrase a finding with.
#[must_use]
pub fn message_for(distinct: usize) -> String {
    format!(
        "member scans a literal list of {distinct} distinct symbols on every call; \
         a case form or a hash table answers in constant time and reads as the set it is"
    )
}

fn span_json(span: ByteSpan) -> Value {
    json!({ "start": span.start().get(), "end": span.end().get() })
}

/// How many distinct symbols a quoted list holds, or `None` when it holds
/// anything that is not a plain symbol.
///
/// Names are compared unqualified and case-folded, the same way the reader and
/// `eql` see them, so `cl:car` and `CAR` are one symbol.
fn distinct_symbols(elements: &[ExpressionView]) -> Option<usize> {
    let mut names = BTreeSet::new();
    for element in elements {
        if !is_symbol_atom(element) {
            return None;
        }
        names.insert(unqualified(atom_text(element)?).to_ascii_lowercase());
    }
    Some(names.len())
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
///
/// `min_elements` is the caller's threshold — the rule passes what
/// `--rule-arg set-membership-via-linear-scan.min-elements=` resolved to, and
/// the report passes [`MIN_ELEMENTS`]'s default.
///
/// # Cost
///
/// `member` is far less dense than `car` or `nth`, and the predicates are still
/// ordered cheapest-first: one `children.len()` rejects every call with keyword
/// arguments, and the reader-prefix comparison inside
/// [`hard_quoted_list_children`] rejects every call whose list is a variable —
/// which between them is nearly every `member` in a real file. Only a call with
/// a literal list counts its elements, and that count is bounded by the
/// literal.
pub fn examine(
    view: &ExpressionView,
    min_elements: usize,
    member_form_count: &mut usize,
    violations: &mut Vec<LinearScanItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !symbol_in(head, &["member"]) {
        return;
    }
    *member_form_count += 1;

    // children: [member, item, list] — exactly, so a call carrying :test, :key
    // or :test-not is left alone.
    if view.children.len() != 3 {
        return;
    }
    let list = &view.children[2];
    let Some(elements) = hard_quoted_list_children(list) else {
        return;
    };
    // An early-out, *not* the threshold guard: the distinct count below can
    // only be smaller, so this decides nothing the next check would not. What
    // it saves is building the set at all for the short lists that are the
    // common case. Mutation-verified as redundant for correctness — removing it
    // fails no test, and the guard that does the work is the one below.
    if elements.len() < min_elements {
        return;
    }
    let Some(distinct) = distinct_symbols(elements) else {
        return;
    };
    // The threshold guard.
    if distinct < min_elements {
        return;
    }

    violations.push(LinearScanItem {
        span: view.span,
        list_span: list.span,
        distinct,
    });
}

/// Collects every long-literal `member` in one file, with the number of
/// `member` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no long literal membership tests here"
/// for Common Lisp and "nothing was looked for" for Clojure, and the two read
/// identically without the flag.
pub fn collect_linear_scans(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<LinearScanItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("member_form_count", json!(0))],
        ));
    }

    let threshold = usize::try_from(MIN_ELEMENTS.default()).unwrap_or(usize::MAX);
    let mut member_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_evaluated_subview(&view, |subview| {
            examine(subview, threshold, &mut member_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("member_form_count", json!(member_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::view_query::for_each_subview;

    const DEFAULT_MIN: usize = 8;

    fn report(input: &str) -> FileFindings<LinearScanItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_linear_scans(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect linear scans")
    }

    /// `examine` applied to every node of a source, which is what the lint rule
    /// sees through the dispatcher — quoting and all.
    fn examined_with(input: &str, min_elements: usize) -> Vec<LinearScanItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        let mut count = 0;
        let mut violations = Vec::new();
        for_each_subview(&tree.root_view(), |view| {
            examine(view, min_elements, &mut count, &mut violations);
        });
        violations
    }

    fn examined(input: &str) -> Vec<LinearScanItem> {
        examined_with(input, DEFAULT_MIN)
    }

    /// Eight distinct symbols, the shortest list the default threshold reports.
    const EIGHT: &str = "'(alpha beta gamma delta epsilon zeta eta theta)";

    // -- positives -----------------------------------------------------------

    #[test]
    fn flags_a_member_against_a_long_literal_list() {
        let violations = examined(&format!("(member x {EIGHT})"));
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].distinct, 8);
    }

    #[test]
    fn flags_the_long_hand_quote_form() {
        let violations =
            examined("(member x (quote (alpha beta gamma delta epsilon zeta eta theta)))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_keyword_and_uppercase_elements() {
        assert_eq!(examined("(member x '(:a :b :c :d :e :f :g :h))").len(), 1);
        assert_eq!(
            examined("(member x '(ALPHA BETA GAMMA DELTA EPSILON ZETA ETA THETA))").len(),
            1
        );
    }

    #[test]
    fn flags_a_package_qualified_member() {
        assert_eq!(examined(&format!("(cl:member x {EIGHT})")).len(), 1);
    }

    // -- the threshold -------------------------------------------------------

    /// The rule's main false-positive guard: a three-element `member` is
    /// completely normal and must never be reported.
    #[test]
    fn does_not_flag_a_short_list() {
        assert!(examined("(member x '(a b c))").is_empty());
        assert!(examined("(member x '(a b c d e f g))").is_empty());
    }

    #[test]
    fn the_threshold_is_the_boundary_it_says_it_is() {
        let seven = "(member x '(a b c d e f g))";
        let eight = format!("(member x {EIGHT})");
        assert!(examined_with(seven, 8).is_empty());
        assert_eq!(examined_with(&eight, 8).len(), 1);
        // Lowered, the seven-element call is reported; raised, the eight is not.
        assert_eq!(examined_with(seven, 7).len(), 1);
        assert!(examined_with(&eight, 9).is_empty());
    }

    /// Duplicates are counted once: eight names of which two repeat is a
    /// seven-name set and stays under the default.
    #[test]
    fn duplicates_count_once() {
        let violations = examined("(member x '(a b c d e f g a))");
        assert!(violations.is_empty());
        assert_eq!(examined("(member x '(a b c d e f g h a))")[0].distinct, 8);
    }

    #[test]
    fn the_declared_default_is_the_one_the_documentation_states() {
        assert_eq!(MIN_ELEMENTS.default(), 8);
        assert_eq!(MIN_ELEMENTS.key(), "min-elements");
    }

    // -- near-miss negatives -------------------------------------------------

    #[test]
    fn does_not_flag_a_call_with_keyword_arguments() {
        assert!(examined(&format!("(member x {EIGHT} :test #'string=)")).is_empty());
        assert!(examined(&format!("(member x {EIGHT} :key #'car)")).is_empty());
    }

    #[test]
    fn does_not_flag_a_computed_or_variable_list() {
        assert!(examined("(member x known-names)").is_empty());
        assert!(examined("(member x (list 'a 'b 'c 'd 'e 'f 'g 'h))").is_empty());
    }

    /// `eql` does not compare strings or lists usefully; that is
    /// `eql-search-literal`'s finding, and a `case` cannot be written over
    /// sublists anyway.
    #[test]
    fn does_not_flag_a_list_that_is_not_all_symbols() {
        assert!(examined("(member x '(a b c d e f g \"h\"))").is_empty());
        assert!(examined("(member x '(a b c d e f g 8))").is_empty());
        assert!(examined("(member x '(a b c d e f g (h i)))").is_empty());
        assert!(examined("(member x '(a b c d e f g #+sbcl h))").is_empty());
    }

    #[test]
    fn does_not_flag_a_quasiquoted_list() {
        assert!(examined("(member x `(alpha beta gamma delta epsilon zeta eta theta))").is_empty());
        assert!(
            examined("(member x `(alpha beta gamma delta epsilon zeta eta ,theta))").is_empty()
        );
    }

    /// `''(a b …)` is the two-element list `(quote (a b …))`.
    #[test]
    fn does_not_flag_a_doubly_quoted_list() {
        assert!(examined(&format!("(member x '{EIGHT})")).is_empty());
    }

    #[test]
    fn does_not_flag_a_different_search_operator() {
        assert!(examined(&format!("(find x {EIGHT})")).is_empty());
        assert!(examined(&format!("(member-if #'oddp {EIGHT})")).is_empty());
        assert!(examined(&format!("(position x {EIGHT})")).is_empty());
    }

    #[test]
    fn does_not_flag_a_wrong_arity_member() {
        assert!(examined("(member x)").is_empty());
        assert!(examined("(member)").is_empty());
    }

    // -- quoting and strings, through the report path ------------------------

    /// The call itself being data is a different question from its argument
    /// being data, and this is the one that suppresses.
    #[test]
    fn the_report_skips_the_five_quote_shapes() {
        for source in [
            format!("'(member x {EIGHT})"),
            format!("(quote (member x {EIGHT}))"),
            format!("`(member x {EIGHT})"),
            format!("'(a ,(member x {EIGHT}))"),
            format!("'(outer (member x {EIGHT}))"),
        ] {
            assert!(
                report(&source).findings.is_empty(),
                "{source} is quoted data"
            );
        }
    }

    #[test]
    fn an_unquote_inside_a_backquote_is_code_again() {
        assert_eq!(
            report(&format!("`(a ,(member x {EIGHT}))")).findings.len(),
            1
        );
    }

    #[test]
    fn a_call_inside_a_string_literal_is_not_a_form() {
        assert!(
            report("(format nil \"(member x '(a b c d e f g h))\")")
                .findings
                .is_empty()
        );
    }

    // -- report envelope -----------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(member x '(a b))", Dialect::Clojure).expect("parse");
        let report =
            collect_linear_scans(Path::new("app.clj"), Dialect::Clojure, &tree).expect("collect");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn the_summary_counts_every_member_scanned_not_only_the_flagged_ones() {
        let report = report(&format!("(member x {EIGHT})\n(member y '(a b))\n"));
        assert_eq!(report.summary, vec![("member_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_list_span() {
        let source = format!("(defun f (x)\n  (member x {EIGHT}))\n");
        let report = report(&source);
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "set-membership-linear-scan");
        assert_eq!(finding.text_columns(), vec!["8".to_owned()]);
        assert_eq!(
            finding.json_fields(),
            vec![
                ("distinct", json!(8)),
                ("list_span", span_json(finding.list_span)),
            ]
        );
        assert!(finding.message().contains("8 distinct symbols"));
    }
}
