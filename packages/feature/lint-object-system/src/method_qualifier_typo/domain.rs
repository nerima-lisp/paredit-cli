//! `method-qualifier-typo` detection: a `defmethod` qualifier that the standard
//! method combination does not define.
//!
//! Under standard method combination the qualifier set is closed: `:before`,
//! `:after`, `:around`, or none at all. Anything else — `:arround`, `:before!`,
//! `:primary` — is not a method that never runs; it is an error signalled when
//! the generic is first called, far from the `defmethod` that caused it. A
//! misspelling here is silent until then.
//!
//! **The rule is off for a file that defines its own method combination**, and
//! that exemption is the whole reason it can be reported at all. A qualifier
//! outside the three is perfectly valid under a non-standard combination:
//! `(defmethod total progn ((x thing)) …)` is correct code the moment some
//! generic declares `(:method-combination progn)`. So two things silence this
//! rule for the entire file:
//!
//! - a `define-method-combination` form, which introduces qualifiers this rule
//!   has no way to enumerate, and
//! - any `defgeneric` carrying a `(:method-combination …)` option.
//!
//! Either counts wherever the file compiler would process it as a top-level
//! form, which includes inside `eval-when`, `progn`, `locally`, `macrolet` and
//! `symbol-macrolet` — see `TOP_LEVEL_WRAPPERS`. Wrapping a
//! `define-method-combination` in `(eval-when (:compile-toplevel …) …)` so
//! later forms in the file can use it is ordinary, not exotic.
//!
//! Whole-file rather than per-generic on purpose: matching each method to the
//! generic whose combination governs it would need cross-file resolution, and
//! guessing it per name would turn a missing `defgeneric` into a false
//! positive.
//!
//! # Severity, and the hole the guard does not close
//!
//! This reports at **warning**, deliberately. The guard is whole-*file*, and
//! the combination that licenses a qualifier is routinely declared somewhere
//! else: `combinations.lisp` holding the `define-method-combination` and
//! `impl.lisp` holding `(defmethod total progn ((x thing)) …)` is a normal
//! layout, and from inside `impl.lisp` the correct code is indistinguishable
//! from the typo. A rule that cannot see across files must not assert `Error`
//! on what it finds there.
//!
//! `paredit-feature-lisp-analysis`'s `method_combination_report` takes the
//! stronger position on the same question — its `domain.rs` records an explicit
//! decision *not* to judge non-standard qualifiers at all, for this reason.
//! This rule judges them behind a guard and at a severity that says so.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding, FindingSeverity};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, symbol_in};
use serde_json::{Value, json};

use crate::support;

/// The complete qualifier vocabulary of the standard method combination.
const STANDARD_QUALIFIERS: [&str; 3] = [":before", ":after", ":around"];

#[derive(Debug, Clone)]
pub struct MethodQualifierTypoItem {
    /// The span of the offending qualifier, not of the whole `defmethod`.
    pub span: ByteSpan,
    pub qualifier: String,
    pub generic: String,
}

impl Finding for MethodQualifierTypoItem {
    fn kind(&self) -> &'static str {
        "method-qualifier-typo"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("qualifier={}", self.qualifier),
            format!("generic={}", self.generic),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("qualifier", json!(self.qualifier)),
            ("generic", json!(self.generic)),
        ]
    }

    /// A warning, not an error, and the guard below is why.
    ///
    /// Standard method combination really does signal on an unknown qualifier,
    /// so the *finding* would deserve `Error` — but only if the rule could tell
    /// that standard combination is in force, and it cannot. The combination
    /// governing a generic is routinely declared in another file (`protocol` in
    /// one, `methods` in another), and this rule sees one file. Asserting
    /// `Error` on a question it documents itself as unable to answer is the
    /// wrong trade: `Warning` is what the evidence supports.
    fn severity(&self) -> FindingSeverity {
        FindingSeverity::Warning
    }

    fn message(&self) -> String {
        format!(
            "{} is not a standard method qualifier on {}: standard method combination \
             defines only :before, :after and :around",
            self.qualifier, self.generic
        )
    }
}

/// The heads whose subforms the file compiler processes as top-level forms in
/// their own right (CLHS 3.2.3.1). A `define-method-combination` written inside
/// one of these is as real as one written at the top, and wrapping definitions
/// in `(eval-when (:compile-toplevel :load-toplevel :execute) …)` is the
/// ordinary way to make them available to later forms in the same file.
const TOP_LEVEL_WRAPPERS: [&str; 5] = [
    "eval-when",
    "progn",
    "locally",
    "macrolet",
    "symbol-macrolet",
];

