//! `racket-case-lambda-single-clause`: a `case-lambda` with exactly one clause.
//!
//! `case-lambda` exists to dispatch on argument count. With one clause there is
//! nothing to dispatch on, and the form is a `lambda` written the long way:
//! `(case-lambda [⟨formals⟩ ⟨body⟩ …])` accepts exactly the argument counts
//! `(lambda ⟨formals⟩ ⟨body⟩ …)` accepts, and computes the same thing.
//!
//! Verified on Racket v9.2: `(define single (case-lambda [(x) (* x 2)]))` then
//! `(single 5)` prints `10`, compiling without a word.
//!
//! The two are not *quite* interchangeable in one respect, and it is worth
//! being explicit because it is the reason the fix is safe rather than a reason
//! it is not: a one-clause `case-lambda` and the corresponding `lambda` differ
//! in the arity-error message they raise and in `object-name`. Neither is
//! behaviour a program can depend on without already depending on an
//! implementation detail, and the rewrite preserves the accepted arities
//! exactly, including the rest-argument spelling `(case-lambda [args …])`.
//!
//! Two shapes are deliberately not flagged:
//!
//! - **`(case-lambda)`**, which is legal — a procedure accepting no argument
//!   count at all — and is occasionally written on purpose.
//! - **Two or more clauses**, which is `case-lambda` doing its job.
//!
//! Scope: Racket only. `case-lambda` is R7RS as well, but
//! `paredit-feature-lint-scheme-idiom` owns the Scheme surface and anchors on
//! no `case-lambda` head; widening this rule to Scheme is a decision for that
//! package, taken against a Scheme corpus this one has not swept.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use serde_json::{Value, json};

use crate::support::{heads_with, is_racket_list, node_context, racket_atom};

/// The head this rule anchors on, shared with its `HeadFilter`.
pub const HEAD: &str = "case-lambda";

/// The dialects this rule models.
pub const DIALECTS: [Dialect; 1] = [Dialect::Racket];

#[derive(Debug, Clone)]
pub struct CaseLambdaSingleClauseItem {
    /// The span of the whole `(case-lambda …)` form, which a fix replaces.
    pub span: ByteSpan,
    /// The span *inside* the sole clause's delimiters — its formals and body,
    /// which a fix copies verbatim into a `lambda`.
    pub clause_inner_span: ByteSpan,
}

impl Finding for CaseLambdaSingleClauseItem {
    fn kind(&self) -> &'static str {
        "racket-case-lambda-single-clause"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec!["clause_count=1".to_owned()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("clause_count", json!(1))]
    }

    fn message(&self) -> String {
        MESSAGE.to_owned()
    }
}

pub const MESSAGE: &str = "case-lambda has one clause, so it dispatches on nothing; this is a lambda written the long way";

/// The arrow spellings that mark a clause as a function *type* rather than a
/// procedure clause.
const ARROWS: [&str; 2] = ["->", "\u{2192}"];

/// The operators whose operand is a **type**, in which `case-lambda` is Typed
/// Racket's legacy spelling of `case->` rather than a procedure at all.
///
/// Found by corpus audit, not by reasoning: `typed-racket-test/fail/cl-bug.rkt`
/// writes `(: f2 (case-lambda (Number * -> Number)))`, which this rule reported
/// as a one-clause procedure. It is a one-clause *arrow type*, and rewriting it
/// to `(lambda (Number * -> Number))` would be nonsense.
const TYPE_POSITION_HEADS: [&str; 4] = [":", "define-type", "require/typed", "cast"];

/// Whether a clause is a function type — `(Number * -> Number)` — rather than
/// a procedure's formals and body.
///
/// Node-local and therefore checked before any ancestor walk. A type clause
/// names its range after an arrow; a procedure clause never has a bare `->` as
/// a direct child, because that position holds formals and then body forms.
fn is_function_type_clause(clause: &ExpressionView) -> bool {
    clause
        .children
        .iter()
        .filter_map(racket_atom)
        .any(|text| ARROWS.contains(&text))
}

