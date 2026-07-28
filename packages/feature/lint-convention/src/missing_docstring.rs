//! `missing-docstring`: a definition that says nothing about itself.
//!
//! Common Lisp puts documentation *in* the definition, where `describe` and
//! `documentation` can find it at runtime. A definition without one is not just
//! undocumented on paper: it is undocumented in the image, so the REPL cannot
//! answer a question about it either.
//!
//! Two details make the rule usable rather than merely correct. First, a
//! *single* string body form is the function's return value, not its
//! documentation — `(defun greeting () "hello")` is documented by nothing and
//! returns a greeting, and reading it as a docstring would be wrong in both
//! directions. Second, `defvar` and friends put the docstring in a fixed
//! position after the value, not at the head of a body.
//!
//! Tagged `pedantic`: whether every internal helper needs a docstring is a
//! project's decision, and on a project that has decided otherwise this rule
//! fires on everything.
//!
//! Report-only. Generating a template is a *transformation* (`refactor
//! add-docstring`), and a lint fix that inserted an empty one would let a
//! project satisfy the rule without documenting anything.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, RuleTag,
    Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_in, unqualified};

pub const META: RuleMeta = RuleMeta::new(
    "missing-docstring",
    RuleCategory::Documentation,
    Severity::Warning,
    "a definition with no docstring, so `documentation` and `describe` have nothing to report",
    Fixability::ReportOnly,
)
.with_tags(&[RuleTag::Pedantic])
.with_explanation(
    RuleExplanation::new(
        "A Common Lisp docstring lives in the image, not only in the file: `(documentation 'f \
         'function)` and `describe` read it at runtime. A definition without one cannot be \
         explained from the REPL.",
    )
    .with_example(
        "(defun retry (n) (loop repeat n do (attempt)))",
        "(defun retry (n) \"Attempt the operation N times.\" (loop repeat n do (attempt)))",
    )
    .with_caveat(
        "A definition whose body is a single string is *returning* that string, not documenting \
         itself, and is reported — reading it the other way would silently accept \
         `(defun greeting () \"hello\")` as documented.",
    ),
);

const HEADS: [NormalizedHead; 9] = [
    NormalizedHead::new("defun"),
    NormalizedHead::new("defmacro"),
    NormalizedHead::new("defgeneric"),
    NormalizedHead::new("defclass"),
    NormalizedHead::new("defvar"),
    NormalizedHead::new("defparameter"),
    NormalizedHead::new("defconstant"),
    NormalizedHead::new("define-condition"),
    NormalizedHead::new("defstruct"),
];

/// Where a defining form keeps its docstring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocstringPlace {
    /// At the head of the body, after the lambda list: `defun`, `defmacro`.
    /// The index is where the body starts.
    BodyHead(usize),
    /// In a fixed slot: `(defvar name value "doc")`.
    Fixed(usize),
    /// In a `(:documentation "…")` option: `defclass`, `define-condition`,
    /// `defgeneric`.
    Option,
}

/// One undocumented definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndocumentedDefinition {
    pub span: ByteSpan,
    pub name: String,
    pub form: String,
}

/// Whether `view` is a string literal.
fn is_string(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::Atom
        && atom_text(view).is_some_and(|text| text.starts_with('"') && text.len() >= 2)
}

fn docstring_place(head: &str) -> Option<DocstringPlace> {
    if symbol_in(head, &["defun", "defmacro"]) {
        // `(defun name (args) . body)`
        return Some(DocstringPlace::BodyHead(3));
    }
    if symbol_in(head, &["defvar", "defparameter", "defconstant"]) {
        // `(defvar name value doc)`
        return Some(DocstringPlace::Fixed(3));
    }
    if symbol_in(head, &["defclass", "define-condition", "defgeneric"]) {
        return Some(DocstringPlace::Option);
    }
    if symbol_in(head, &["defstruct"]) {
        // `(defstruct name doc . slots)` — the docstring, when present, comes
        // straight after the name-or-options.
        return Some(DocstringPlace::Fixed(2));
    }
    None
}

