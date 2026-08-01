//! `execution-order-dependency`: a top-level form that runs at load time and
//! names something this file does not define until later.
//!
//! Common Lisp loads a file by evaluating its top-level forms in order.
//! `defun`/`defmacro`/`defclass`/`defgeneric`/`defmethod`/`define-condition`/
//! `defstruct`/`deftype`/`defpackage` do not *run* anything when loaded — they
//! install a definition for later use, so a forward reference inside one
//! (`(defun a () (b))` before `b` is defined) is completely ordinary: by the
//! time `a` is ever *called*, the whole file has loaded. `defvar` and
//! `defparameter` are different: they evaluate their value form immediately,
//! and so does every other top-level form — a bare call, a `setf`, a `let`
//! run for effect. When one of those references a name this file only defines
//! *later*, its result depends on load order in a way that will not survive
//! the file being reordered, split, or partially reloaded from the REPL.
//!
//! Judging "later" needs to see the whole file's definitions at once, which
//! is why this is a whole-document rule.
//!
//! Scope, by design: only forward references to a *same-file* definition are
//! read. A name this file never defines is out of scope — it might come from
//! a library loaded earlier, which this rule cannot see and must not guess
//! about.
//!
//! Report-only: whether to move the definition earlier or the use later is a
//! judgement this tool does not make.
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
    "a top-level form that runs at load time and references a name this file only defines later",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "A file loads by evaluating its top-level forms in order. defun/defmacro/defclass and \
         friends only install a definition, so a forward reference inside one is ordinary — by \
         the time it runs, the file has finished loading. defvar/defparameter and every other \
         top-level form run immediately, and a forward reference inside one depends on the file's \
         current order to work at all.",
    )
    .with_example(
        "(defparameter *config* (build-config))\n(defun build-config () ...)",
        "(defun build-config () ...)\n(defparameter *config* (build-config))",
    )
    .with_caveat(
        "Only forward references to a definition this same file provides are read. A name from a \
         library loaded earlier is out of scope — this rule cannot see it and does not guess.",
    ),
);

/// Heads that only install a definition and evaluate no body at load time:
/// a forward reference inside one is not an order dependency.
const NON_EXECUTING_HEADS: [&str; 12] = [
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

/// Heads that evaluate exactly their value form (`children[2]`) at load time,
/// nothing else about the form.
const VALUE_EXECUTING_HEADS: [&str; 3] = ["defvar", "defparameter", "defconstant"];

/// One top-level form that runs at load time and forward-references a
/// same-file definition.
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

/// Every load-order dependency in `root`.
#[must_use]
pub fn collect(root: &ExpressionView) -> Vec<ExecutionOrderItem> {
    let defined = defined_at(root);
    let mut found = Vec::new();

    for (index, form) in root.children.iter().enumerate() {
        let head = list_head(form);

        // A form that only installs a definition evaluates no body now.
        if head.is_some_and(|head| symbol_in(head, &NON_EXECUTING_HEADS)) {
            continue;
        }

        // defvar/defparameter/defconstant evaluate exactly their value form.
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
                    "this form runs at load time and references {}, which this file only defines \
                     later; its result depends on the file's current order",
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
        // `a`'s body does not run until `a` is called, by which time the
        // whole file has loaded.
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
}