/// The span between a list's delimiters, or `None` when the node is not a
/// delimited list wide enough to have an inside.
///
/// Both delimiters are single ASCII bytes, so the arithmetic cannot land in the
/// middle of a character.
fn inner_span(view: &ExpressionView) -> Option<ByteSpan> {
    let (start, end) = (view.span.start().get(), view.span.end().get());
    let inner_end = end.checked_sub(1)?;
    let inner_start = start.checked_add(1)?;
    (inner_start <= inner_end).then(|| {
        ByteSpan::new(
            paredit_core_syntax::sexpr::ByteOffset::new(inner_start),
            paredit_core_syntax::sexpr::ByteOffset::new(inner_end),
        )
    })
}

/// Examines one node, which the caller has already narrowed to a `case-lambda`
/// head.
pub fn examine_case_lambda(
    tree: &SyntaxTree,
    view: &ExpressionView,
    case_lambda_form_count: &mut usize,
    violations: &mut Vec<CaseLambdaSingleClauseItem>,
) {
    if !heads_with(view, HEAD) {
        return;
    }
    *case_lambda_form_count += 1;

    // children: [case-lambda, clause…]. Exactly one clause is the subject.
    let [_, only] = view.children.as_slice() else {
        return;
    };
    // A clause is a list of formals plus a body. Anything else — an atom in
    // clause position — is malformed, and not this rule's complaint to make.
    if !is_racket_list(only) {
        return;
    }
    // A clause needs formals and at least one body form. `[(x)]` is a syntax
    // error rather than an idiom.
    if only.children.len() < 2 {
        return;
    }
    // Node-local, so it runs before any ancestor walk: a clause naming an arrow
    // is a function *type*, and `case-lambda` in type position is Typed
    // Racket's legacy `case->`.
    if is_function_type_clause(only) {
        return;
    }
    let Some(clause_inner_span) = inner_span(only) else {
        return;
    };

    // Last, and only for a node that would otherwise be reported. One descent
    // answers both the quoting question and the enclosing-operator one.
    let context = node_context(tree, view.span);
    if context.unevaluated || context.in_macro_template {
        return;
    }
    // The belt to the arrow test's braces: an operand of `:` or `define-type`
    // is a type whatever it looks like inside.
    if context
        .parent_head
        .is_some_and(|head| TYPE_POSITION_HEADS.contains(&head.as_str()))
    {
        return;
    }

    violations.push(CaseLambdaSingleClauseItem {
        span: view.span,
        clause_inner_span,
    });
}