/// Whether the form carries a `(:documentation "…")` option anywhere.
fn has_documentation_option(view: &ExpressionView) -> bool {
    view.children.iter().any(|child| {
        child
            .children
            .first()
            .and_then(atom_text)
            .is_some_and(|key| key.eq_ignore_ascii_case(":documentation"))
    })
}

/// Reads one definition and reports it if it documents nothing.
#[must_use]
pub fn examine(view: &ExpressionView) -> Option<UndocumentedDefinition> {
    let head = list_head(view)?;
    let place = docstring_place(head)?;
    let name_view = view.children.get(1)?;
    // `(defstruct (name (:option …)) …)` and `(defmethod (setf x) …)` name
    // themselves with a list; the name is its first element.
    let name = atom_text(name_view).or_else(|| name_view.children.first().and_then(atom_text))?;

    let documented = match place {
        DocstringPlace::BodyHead(start) => {
            let body = view.children.get(start..).unwrap_or(&[]);
            // A lone string is the return value, not documentation.
            body.len() > 1 && is_string(&body[0])
        }
        DocstringPlace::Fixed(index) => view.children.get(index).is_some_and(is_string),
        DocstringPlace::Option => has_documentation_option(view),
    };

    (!documented).then(|| UndocumentedDefinition {
        span: view.span,
        name: name.to_owned(),
        form: unqualified(head).to_ascii_lowercase(),
    })
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
                    "{} {} has no docstring, so `documentation` and `describe` can say nothing \
                     about it",
                    found.form, found.name
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

    fn undocumented(input: &str) -> Option<String> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view).map(|found| found.name)
    }

    #[test]
    fn flags_an_undocumented_function() {
        assert_eq!(undocumented("(defun f (x) (+ x 1))"), Some("f".to_owned()));
    }

    #[test]
    fn accepts_a_documented_function() {
        assert_eq!(undocumented(r#"(defun f (x) "Add one." (+ x 1))"#), None);
    }

    #[test]
    fn a_lone_string_body_is_a_return_value_not_a_docstring() {
        assert_eq!(
            undocumented(r#"(defun greeting () "hello")"#),
            Some("greeting".to_owned())
        );
    }

    #[test]
    fn reads_the_fixed_slot_of_a_variable_definition() {
        assert_eq!(
            undocumented("(defparameter *timeout* 30)"),
            Some("*timeout*".to_owned())
        );
        assert_eq!(
            undocumented(r#"(defparameter *timeout* 30 "Seconds to wait.")"#),
            None
        );
    }

    #[test]
    fn reads_the_documentation_option_of_a_class() {
        assert_eq!(
            undocumented("(defclass point () (x y))"),
            Some("point".to_owned())
        );
        assert_eq!(
            undocumented(r#"(defclass point () (x y) (:documentation "A point."))"#),
            None
        );
    }

    #[test]
    fn reads_a_generic_functions_documentation_option() {
        assert_eq!(
            undocumented("(defgeneric area (shape))"),
            Some("area".to_owned())
        );
        assert_eq!(
            undocumented(r#"(defgeneric area (shape) (:documentation "The area."))"#),
            None
        );
    }

    #[test]
    fn reads_a_defstruct_with_options() {
        assert_eq!(
            undocumented("(defstruct (point (:conc-name pt-)) x y)"),
            Some("point".to_owned())
        );
        assert_eq!(undocumented(r#"(defstruct point "A point." x y)"#), None);
    }

    #[test]
    fn does_not_flag_a_form_that_is_not_a_definition() {
        assert_eq!(undocumented("(let ((x 1)) x)"), None);
    }

    #[test]
    fn does_not_flag_a_definition_with_no_name() {
        assert_eq!(undocumented("(defun)"), None);
    }
}
