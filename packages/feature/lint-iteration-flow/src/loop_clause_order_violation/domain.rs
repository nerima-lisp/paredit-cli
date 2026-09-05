//! Common Lisp `loop` clause-order detection: a clause written in a position
//! the macro's grammar does not admit.
//!
//! CLHS 6.1.1.1.1 gives the extended `loop` this shape:
//!
//! ```text
//! loop [name-clause] {variable-clause}* {main-clause}*
//! ```
//!
//! Two consequences of that production are checkable from the clause sequence
//! alone, and this rule checks exactly those two and nothing else:
//!
//! - **`named` must be the first clause.** The grammar admits at most one
//!   name-clause and only in first position.
//! - **No variable clause may follow a *body* clause.** `with`, `for`, and
//!   `as` are variable clauses; once body code has been written, the
//!   variable-clause section is closed.
//!
//! # What this rule deliberately does not flag
//!
//! - **A variable clause after `while`, `until`, or `repeat`.** The CLHS
//!   grammar files all three under `main-clause`, but implementations do not:
//!   `(loop repeat n for x = (random 10) collect x)`,
//!   `(loop for x in l while (p x) for y = (f x) collect y)`, and
//!   `(loop for x in l until (done x) for y = (f x) sum y)` are all accepted
//!   by SBCL and all are stock idioms. Only body code — `do`, an accumulation
//!   verb, a conditional, `always`/`never`/`thereis` — actually closes the
//!   variable-clause section, and this rule blocks on exactly that set. See
//!   [`crate::loop_syntax::is_body_clause_keyword`].
//! - **`with` after `for`, and `for` after `with`.** Both are variable
//!   clauses, so the grammar admits them in either order. This is the one
//!   ordering complaint a reader might expect here and it would be wrong:
//!   `(loop for x in items with total = 0 …)` is legal, and warning about it
//!   would be a warning on correct code.
//! - **`finally` not last, and `initially` not first.** The grammar lists
//!   `initial-final` under *both* `variable-clause` and `main-clause`, so
//!   neither position is required. What a misplaced `finally` can actually
//!   cost is dead epilogue code, which is
//!   [`crate::loop_unreachable_finally_clause`]'s subject.
//! - **A second `named` clause, duplicate `into` targets, clause arity, or
//!   anything about a clause's operands.** This rule reads clause *order*.
//! - **Anything at all in a form [`crate::loop_syntax`] declines to read** —
//!   an iteration path (`being the hash-keys …`) or a simple `loop`. See that
//!   module's doc for the full list.
//! - **Anything inside quoted or quasiquoted data**, at any depth. A `loop`
//!   written in `'(progn (loop …))` or in a macro template is the data or the
//!   expansion's code, not this call's, and the walk stops at the quote.
//!
//! Conditional clauses (`when`/`if`/`unless`/`else`/`end`) count as body
//! clauses but their nesting is not tracked, so a variable clause written
//! inside a conditional branch is reported the same way as one written after
//! it — which is correct, since neither is legal.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use serde_json::{Value, json};

use crate::loop_syntax::{LoopScan, is_body_clause_keyword, is_variable_clause_keyword};
use crate::support::for_each_evaluated_subview;

/// Which of the two order productions was broken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClauseOrderProblem {
    /// A `with`/`for`/`as` clause written after a main clause.
    VariableClauseAfterMainClause,
    /// A `named` clause that is not the loop's first clause.
    NamedClauseNotFirst,
}

