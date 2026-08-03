//! `racket-parameterize-empty-bindings`: a `parameterize` that rebinds nothing.
//!
//! `(parameterize () ⟨body⟩ …)` installs no parameterization, so it is exactly
//! its own body run in an internal-definition context — the extension of the
//! continuation mark frame is not observable to the program. Verified on Racket
//! v9.2: `(parameterize () 'body-ran)` evaluates to `'body-ran` and compiles
//! without a word.
//!
//! It is nearly always the residue of an edit that removed the last binding, or
//! a binding list built by a macro that expanded to nothing — in which case the
//! *macro* is the thing to look at, which is why the finding names the form
//! rather than proposing a rewrite.
//!
//! # Report-only, deliberately
//!
//! The obvious fix is wrong often enough not to ship. Replacing the form with
//! its body is only valid where the body's several forms are legal in the
//! surrounding position, which is true in a body context and false in an
//! expression one; replacing it with `(let () …)` is always valid but is not
//! obviously an improvement over leaving the `parameterize` in place, and it
//! changes an internal-definition context into a different internal-definition
//! context in a way a reader has to re-check. Neither is mechanical enough for
//! `--fix`, so this rule is `ReportOnly`.
//!
//! # Honest denominator
//!
//! This shape does not occur in the audited corpus at all: **0** occurrences of
//! `(parameterize ()` across 4492 files of `racket/racket` and
//! `racket/typed-racket`, against 1188 `parameterize` forms scanned. The clean
//! sweep is therefore real — the corpus exercises the anchored head heavily —
//! but the rule has no positive evidence from third-party code, only from its
//! own dangerous twin. It is reported that way rather than as a proven catch.
//!
//! Scope: Racket only. `parameterize` is Racket's; R7RS spells the same idea
//! `parameterize` too, but the Scheme package owns that surface.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use serde_json::{Value, json};

use crate::support::{heads_with, is_inert_at, is_racket_list};

/// The head this rule anchors on, shared with its `HeadFilter`.
pub const HEAD: &str = "parameterize";

/// The dialects this rule models.
pub const DIALECTS: [Dialect; 1] = [Dialect::Racket];

#[derive(Debug, Clone)]
pub struct ParameterizeEmptyBindingsItem {
    /// The span of the whole `(parameterize () …)` form.
    pub span: ByteSpan,
    /// The span of the empty binding list, which is the part at fault.
    pub bindings_span: ByteSpan,
    /// How many body forms the wrapper is doing nothing to.
    pub body_form_count: usize,
}

impl Finding for ParameterizeEmptyBindingsItem {
    fn kind(&self) -> &'static str {
        "racket-parameterize-empty-bindings"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("body_form_count={}", self.body_form_count)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("body_form_count", json!(self.body_form_count))]
    }

    fn message(&self) -> String {
        MESSAGE.to_owned()
    }
}

pub const MESSAGE: &str = "parameterize rebinds no parameter, so it is exactly its own body; check whether a binding \
     was removed or a macro expanded to none";

/// Examines one node, which the caller has already narrowed to a
/// `parameterize` head.
pub fn examine_parameterize(
    tree: &SyntaxTree,
    view: &ExpressionView,
    parameterize_form_count: &mut usize,
    violations: &mut Vec<ParameterizeEmptyBindingsItem>,
) {
    if !heads_with(view, HEAD) {
        return;
    }
    *parameterize_form_count += 1;

    // children: [parameterize, bindings, body…].
    let Some(bindings) = view.children.get(1) else {
        return;
    };
    // A binding list is a list. An atom there is malformed — a macro's pattern
    // variable, most likely — and not this rule's complaint to make.
    if !is_racket_list(bindings) {
        return;
    }
    if !bindings.children.is_empty() {
        return;
    }
    // A `parameterize` with no body at all is a syntax error, not an idiom.
    let body_form_count = view.children.len() - 2;
    if body_form_count == 0 {
        return;
    }

    if is_inert_at(tree, view.span) {
        return;
    }

    violations.push(ParameterizeEmptyBindingsItem {
        span: view.span,
        bindings_span: bindings.span,
        body_form_count,
    });
}

