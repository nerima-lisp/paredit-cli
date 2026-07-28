//! `destructive-function-naming`: a function that destroys its argument
//! without saying so.
//!
//! `(defun trim (xs) (nreverse xs))` returns the right answer and leaves the
//! caller's list in an undefined state. Common Lisp marks that with an `n`
//! prefix — `reverse`/`nreverse`, `union`/`nunion` — and a function that does
//! the destroying without wearing the mark is one whose callers will not know
//! to copy first.
//!
//! The evidence has to be a destructive operator applied to *the function's own
//! parameter*, and that is what makes the rule precise. `(defun f (xs) (let ((a
//! (copy-list xs))) (nreverse a)))` destroys a local, is perfectly safe, and is
//! not reported — the parameter check is the entire difference between a useful
//! rule and a rule that fires on every use of `nreverse` anywhere.
//!
//! Report-only: renaming a function is a refactor with call sites.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, RuleTag,
    Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{
    atom_text, for_each_subview, list_head, symbol_in, unqualified,
};

pub const META: RuleMeta = RuleMeta::new(
    "destructive-function-naming",
    RuleCategory::Naming,
    Severity::Warning,
    "a function that destructively modifies one of its own parameters but is not named for it",
    Fixability::ReportOnly,
)
.with_tags(&[RuleTag::Pedantic])
.with_explanation(
    RuleExplanation::new(
        "A caller decides whether to copy an argument from the callee's name. A function that \
         applies a destructive operator to a parameter, under a name with no `n` prefix and no \
         `!` suffix, tells the caller the opposite of the truth.",
    )
    .with_example(
        "(defun trim (xs) (nreverse xs))",
        "(defun ntrim (xs) (nreverse xs))",
    )
    .with_caveat(
        "Destroying a *local* is safe and never reported: `(let ((a (copy-list xs))) (nreverse \
         a))` is the standard way to write a non-destructive function.",
    ),
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("defun")];

/// The operators that modify their argument in place.
const DESTRUCTIVE: [&str; 13] = [
    "nreverse",
    "nconc",
    "nsubst",
    "nsubstitute",
    "nunion",
    "nintersection",
    "nset-difference",
    "nset-exclusive-or",
    "nbutlast",
    "nstring-downcase",
    "nstring-upcase",
    "delete",
    "sort",
];

/// The lambda-list markers that are not parameter names.
const LAMBDA_LIST_KEYWORDS: [&str; 6] = [
    "&optional",
    "&rest",
    "&key",
    "&aux",
    "&body",
    "&allow-other-keys",
];

/// One unmarked destructive function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnmarkedDestructive {
    pub span: ByteSpan,
    pub function: String,
    pub parameter: String,
    pub operator: String,
}

/// Whether `name` already announces that it destroys.
///
/// The `n` prefix is the standard's own convention and `!` the Scheme-derived
/// one that many Common Lisp projects also use; either counts.
#[must_use]
pub fn announces_destruction(name: &str) -> bool {
    let stripped = unqualified(name);
    stripped.ends_with('!')
        || (stripped.len() > 1
            && stripped.starts_with(['n', 'N'])
            && !stripped[1..].starts_with('-'))
}

/// The required and optional parameter names of a lambda list, without its
/// keywords or their defaults.
fn parameter_names(lambda_list: &ExpressionView) -> Vec<String> {
    lambda_list
        .children
        .iter()
        .filter_map(|parameter| {
            // `(x default)` in an `&optional`/`&key` section names `x`.
            let name =
                atom_text(parameter).or_else(|| parameter.children.first().and_then(atom_text))?;
            (!symbol_in(name, &LAMBDA_LIST_KEYWORDS)).then(|| name.to_owned())
        })
        .collect()
}

