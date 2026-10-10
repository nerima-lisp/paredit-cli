//! `execution-order-dependency`: syntactic candidates for same-file forward
//! references in top-level forms and value initializers.
//!
//! This whole-document heuristic compares atom spellings with later definition
//! names. It skips definition heads, but does not prove they are effect-free.
//! It does not resolve packages, distinguish function and value namespaces,
//! expand macros, or model lexical scope, quotation, or delayed evaluation.
//! A finding is not proof that loading fails or that reordering is safe;
//! definitions may already exist in the environment. Report-only.
//!
//! Scope: Common Lisp only.

use std::collections::HashSet;

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{
    atom_text, for_each_subview, list_head, symbol_in, unqualified,
};

pub const META: RuleMeta = RuleMeta::new(
    "execution-order-dependency",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a top-level form contains a syntactic reference candidate to a later same-file definition",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "This heuristic finds atom spellings matching later same-file definitions in top-level \
         forms or value initializers. Review whether each match is actually evaluated and which \
         binding it denotes before treating it as a load-order dependency.",
    )
    .with_example(
        "(defparameter *config* (build-config))\n(defun build-config () ...)",
        "(defun build-config () ...)\n(defparameter *config* (build-config))",
    )
    .with_caveat(
        "Matching ignores packages, namespaces, lexical bindings, quotation, and delayed \
         evaluation. Definition heads are skipped without proving they are effect-free. \
         Existing environment definitions are unknown; no safe reordering or load failure is proved.",
    ),
);

/// Definition and declaration heads excluded from this syntactic scan.
const DEFINITION_HEADS: [&str; 12] = [
    "defun",
    "defmacro",
    "defgeneric",
    "defmethod",
    "defclass",
    "defstruct",
    "define-condition",
    "deftype",
    "defpackage",
    "in-package",
    "declaim",
    "declare",
];

/// Heads for which only the initializer is scanned. `defvar` may skip it.
const VALUE_EXECUTING_HEADS: [&str; 3] = ["defvar", "defparameter", "defconstant"];

/// One top-level form containing a same-file forward-reference candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionOrderItem {
    pub span: ByteSpan,
    /// The name referenced before its own definition.
    pub name: String,
}

/// Every symbol atom anywhere inside `view`, lower-cased and package-prefix
/// stripped. An over-approximation — a lambda-list parameter is collected the
/// same as a call head — traded for not having to model every operator's
/// argument shape; a name that is not in the candidate set is simply ignored.
fn referenced_names(view: &ExpressionView) -> HashSet<String> {
    let mut names = HashSet::new();
    if let Some(text) = atom_text(view) {
        names.insert(unqualified(text).to_ascii_lowercase());
    }
    for_each_subview(view, |subview| {
        if let Some(text) = atom_text(subview) {
            names.insert(unqualified(text).to_ascii_lowercase());
        }
    });
    names
}

/// Every name this document defines, and the index of the top-level form
/// that first defines it.
fn defined_at(root: &ExpressionView) -> Vec<(String, usize)> {
    let mut found = Vec::new();
    for (index, form) in root.children.iter().enumerate() {
        let Some(head) = list_head(form) else {
            continue;
        };
        if !symbol_in(
            head,
            &[
                "defun",
                "defmacro",
                "defgeneric",
                "defmethod",
                "defclass",
                "defstruct",
                "define-condition",
                "deftype",
                "defvar",
                "defparameter",
                "defconstant",
            ],
        ) {
            continue;
        }
        let Some(name) = form.children.get(1).and_then(atom_text) else {
            continue;
        };
        found.push((unqualified(name).to_ascii_lowercase(), index));
    }
    found
}

/// Every syntactic forward-reference candidate in `root`.
#[must_use]
pub fn collect(root: &ExpressionView) -> Vec<ExecutionOrderItem> {
    let defined = defined_at(root);
    let mut found = Vec::new();

    for (index, form) in root.children.iter().enumerate() {
        let head = list_head(form);

        if head.is_some_and(|head| symbol_in(head, &DEFINITION_HEADS)) {
            continue;
        }

        // Definition names and docstrings are not initializer candidates.
        let scanned = if head.is_some_and(|head| symbol_in(head, &VALUE_EXECUTING_HEADS)) {
            form.children.get(2)
        } else {
            Some(form)
        };
        let Some(scanned) = scanned else {
            continue;
        };

        let referenced = referenced_names(scanned);
        for (name, defined_index) in &defined {
            if *defined_index > index && referenced.contains(name) {
                found.push(ExecutionOrderItem {
                    span: form.span,
                    name: name.clone(),
                });
            }
        }
    }
    found
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        // Whether a reference is "forward" needs every other top-level
        // definition's position in the same document.
        HeadFilter::WholeTree
    }

    fn check(
        &self,
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        for item in collect(view) {
            sink.report(
                item.span,
                format!(
                    "this form contains a syntactic reference candidate to {}, defined later \
                     in this file; verify binding and evaluation context before changing load order",
                    item.name
                ),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    fn names(input: &str) -> Vec<String> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        collect(&tree.root_view())
            .into_iter()
            .map(|item| item.name)
            .collect()
    }

    #[test]
    fn flags_a_bare_call_to_a_function_defined_later() {
        assert_eq!(
            names("(process-data)\n(defun process-data () t)"),
            vec!["process-data"]
        );
    }

    #[test]
    fn flags_a_defparameter_whose_value_form_calls_a_function_defined_later() {
        assert_eq!(
            names("(defparameter *config* (build-config))\n(defun build-config () t)"),
            vec!["build-config"]
        );
    }

    #[test]
    fn flags_a_setf_referencing_a_variable_defined_later() {
        assert_eq!(
            names("(setf *state* :ready)\n(defvar *state* nil)"),
            vec!["*state*"]
        );
    }

    #[test]
    fn does_not_flag_a_call_to_a_function_defined_earlier() {
        assert!(names("(defun helper () t)\n(helper)").is_empty());
    }

    #[test]
    fn does_not_flag_a_forward_reference_inside_a_defun_body() {
        // Definition bodies are outside this heuristic's scanned forms.
        assert!(names("(defun a () (b))\n(defun b () t)").is_empty());
    }

    #[test]
    fn does_not_flag_a_forward_reference_to_a_name_this_file_never_defines() {
        // `some-library-function` might come from a library loaded earlier;
        // this rule cannot see that and does not guess.
        assert!(names("(some-library-function)\n(defun other () t)").is_empty());
    }

    #[test]
    fn does_not_flag_a_defvar_with_no_value_form() {
        assert!(names("(defvar *x*)\n(defun f () t)").is_empty());
    }

    #[test]
    fn quoted_atom_is_not_an_atom_reference_candidate() {
        assert!(names("(defparameter *data* 'later)\n(defun later () t)").is_empty());
    }

    #[test]
    fn quoted_forms_and_delayed_names_are_candidates_not_proven_dependencies() {
        assert_eq!(
            names("(defparameter *data* (quote (later)))\n(defun later () t)"),
            vec!["later"]
        );
        assert_eq!(
            names("(defparameter *callback* (lambda () (later)))\n(defun later () t)"),
            vec!["later"]
        );
    }

    #[test]
    fn package_matching_is_explicitly_a_spelling_heuristic() {
        assert_eq!(
            names("(other:later)\n(defun app:later () t)"),
            vec!["later"]
        );
    }
}
