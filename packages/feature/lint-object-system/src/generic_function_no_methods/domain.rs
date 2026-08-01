//! `generic-function-no-methods` detection: a `defgeneric` that no `defmethod`
//! in the same file ever specializes.
//!
//! A generic function that nothing in its own file specializes is usually a
//! rename that missed one side, or a method deleted without its declaration.
//!
//! It is *not* necessarily broken, and this rule does not say it is. Splitting
//! `protocol.lisp` from `methods.lisp` is ordinary ASDF practice, and from
//! inside `protocol.lisp` the two look identical. So the finding is phrased as
//! what one file establishes — no `defmethod` here specializes it — with the
//! runtime consequence stated conditionally, and it reports at the lowest
//! severity the engine has.
//!
//! Four things count as supplying a method, and any of them silences this:
//!
//! - a `defmethod` for the same name, anywhere at the file's top level;
//! - a `(:method …)` option inside the `defgeneric` itself, which defines one
//!   without a separate form;
//! - an explicit `add-method` call anywhere in the file;
//! - `(defgeneric … (:generic-function-class …))`, which hands dispatch to a
//!   metaclass this rule cannot read.
//!
//! **Same file only, and that is the rule's real limit.** A generic declared in
//! one file and specialized in twenty others is the normal shape of a CLOS
//! protocol, and it looks exactly like the defect from inside one file. What
//! makes this reportable anyway is the same judgement `lint-convention`'s
//! `method-lambda-list-mismatch` already makes: within one file, a `defgeneric`
//! with no `defmethod` beside it is worth a look.
//!
//! # Overlap with `inspect generic-dispatch`
//!
//! `paredit-feature-lisp-analysis`'s `generic_dispatch_report` — reachable as
//! `inspect generic-dispatch`, and not registered as a lint rule — answers a
//! neighbouring question, and the two **disagree in both directions**. Running
//! both today can give different answers on one file; that is expected, and
//! this is where it is written down.
//!
//! - *What each answers.* This rule answers "does any `defmethod` in this file
//!   specialize this generic", one `defgeneric` at a time, as a lint finding
//!   with a severity and a suppression comment. The report answers the broader
//!   "what does dispatch look like here", and is a survey rather than a
//!   judgement.
//! - *Where this rule is quieter.* It has three silencers the report does not:
//!   a `(:method …)` option inside the `defgeneric`, a
//!   `(:generic-function-class …)` option handing dispatch to a metaclass
//!   neither can read, and an `add-method` call anywhere in the file.
//! - *Where the report is quieter.* It drops `(setf name)` generics entirely;
//!   this rule matches them to their `(setf name)` methods.
//!
//! Neither is a superset. Consolidating them is a separate change, and would
//! have to decide which of the two questions survives.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, symbol_is};
use serde_json::{Value, json};

use crate::support;

#[derive(Debug, Clone)]
pub struct GenericFunctionNoMethodsItem {
    /// The span of the whole `defgeneric` form.
    pub span: ByteSpan,
    pub generic: String,
}

impl Finding for GenericFunctionNoMethodsItem {
    fn kind(&self) -> &'static str {
        "generic-function-no-methods"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("generic={}", self.generic)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("generic", json!(self.generic))]
    }

    /// States what one file can establish and no more.
    ///
    /// The old wording claimed the call "signals `no-applicable-method`", which
    /// is a claim about run time that this pass cannot make: the methods may be
    /// in `methods.lisp` next door. What *is* known is that no `defmethod` here
    /// specializes it, so that is what this says, and the consequence is
    /// conditional on the thing the rule cannot see.
    fn message(&self) -> String {
        format!(
            "no defmethod in this file specializes generic function {}: if its methods are not \
             defined in another file, it has nothing to dispatch to",
            self.generic
        )
    }
}

/// The class options a `defgeneric` carries — `(:method …)`,
/// `(:generic-function-class …)` and the rest — as their leading keywords.
fn options(view: &ExpressionView) -> impl Iterator<Item = &str> {
    view.children
        .iter()
        .skip(3)
        .filter_map(|option| option.children.first().and_then(atom_text))
}