/// Whether the file declares a method combination of its own, under which
/// arbitrary qualifiers are valid.
///
/// Reached only once a suspect qualifier has already been found, so a file full
/// of well-qualified methods never pays for this scan.
fn defines_a_method_combination(tree: &SyntaxTree) -> bool {
    support::top_level_forms(tree)
        .filter_map(|form| support::top_level_view(tree, form.index))
        .any(|view| declares_a_combination(&view))
}

/// Whether `view` — a form the compiler processes at top level — introduces a
/// method combination, looking through [`TOP_LEVEL_WRAPPERS`] to the forms
/// inside them.
///
/// Every wrapper's whole body is scanned rather than the exact body offset per
/// wrapper. Over-scanning here can only *silence* this rule, which is the safe
/// direction: the finding it suppresses would have been a claim about code
/// under a combination this pass cannot enumerate.
fn declares_a_combination(view: &ExpressionView) -> bool {
    if support::is_quoted_here(view) {
        return false;
    }
    if support::calls_head(view, "define-method-combination") {
        return true;
    }
    if support::calls_head(view, "defgeneric") {
        return view.children.iter().skip(3).any(|option| {
            option
                .children
                .first()
                .and_then(atom_text)
                .is_some_and(|key| key.eq_ignore_ascii_case(":method-combination"))
        });
    }
    if TOP_LEVEL_WRAPPERS
        .iter()
        .any(|head| support::calls_head(view, head))
    {
        return view.children.iter().skip(1).any(declares_a_combination);
    }
    false
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_method_qualifier_typo(
    tree: &SyntaxTree,
    view: &ExpressionView,
    qualifier_count: &mut usize,
    violations: &mut Vec<MethodQualifierTypoItem>,
) {
    let Some(method) = support::method_form(view) else {
        return;
    };
    if method.qualifiers.is_empty() {
        return;
    }
    let Some(site) = support::locate(tree, view.span) else {
        return;
    };
    if site.quoted || !site.top_level {
        return;
    }
    *qualifier_count += method.qualifiers.len();

    let suspect: Vec<&&ExpressionView> = method
        .qualifiers
        .iter()
        .filter(|qualifier| {
            support::symbol_text(qualifier)
                .is_none_or(|text| !symbol_in(text, &STANDARD_QUALIFIERS))
        })
        .collect();
    if suspect.is_empty() || defines_a_method_combination(tree) {
        return;
    }

    let generic = support::definition_name(view).unwrap_or_else(|| "an unnamed generic".to_owned());
    for qualifier in suspect {
        violations.push(MethodQualifierTypoItem {
            span: qualifier.span,
            qualifier: support::symbol_text(qualifier)
                .unwrap_or_default()
                .to_owned(),
            generic: generic.clone(),
        });
    }
}

/// Collects every unrecognized method qualifier in one file, with the number of
/// qualifiers scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: the closed qualifier set is Common Lisp's standard method
/// combination, not a general Lisp idea.
pub fn build_method_qualifier_typo_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<MethodQualifierTypoItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("qualifier_count", json!(0))],
        ));
    }

    let mut qualifier_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_method_qualifier_typo(tree, subview, &mut qualifier_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("qualifier_count", json!(qualifier_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<MethodQualifierTypoItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_method_qualifier_typo_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build method qualifier typo report")
    }

    fn violations(input: &str) -> Vec<MethodQualifierTypoItem> {
        report(input).findings
    }

    #[test]
    fn flags_a_misspelled_qualifier() {
        let found = violations("(defmethod draw :arround ((s circle)) s)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].qualifier, ":arround");
        assert_eq!(found[0].generic, "draw");
    }

    #[test]
    fn flags_a_qualifier_that_is_not_a_keyword_at_all() {
        let found = violations("(defmethod draw around ((s circle)) s)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].qualifier, "around");
    }

    #[test]
    fn flags_every_suspect_qualifier_on_one_method() {
        let found = violations("(defmethod draw :arround :befor ((s circle)) s)");
        assert_eq!(found.len(), 2);
    }

    /// The near miss: all three standard qualifiers, in any case.
    #[test]
    fn does_not_flag_the_standard_qualifiers() {
        let found = violations(
            "(defmethod draw :before ((s circle)) s)\n\
             (defmethod draw :after ((s circle)) s)\n\
             (defmethod draw :AROUND ((s circle)) (call-next-method))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_method_with_no_qualifier() {
        let found = violations("(defmethod draw ((s circle)) s)");
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_setf_method_whose_name_is_a_list() {
        let found = violations("(defmethod (setf width) (v (s circle)) v)");
        assert!(
            found.is_empty(),
            "the name is at index 1 and is never read as a qualifier"
        );
    }

    #[test]
    fn a_file_defining_its_own_method_combination_is_exempt() {
        let found = violations(
            "(define-method-combination progn :identity-with-one-argument t)\n\
             (defmethod total progn ((x thing)) (weight x))",
        );
        assert!(found.is_empty());
    }

    /// The reported false positive: the exemption scanned only bare top-level
    /// forms, so the ordinary `eval-when` wrapper that makes a combination
    /// available to the rest of the file hid it.
    #[test]
    fn a_method_combination_inside_an_eval_when_exempts_the_file() {
        let found = violations(
            "(eval-when (:compile-toplevel :load-toplevel :execute)\n  \
             (define-method-combination progn :identity-with-one-argument t))\n\
             (defmethod total progn ((x thing)) (weight x))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn the_other_top_level_wrappers_exempt_the_file_too() {
        for wrapper in [
            "(progn (define-method-combination progn))",
            "(locally (define-method-combination progn))",
            "(macrolet () (define-method-combination progn))",
            "(symbol-macrolet () (define-method-combination progn))",
            "(eval-when (:execute) (progn (define-method-combination progn)))",
            "(eval-when (:execute) (defgeneric total (x) (:method-combination progn)))",
        ] {
            let found = violations(&format!(
                "{wrapper}\n(defmethod total progn ((x thing)) (weight x))"
            ));
            assert!(found.is_empty(), "for {wrapper}");
        }
    }

    /// The near miss for the wrapper walk: a `define-method-combination` that
    /// is quoted data defines nothing, so it must not exempt anything.
    #[test]
    fn a_quoted_method_combination_inside_a_wrapper_does_not_exempt() {
        let found = violations(
            "(progn '(define-method-combination progn))\n\
             (defmethod total progn ((x thing)) (weight x))",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].qualifier, "progn");
    }

    #[test]
    fn a_generic_declaring_a_method_combination_exempts_the_file() {
        let found = violations(
            "(defgeneric total (x) (:method-combination progn))\n\
             (defmethod total progn ((x thing)) (weight x))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn a_generic_with_ordinary_options_does_not_exempt_the_file() {
        let found = violations(
            "(defgeneric draw (s) (:documentation \"Draw S.\"))\n\
             (defmethod draw :arround ((s circle)) s)",
        );
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn does_not_flag_a_quoted_defmethod() {
        let found = violations("(setf template '(defmethod draw :arround ((s circle)) s))");
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_defmethod_inside_an_unescaped_quasiquote() {
        let found = violations("(defmacro wrap () `(defmethod draw :arround ((s circle)) s))");
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_defmethod_written_inside_a_string() {
        let found = violations("(defparameter *doc* \"(defmethod draw :arround ((s c)) s)\")");
        assert!(found.is_empty());
    }

    #[test]
    fn the_denominator_counts_every_qualifier_scanned() {
        let built = report(
            "(defmethod draw :before ((s circle)) s)\n(defmethod draw :arround ((s square)) s)",
        );
        assert_eq!(built.summary, vec![("qualifier_count", json!(2))]);
        assert_eq!(built.findings.len(), 1);
    }

    /// A warning, not an error. The exemption above is whole-*file*, and a
    /// combination declared in another file leaks straight through it, so this
    /// rule must not assert more than it can see.
    #[test]
    fn a_finding_is_a_warning_and_carries_its_line() {
        let built = report("(defun f ())\n(defmethod draw :arround ((s circle)) s)\n");
        let finding = &built.findings[0];
        assert_eq!(built.line_of(finding), 2);
        assert_eq!(finding.kind(), "method-qualifier-typo");
        assert_eq!(finding.severity(), FindingSeverity::Warning);
        assert_eq!(
            finding.text_columns(),
            vec!["qualifier=:arround".to_owned(), "generic=draw".to_owned()]
        );
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(defmethod draw :arround ((s c)) s)", Dialect::Clojure)
                .expect("parse");
        let built =
            build_method_qualifier_typo_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build report");
        assert!(!built.dialect_modelled);
        assert!(built.findings.is_empty());
    }
}
