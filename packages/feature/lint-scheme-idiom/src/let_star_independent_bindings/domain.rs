//! `scheme-let-star-independent-bindings` detection: a `let*` with two or more
//! bindings, none of whose initializers can depend on another, so the
//! sequential scope `let*` exists to provide is unused.
//!
//! Two things have to hold before `let*` can be called `let`, and getting
//! either wrong silently changes what the program means.
//!
//! **Independence is about the whole group, not the prefix.** `let*` scopes
//! each initializer over the names bound *before* it, so the obvious test —
//! "does an initializer mention an earlier name" — is not enough. Given
//! `(let ((x 1)) (let* ((x 2) (y x)) y))`, the `y` initializer mentions a name
//! bound in the same group but not before itself under a naive reading of the
//! source order; under `let*` it reads the new `x` and the form is 2, under
//! `let` it reads the outer `x` and the form is 1. The condition enforced here
//! is therefore that no initializer names *any* variable the same `let*` binds.
//!
//! **Evaluation order is not the same guarantee.** R7RS 4.2.2 evaluates a
//! `let*` initializer list strictly left to right, while a `let`'s
//! initializers "are evaluated in the current environment (in some unspecified
//! order)". Two initializers that are independent by name can still be ordered
//! by effect — `(let* ((a (read)) (b (read))) …)` binds `a` to the first datum
//! under `let*` and to either datum under `let`. So this rule also requires
//! every initializer to be a form with no evaluation order to observe: an atom
//! (a literal or a variable reference) or a quoted datum. That restriction is
//! what makes the rewrite sound rather than merely plausible, and it is the
//! reason a `let*` over procedure calls is never reported here, independent or
//! not.
//!
//! A duplicate binding name is a third exclusion, and a hard one: R7RS 4.2.2
//! requires a `let`'s variables to be distinct, while `let*` permits a name to
//! be rebound. `(let* ((x 1) (x 2)) x)` is legal; the `let` it would become is
//! not.
//!
//! Scope: Scheme and Racket. `redundant-let-star` in
//! `paredit-feature-lint-form-shape` is `COMMON_LISP_ONLY` and covers the
//! zero- and one-binding shapes only — it declines two or more bindings
//! explicitly (`packages/feature/lint-form-shape/src/redundant_let_star/domain.rs:113`).
//! This rule starts where that one stops, so the two cannot both fire on one
//! form even if that rule's dialect scope is widened later.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, ReaderPrefix, SyntaxTree};
use serde_json::{Value, json};

use crate::support::{heads_with, is_scheme_list, is_unevaluated_at, scheme_atom, scheme_head};

/// The head this rule anchors on, shared with its `HeadFilter`.
pub const HEAD: &str = "let*";

/// The dialects this rule models.
pub const DIALECTS: [Dialect; 2] = [Dialect::Scheme, Dialect::Racket];

/// The smallest group this rule speaks about. One binding or none is
/// `redundant-let-star`'s subject, and repeating it here would mean two rules
/// reporting one form the moment that rule's dialect scope widened.
const MIN_BINDINGS: usize = 2;

#[derive(Debug, Clone)]
pub struct LetStarIndependentBindingsItem {
    /// The span of the whole `(let* … …)` form.
    pub span: ByteSpan,
    /// The span of the `let*` head symbol, which a fix replaces with `let`.
    pub head_span: ByteSpan,
    /// How many bindings were found independent.
    pub binding_count: usize,
}

impl Finding for LetStarIndependentBindingsItem {
    fn kind(&self) -> &'static str {
        "scheme-let-star-independent-bindings"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("bindings={}", self.binding_count)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("binding_count", json!(self.binding_count))]
    }

    fn message(&self) -> String {
        message_for(self.binding_count)
    }
}

/// The one sentence both the standalone report and the lint rule write.
#[must_use]
pub fn message_for(binding_count: usize) -> String {
    format!(
        "let* with {binding_count} independent bindings is just let; \
         no initializer names another binding of the group"
    )
}

/// Whether `view` is an initializer whose evaluation has no order to observe:
/// an atom, or a quoted datum.
///
/// Both are what R7RS 4.1 calls a literal or a variable reference — nothing
/// that can read, print, mutate, or diverge. Anything else, including a call
/// this crate could guess is pure, is refused: guessing purity is how a lint
/// rule ends up rewriting `(let* ((a (read)) (b (read))) …)`.
fn is_order_independent_init(view: &ExpressionView) -> bool {
    if view.reader_prefixes.contains(&ReaderPrefix::Quote) {
        return true;
    }
    scheme_atom(view).is_some() || scheme_head(view) == Some("quote")
}