/// Whether the `defgeneric` itself already supplies a method, or hands dispatch
/// to something this rule does not model.
fn defines_its_own_methods(view: &ExpressionView) -> bool {
    options(view).any(|option| {
        option.eq_ignore_ascii_case(":method")
            || option.eq_ignore_ascii_case(":generic-function-class")
    })
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_generic_function_no_methods(
    tree: &SyntaxTree,
    view: &ExpressionView,
    defgeneric_form_count: &mut usize,
    violations: &mut Vec<GenericFunctionNoMethodsItem>,
) {
    if !support::calls_head(view, "defgeneric") {
        return;
    }
    let Some(site) = support::locate(tree, view.span) else {
        return;
    };
    if site.quoted || !site.top_level {
        return;
    }
    *defgeneric_form_count += 1;

    let Some(generic) = support::definition_name(view) else {
        return;
    };
    if defines_its_own_methods(view) {
        return;
    }

    let specialized = support::top_level_forms(tree).any(|form| {
        symbol_is(form.head, "defmethod")
            && support::method_signature(tree, form.index)
                .is_some_and(|signature| signature.name == generic)
    });
    if specialized || installs_methods_by_hand(tree) {
        return;
    }

    violations.push(GenericFunctionNoMethodsItem {
        span: view.span,
        generic,
    });
}

/// Whether the file calls `add-method`, which builds a method object and
/// installs it without a `defmethod` form.
///
/// Which generic it installs into is a runtime value, so one such call anywhere
/// is taken as covering every generic in the file. Reached only on the path
/// that is about to report, so the file walk it costs is paid by the rare file
/// that has a method-less generic, not by every file with a `defgeneric`.
fn installs_methods_by_hand(tree: &SyntaxTree) -> bool {
    support::top_level_forms(tree)
        .filter_map(|form| support::top_level_view(tree, form.index))
        .any(|view| support::calls_any(&view, &["add-method"]))
}

/// Collects every method-less generic function in one file, with the number of
/// `defgeneric` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "every generic here has a method" for
/// Common Lisp and "nothing was looked for" for Clojure.
pub fn build_generic_function_no_methods_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<GenericFunctionNoMethodsItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("defgeneric_form_count", json!(0))],
        ));
    }

    let mut defgeneric_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_generic_function_no_methods(
                tree,
                subview,
                &mut defgeneric_form_count,
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
        vec![("defgeneric_form_count", json!(defgeneric_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<GenericFunctionNoMethodsItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_generic_function_no_methods_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build generic function no methods report")
    }

    fn violations(input: &str) -> Vec<GenericFunctionNoMethodsItem> {
        report(input).findings
    }

    #[test]
    fn flags_a_defgeneric_with_no_defmethod() {
        let found = violations("(defgeneric draw (shape stream))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].generic, "draw");
    }

    #[test]
    fn flags_only_the_generic_that_has_no_method() {
        let found = violations(
            "(defgeneric draw (s))\n(defgeneric erase (s))\n(defmethod erase ((s circle)) s)",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].generic, "draw");
    }

    /// The near miss: the method is there, so nothing is reported.
    #[test]
    fn does_not_flag_a_defgeneric_a_defmethod_specializes() {
        let found = violations("(defgeneric draw (s))\n(defmethod draw ((s circle)) s)");
        assert!(found.is_empty());
    }

    #[test]
    fn matches_the_method_by_name_across_case_and_package() {
        let found = violations("(defgeneric CL-USER:draw (s))\n(defmethod Draw ((s circle)) s)");
        assert!(found.is_empty());
    }

    #[test]
    fn matches_a_setf_generic_to_its_setf_method() {
        let found =
            violations("(defgeneric (setf width) (v s))\n(defmethod (setf width) (v (s c)) v)");
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_defgeneric_carrying_a_method_option() {
        let found = violations("(defgeneric draw (s)\n  (:method ((s circle)) s))");
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_defgeneric_with_a_custom_generic_function_class() {
        let found = violations("(defgeneric draw (s)\n  (:generic-function-class my-gf))");
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_when_the_file_installs_methods_by_hand() {
        let found =
            violations("(defgeneric draw (s))\n(defun setup () (add-method #'draw (make-it)))");
        assert!(found.is_empty());
    }

    #[test]
    fn a_documentation_option_is_not_mistaken_for_a_method() {
        let found = violations("(defgeneric draw (s)\n  (:documentation \"Draw S.\"))");
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn does_not_flag_a_quoted_defgeneric() {
        let found = violations("(setf template '(defgeneric draw (shape stream)))");
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_defgeneric_inside_an_unescaped_quasiquote() {
        let found = violations("(defmacro protocol () `(defgeneric draw (shape stream)))");
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_defgeneric_written_inside_a_string() {
        let found = violations("(defparameter *doc* \"(defgeneric draw (shape stream))\")");
        assert!(found.is_empty());
    }

    #[test]
    fn a_quoted_defmethod_does_not_count_as_specializing() {
        let found = violations("(defgeneric draw (s))\n'(defmethod draw ((s circle)) s)");
        assert_eq!(
            found.len(),
            1,
            "a defmethod inside quoted data defines no method"
        );
    }

    #[test]
    fn the_denominator_counts_every_defgeneric_scanned() {
        let built =
            report("(defgeneric draw (s))\n(defgeneric erase (s))\n(defmethod erase ((s c)) s)");
        assert_eq!(built.summary, vec![("defgeneric_form_count", json!(2))]);
        assert_eq!(built.findings.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(defgeneric draw (s))", Dialect::Clojure)
            .expect("parse");
        let built =
            build_generic_function_no_methods_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build report");
        assert!(!built.dialect_modelled);
        assert!(built.findings.is_empty());
    }

    #[test]
    fn a_finding_carries_its_line_and_its_message() {
        let built = report("(defun f ())\n(defgeneric draw (s))\n");
        let finding = &built.findings[0];
        assert_eq!(built.line_of(finding), 2);
        assert_eq!(finding.kind(), "generic-function-no-methods");
        assert_eq!(finding.text_columns(), vec!["generic=draw".to_owned()]);
    }

    /// The message must state what one file establishes, not what happens at
    /// run time. `protocol.lisp` / `methods.lisp` is a normal split, and the
    /// old wording called it a `no-applicable-method` error outright.
    #[test]
    fn the_message_makes_no_claim_the_pass_cannot_establish() {
        let built = report("(defgeneric deserialize (class stream))");
        let message = built.findings[0].message();
        assert!(
            !message.contains("no-applicable-method"),
            "asserting the runtime signal needs cross-file knowledge: {message}"
        );
        assert!(
            message.contains("no defmethod in this file specializes"),
            "the finding must say what it actually checked: {message}"
        );
        assert!(
            message.contains("if its methods are not defined in another file"),
            "the consequence must stay conditional on the blind spot: {message}"
        );
    }

    /// The category a `--category` filter selects on. `dead-code` is for forms
    /// that cannot execute; a method-less `defgeneric` executes fine, and what
    /// is wrong is generic/method disagreement.
    #[test]
    fn the_rule_is_categorized_as_an_object_system_defect() {
        use paredit_core_lint_engine::model::RuleCategory;

        let meta = &crate::generic_function_no_methods::rule::META;
        assert_eq!(meta.category(), RuleCategory::ObjectSystem);
    }
}
