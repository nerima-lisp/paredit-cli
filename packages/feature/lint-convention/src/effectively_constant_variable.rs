//! `effectively-constant-variable`: a `defparameter`/`defvar` never
//! reassigned in the file that defines it.
//!
//! `defvar` and `defparameter` both mean "this may change over the program's
//! lifetime" — that is the whole reason Common Lisp has them as something
//! other than `defconstant`. When a file defines one, gives it an initial
//! value, and then never assigns to it again anywhere in that same file, the
//! "may change" is unused: nothing in the file exercises it, and a reader has
//! to search the whole codebase to learn whether anything ever does.
//! `defconstant` says the same binding more precisely, and lets the compiler
//! fold uses of it.
//!
//! Correlating "defined here" with "assigned nowhere in this file" needs the
//! whole document at once, which is why this is a whole-tree rule rather than
//! a per-node one.
//!
//! Scope, by design: only same-file evidence is read. A variable assigned
//! from a different file — a setup function that lives elsewhere, a test
//! fixture — is reported the same as one nothing ever assigns, because this
//! rule cannot see the other file. That makes a finding here a starting point
//! for a search, not a proof.
//!
//! Report-only: whether the variable is actually meant to change, and whether
//! `defconstant` fits (its value must be a compile-time constant, which a
//! `defparameter` initializer is not required to be), is a judgement this
//! tool does not make.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, RuleCategory, RuleExplanation, RuleMeta, RuleTag, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{
    atom_text, for_each_subview, list_head, symbol_in, unqualified,
};

pub const META: RuleMeta = RuleMeta::new(
    "effectively-constant-variable",
    RuleCategory::Naming,
    Severity::Warning,
    "a defparameter/defvar with an initial value that is never reassigned in the same file",
    Fixability::ReportOnly,
)
.with_tags(&[RuleTag::Pedantic])
.with_explanation(
    RuleExplanation::new(
        "defvar and defparameter both mean the value may change over the program's lifetime. \
         When nothing in the defining file ever assigns to one again, that possibility is unused \
         here, and defconstant says the same binding more precisely.",
    )
    .with_example(
        "(defparameter *max-retries* 3)",
        "(defconstant +max-retries+ 3)",
    )
    .with_caveat(
        "Only same-file evidence is read: a variable assigned from another file is reported the \
         same as one nothing ever assigns. A finding here is a starting point for a search, not a \
         proof that defconstant fits — its value must be a compile-time constant, which a \
         defparameter initializer need not be.",
    ),
);

/// The operators this rule reads as "this name might change".
const MUTATORS: [&str; 10] = [
    "setf", "setq", "incf", "decf", "push", "pushnew", "pop", "rotatef", "shiftf", "remf",
];

/// One `defparameter`/`defvar` with an initial value, never reassigned in the
/// file that defines it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectivelyConstantItem {
    pub span: ByteSpan,
    pub name: String,
}

/// Every name a mutator anywhere in the document targets.
fn mutated_names(root: &ExpressionView) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    for form in &root.children {
        for_each_subview(form, |subview| {
            let Some(operator) = list_head(subview) else {
                return;
            };
            if !symbol_in(operator, &MUTATORS) {
                return;
            }
            // Every place a mutator takes sits at an odd index, which covers
            // the multi-pair spelling of setf/setq/rotatef/shiftf without
            // modelling each operator's exact arity; push/pop/incf/decf take
            // their place first, so every argument is read and a non-matching
            // one is simply not in the candidate set anyway.
            for argument in subview.children.iter().skip(1) {
                if let Some(name) = atom_text(argument) {
                    names.insert(unqualified(name).to_ascii_lowercase());
                }
            }
        });
    }
    names
}

/// Every `defparameter`/`defvar` in `root` whose name never appears as a
/// mutator's target anywhere in the same document.
#[must_use]
pub fn collect(root: &ExpressionView) -> Vec<EffectivelyConstantItem> {
    let mutated = mutated_names(root);
    let mut found = Vec::new();
    for form in &root.children {
        let Some(head) = list_head(form) else {
            continue;
        };
        if !symbol_in(head, &["defvar", "defparameter"]) {
            continue;
        }
        // A value-less defvar is a special-variable declaration, not a
        // constant candidate: defconstant requires a value, and this form
        // has none to offer it.
        if form.children.len() < 3 {
            continue;
        }
        let Some(name) = form.children.get(1).and_then(atom_text) else {
            continue;
        };
        let lowered = unqualified(name).to_ascii_lowercase();
        if !mutated.contains(&lowered) {
            found.push(EffectivelyConstantItem {
                span: form.span,
                name: name.to_owned(),
            });
        }
    }
    found
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        // Whether a name is ever reassigned is a whole-document question: no
        // per-node predicate can see the rest of the file.
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
                    "{} is never reassigned in this file; consider defconstant if its value is a \
                     compile-time constant",
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
    fn flags_a_defparameter_never_reassigned() {
        assert_eq!(
            names("(defparameter *max-retries* 3)"),
            vec!["*max-retries*"]
        );
    }

    #[test]
    fn flags_a_defvar_with_a_value_never_reassigned() {
        assert_eq!(names("(defvar *limit* 10)"), vec!["*limit*"]);
    }

    #[test]
    fn does_not_flag_a_variable_later_setf() {
        assert!(
            names("(defparameter *count* 0) (defun bump () (setf *count* (1+ *count*)))")
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_variable_later_pushed_onto() {
        assert!(names("(defparameter *log* nil) (defun record (x) (push x *log*))").is_empty());
    }

    #[test]
    fn does_not_flag_a_value_less_defvar_declaration() {
        assert!(names("(defvar *state*)").is_empty());
    }

    #[test]
    fn does_not_flag_a_defconstant() {
        assert!(names("(defconstant +max+ 3)").is_empty());
    }

    #[test]
    fn reads_every_pair_of_a_multi_place_setf() {
        assert!(
            names("(defparameter *a* 1) (defparameter *b* 2) (defun f () (setf *a* 1 *b* 2))")
                .is_empty()
        );
    }
}
