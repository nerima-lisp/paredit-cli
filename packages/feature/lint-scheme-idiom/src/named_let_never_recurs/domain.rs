//! `scheme-named-let-never-recurs` detection: a named `let` whose loop name is
//! never mentioned in its body.
//!
//! R7RS 4.2.4 defines `(let ⟨variable⟩ ⟨bindings⟩ ⟨body⟩)` as binding
//! `⟨variable⟩` "within `⟨body⟩` to a procedure whose formal arguments are the
//! bound variables and whose body is `⟨body⟩`" — the name exists for one
//! purpose, which is to be called again. A named `let` whose body never
//! mentions the name is a plain `let` with a procedure bound beside it that
//! nothing can reach; either the recursive call was forgotten, or the name is
//! left over from an edit.
//!
//! Deliberately *not* a tail-position rule. R7RS 3.5 guarantees that a call in
//! tail position consumes no stack, but it does not require code to make its
//! calls there, and non-tail named-let recursion is ordinary correct Scheme:
//! `(let loop ((i 0)) (if (< i n) (cons i (loop (+ i 1))) '()))` builds a list
//! and is exactly how R7RS-era code is written. Reporting it would be a
//! complaint about working code. What is reported here instead is the case
//! where the loop provably cannot iterate at all.
//!
//! The search is deliberately literal and deliberately over-generous: any atom
//! anywhere in the body spelled like the loop name counts as a mention, even
//! inside a quoted list and even where an inner binder has shadowed it. Both
//! over-counts suppress a finding rather than create one. (A directly quoted
//! symbol is the exception, and not by choice: the parser keeps `'` in the
//! atom's own text, so `'loop` and `loop` are already different strings.)
//!
//! Known false negative, stated rather than papered over: a `syntax-case`
//! macro that reaches the name through `datum->syntax` mentions it nowhere in
//! the template, and this rule will call the loop dead. `syntax-rules` is
//! hygienic (R7RS 4.3.2), so the ordinary macro case cannot reach a template's
//! `loop` from outside and is analysed correctly.
//!
//! Scope: Scheme and Racket.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::scheme::scheme_identifier_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use serde_json::{Value, json};

use crate::support::{heads_with, is_scheme_list, node_context, scheme_atom};

/// The head this rule anchors on, shared with its `HeadFilter`.
pub const HEAD: &str = "let";

/// The dialects this rule models.
pub const DIALECTS: [Dialect; 2] = [Dialect::Scheme, Dialect::Racket];

#[derive(Debug, Clone)]
pub struct NamedLetNeverRecursItem {
    /// The span of the whole named `let`.
    pub span: ByteSpan,
    /// The loop name, for the message.
    pub name: String,
    /// The span a fix deletes: from the end of the `let` head through the end
    /// of the loop name, which is exactly the name and the whitespace before
    /// it. `None` when that gap holds anything but whitespace — a comment or a
    /// `#;` datum comment there would be destroyed by the deletion.
    pub fix_span: Option<ByteSpan>,
}

impl Finding for NamedLetNeverRecursItem {
    fn kind(&self) -> &'static str {
        "scheme-named-let-never-recurs"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("name={}", self.name)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("name", json!(self.name))]
    }

    fn message(&self) -> String {
        message_for(&self.name)
    }
}

/// The one sentence both the standalone report and the lint rule write.
#[must_use]
pub fn message_for(name: &str) -> String {
    format!("named let {name} is never called in its own body, so the loop cannot iterate")
}

/// Whether any atom anywhere under `view` is spelled exactly `name`.
fn mentions(view: &ExpressionView, name: &str) -> bool {
    let mut stack = vec![view];
    while let Some(node) = stack.pop() {
        if scheme_atom(node) == Some(name) {
            return true;
        }
        stack.extend(node.children.iter());
    }
    false
}

/// Examines one node.
pub fn examine_named_let(
    tree: &SyntaxTree,
    view: &ExpressionView,
    named_let_form_count: &mut usize,
    violations: &mut Vec<NamedLetNeverRecursItem>,
) {
    if !heads_with(view, HEAD) {
        return;
    }
    // `(let ⟨variable⟩ ⟨bindings⟩ ⟨body⟩…)`: the name, the binding list, and
    // at least one body form. Anything shorter is a plain `let` or malformed.
    let [head, name_node, bindings, body @ ..] = view.children.as_slice() else {
        return;
    };
    let Some(name) = scheme_identifier_text(name_node) else {
        return;
    };
    if !is_scheme_list(bindings) || body.is_empty() {
        return;
    }
    *named_let_form_count += 1;

    if body.iter().any(|form| mentions(form, name)) {
        return;
    }
    // Last: the only non-node-local check. See `crate::support::node_context`.
    let context = node_context(tree, view.span);
    if context.unevaluated {
        return;
    }
    // A named `let` in a macro template is not a loop this rule can read. Both
    // the name and the body are pattern variables the macro's caller fills in,
    // so the recursive call is nowhere in the template's own text. Guile's
    // `ice-9/match.upstream.scm` and `srfi/srfi-71.scm` are exactly this and
    // were the only two false positives this rule produced over its corpus.
    if context.in_macro_template {
        return;
    }

    let gap = ByteSpan::new(head.span.end(), name_node.span.start());
    let gap_is_blank = tree
        .source()
        .get(gap.start().get()..gap.end().get())
        .is_some_and(|text| text.chars().all(char::is_whitespace));

    violations.push(NamedLetNeverRecursItem {
        span: view.span,
        name: name.to_owned(),
        fix_span: gap_is_blank.then(|| ByteSpan::new(head.span.end(), name_node.span.end())),
    });
}

