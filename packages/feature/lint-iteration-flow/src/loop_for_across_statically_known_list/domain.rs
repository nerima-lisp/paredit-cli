//! Common Lisp `loop for … across` detection: iterating with `across` over a
//! value that is provably a list.
//!
//! `across` is the `loop` sub-clause for **vectors**. CLHS requires its
//! operand to be a vector, and every implementation signals a type error at
//! run time when handed a list. Because the mistake is a one-word swap
//! (`across` for `in`), it survives review easily and only shows up when the
//! branch runs.
//!
//! # What this rule recognises
//!
//! `across` whose operand is provably a list, by one of three static facts:
//!
//! - a **quoted list literal** written in place — `across '(1 2 3)`;
//! - a **list constructor** called in place — `across (list a b)`, and
//!   likewise `list*`, `cons`, `make-list`, `append`, `nconc`, `copy-list`;
//! - a **variable bound in the same `loop`** by `with v = <list>` or
//!   `for v = <list>`, where `<list>` is one of the two above.
//!
//! # What this rule deliberately does not flag
//!
//! - **A value bound outside the `loop`.** Proving the type of a `let` or
//!   parameter binding is the semantic layer's job, and reaching for it costs
//!   a whole-file pass per rule; this rule stays inside the one form it was
//!   handed.
//! - **`for v = <list> then <step>`.** The step form runs from the second
//!   iteration on and may well produce a vector, so the initial value proves
//!   nothing.
//! - **`nil` and `'()`.** `(loop for x across nil)` is also a type error, but
//!   `nil` is idiomatic as an "empty, filled in later" placeholder and the
//!   warning would land on code whose author already knows.
//! - **Anything in a form [`crate::loop_syntax`] declines to read.**
//!
//! A string operand is correct code — a string *is* a vector — and is never
//! flagged.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::expression_equality::render_expression;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{is_paren_list, list_head, unqualified};
use serde_json::{Value, json};

use crate::loop_syntax::{LoopScan, is_variable_clause_keyword};
use crate::support::for_each_evaluated_subview;

/// Operators whose result is always a list (or, for `append`/`nconc` with a
/// non-list tail, at least never a vector).
const LIST_CONSTRUCTORS: [&str; 7] = [
    "list",
    "list*",
    "cons",
    "make-list",
    "append",
    "nconc",
    "copy-list",
];

/// Why this rule believes the operand is a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListEvidence {
    /// `across '(1 2 3)`.
    QuotedListLiteral,
    /// `across (list a b)`.
    ListConstructor,
    /// `across v`, where the same `loop` bound `v` to a list.
    BoundInThisLoop,
}

impl ListEvidence {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::QuotedListLiteral => "quoted-list-literal",
            Self::ListConstructor => "list-constructor",
            Self::BoundInThisLoop => "bound-to-a-list-in-this-loop",
        }
    }

    const fn phrase(self) -> &'static str {
        match self {
            Self::QuotedListLiteral => "a quoted list literal",
            Self::ListConstructor => "a list constructor",
            Self::BoundInThisLoop => "bound to a list by this same loop",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoopForAcrossListItem {
    /// The span of the `across` operand.
    pub span: ByteSpan,
    /// That operand, re-rendered from the tree.
    pub sequence: String,
    pub evidence: ListEvidence,
}

impl Finding for LoopForAcrossListItem {
    fn kind(&self) -> &'static str {
        "loop-for-across-statically-known-list"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("sequence={}", self.sequence),
            format!("evidence={}", self.evidence.as_str()),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("sequence", json!(self.sequence)),
            ("evidence", json!(self.evidence.as_str())),
        ]
    }

    fn message(&self) -> String {
        format!(
            "loop `across` needs a vector, but {} is {}; use `in` to walk a list",
            self.sequence,
            self.evidence.phrase()
        )
    }
}

/// The static evidence that `view` is a list, if there is any.
fn list_evidence(view: &ExpressionView) -> Option<ListEvidence> {
    if !is_paren_list(view) {
        return None;
    }
    if view.reader_prefixes == [ReaderPrefix::Quote] {
        return Some(ListEvidence::QuotedListLiteral);
    }
    if !view.reader_prefixes.is_empty() {
        return None;
    }
    let head = unqualified(list_head(view)?).to_ascii_lowercase();
    LIST_CONSTRUCTORS
        .contains(&head.as_str())
        .then_some(ListEvidence::ListConstructor)
}