impl ClauseOrderProblem {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::VariableClauseAfterMainClause => "variable-clause-after-main-clause",
            Self::NamedClauseNotFirst => "named-clause-not-first",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoopClauseOrderItem {
    /// The span of the misplaced clause keyword.
    pub span: ByteSpan,
    /// The misplaced clause keyword, lowercased.
    pub clause: String,
    /// The earlier keyword whose position makes this one illegal.
    pub after: String,
    pub problem: ClauseOrderProblem,
}

impl Finding for LoopClauseOrderItem {
    /// The rule's own name. The two productions are told apart by `problem`,
    /// which is reported as its own field.
    fn kind(&self) -> &'static str {
        "loop-clause-order-violation"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("clause={}", self.clause),
            format!("after={}", self.after),
            format!("problem={}", self.problem.as_str()),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("clause", json!(self.clause)),
            ("after", json!(self.after)),
            ("problem", json!(self.problem.as_str())),
        ]
    }

    fn message(&self) -> String {
        match self.problem {
            ClauseOrderProblem::VariableClauseAfterMainClause => format!(
                "loop variable clause `{}` follows the main clause `{}`; \
                 every variable clause must precede every main clause",
                self.clause, self.after
            ),
            ClauseOrderProblem::NamedClauseNotFirst => format!(
                "loop `named` clause follows `{}`; `named` must be the loop's first clause",
                self.after
            ),
        }
    }
}

pub fn examine_loop_clause_order(
    view: &ExpressionView,
    scan: &mut LoopScan,
    violations: &mut Vec<LoopClauseOrderItem>,
) {
    let Some(clauses) = scan.read(view) else {
        return;
    };

    // The first unambiguous body clause closes the variable-clause section.
    // `initially`/`finally`/`while`/`until`/`repeat` are not body clauses
    // here; see the module doc.
    let mut first_body: Option<&str> = None;
    let mut first_keyword: Option<&str> = None;

    for token in &clauses.tokens {
        let Some(keyword) = token.keyword() else {
            continue;
        };

        if keyword == "named" {
            if let Some(earlier) = first_keyword {
                violations.push(LoopClauseOrderItem {
                    span: token.span(),
                    clause: keyword.to_owned(),
                    after: earlier.to_owned(),
                    problem: ClauseOrderProblem::NamedClauseNotFirst,
                });
            }
        } else if is_variable_clause_keyword(keyword) {
            if let Some(body) = first_body {
                violations.push(LoopClauseOrderItem {
                    span: token.span(),
                    clause: keyword.to_owned(),
                    after: body.to_owned(),
                    problem: ClauseOrderProblem::VariableClauseAfterMainClause,
                });
            }
        } else if first_body.is_none() && is_body_clause_keyword(keyword) {
            first_body = Some(keyword);
        }

        if first_keyword.is_none() {
            first_keyword = Some(keyword);
        }
    }
}

