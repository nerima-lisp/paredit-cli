//! `sql-query-string-built-via-format`: a SQL statement with a value pasted into
//! it.
//!
//! `(query conn (format nil "SELECT * FROM users WHERE name = '~a'" name))` is a
//! SQL statement whose text depends on `name`. A `name` of `' OR 1=1 --` is not
//! a value in that statement; it is syntax. Every SQL client in the Common Lisp
//! ecosystem takes parameters separately — `(query conn "SELECT * FROM users
//! WHERE name = $1" name)`, `(execute stmt name)` — and a parameter is never
//! parsed as SQL.
//!
//! # What is required to fire
//!
//! An argument of a query-shaped call that is *all* of:
//!
//! 1. assembled by `format nil` / `concatenate` / `strcat`;
//! 2. carrying a literal fragment that spells a whole SQL statement — a
//!    `select`…`from`, `insert`…`into`, `update`…`set`, or `delete`…`from` pair,
//!    matched as **words**;
//! 3. with at least one spliced part that is not a literal.
//!
//! All three matter:
//!
//! - Without (1), `(query conn "SELECT * FROM users")` — a literal statement,
//!   which is the safe thing — would be reported.
//! - Without (2), any string built inside any function named `execute` would be.
//!   The keyword *pair* rather than a single keyword is deliberate:
//!   `(format nil "~a rows selected" n)` contains neither `select` as a word nor
//!   a `from`, and `"Insert the disc"` has no `into`.
//! - Without (3), `(format nil "SELECT * FROM users WHERE id = ~a" 42)` — a
//!   literal spliced into a literal, which cannot inject — would be.
//!
//! # Not a duplicate of `subprocess-string-building`
//!
//! Same technique, disjoint heads (`run-program` and friends there, query
//! operators here) and a different interpreter: a shell that splits on `;`
//! versus a SQL parser that ends a string at `'`. No form can draw both. This
//! rule also requires the SQL-statement evidence and the non-literal part, which
//! that rule does not.
//!
//! Limits, by design: a project that names its query function something else
//! entirely is not covered — the head list is a closed set, and widening it to
//! "any call" would trade a false negative for constant noise. A statement built
//! in one function and passed to another is not covered either; this rule reads
//! one call.
//!
//! Report-only. The rewrite is a different calling convention (bind parameters),
//! not a substitution.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{list_head, symbol_in};

use crate::support::{contains_word, is_unevaluated_at, string_build};

pub const META: RuleMeta = RuleMeta::new(
    "sql-query-string-built-via-format",
    RuleCategory::Security,
    Severity::Error,
    "a SQL statement assembled from a value, so the value becomes statement syntax rather than \
     data",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "A statement built by string interpolation is parsed as SQL after the value is already in \
         it, so a value containing a quote or a comment marker becomes syntax. Passing the value \
         as a bound parameter means the SQL is parsed first and the value never can be.",
    )
    .with_example(
        "(query conn (format nil \"SELECT * FROM users WHERE name = '~a'\" name))",
        "(query conn \"SELECT * FROM users WHERE name = $1\" name)",
    )
    .with_caveat(
        "Both halves of a SQL statement must appear as words in the literal text — `select`+`from`, \
         `insert`+`into`, `update`+`set`, `delete`+`from` — and at least one spliced part must be \
         non-literal. A literal statement, or one whose only interpolation is a constant, is not \
         reported.",
    ),
);

const HEADS: [NormalizedHead; 7] = [
    NormalizedHead::new("query"),
    NormalizedHead::new("execute"),
    NormalizedHead::new("db-query"),
    NormalizedHead::new("execute-query"),
    NormalizedHead::new("execute-non-query"),
    NormalizedHead::new("sql-query"),
    NormalizedHead::new("run-query"),
];

const QUERY_OPERATORS: [&str; 7] = [
    "query",
    "execute",
    "db-query",
    "execute-query",
    "execute-non-query",
    "sql-query",
    "run-query",
];

/// The keyword pairs that make a string a SQL *statement* rather than a string
/// that happens to contain an English word.
const STATEMENT_SHAPES: [(&str, &str); 4] = [
    ("select", "from"),
    ("insert", "into"),
    ("update", "set"),
    ("delete", "from"),
];

/// One interpolated SQL statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterpolatedStatement {
    pub span: ByteSpan,
    pub builder: &'static str,
    /// The statement kind the literal text spells: `select`, `insert`, …
    pub statement: &'static str,
}

/// The SQL statement `fragments` spell between them, if any.
///
/// Read across fragments rather than per fragment: `(concatenate 'string
/// "SELECT * " "FROM t WHERE id = " id)` splits the pair over two literals and
/// is the same statement.
fn statement_kind(fragments: &[&str]) -> Option<&'static str> {
    let joined = fragments.join(" ");
    STATEMENT_SHAPES
        .iter()
        .find(|(verb, particle)| contains_word(&joined, verb) && contains_word(&joined, particle))
        .map(|(verb, _)| *verb)
}