/// Collects every empty-binding `parameterize` in one file, with the number of
/// `parameterize` forms scanned as the denominator beside them.
pub fn build_parameterize_empty_bindings_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ParameterizeEmptyBindingsItem>> {
    let modelled = DIALECTS.contains(&dialect);
    let mut parameterize_form_count = 0;
    let mut violations = Vec::new();

    if modelled {
        let root = tree.root_view();
        let mut stack: Vec<&ExpressionView> = root.children.iter().rev().collect();
        while let Some(view) = stack.pop() {
            examine_parameterize(tree, view, &mut parameterize_form_count, &mut violations);
            stack.extend(view.children.iter().rev());
        }
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        tree.source(),
        violations,
        vec![("parameterize_form_count", json!(parameterize_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<ParameterizeEmptyBindingsItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Racket).expect("parse input");
        build_parameterize_empty_bindings_report(Path::new("main.rkt"), Dialect::Racket, &tree)
            .expect("build report")
    }

    fn findings(input: &str) -> Vec<ParameterizeEmptyBindingsItem> {
        report(input).findings
    }

    fn scanned(input: &str) -> u64 {
        report(input)
            .summary
            .iter()
            .find(|(name, _)| *name == "parameterize_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("parameterize_form_count in the summary")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_an_empty_binding_list() {
        let source = "(parameterize () (run))";
        let found = findings(source);
        assert_eq!(found.len(), 1);
        assert_eq!(slice(source, found[0].bindings_span), "()");
        assert_eq!(found[0].body_form_count, 1);
    }

    #[test]
    fn counts_every_body_form() {
        assert_eq!(
            findings("(parameterize () (a) (b) (c))")[0].body_form_count,
            3
        );
    }

    #[test]
    fn a_bracketed_empty_binding_list_is_found() {
        assert_eq!(findings("(parameterize [] (run))").len(), 1);
    }

    #[test]
    fn does_not_flag_a_parameterize_that_binds_something() {
        let source = "(parameterize ([current-output-port p]) (run))";
        assert_eq!(scanned(source), 1);
        assert!(findings(source).is_empty());
    }

    #[test]
    fn does_not_flag_a_parameterize_with_no_body() {
        assert_eq!(scanned("(parameterize ())"), 1);
        assert!(findings("(parameterize ())").is_empty());
    }

    #[test]
    fn does_not_flag_a_parameterize_with_no_binding_list_at_all() {
        assert!(findings("(parameterize)").is_empty());
    }

    #[test]
    fn does_not_flag_an_atom_in_binding_list_position() {
        assert!(findings("(parameterize bindings (run))").is_empty());
    }

    #[test]
    fn does_not_case_fold_the_head() {
        assert_eq!(scanned("(PARAMETERIZE () (run))"), 0);
    }

    #[test]
    fn does_not_flag_a_qualified_head_that_merely_ends_in_parameterize() {
        assert_eq!(scanned("(racket:parameterize () (run))"), 0);
    }

    /// `parameterize*` is a different operator with the same shape; this rule
    /// is not registered for it and must not answer for it.
    #[test]
    fn does_not_flag_parameterize_star() {
        assert_eq!(scanned("(parameterize* () (run))"), 0);
    }

    #[test]
    fn does_not_flag_a_quoted_parameterize_shape() {
        assert!(findings("'(parameterize () (run))").is_empty());
        assert!(findings("(quote (parameterize () (run)))").is_empty());
        assert!(findings("`(a (parameterize () (run)))").is_empty());
    }

    #[test]
    fn flags_an_unquoted_parameterize_inside_a_quasiquote() {
        assert_eq!(findings("`(a ,(parameterize () (run)))").len(), 1);
    }

    #[test]
    fn does_not_flag_a_vector_constant_that_looks_like_a_parameterize_call() {
        assert_eq!(scanned("#(parameterize () (run))"), 0);
    }

    #[test]
    fn does_not_flag_a_parameterize_inside_a_macro_template() {
        assert!(
            findings("(define-syntax m (syntax-rules () ((_ b) (parameterize () b))))").is_empty()
        );
    }

    #[test]
    fn the_summary_counts_every_parameterize_scanned() {
        let source = "(parameterize () (a))\n(parameterize ([p v]) (b))\n";
        assert_eq!(scanned(source), 2);
        assert_eq!(findings(source).len(), 1);
    }

    #[test]
    fn the_same_bytes_are_flagged_as_racket_and_unmodelled_elsewhere() {
        let source = "(parameterize () (run))\n";
        assert_eq!(findings(source).len(), 1);
        for dialect in [Dialect::Scheme, Dialect::CommonLisp, Dialect::Clojure] {
            let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
            let report =
                build_parameterize_empty_bindings_report(Path::new("f.scm"), dialect, &tree)
                    .expect("build report");
            assert!(!report.dialect_modelled, "{dialect:?}");
            assert!(report.findings.is_empty(), "{dialect:?}");
        }
    }

    #[test]
    fn a_finding_carries_its_line_and_its_fields() {
        let report = report("#lang racket\n(parameterize () (run))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "racket-parameterize-empty-bindings");
        assert_eq!(finding.json_fields(), vec![("body_form_count", json!(1))]);
        assert_eq!(finding.message(), MESSAGE);
    }
}
