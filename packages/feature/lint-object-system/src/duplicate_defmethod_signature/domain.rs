//! `duplicate-defmethod-signature` detection: two `defmethod`s in one file for
//! the same generic, with identical qualifiers *and* identical specializers.
//!
//! CLOS identifies a method by exactly that triple. Defining it twice does not
//! define two methods and does not signal: the second `defmethod` replaces the
//! first in the generic function, silently, and the first body is simply gone.
//! In a file that is loaded twice this is invisible; in a file that is loaded
//! once it means one of the two bodies has never run.
//!
//! The comparison normalizes what CLOS itself normalizes and nothing else:
//!
//! - names fold case and drop package qualification, as the reader does;
//! - an unspecialized required parameter is `t`, so `(x)` and `(x t)` are one
//!   specializer list;
//! - `&optional` and everything after it is dropped, because only required
//!   parameters are specialized. Two methods differing only in their optional
//!   arguments really do replace each other.
//!
//! The *later* definition is the one reported: it is the one that took effect,
//! and the one a reader has to look at to find out what was lost.
//!
//! # Overlap with `inspect duplicate-methods`
//!
//! `paredit-feature-lisp-analysis`'s `duplicate_method_report` asks a
//! neighbouring question. It is not registered as a lint rule, and the two
//! **disagree in both directions** — running both on one file can give
//! different answers, which is expected rather than a bug in either.
//!
//! - *What each answers.* This rule answers "does a later `defmethod` in this
//!   file silently replace an earlier one", as a lint finding with a severity
//!   and a suppression comment. The report answers "which methods across the
//!   scanned files share a dispatch identity", as a survey.
//! - *Where this rule is more precise.* The report's specializer reader falls
//!   back to `"T"` for any *list* specializer, so it reports `(eql 0)` and
//!   `(eql 1)` as duplicates of each other — they are two distinct methods.
//!   This rule renders an `(eql …)` specializer structurally and keeps them
//!   apart. That fallback also contradicts the report's own module doc; it is a
//!   **pre-existing bug in that package**, noted here and deliberately not
//!   fixed from this one.
//! - *Where this rule is more precise, second case.* The report skips
//!   `(setf name)` methods and methods carrying two qualifiers; this rule reads
//!   both.
//! - *Where the report is broader.* It correlates across every file passed to
//!   it, where this rule sees one at a time — a method redefined in another
//!   file is invisible here. It also reads `|…|` escaping correctly, so
//!   `|DRAW|` and `draw` stay distinct and `|draw|` keeps its case; this rule's
//!   normalization folds case straight through the bars and gets both wrong.
//!
//! Neither is a superset. Consolidating them is a separate change.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding, FindingSeverity};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, symbol_is};
use serde_json::{Value, json};

use crate::support::{self, MethodSignature};

#[derive(Debug, Clone)]
pub struct DuplicateDefmethodSignatureItem {
    /// The span of the later `defmethod` — the one that took effect.
    pub span: ByteSpan,
    pub generic: String,
    /// The qualifiers and specializers both definitions share, rendered the way
    /// they were compared.
    pub signature: String,
    /// Where the definition this one replaced starts, so a reader can go
    /// straight to the body that was lost.
    pub replaces_offset: usize,
}

impl Finding for DuplicateDefmethodSignatureItem {
    fn kind(&self) -> &'static str {
        "duplicate-defmethod-signature"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("generic={}", self.generic),
            format!("signature={}", self.signature),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("generic", json!(self.generic)),
            ("signature", json!(self.signature)),
            ("replaces_offset", json!(self.replaces_offset)),
        ]
    }

    /// An error, not the envelope's default warning: one of the two bodies
    /// provably never runs.
    fn severity(&self) -> FindingSeverity {
        FindingSeverity::Error
    }

    fn message(&self) -> String {
        format!(
            "this defmethod on {} repeats an earlier one's signature ({}): CLOS replaces the \
             earlier method rather than adding a second, so its body never runs",
            self.generic, self.signature
        )
    }
}

/// The qualifiers and specializers, rendered the way they were compared, so a
/// reader can see what made the two definitions the same method.
fn render_signature(signature: &MethodSignature) -> String {
    let mut parts = signature.qualifiers.clone();
    parts.push(format!("({})", signature.specializers.join(" ")));
    parts.join(" ")
}

pub fn examine_duplicate_defmethod_signature(
    tree: &SyntaxTree,
    view: &ExpressionView,
    defmethod_form_count: &mut usize,
    violations: &mut Vec<DuplicateDefmethodSignatureItem>,
) {
    if !support::calls_head(view, "defmethod") {
        return;
    }
    let Some(site) = support::locate(tree, view.span) else {
        return;
    };
    if site.quoted || !site.top_level {
        return;
    }
    *defmethod_form_count += 1;

    let Some(signature) = support::method_signature(tree, site.top_level_index) else {
        return;
    };

    // Only earlier definitions: the later one is the one that took effect, and
    // reporting both ends of every pair would say the same thing twice.
    let replaced = support::top_level_forms(tree)
        .take_while(|form| form.index < site.top_level_index)
        .filter(|form| symbol_is(form.head, "defmethod"))
        .find(|form| support::method_signature(tree, form.index).as_ref() == Some(&signature));

    if let Some(earlier) = replaced {
        violations.push(DuplicateDefmethodSignatureItem {
            span: view.span,
            generic: signature.name.clone(),
            signature: render_signature(&signature),
            replaces_offset: earlier.span.start().get(),
        });
    }
}

