//! `ignore-declaration-conflict`: an `ignore` declaration that contradicts the
//! code around it.
//!
//! Two mistakes, one shape. `(declare (ignore x))` followed by a body that uses
//! `x` is a promise the body immediately breaks — most implementations signal a
//! warning, some an error, and the reader is left unsure which of the two
//! statements to believe. `(declare (ignore y))` where `y` is not a parameter
//! at all is usually a rename that updated the lambda list and not the
//! declaration, and it silences nothing.
//!
//! They are one rule because they are one reading: gather the lambda list,
//! gather the declared-ignored names, gather the body's references, and compare.
//! Splitting them would mean two rules doing the same three walks.
//!
//! Not pedantic and not a warning: the first case is a compile-time error in
//! several implementations, and the second is dead text that hides a real
//! unused parameter.
//!
//! Report-only. Which of the two statements is wrong — the declaration or the
//! body — is exactly what the rule cannot know.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head, symbol_in};

pub const META: RuleMeta = RuleMeta::new(
    "ignore-declaration-conflict",
    RuleCategory::Declaration,
    Severity::Error,
    "an (ignore x) declaration whose variable the body uses, or which names no parameter at all",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "`(declare (ignore x))` states that the body will not refer to `x`. A body that refers to \
         it anyway makes the two statements contradict, which most implementations diagnose; a \
         declaration naming something the lambda list does not bind silences nothing and hides \
         the parameter that really is unused.",
    )
    .with_example(
        "(defun f (a b) (declare (ignore b)) (+ a b))",
        "(defun f (a b) (declare (ignorable b)) (+ a b))",
    )
    .with_caveat(
        "`ignorable` is the declaration for \"may or may not be used\" and is never reported.",
    ),
);

const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("defun"),
    NormalizedHead::new("defmacro"),
    NormalizedHead::new("defmethod"),
    NormalizedHead::new("lambda"),
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

/// What is wrong with one `ignore` declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoreConflict {
    /// Declared ignored, then used.
    UsedAnyway,
    /// Declared ignored, but not bound by the lambda list.
    NotBound,
}

impl IgnoreConflict {
    #[must_use]
    pub const fn as_message(self) -> &'static str {
        match self {
            Self::UsedAnyway => {
                "is declared ignored and then used, which most implementations diagnose"
            }
            Self::NotBound => {
                "is declared ignored but is not a parameter, so the declaration silences nothing"
            }
        }
    }
}

/// One contradictory declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictingIgnore {
    pub span: ByteSpan,
    pub variable: String,
    pub conflict: IgnoreConflict,
}

/// The parameter names a lambda list binds.
fn parameter_names(lambda_list: &ExpressionView) -> Vec<String> {
    lambda_list
        .children
        .iter()
        .filter_map(|parameter| {
            // A `defmethod` specialiser `(x integer)` and an `&optional (x d)`
            // default both name the parameter first.
            let name =
                atom_text(parameter).or_else(|| parameter.children.first().and_then(atom_text))?;
            (!symbol_in(name, &LAMBDA_LIST_KEYWORDS)).then(|| name.to_owned())
        })
        .collect()
}

/// The names declared `ignore` in the leading declarations of `body`.
///
/// `ignorable` is deliberately excluded: it means "this may or may not be
/// used", which is precisely the statement neither half of this rule can
/// contradict.
fn ignored_names(body: &[ExpressionView]) -> Vec<(String, ByteSpan)> {
    let mut names = Vec::new();
    for form in body {
        if !list_head(form).is_some_and(|head| symbol_in(head, &["declare"])) {
            // Declarations only lead a body; anything else ends the section.
            break;
        }
        for specifier in form.children.iter().skip(1) {
            if !list_head(specifier).is_some_and(|head| symbol_in(head, &["ignore"])) {
                continue;
            }
            for name in specifier.children.iter().skip(1) {
                if let Some(text) = atom_text(name) {
                    names.push((text.to_owned(), name.span));
                }
            }
        }
    }
    names
}

/// Whether any form in `body` refers to `name`, skipping the declarations
/// themselves.
fn body_uses(body: &[ExpressionView], name: &str) -> bool {
    let mut used = false;
    for form in body {
        if list_head(form).is_some_and(|head| symbol_in(head, &["declare"])) {
            continue;
        }
        for_each_subview(form, |view| {
            used |= atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case(name));
        });
    }
    used
}