/// The name a binding introduces, given a well-formed `(⟨variable⟩ ⟨init⟩)`
/// pair, together with its initializer. `None` for any other shape — a
/// malformed binding list is a different rule's subject, and guessing at it
/// here would be guessing at what the author meant.
fn binding_parts(view: &ExpressionView) -> Option<(&str, &ExpressionView)> {
    if !is_scheme_list(view) {
        return None;
    }
    let [name, init] = view.children.as_slice() else {
        return None;
    };
    Some((scheme_atom(name)?, init))
}

/// Examines one node.
pub fn examine_let_star(
    tree: &SyntaxTree,
    view: &ExpressionView,
    let_star_form_count: &mut usize,
    violations: &mut Vec<LetStarIndependentBindingsItem>,
) {
    if !heads_with(view, HEAD) {
        return;
    }
    *let_star_form_count += 1;

    let Some(binding_list) = view.children.get(1) else {
        return;
    };
    if !is_scheme_list(binding_list) || binding_list.children.len() < MIN_BINDINGS {
        return;
    }

    let mut names: Vec<&str> = Vec::with_capacity(binding_list.children.len());
    let mut inits: Vec<&ExpressionView> = Vec::with_capacity(binding_list.children.len());
    for binding in &binding_list.children {
        let Some((name, init)) = binding_parts(binding) else {
            return;
        };
        if !is_order_independent_init(init) {
            return;
        }
        // R7RS 4.2.2: a `let`'s variables must be distinct, a `let*`'s need
        // not be. Rewriting a rebinding group would produce an error, not a
        // simplification.
        if names.contains(&name) {
            return;
        }
        names.push(name);
        inits.push(init);
    }

    // An initializer is a reference to a sibling exactly when it is the bare
    // symbol of one. A quoted datum is not: the parser keeps `'` in an atom's
    // own text, so `'x` and `x` are already different strings here, and every
    // other shape was refused above.
    let depends_on_the_group = inits
        .iter()
        .filter_map(|init| scheme_atom(init))
        .any(|text| names.contains(&text));
    if depends_on_the_group {
        return;
    }
    // Last: the only non-node-local check. See `crate::support::node_context`.
    if is_unevaluated_at(tree, view.span) {
        return;
    }

    violations.push(LetStarIndependentBindingsItem {
        span: view.span,
        head_span: view.children[0].span,
        binding_count: names.len(),
    });
}