/// Collects every one-clause `case-lambda` in one file, with the number of
/// `case-lambda` forms scanned as the denominator beside them.
pub fn build_case_lambda_single_clause_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<CaseLambdaSingleClauseItem>> {
    let modelled = DIALECTS.contains(&dialect);
    let mut case_lambda_form_count = 0;
    let mut violations = Vec::new();

    if modelled {
        let root = tree.root_view();
        let mut stack: Vec<&ExpressionView> = root.children.iter().rev().collect();
        while let Some(view) = stack.pop() {
            examine_case_lambda(tree, view, &mut case_lambda_form_count, &mut violations);
            stack.extend(view.children.iter().rev());
        }
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        tree.source(),
        violations,
        vec![("case_lambda_form_count", json!(case_lambda_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<CaseLambdaSingleClauseItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Racket).expect("parse input");
        build_case_lambda_single_clause_report(Path::new("main.rkt"), Dialect::Racket, &tree)
            .expect("build report")
    }

    fn findings(input: &str) -> Vec<CaseLambdaSingleClauseItem> {
        report(input).findings
    }

    fn scanned(input: &str) -> u64 {
        report(input)
            .summary
            .iter()
            .find(|(name, _)| *name == "case_lambda_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("case_lambda_form_count in the summary")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_a_single_clause_case_lambda() {
        let source = "(case-lambda [(x) (* x 2)])";
        let found = findings(source);
        assert_eq!(found.len(), 1);
        assert_eq!(slice(source, found[0].clause_inner_span), "(x) (* x 2)");
    }

    /// The rest-argument spelling: the formals are a bare identifier, not a
    /// list. The rewrite is `(lambda args …)` and is equally correct.
    #[test]
    fn flags_a_rest_argument_clause() {
        let source = "(case-lambda [args (length args)])";
        let found = findings(source);
        assert_eq!(found.len(), 1);
        assert_eq!(
            slice(source, found[0].clause_inner_span),
            "args (length args)"
        );
    }

    #[test]
    fn a_parenthesised_clause_is_read_like_a_bracketed_one() {
        let source = "(case-lambda ((x) (* x 2)))";
        let found = findings(source);
        assert_eq!(found.len(), 1);
        assert_eq!(slice(source, found[0].clause_inner_span), "(x) (* x 2)");
    }

    #[test]
    fn the_clause_inner_span_keeps_a_multi_form_body_whole() {
        let source = "(case-lambda [(x) (log x) (* x 2)])";
        assert_eq!(
            slice(source, findings(source)[0].clause_inner_span),
            "(x) (log x) (* x 2)"
        );
    }

    #[test]
    fn does_not_flag_a_case_lambda_doing_its_job() {
        let source = "(case-lambda [(x) x] [(x y) (+ x y)])";
        assert_eq!(scanned(source), 1);
        assert!(findings(source).is_empty());
    }

    /// Legal, and occasionally written on purpose: a procedure accepting no
    /// argument count at all.
    #[test]
    fn does_not_flag_a_case_lambda_with_no_clauses() {
        assert_eq!(scanned("(case-lambda)"), 1);
        assert!(findings("(case-lambda)").is_empty());
    }

    #[test]
    fn does_not_flag_a_clause_with_no_body() {
        assert!(findings("(case-lambda [(x)])").is_empty());
    }

    #[test]
    fn does_not_flag_an_atom_in_clause_position() {
        assert!(findings("(case-lambda x)").is_empty());
    }

    /// A `#(…)` vector constant is not a clause, and unlike an atom it *has*
    /// children — so the "clause must be a list" test is what rejects it, and
    /// the arity test below cannot. Without this case that guard survived every
    /// mutation, because an atom clause was already stopped by the arity test.
    ///
    /// The fix is what makes this matter rather than the finding: unwrapping
    /// `#((x) x)` to `(lambda (x) x)` would turn a vector constant into a
    /// procedure.
    #[test]
    fn does_not_flag_a_vector_literal_in_clause_position() {
        assert_eq!(scanned("(case-lambda #((x) x))"), 1);
        assert!(findings("(case-lambda #((x) x))").is_empty());
    }

    // -- the corpus false positive, and its guards ---------------------------

    /// Reduced verbatim from `typed-racket-test/fail/cl-bug.rkt:6`, the only
    /// false positive this rule produced over the 4492-file Racket corpus.
    /// `case-lambda` in type position is Typed Racket's legacy spelling of
    /// `case->`, and `(Number * -> Number)` is an arrow type, not formals and
    /// a body.
    #[test]
    fn does_not_flag_a_case_lambda_used_as_a_type() {
        assert!(
            findings("(: f2 (case-lambda (Number * -> Number)))\n(define (f2 x y) (+ x y))\n")
                .is_empty()
        );
    }

    /// The arrow test is node-local and catches the type clause on its own,
    /// with no ancestor at all — which is what makes it cheap enough to run
    /// before the quote guard.
    #[test]
    fn a_clause_naming_an_arrow_is_a_function_type_wherever_it_appears() {
        assert!(findings("(case-lambda (Number * -> Number))").is_empty());
        assert!(findings("(case-lambda (String -> String))").is_empty());
        assert!(findings("(case-lambda (Number * \u{2192} Number))").is_empty());
    }

    /// The parent-head guard covers a type spelled without a visible arrow.
    #[test]
    fn does_not_flag_a_case_lambda_in_any_type_position() {
        for head in [":", "define-type", "require/typed", "cast"] {
            assert!(
                findings(&format!("({head} f (case-lambda ((x) x)))")).is_empty(),
                "{head} takes a type, not a procedure"
            );
        }
    }

    /// The guards must not swallow the real thing. `typed-racket-test` writes
    /// both shapes in the same corpus, and only the arrow one is a type.
    #[test]
    fn still_flags_a_real_case_lambda_in_a_typed_racket_file() {
        // From `typed-racket-test/succeed/shallow/case-lambda.rkt:6`.
        assert_eq!(
            findings("(: f0 (case-> (-> Symbol Symbol)))\n(define f0 (case-lambda [(x) x]))\n")
                .len(),
            1
        );
        // From `typed-racket-test/fail/case-lambda4.rkt:9` — rest-argument
        // formals, which contain no arrow.
        assert_eq!(
            findings("(define f (case-lambda ((x . w) \"hello\")))").len(),
            1
        );
        // From `typed-racket-test/succeed/case-lambda1.rkt:7`.
        assert_eq!(findings("(define f (case-lambda (w (first w))))").len(), 1);
    }

    #[test]
    fn does_not_case_fold_the_head() {
        assert_eq!(scanned("(CASE-LAMBDA [(x) x])"), 0);
    }

    #[test]
    fn does_not_flag_a_qualified_head_that_merely_ends_in_case_lambda() {
        assert_eq!(scanned("(racket:case-lambda [(x) x])"), 0);
    }

    #[test]
    fn does_not_flag_a_quoted_case_lambda_shape() {
        assert!(findings("'(case-lambda [(x) x])").is_empty());
        assert!(findings("(quote (case-lambda [(x) x]))").is_empty());
        assert!(findings("`(a (case-lambda [(x) x]))").is_empty());
    }

    #[test]
    fn flags_an_unquoted_case_lambda_inside_a_quasiquote() {
        assert_eq!(findings("`(a ,(case-lambda [(x) x]))").len(), 1);
    }

    #[test]
    fn does_not_flag_a_vector_constant_that_looks_like_a_case_lambda_call() {
        assert_eq!(scanned("#(case-lambda [(x) x])"), 0);
    }

    #[test]
    fn does_not_flag_a_case_lambda_inside_a_macro_template() {
        assert!(
            findings("(define-syntax m (syntax-rules () ((_ b) (case-lambda [(x) b]))))")
                .is_empty()
        );
    }

    #[test]
    fn finds_a_nested_case_lambda() {
        assert_eq!(
            findings("(define f (case-lambda [(x) (case-lambda [(y) y])]))").len(),
            2
        );
    }

    #[test]
    fn the_summary_counts_every_case_lambda_scanned() {
        let source = "(case-lambda [(x) x])\n(case-lambda [(x) x] [(x y) y])\n(case-lambda)\n";
        assert_eq!(scanned(source), 3);
        assert_eq!(findings(source).len(), 1);
    }

    #[test]
    fn the_same_bytes_are_flagged_as_racket_and_unmodelled_elsewhere() {
        // Parenthesized, not bracketed: the Common Lisp and Clojure readers
        // reject `[`, so a bracketed fixture would fail to parse for the
        // controls and prove nothing about scope.
        let source = "(case-lambda ((x) x))\n";
        assert_eq!(findings(source).len(), 1);
        for dialect in [Dialect::Scheme, Dialect::CommonLisp, Dialect::Clojure] {
            let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
            let report = build_case_lambda_single_clause_report(Path::new("f.scm"), dialect, &tree)
                .expect("build report");
            assert!(!report.dialect_modelled, "{dialect:?}");
            assert!(report.findings.is_empty(), "{dialect:?}");
        }
    }

    #[test]
    fn a_finding_carries_its_line_and_its_fields() {
        let report = report("#lang racket\n(case-lambda [(x) (* x 2)])\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "racket-case-lambda-single-clause");
        assert_eq!(finding.json_fields(), vec![("clause_count", json!(1))]);
        assert_eq!(finding.message(), MESSAGE);
    }
}