pub fn examine_loop_for_across(
    view: &ExpressionView,
    scan: &mut LoopScan,
    violations: &mut Vec<LoopForAcrossListItem>,
) {
    let Some(clauses) = scan.read(view) else {
        return;
    };

    // Pass 1: `with v = <list>` / `for v = <list>`, with no `then` step. A
    // loop variable's scope is the whole loop, so an `across` above the
    // binding still sees it.
    let mut list_variables: Vec<String> = Vec::new();
    for (position, token) in clauses.tokens.iter().enumerate() {
        if !token.keyword().is_some_and(is_variable_clause_keyword) {
            continue;
        }
        let Some(name) = clauses
            .symbol_after(position)
            .and_then(|token| token.operand_symbol().map(str::to_owned))
        else {
            continue;
        };
        if clauses
            .token(position + 2)
            .and_then(|token| token.keyword())
            != Some("=")
        {
            continue;
        }
        // A `then` step form can produce anything from the second pass on.
        if clauses
            .token(position + 4)
            .and_then(|token| token.keyword())
            == Some("then")
        {
            continue;
        }
        let Some(value) = clauses.token(position + 3) else {
            continue;
        };
        if list_evidence(value.view).is_some() {
            list_variables.push(name);
        }
    }

    // Pass 2: every `across` operand.
    for (position, token) in clauses.tokens.iter().enumerate() {
        if token.keyword() != Some("across") {
            continue;
        }
        let Some(operand) = clauses.token(position + 1) else {
            continue;
        };
        let evidence = list_evidence(operand.view).or_else(|| {
            let symbol = operand.operand_symbol()?;
            list_variables
                .iter()
                .any(|name| name == symbol)
                .then_some(ListEvidence::BoundInThisLoop)
        });
        if let Some(evidence) = evidence {
            violations.push(LoopForAcrossListItem {
                span: operand.view.span,
                sequence: render_expression(operand.view),
                evidence,
            });
        }
    }
}