/// Collects every dead named `let` in one file, with the number of named `let`
/// forms scanned as the denominator beside them.
///
/// The denominator counts *named* lets only, not every `let`: a plain `let` is
/// not a form this rule could ever have an opinion about, and counting it
/// would make the ratio unreadable.
pub fn build_named_let_never_recurs_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<NamedLetNeverRecursItem>> {
    if !DIALECTS.contains(&dialect) {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("named_let_form_count", json!(0))],
        ));
    }

    let mut named_let_form_count = 0;
    let mut violations = Vec::new();
    let root = tree.root_view();
    let mut stack: Vec<&ExpressionView> = root.children.iter().rev().collect();
    while let Some(view) = stack.pop() {
        examine_named_let(tree, view, &mut named_let_form_count, &mut violations);
        stack.extend(view.children.iter().rev());
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("named_let_form_count", json!(named_let_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<NamedLetNeverRecursItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Scheme).expect("parse input");
        build_named_let_never_recurs_report(Path::new("test.scm"), Dialect::Scheme, &tree)
            .expect("build named-let report")
    }

    fn findings(input: &str) -> Vec<NamedLetNeverRecursItem> {
        report(input).findings
    }

    fn scanned(input: &str) -> u64 {
        report(input)
            .summary
            .iter()
            .find(|(name, _)| *name == "named_let_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("named_let_form_count in the summary")
    }

    /// Applies the offered fix and returns the rewritten source.
    fn fixed(input: &str) -> String {
        let found = findings(input);
        let span = found[0].fix_span.expect("a fix");
        let mut rewritten = input.to_owned();
        rewritten.replace_range(span.start().get()..span.end().get(), "");
        SyntaxTree::parse_with_dialect(&rewritten, Dialect::Scheme)
            .expect("the fixed source must still parse");
        rewritten
    }

    #[test]
    fn flags_a_named_let_whose_body_never_calls_it() {
        let found = findings("(let loop ((i 0)) (display i))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "loop");
    }

    #[test]
    fn does_not_flag_a_named_let_that_calls_itself() {
        assert_eq!(
            scanned("(let loop ((i 0)) (when (< i 3) (loop (+ i 1))))"),
            1
        );
        assert!(findings("(let loop ((i 0)) (when (< i 3) (loop (+ i 1))))").is_empty());
    }

    #[test]
    fn does_not_flag_a_non_tail_recursive_named_let() {
        // R7RS 3.5 guarantees tail calls; it does not forbid non-tail ones.
        // This is ordinary correct Scheme and must never be reported.
        let source = "(let loop ((i 0)) (if (< i n) (cons i (loop (+ i 1))) '()))";
        assert_eq!(scanned(source), 1);
        assert!(findings(source).is_empty());
    }

    #[test]
    fn does_not_flag_a_name_mentioned_other_than_as_a_head() {
        // Passing the loop to another procedure is still using it.
        assert!(findings("(let loop ((i 0)) (for-each loop xs))").is_empty());
    }

    /// `'loop` is a symbol literal, not a call. The parser keeps the prefix in
    /// the atom's own text (`"'loop"`), so the comparison against the bare
    /// name distinguishes the two for free — and the loop really is dead here.
    #[test]
    fn a_directly_quoted_symbol_is_not_a_mention() {
        assert_eq!(findings("(let loop ((i 0)) (display 'loop))").len(), 1);
    }

    /// Inside a quoted *list* the elements carry no prefix of their own, so
    /// this reads as a mention and suppresses the finding. Over-generous on
    /// purpose: the over-count can only hide a finding, never invent one.
    #[test]
    fn a_mention_inside_a_quoted_list_suppresses_the_finding() {
        assert!(findings("(let loop ((i 0)) (display '(a loop)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_plain_let() {
        assert_eq!(scanned("(let ((i 0)) (display i))"), 0);
        assert!(findings("(let ((i 0)) (display i))").is_empty());
    }

    #[test]
    fn does_not_flag_a_let_with_no_body() {
        assert_eq!(scanned("(let loop ((i 0)))"), 0);
    }

    #[test]
    fn does_not_flag_a_named_let_whose_second_form_is_not_a_binding_list() {
        assert_eq!(scanned("(let loop x y)"), 0);
    }

    #[test]
    fn does_not_treat_a_literal_as_a_loop_name() {
        // `(let "x" ((i 0)) …)` is malformed, not a named let.
        assert_eq!(scanned("(let \"x\" ((i 0)) (display i))"), 0);
        assert_eq!(scanned("(let 1 ((i 0)) (display i))"), 0);
    }

    #[test]
    fn does_not_flag_a_quoted_named_let_shape() {
        assert!(findings("'(let loop ((i 0)) (display i))").is_empty());
    }

    #[test]
    fn does_not_flag_a_vector_constant_that_looks_like_a_named_let() {
        assert_eq!(scanned("#(let loop ((i 0)) (display i))"), 0);
    }

    #[test]
    fn does_not_case_fold_the_head() {
        assert_eq!(scanned("(LET loop ((i 0)) (display i))"), 0);
    }

    #[test]
    fn brackets_are_accepted_for_the_binding_list() {
        assert_eq!(findings("(let loop ([i 0]) (display i))").len(), 1);
    }

    #[test]
    fn the_fix_removes_only_the_loop_name() {
        assert_eq!(
            fixed("(let loop ((i 0)) (display i))"),
            "(let ((i 0)) (display i))"
        );
    }

    #[test]
    fn the_fix_survives_a_multi_line_form() {
        assert_eq!(
            fixed("(let loop\n    ((i 0))\n  (display i))"),
            "(let\n    ((i 0))\n  (display i))"
        );
    }

    #[test]
    fn a_comment_between_the_head_and_the_name_withholds_the_fix() {
        let found = findings("(let ; why\n  loop ((i 0)) (display i))");
        assert_eq!(found.len(), 1, "the finding is still reported");
        assert!(
            found[0].fix_span.is_none(),
            "a fix here would delete the comment"
        );

        // The same shape without the comment is the ordinary case and still
        // gets its fix — without this the assertion above would pass for the
        // wrong reason.
        let plain = findings("(let\n  loop ((i 0)) (display i))");
        assert!(plain[0].fix_span.is_some());
    }

    /// Reduced from `ice-9/match.upstream.scm:873`, one of the two false
    /// positives the Guile corpus produced. `loop` is a `syntax-rules` pattern
    /// variable and `body` is the caller's body: the recursive call is nowhere
    /// in the template, so the loop is not dead, it is not yet written.
    #[test]
    fn does_not_flag_a_named_let_in_a_syntax_rules_template() {
        let source = "(define-syntax match-named-let\n  (syntax-rules ()\n    ((_ loop ((pat expr var) ...) () . body)\n     (let loop ((var expr) ...)\n       (match-let ((pat var) ...)\n         . body)))))";
        assert_eq!(scanned(source), 1, "the form is still counted as scanned");
        assert!(findings(source).is_empty());
    }

    /// Reduced from `srfi/srfi-71.scm:42`, the other corpus false positive.
    #[test]
    fn does_not_flag_a_named_let_in_a_second_syntax_rules_template() {
        let source = "(define-syntax r5rs-let\n  (syntax-rules ()\n    ((r5rs-let tag ((v x) ...) body1 body ...)\n     (let tag ((v x) ...) body1 body ...))))";
        assert!(findings(source).is_empty());
    }

    /// The suppression is about macro *definitions*, not about every form that
    /// happens to sit near one. An ordinary named let beside a macro is still
    /// analysed, or the exemption above would silence the whole file.
    #[test]
    fn still_flags_a_named_let_outside_the_macro_definition_beside_it() {
        let source = "(define-syntax noop (syntax-rules () ((_) #f)))\n(define (f) (let loop ((i 0)) (display i)))";
        assert_eq!(findings(source).len(), 1);
    }

    #[test]
    fn finds_a_nested_named_let() {
        assert_eq!(
            findings("(define (f) (let loop ((i 0)) (display i)))").len(),
            1
        );
    }

    #[test]
    fn a_dialect_this_rule_does_not_model_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(let loop ((i 0)) (display i))", Dialect::CommonLisp)
                .expect("parse");
        let report =
            build_named_let_never_recurs_report(Path::new("a.lisp"), Dialect::CommonLisp, &tree)
                .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn racket_is_modelled_too() {
        let tree =
            SyntaxTree::parse_with_dialect("(let loop ([i 0]) (displayln i))", Dialect::Racket)
                .expect("parse");
        let report =
            build_named_let_never_recurs_report(Path::new("a.rkt"), Dialect::Racket, &tree)
                .expect("build report");
        assert!(report.dialect_modelled);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_name() {
        let report = report("(define (f)\n  (let loop ((i 0)) (display i)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "scheme-named-let-never-recurs");
        assert_eq!(finding.json_fields(), vec![("name", json!("loop"))]);
        assert_eq!(finding.text_columns(), vec!["name=loop".to_owned()]);
        assert_eq!(finding.message(), message_for("loop"));
    }
}