/// Reads one query-shaped call.
#[must_use]
pub fn examine(view: &ExpressionView, context: &RuleContext<'_>) -> Option<InterpolatedStatement> {
    let head = list_head(view)?;
    if !symbol_in(head, &QUERY_OPERATORS) {
        return None;
    }

    // Any argument: the connection is the first argument in some clients and
    // absent in others, so the statement's position is not fixed.
    let found = view.children.iter().skip(1).find_map(|argument| {
        let build = string_build(argument)?;
        if build.interpolated().is_empty() {
            return None;
        }
        let statement = statement_kind(&build.literal_fragments())?;
        Some((argument.span, build.builder(), statement))
    })?;

    // Asked last, and only once there is something to report.
    if is_unevaluated_at(context.tree(), view.span) {
        return None;
    }
    Some(InterpolatedStatement {
        span: found.0,
        builder: found.1,
        statement: found.2,
    })
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        if let Some(found) = examine(view, context) {
            sink.report(
                found.span,
                format!(
                    "this {} statement is assembled by {}, so a spliced value containing a quote \
                     or a comment marker becomes statement syntax; pass the value as a bound \
                     parameter instead",
                    found.statement, found.builder
                ),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::testing::findings_for_heads;

    fn statements(input: &str) -> Vec<&'static str> {
        findings_for_heads(input, &QUERY_OPERATORS, |view, context| {
            examine(view, context)
                .map(|found| found.statement)
                .into_iter()
                .collect::<Vec<_>>()
        })
    }

    #[test]
    fn flags_a_format_built_select() {
        assert_eq!(
            statements(r#"(query conn (format nil "SELECT * FROM users WHERE name = '~a'" name))"#),
            vec!["select"]
        );
    }

    #[test]
    fn flags_a_concatenated_statement_split_over_two_literals() {
        assert_eq!(
            statements(r#"(execute (concatenate 'string "SELECT * " "FROM t WHERE id = " id))"#),
            vec!["select"]
        );
    }

    #[test]
    fn flags_every_statement_kind() {
        assert_eq!(
            statements(r#"(db-query (format nil "INSERT INTO t VALUES (~a)" v))"#),
            vec!["insert"]
        );
        assert_eq!(
            statements(r#"(execute-query (format nil "UPDATE t SET a = ~a" v))"#),
            vec!["update"]
        );
        assert_eq!(
            statements(r#"(run-query (format nil "DELETE FROM t WHERE id = ~a" v))"#),
            vec!["delete"]
        );
    }

    #[test]
    fn reads_a_package_qualified_operator() {
        assert_eq!(
            statements(r#"(dbi:execute (format nil "SELECT a FROM t WHERE b = ~a" v))"#),
            vec!["select"]
        );
    }

    // --- near misses ------------------------------------------------------

    #[test]
    fn does_not_flag_a_literal_statement() {
        assert!(statements(r#"(query conn "SELECT * FROM users")"#).is_empty());
        assert!(statements(r#"(query conn "SELECT * FROM users WHERE id = $1" id)"#).is_empty());
    }

    #[test]
    fn does_not_flag_an_all_literal_interpolation() {
        assert!(
            statements(r#"(query conn (format nil "SELECT * FROM users WHERE id = ~a" 42))"#)
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_string_that_merely_contains_an_english_word() {
        assert!(statements(r#"(execute (format nil "~a rows selected" n))"#).is_empty());
        assert!(statements(r#"(execute (format nil "Insert the disc ~a" n))"#).is_empty());
        assert!(statements(r#"(execute (format nil "update available: ~a" n))"#).is_empty());
    }

    #[test]
    fn does_not_flag_half_a_statement() {
        // `select` with no `from` is not a statement this rule claims to read.
        assert!(statements(r#"(execute (format nil "select ~a" n))"#).is_empty());
    }

    #[test]
    fn does_not_flag_a_printing_format() {
        assert!(
            statements(r#"(execute (format t "SELECT * FROM t WHERE id = ~a" id))"#).is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_call_with_no_arguments() {
        assert!(statements("(query)").is_empty());
    }

    // --- quote and string contexts ---------------------------------------

    #[test]
    fn does_not_flag_a_quoted_form() {
        let form = r#"(query conn (format nil "SELECT * FROM t WHERE id = ~a" id))"#;
        assert!(statements(&format!("'{form}")).is_empty());
        assert!(statements(&format!("'(progn {form})")).is_empty());
        assert!(statements(&format!("(quote {form})")).is_empty());
        assert!(statements(&format!("`{form}")).is_empty());
        assert!(statements(&format!("'(a ,{form})")).is_empty());
    }

    #[test]
    fn flags_an_unquoted_form_inside_a_backquote() {
        let form = r#"(query conn (format nil "SELECT * FROM t WHERE id = ~a" id))"#;
        assert_eq!(statements(&format!("`(a ,{form})")), vec!["select"]);
    }

    #[test]
    fn does_not_flag_text_inside_a_string_literal() {
        assert!(
            statements(
                r#"(log-it "(query conn (format nil \"SELECT * FROM t WHERE id = ~a\" id))")"#
            )
            .is_empty()
        );
    }
}