/// Collects every `across` over a provable list in one file, with the number
/// of `loop` forms scanned and the number actually modelled beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_loop_for_across_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<LoopForAcrossListItem>> {
    let mut scan = LoopScan::default();
    let mut violations = Vec::new();

    if dialect == Dialect::CommonLisp {
        for index in 0..tree.root_children().len() {
            let view = tree.select_path(&SexprPath::root_child(index))?.view();
            for_each_evaluated_subview(&view, |subview| {
                examine_loop_for_across(subview, &mut scan, &mut violations);
            });
        }
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect == Dialect::CommonLisp,
        tree.source(),
        violations,
        vec![
            ("loop_form_count", json!(scan.loop_form_count)),
            (
                "modelled_loop_form_count",
                json!(scan.modelled_loop_form_count),
            ),
        ],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<LoopForAcrossListItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_loop_for_across_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build loop for across report")
    }

    fn violations(input: &str) -> Vec<LoopForAcrossListItem> {
        report(input).findings
    }

    #[test]
    fn flags_across_a_quoted_list_literal() {
        let found = violations("(loop for x across '(1 2 3) collect x)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].evidence, ListEvidence::QuotedListLiteral);
    }

    #[test]
    fn flags_across_a_list_constructor() {
        let found = violations("(loop for x across (list a b) collect x)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].evidence, ListEvidence::ListConstructor);
        assert_eq!(
            violations("(loop for x across (append a b) collect x)").len(),
            1
        );
        assert_eq!(
            violations("(loop for x across (cons a b) collect x)").len(),
            1
        );
    }

    #[test]
    fn flags_across_a_variable_bound_to_a_list_in_the_same_loop() {
        let found = violations("(loop with items = (list 1 2) for x across items collect x)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].evidence, ListEvidence::BoundInThisLoop);
    }

    /// A loop variable's scope is the whole loop, so the binding may be
    /// written below the use.
    #[test]
    fn flags_across_a_variable_bound_later_in_the_same_loop() {
        let found = violations("(loop for x across items with items = '(1 2) collect x)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].evidence, ListEvidence::BoundInThisLoop);
    }

    #[test]
    fn does_not_flag_across_a_vector_literal_or_a_string() {
        assert!(violations("(loop for x across #(1 2 3) collect x)").is_empty());
        assert!(violations("(loop for c across \"hello\" collect c)").is_empty());
        assert!(violations("(loop for x across (make-array 3) collect x)").is_empty());
    }

    #[test]
    fn does_not_flag_across_a_variable_bound_outside_the_loop() {
        assert!(
            violations("(let ((items (list 1 2))) (loop for x across items collect x))").is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_binding_with_a_then_step() {
        assert!(
            violations("(loop for v = (list 1) then (make-array 2) for x across v collect x)")
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_across_nil() {
        assert!(violations("(loop for x across nil collect x)").is_empty());
    }

    #[test]
    fn does_not_flag_in_over_a_list() {
        assert!(violations("(loop for x in '(1 2 3) collect x)").is_empty());
        assert!(violations("(loop for x in (list a b) collect x)").is_empty());
    }

    #[test]
    fn does_not_flag_a_realistic_extended_loop() {
        let found = violations(
            "(loop for i from 0 below (length vec)\n\
             \x20     for c across vec\n\
             \x20     with seen = 0\n\
             \x20     when (alpha-char-p c) count c into letters\n\
             \x20     finally (return (list seen letters)))",
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn does_not_flag_a_loop_with_when_and_else() {
        let found = violations(
            "(loop for c across text\n\
             \x20     when (upper-case-p c) collect c into upper\n\
             \x20     else collect c into lower\n\
             \x20     end\n\
             \x20     finally (return (list upper lower)))",
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn says_nothing_about_a_being_the_hash_keys_loop() {
        let report = report(
            "(loop for k being the hash-keys of table for x across '(1 2) collect (cons k x))",
        );
        assert!(report.findings.is_empty());
        assert_eq!(
            report.summary,
            vec![
                ("loop_form_count", json!(1)),
                ("modelled_loop_form_count", json!(0)),
            ]
        );
    }

    #[test]
    fn does_not_flag_a_quoted_loop() {
        assert!(violations("(defparameter *f* '(loop for x across '(1 2) collect x))").is_empty());
    }

    #[test]
    fn does_not_flag_a_quasiquoted_loop_with_no_unquote() {
        assert!(violations("(defmacro m () `(loop for x across '(1 2) collect x))").is_empty());
    }

    /// The quote need not sit on the `loop` itself: a target nested *inside*
    /// quoted data is still data.
    #[test]
    fn does_not_flag_a_loop_nested_inside_quoted_data() {
        assert!(
            violations("(defparameter *f* '(progn (loop for x across '(1 2) collect x)))")
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_loop_nested_inside_a_macro_template() {
        assert!(
            violations(
                "(defmacro m (&body body)\n  `(progn (loop for x across '(1 2) collect x) ,@body))"
            )
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_clause_keywords_inside_a_string_literal() {
        assert!(
            violations("(defparameter *doc* \"loop for x across '(1 2) collect x\")").is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_loop_whose_variable_is_named_across() {
        assert!(violations("(loop for across in items collect across)").is_empty());
    }

    #[test]
    fn finds_an_across_nested_in_a_function_body() {
        let found = violations("(defun f ()\n  (loop for x across (list 1 2) collect x))");
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(
            "(loop for x across '(1 2) collect x)",
            Dialect::Clojure,
        )
        .expect("parse");
        let report = build_loop_for_across_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build loop for across report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(loop for x across vec collect x)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_fields() {
        let report = report("(defun f ()\n  (loop for x across (list 1 2) collect x))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "loop-for-across-statically-known-list");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("sequence", json!("(list 1 2)")),
                ("evidence", json!("list-constructor")),
            ]
        );
        assert_eq!(
            finding.text_columns(),
            vec![
                "sequence=(list 1 2)".to_owned(),
                "evidence=list-constructor".to_owned(),
            ]
        );
        assert_eq!(
            finding.message(),
            "loop `across` needs a vector, but (list 1 2) is a list constructor; \
             use `in` to walk a list"
        );
    }

    #[test]
    fn the_summary_counts_every_loop_scanned_not_only_the_flagged_ones() {
        let report =
            report("(loop for x across (list 1) collect x)\n(loop for y across vec collect y)\n");
        assert_eq!(
            report.summary,
            vec![
                ("loop_form_count", json!(2)),
                ("modelled_loop_form_count", json!(2)),
            ]
        );
        assert_eq!(report.findings.len(), 1);
    }
}