/// Reads one `defun` and reports the parameter it destroys.
#[must_use]
pub fn examine(view: &ExpressionView) -> Option<UnmarkedDestructive> {
    if !list_head(view).is_some_and(|head| symbol_in(head, &["defun"])) {
        return None;
    }
    let function = atom_text(view.children.get(1)?)?;
    if announces_destruction(function) {
        return None;
    }
    let parameters = parameter_names(view.children.get(2)?);
    if parameters.is_empty() {
        return None;
    }

    let mut found = None;
    for form in view.children.iter().skip(3) {
        for_each_subview(form, |subview| {
            if found.is_some() {
                return;
            }
            let Some(operator) = list_head(subview) else {
                return;
            };
            if !symbol_in(operator, &DESTRUCTIVE) {
                return;
            }
            // Only a bare parameter in an argument position counts. A parameter
            // reached through another call — `(nreverse (copy-list xs))` — is a
            // fresh list, which is the safe idiom.
            let destroyed = subview.children.iter().skip(1).find_map(|argument| {
                let name = atom_text(argument)?;
                parameters
                    .iter()
                    .find(|parameter| name.eq_ignore_ascii_case(parameter))
                    .cloned()
            });
            if let Some(parameter) = destroyed {
                found = Some(UnmarkedDestructive {
                    span: view.span,
                    function: function.to_owned(),
                    parameter,
                    operator: unqualified(operator).to_ascii_lowercase(),
                });
            }
        });
    }
    found
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        if let Some(found) = examine(view) {
            sink.report(
                found.span,
                format!(
                    "{} applies {} to its parameter {}, so a caller's argument is destroyed; the \
                     convention is an n prefix or a ! suffix",
                    found.function, found.operator, found.parameter
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
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn parameter(input: &str) -> Option<String> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view).map(|found| found.parameter)
    }

    #[test]
    fn reads_both_conventions() {
        assert!(announces_destruction("nreverse"));
        assert!(announces_destruction("trim!"));
        assert!(!announces_destruction("trim"));
        // `n-` is a hyphenated word starting with n, not the prefix.
        assert!(!announces_destruction("n-th-item"));
        assert!(!announces_destruction("n"));
    }

    #[test]
    fn flags_a_destroyed_parameter() {
        assert_eq!(
            parameter("(defun trim (xs) (nreverse xs))"),
            Some("xs".to_owned())
        );
        assert_eq!(
            parameter("(defun merge-all (as bs) (nconc as bs))"),
            Some("as".to_owned())
        );
    }

    #[test]
    fn does_not_flag_a_function_that_announces_it() {
        assert_eq!(parameter("(defun ntrim (xs) (nreverse xs))"), None);
        assert_eq!(parameter("(defun trim! (xs) (nreverse xs))"), None);
    }

    #[test]
    fn does_not_flag_destruction_of_a_fresh_local() {
        assert_eq!(
            parameter("(defun trim (xs) (let ((a (copy-list xs))) (nreverse a)))"),
            None
        );
    }

    #[test]
    fn does_not_flag_destruction_of_a_fresh_result() {
        assert_eq!(
            parameter("(defun trim (xs) (nreverse (copy-list xs)))"),
            None
        );
    }

    #[test]
    fn reads_optional_and_key_parameters() {
        assert_eq!(
            parameter("(defun trim (a &optional (b nil)) (nreverse b))"),
            Some("b".to_owned())
        );
    }

    #[test]
    fn does_not_read_a_lambda_list_keyword_as_a_parameter() {
        assert_eq!(
            parameter("(defun trim (&rest args) (nreverse &rest))"),
            None
        );
    }

    #[test]
    fn does_not_flag_a_non_destructive_body() {
        assert_eq!(parameter("(defun trim (xs) (reverse xs))"), None);
    }

    #[test]
    fn does_not_flag_a_function_with_no_parameters() {
        assert_eq!(parameter("(defun reset () (nreverse *log*))"), None);
    }

    #[test]
    fn finds_the_call_nested_deep_in_the_body() {
        assert_eq!(
            parameter("(defun trim (xs) (when xs (values (nreverse xs))))"),
            Some("xs".to_owned())
        );
    }
}