/// Every contradictory `ignore` declaration in one definition.
#[must_use]
pub fn examine(view: &ExpressionView) -> Vec<ConflictingIgnore> {
    let Some(head) = list_head(view) else {
        return Vec::new();
    };
    if !symbol_in(head, &["defun", "defmacro", "defmethod", "lambda"]) {
        return Vec::new();
    }
    // `lambda` puts its list at index 1; the definers at index 2.
    let list_index = if symbol_in(head, &["lambda"]) { 1 } else { 2 };
    let Some(lambda_list) = view.children.get(list_index) else {
        return Vec::new();
    };
    let parameters = parameter_names(lambda_list);
    let body = view.children.get(list_index + 1..).unwrap_or(&[]);

    ignored_names(body)
        .into_iter()
        .filter_map(|(name, span)| {
            let bound = parameters
                .iter()
                .any(|parameter| parameter.eq_ignore_ascii_case(&name));
            let conflict = if !bound {
                IgnoreConflict::NotBound
            } else if body_uses(body, &name) {
                IgnoreConflict::UsedAnyway
            } else {
                return None;
            };
            Some(ConflictingIgnore {
                span,
                variable: name,
                conflict,
            })
        })
        .collect()
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
        for conflict in examine(view) {
            sink.report(
                conflict.span,
                format!("{} {}", conflict.variable, conflict.conflict.as_message()),
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

    fn conflicts(input: &str) -> Vec<(String, IgnoreConflict)> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view)
            .into_iter()
            .map(|item| (item.variable, item.conflict))
            .collect()
    }

    #[test]
    fn flags_an_ignored_parameter_the_body_uses() {
        assert_eq!(
            conflicts("(defun f (a b) (declare (ignore b)) (+ a b))"),
            vec![("b".to_owned(), IgnoreConflict::UsedAnyway)]
        );
    }

    #[test]
    fn flags_an_ignore_of_something_that_is_not_a_parameter() {
        assert_eq!(
            conflicts("(defun f (a) (declare (ignore b)) a)"),
            vec![("b".to_owned(), IgnoreConflict::NotBound)]
        );
    }

    #[test]
    fn accepts_a_genuinely_ignored_parameter() {
        assert!(conflicts("(defun f (a b) (declare (ignore b)) a)").is_empty());
    }

    #[test]
    fn does_not_flag_ignorable() {
        assert!(conflicts("(defun f (a b) (declare (ignorable b)) (+ a b))").is_empty());
    }

    #[test]
    fn reads_a_lambda_and_a_method() {
        assert_eq!(
            conflicts("(lambda (a) (declare (ignore a)) a)"),
            vec![("a".to_owned(), IgnoreConflict::UsedAnyway)]
        );
        assert_eq!(
            conflicts("(defmethod area ((s square) n) (declare (ignore n)) (* n n))"),
            vec![("n".to_owned(), IgnoreConflict::UsedAnyway)]
        );
    }

    #[test]
    fn reads_a_specialised_parameter_as_bound() {
        assert!(
            conflicts("(defmethod area ((s square) n) (declare (ignore s)) (* n n))").is_empty()
        );
    }

    #[test]
    fn reads_several_names_in_one_declaration() {
        let found = conflicts("(defun f (a b) (declare (ignore a b)) (+ a b))");
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|(_, c)| *c == IgnoreConflict::UsedAnyway));
    }

    #[test]
    fn finds_a_use_nested_deep_in_the_body() {
        assert_eq!(
            conflicts("(defun f (a b) (declare (ignore b)) (when a (list (list b))))"),
            vec![("b".to_owned(), IgnoreConflict::UsedAnyway)]
        );
    }

    #[test]
    fn a_declaration_after_a_body_form_is_not_a_leading_declaration() {
        // Declarations lead a body; anything after a real form is not one, and
        // reading it as one would report a variable nothing declared.
        assert!(conflicts("(defun f (a) a (declare (ignore zzz)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_definition_with_no_declarations() {
        assert!(conflicts("(defun f (a b) (+ a b))").is_empty());
    }
}