/// Collects every convertible `let*` in one file, with the number of `let*`
/// forms scanned as the denominator beside them.
pub fn build_let_star_independent_bindings_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<LetStarIndependentBindingsItem>> {
    if !DIALECTS.contains(&dialect) {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("let_star_form_count", json!(0))],
        ));
    }

    let mut let_star_form_count = 0;
    let mut violations = Vec::new();
    let root = tree.root_view();
    let mut stack: Vec<&ExpressionView> = root.children.iter().rev().collect();
    while let Some(view) = stack.pop() {
        examine_let_star(tree, view, &mut let_star_form_count, &mut violations);
        stack.extend(view.children.iter().rev());
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("let_star_form_count", json!(let_star_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<LetStarIndependentBindingsItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Scheme).expect("parse input");
        build_let_star_independent_bindings_report(Path::new("test.scm"), Dialect::Scheme, &tree)
            .expect("build let* report")
    }

    fn findings(input: &str) -> Vec<LetStarIndependentBindingsItem> {
        report(input).findings
    }

    fn scanned(input: &str) -> u64 {
        report(input)
            .summary
            .iter()
            .find(|(name, _)| *name == "let_star_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("let_star_form_count in the summary")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_two_literal_bindings() {
        let source = "(let* ((x 1) (y 2)) (+ x y))";
        let found = findings(source);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].binding_count, 2);
        assert_eq!(slice(source, found[0].head_span), "let*");
    }

    #[test]
    fn flags_bracketed_bindings() {
        assert_eq!(findings("(let* ([x 1] [y 2]) (+ x y))").len(), 1);
    }

    #[test]
    fn flags_free_variable_references() {
        // `a` and `b` are bound outside the group, so neither initializer can
        // see anything the group introduces.
        assert_eq!(findings("(let* ((x a) (y b)) (+ x y))").len(), 1);
    }

    #[test]
    fn flags_quoted_data_initializers() {
        assert_eq!(findings("(let* ((x '(1 2)) (y 'sym)) x)").len(), 1);
        assert_eq!(findings("(let* ((x (quote (1 2))) (y 3)) x)").len(), 1);
    }

    /// `'x` names no variable, so a group whose only apparent dependency is a
    /// quoted symbol really is independent.
    #[test]
    fn a_quoted_symbol_spelled_like_a_sibling_is_not_a_dependency() {
        assert_eq!(findings("(let* ((x 1) (y 'x)) y)").len(), 1);
    }

    #[test]
    fn does_not_flag_a_forward_dependency() {
        assert_eq!(scanned("(let* ((x 1) (y x)) y)"), 1);
        assert!(findings("(let* ((x 1) (y x)) y)").is_empty());
    }

    /// The case a "does an initializer mention an *earlier* name" test gets
    /// wrong. `(let ((x 1)) (let* ((x 2) (y x)) y))` is 2; the `let` rewrite
    /// would be 1.
    #[test]
    fn does_not_flag_a_shadowing_group_whose_later_init_reads_the_new_name() {
        let found = findings("(let ((x 1)) (let* ((x 2) (y x)) y))");
        assert!(found.is_empty());
    }

    /// The mirror image: an initializer naming a *later* binding of the same
    /// group is refused too, even though `let*` and `let` happen to agree
    /// there. Being wrong in the safe direction is the point.
    #[test]
    fn does_not_flag_an_init_that_names_a_later_binding() {
        assert!(findings("(let ((y 1)) (let* ((x y) (y 2)) x))").is_empty());
    }

    #[test]
    fn does_not_flag_call_initializers() {
        // `(read)` twice: independent by name, ordered by effect. R7RS 4.2.2
        // leaves a `let`'s initializer order unspecified.
        assert_eq!(scanned("(let* ((a (read)) (b (read))) (list a b))"), 1);
        assert!(findings("(let* ((a (read)) (b (read))) (list a b))").is_empty());
        assert!(findings("(let* ((x (f 1)) (y 2)) x)").is_empty());
    }

    #[test]
    fn does_not_flag_a_quasiquoted_initializer_that_can_evaluate() {
        assert!(findings("(let* ((x `(a ,(f))) (y 2)) x)").is_empty());
    }

    #[test]
    fn does_not_flag_duplicate_binding_names() {
        // Legal `let*`, illegal `let` (R7RS 4.2.2 requires distinct variables).
        assert_eq!(scanned("(let* ((x 1) (x 2)) x)"), 1);
        assert!(findings("(let* ((x 1) (x 2)) x)").is_empty());
    }

    #[test]
    fn does_not_flag_one_or_zero_bindings() {
        // `redundant-let-star`'s subject, deliberately left to it.
        assert_eq!(scanned("(let* ((x 1)) x)"), 1);
        assert!(findings("(let* ((x 1)) x)").is_empty());
        assert!(findings("(let* () x)").is_empty());
    }

    #[test]
    fn does_not_flag_a_malformed_binding() {
        assert!(findings("(let* ((x) (y 2)) y)").is_empty());
        assert!(findings("(let* ((x 1 2) (y 2)) y)").is_empty());
        assert!(findings("(let* (x (y 2)) y)").is_empty());
        assert!(findings("(let* (((f a) 1) (y 2)) y)").is_empty());
    }

    #[test]
    fn does_not_flag_a_non_list_binding_list() {
        assert!(findings("(let* x (y 2))").is_empty());
    }

    #[test]
    fn does_not_flag_a_quoted_let_star_shape() {
        assert!(findings("'(let* ((x 1) (y 2)) x)").is_empty());
    }

    #[test]
    fn does_not_flag_a_vector_constant_that_looks_like_a_let_star() {
        assert_eq!(scanned("#(let* ((x 1) (y 2)) x)"), 0);
    }

    #[test]
    fn does_not_case_fold_the_head() {
        assert_eq!(scanned("(LET* ((x 1) (y 2)) x)"), 0);
    }

    #[test]
    fn finds_a_nested_let_star() {
        assert_eq!(
            findings("(define (f) (let* ((x 1) (y 2)) (+ x y)))").len(),
            1
        );
    }

    #[test]
    fn the_summary_counts_every_let_star_scanned() {
        assert_eq!(
            scanned("(let* ((x 1) (y 2)) x)\n(let* ((a 1) (b a)) b)\n"),
            2
        );
        assert_eq!(
            findings("(let* ((x 1) (y 2)) x)\n(let* ((a 1) (b a)) b)\n").len(),
            1
        );
    }

    #[test]
    fn a_dialect_this_rule_does_not_model_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(let* ((x 1) (y 2)) x)", Dialect::CommonLisp)
            .expect("parse");
        let report = build_let_star_independent_bindings_report(
            Path::new("a.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn racket_is_modelled_too() {
        let tree = SyntaxTree::parse_with_dialect("(let* ([x 1] [y 2]) x)", Dialect::Racket)
            .expect("parse");
        let report =
            build_let_star_independent_bindings_report(Path::new("a.rkt"), Dialect::Racket, &tree)
                .expect("build report");
        assert!(report.dialect_modelled);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_binding_count() {
        let report = report("(define (f)\n  (let* ((x 1) (y 2)) (+ x y)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "scheme-let-star-independent-bindings");
        assert_eq!(finding.json_fields(), vec![("binding_count", json!(2))]);
        assert_eq!(finding.text_columns(), vec!["bindings=2".to_owned()]);
        assert_eq!(finding.message(), message_for(2));
    }
}