/// Collects every replaced `defmethod` in one file, with the number of
/// `defmethod` forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_duplicate_defmethod_signature_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DuplicateDefmethodSignatureItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("defmethod_form_count", json!(0))],
        ));
    }

    let mut defmethod_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_duplicate_defmethod_signature(
                tree,
                subview,
                &mut defmethod_form_count,
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
        vec![("defmethod_form_count", json!(defmethod_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<DuplicateDefmethodSignatureItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_duplicate_defmethod_signature_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build duplicate defmethod signature report")
    }

    fn violations(input: &str) -> Vec<DuplicateDefmethodSignatureItem> {
        report(input).findings
    }

    #[test]
    fn flags_the_second_of_two_identical_definitions() {
        let found = violations(
            "(defmethod draw ((s circle) stream) 1)\n(defmethod draw ((s circle) stream) 2)",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].generic, "draw");
        assert_eq!(found[0].signature, "(circle t)");
        assert!(
            found[0].span.start().get() > found[0].replaces_offset,
            "the later definition is the one reported"
        );
    }

    #[test]
    fn flags_a_repeat_whose_qualifiers_also_match() {
        let found = violations(
            "(defmethod draw :around ((s circle)) 1)\n(defmethod draw :around ((s circle)) 2)",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].signature, ":around (circle)");
    }

    #[test]
    fn treats_an_unspecialized_parameter_and_an_explicit_t_as_one_method() {
        let found = violations(
            "(defmethod draw ((s circle) stream) 1)\n(defmethod draw ((s circle) (stream t)) 2)",
        );
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn folds_case_and_package_qualification() {
        let found =
            violations("(defmethod cl-user:draw ((s Circle)) 1)\n(defmethod DRAW ((s circle)) 2)");
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn flags_each_later_repeat_of_a_signature_defined_three_times() {
        let found = violations(
            "(defmethod draw ((s circle)) 1)\n\
             (defmethod draw ((s circle)) 2)\n\
             (defmethod draw ((s circle)) 3)",
        );
        assert_eq!(found.len(), 2);
    }

    /// The near miss: same generic, different specializer — two real methods.
    #[test]
    fn does_not_flag_methods_on_different_specializers() {
        let found = violations("(defmethod draw ((s circle)) 1)\n(defmethod draw ((s square)) 2)");
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_methods_whose_qualifiers_differ() {
        let found =
            violations("(defmethod draw ((s circle)) 1)\n(defmethod draw :around ((s circle)) 2)");
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_methods_on_different_generics() {
        let found = violations("(defmethod draw ((s circle)) 1)\n(defmethod erase ((s circle)) 2)");
        assert!(found.is_empty());
    }

    #[test]
    fn distinguishes_two_eql_specializers_on_different_values() {
        let found =
            violations("(defmethod area ((n (eql 0))) 0)\n(defmethod area ((n (eql 1))) 1)");
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_quoted_repeat() {
        let found = violations(
            "(defmethod draw ((s circle)) 1)\n(setf template '(defmethod draw ((s circle)) 2))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_repeat_inside_an_unescaped_quasiquote() {
        let found = violations(
            "(defmethod draw ((s circle)) 1)\n(defmacro m () `(defmethod draw ((s circle)) 2))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_repeat_written_inside_a_string() {
        let found = violations(
            "(defmethod draw ((s circle)) 1)\n(defparameter *doc* \"(defmethod draw ((s circle)) 2)\")",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn the_denominator_counts_every_defmethod_scanned() {
        let built = violations("(defmethod draw ((s circle)) 1)");
        assert!(built.is_empty());
        let scanned = report("(defmethod draw ((s circle)) 1)\n(defmethod draw ((s circle)) 2)");
        assert_eq!(scanned.summary, vec![("defmethod_form_count", json!(2))]);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(
            "(defmethod draw ((s circle)) 1)\n(defmethod draw ((s circle)) 2)",
            Dialect::Clojure,
        )
        .expect("parse");
        let built = build_duplicate_defmethod_signature_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!built.dialect_modelled);
        assert!(built.findings.is_empty());
    }

    #[test]
    fn a_finding_is_an_error_and_carries_its_line() {
        let built = report("(defmethod draw ((s circle)) 1)\n(defmethod draw ((s circle)) 2)\n");
        let finding = &built.findings[0];
        assert_eq!(built.line_of(finding), 2);
        assert_eq!(finding.kind(), "duplicate-defmethod-signature");
        assert_eq!(finding.severity(), FindingSeverity::Error);
        assert!(finding.message().contains("never runs"));
    }
}