/// Collects every misplaced `loop` clause in one file, with the number of
/// `loop` forms scanned and the number actually modelled beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_loop_clause_order_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<LoopClauseOrderItem>> {
    let mut scan = LoopScan::default();
    let mut violations = Vec::new();

    if dialect == Dialect::CommonLisp {
        for index in 0..tree.root_children().len() {
            let view = tree.select_path(&SexprPath::root_child(index))?.view();
            for_each_evaluated_subview(&view, |subview| {
                examine_loop_clause_order(subview, &mut scan, &mut violations);
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

    fn report(input: &str) -> FileFindings<LoopClauseOrderItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_loop_clause_order_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build loop clause order report")
    }

    fn violations(input: &str) -> Vec<LoopClauseOrderItem> {
        report(input).findings
    }

    #[test]
    fn flags_a_for_clause_after_a_collect_clause() {
        let found = violations("(loop for x in items collect x for y in others collect y)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].clause, "for");
        assert_eq!(found[0].after, "collect");
        assert_eq!(
            found[0].problem,
            ClauseOrderProblem::VariableClauseAfterMainClause
        );
    }

    #[test]
    fn flags_a_with_clause_after_a_do_clause() {
        let found = violations("(loop for x in items do (print x) with total = 0)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].clause, "with");
        assert_eq!(found[0].after, "do");
    }

    #[test]
    fn flags_a_named_clause_that_is_not_first() {
        let found = violations("(loop for x in items named outer collect x)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].clause, "named");
        assert_eq!(found[0].after, "for");
        assert_eq!(found[0].problem, ClauseOrderProblem::NamedClauseNotFirst);
    }

    #[test]
    fn does_not_flag_a_named_clause_written_first() {
        assert!(violations("(loop named outer for x in items collect x)").is_empty());
    }

    /// The ordering complaint this rule must *not* make: two variable clauses
    /// may appear in either order.
    #[test]
    fn does_not_flag_a_with_clause_after_a_for_clause() {
        assert!(violations("(loop for x in items with total = 0 collect (+ x total))").is_empty());
    }

    #[test]
    fn does_not_flag_a_for_clause_after_a_with_clause() {
        assert!(violations("(loop with total = 0 for x in items collect (+ x total))").is_empty());
    }

    /// `initially` and `finally` are admitted as variable clauses too, so a
    /// `for` after either of them is legal.
    #[test]
    fn does_not_flag_a_for_clause_after_initially_or_finally() {
        assert!(violations("(loop initially (setup) for x in items collect x)").is_empty());
        assert!(violations("(loop finally (report) for x in items collect x)").is_empty());
    }

    /// A realistic extended loop with several clause types must stay silent.
    #[test]
    fn does_not_flag_a_realistic_extended_loop() {
        let found = violations(
            "(loop for x from 1 to 10\n\
             \x20     for y = (* x x)\n\
             \x20     with limit = 50\n\
             \x20     while (< y limit)\n\
             \x20     collect y into squares\n\
             \x20     count y into seen\n\
             \x20     finally (return (values squares seen)))",
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn does_not_flag_a_loop_with_when_and_else() {
        let found = violations(
            "(loop for x in items\n\
             \x20     when (evenp x) collect x into evens\n\
             \x20     else collect x into odds\n\
             \x20     end\n\
             \x20     finally (return (list evens odds)))",
        );
        assert!(found.is_empty(), "{found:?}");
    }

    /// The `being` iteration path is not modelled, so nothing is reported for
    /// a loop that uses one — including one whose clause order is unusual.
    #[test]
    fn says_nothing_about_a_being_the_hash_keys_loop() {
        let report = report(
            "(loop for k being the hash-keys of table using (hash-value v) collect (cons k v))",
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

    /// A `loop` inside `'(...)` is unevaluated data; a rule must not warn
    /// about a shape someone wrote as a literal.
    #[test]
    fn does_not_flag_a_quoted_loop() {
        assert!(violations("(defparameter *form* '(loop collect x for y in z))").is_empty());
    }

    #[test]
    fn does_not_flag_a_quasiquoted_loop_with_no_unquote() {
        assert!(violations("(defmacro m () `(loop collect x for y in z))").is_empty());
    }

    /// The quote need not sit on the `loop` itself. A target nested *inside*
    /// quoted data is still data, and the walk must not descend to it.
    #[test]
    fn does_not_flag_a_loop_nested_inside_quoted_data() {
        assert!(violations("(defparameter *a* '(progn (loop collect x for y in z)))").is_empty());
    }

    /// The realistic bite: a macro template. The `loop` written here is the
    /// expansion's code, not this macro's, and the expansion is never seen.
    #[test]
    fn does_not_flag_a_loop_nested_inside_a_macro_template() {
        assert!(
            violations("(defmacro m (&body body)\n  `(progn (loop collect x for y in z) ,@body))")
                .is_empty()
        );
    }

    // --- Clause order against what SBCL actually accepts -------------------
    //
    // The CLHS grammar files `while`, `until`, and `repeat` under
    // `main-clause`, but implementations only reject a variable clause after
    // *body* code. Each case below was checked against SBCL.

    /// `(loop repeat n for x = (random 10) collect x)` — a stock idiom.
    #[test]
    fn does_not_flag_a_for_clause_after_repeat() {
        assert!(violations("(loop repeat n for x = (random 10) collect x)").is_empty());
    }

    /// `(loop for x in l while (p x) for y = (f x) collect y)` — accepted,
    /// returns `(2 4 6)` for the obvious `l`, `p`, and `f`.
    #[test]
    fn does_not_flag_a_for_clause_after_while() {
        assert!(violations("(loop for x in l while (p x) for y = (f x) collect y)").is_empty());
    }

    /// `(loop for x in l until (done x) for y = (f x) sum y)` — accepted,
    /// returns `12`.
    #[test]
    fn does_not_flag_a_for_clause_after_until() {
        assert!(violations("(loop for x in l until (done x) for y = (f x) sum y)").is_empty());
    }

    /// A `with` clause after `while` is accepted for the same reason.
    #[test]
    fn does_not_flag_a_with_clause_after_while() {
        assert!(
            violations("(loop for x in l while (p x) with cache = 1 collect (list x cache))")
                .is_empty()
        );
    }

    /// `do` *is* body code, and SBCL rejects a variable clause after it.
    #[test]
    fn flags_a_for_clause_after_do() {
        let found = violations("(loop for x in l do (side x) for y = (g x) collect y)");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].clause, "for");
        assert_eq!(found[0].after, "do");
    }

    /// `always` is the half of the CLHS `termination-test` group that SBCL
    /// does treat as body code.
    #[test]
    fn flags_a_for_clause_after_always() {
        let found = violations("(loop for x in l always (p x) for y in m collect y)");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].after, "always");
    }

    // --- Operands spelled like clause keywords -----------------------------
    //
    // `count` and `end` are everyday variable names bound *outside* the loop,
    // so the bound-name guard cannot see them. All three forms compile and run
    // under SBCL.

    #[test]
    fn does_not_flag_a_variable_read_after_to() {
        assert!(
            violations(
                "(defun v1 (count) (loop for i from 1 to count for j from 1 to 3 \
                 collect (list i j)))"
            )
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_variable_read_after_below() {
        assert!(
            violations(
                "(defun v2 (end) (loop for i from 0 below end for j from 0 below 3 \
                 collect (cons i j)))"
            )
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_variable_read_after_an_equals_sign() {
        assert!(
            violations(
                "(defun v3 (count items) (loop with limit = count for x in items \
                 collect (* x limit)))"
            )
            .is_empty()
        );
    }

    /// A clause keyword spelled inside a string is data, not a clause.
    #[test]
    fn does_not_flag_clause_keywords_inside_a_string_literal() {
        assert!(violations("(defparameter *doc* \"loop collect x for y in z\")").is_empty());
        assert!(violations("(loop for x in items do (print \"for with collect\"))").is_empty());
    }

    /// A variable named after a clause keyword must not be read as one.
    #[test]
    fn does_not_flag_a_loop_whose_variable_is_named_like_a_keyword() {
        assert!(violations("(loop for count from 1 to 3 collect count)").is_empty());
        assert!(violations("(loop with sum = 0 for x in items do (incf sum x))").is_empty());
    }

    #[test]
    fn does_not_flag_a_simple_loop() {
        assert!(violations("(loop (process) (maybe-exit))").is_empty());
    }

    #[test]
    fn finds_a_loop_nested_in_a_function_body() {
        let found = violations("(defun f (items)\n  (loop for x in items collect x with y = 1))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].clause, "with");
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(
            "(loop for x in items collect x for y in z)",
            Dialect::Clojure,
        )
        .expect("parse");
        let report = build_loop_clause_order_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build loop clause order report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(
            report.summary,
            vec![
                ("loop_form_count", json!(0)),
                ("modelled_loop_form_count", json!(0)),
            ]
        );
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(loop for x in items collect x)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_fields() {
        let report = report("(defun f (items)\n  (loop collect 1 for x in items))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "loop-clause-order-violation");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("clause", json!("for")),
                ("after", json!("collect")),
                ("problem", json!("variable-clause-after-main-clause")),
            ]
        );
        assert_eq!(
            finding.text_columns(),
            vec![
                "clause=for".to_owned(),
                "after=collect".to_owned(),
                "problem=variable-clause-after-main-clause".to_owned(),
            ]
        );
        assert_eq!(
            finding.message(),
            "loop variable clause `for` follows the main clause `collect`; \
             every variable clause must precede every main clause"
        );
    }

    #[test]
    fn the_summary_counts_every_loop_scanned_not_only_the_flagged_ones() {
        let report = report("(loop collect 1 for x in items)\n(loop for y in items collect y)\n");
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
