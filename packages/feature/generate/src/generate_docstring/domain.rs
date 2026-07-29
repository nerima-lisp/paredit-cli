//! Inserting a docstring template at the position Common Lisp expects it.
//!
//! Where the docstring goes is not "somewhere near the top" — it is a fixed
//! grammar position that differs by definition head, and getting it wrong
//! changes what the form means rather than merely how it looks:
//!
//! - `defun` and `defmacro` read a lone string as their *body*, not their
//!   documentation. `(defun greeting () "hello")` returns a greeting; it is
//!   not "undocumented", it is a one-line function. A docstring is only a
//!   docstring when a real body form follows it, so a definition with no body
//!   gets a trailing `nil` inserted along with the template — otherwise the
//!   template itself would become the return value it was inserted next to.
//! - `defvar`, `defparameter`, and `defconstant` keep the docstring in a
//!   fixed slot *after* the value. A definition with no value has nowhere
//!   safe to put one — inserting there would occupy the value position
//!   instead — so those are refused rather than guessed at.
//! - `defclass`, `define-condition`, and `defgeneric` take a
//!   `(:documentation "...")` option instead, which this inserts as a new
//!   option rather than assuming one belongs at a fixed position among
//!   whatever options are already there.
//! - `defstruct` keeps its docstring in a fixed slot *before* its slots,
//!   which is the one case where "insert before the closing paren" would be
//!   wrong; docstring detection and defect-shape detection here mirror the
//!   `missing-docstring` lint rule's, independently, the way `fold` and
//!   `line_of` are re-derived in every report rather than shared.

use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_in};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocstringPlace {
    /// After the lambda list, so the index is where the body starts.
    BodyHead(usize),
    /// At a fixed child index.
    Fixed(usize),
    /// As a `(:documentation "...")` option, inserted as a new child before
    /// the form's closing delimiter.
    Option,
}

/// One insertion ready to apply: `insertion_offset` is an absolute byte
/// offset into the document the target was parsed from, and
/// `insertion_text` is inserted there verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocstringPlan {
    pub name: String,
    pub insertion_offset: usize,
    pub insertion_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocstringOutcome {
    Ready(DocstringPlan),
    /// The definition already documents itself.
    AlreadyDocumented,
    /// Not a definition this generator knows how to document, or one whose
    /// shape has nowhere safe to put a docstring (a `defvar` with no value).
    Unsupported {
        reason: &'static str,
    },
}

fn docstring_place(head: &str) -> Option<DocstringPlace> {
    if symbol_in(head, &["defun", "defmacro"]) {
        return Some(DocstringPlace::BodyHead(3));
    }
    if symbol_in(head, &["defvar", "defparameter", "defconstant"]) {
        return Some(DocstringPlace::Fixed(3));
    }
    if symbol_in(head, &["defclass", "define-condition", "defgeneric"]) {
        return Some(DocstringPlace::Option);
    }
    if symbol_in(head, &["defstruct"]) {
        return Some(DocstringPlace::Fixed(2));
    }
    None
}

fn is_string(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::Atom
        && atom_text(view).is_some_and(|text| text.starts_with('"') && text.len() >= 2)
}

fn has_documentation_option(view: &ExpressionView) -> bool {
    view.children.iter().any(|child| {
        child
            .children
            .first()
            .and_then(atom_text)
            .is_some_and(|key| key.eq_ignore_ascii_case(":documentation"))
    })
}

fn definition_name(view: &ExpressionView) -> Option<&str> {
    let name_view = view.children.get(1)?;
    atom_text(name_view).or_else(|| name_view.children.first().and_then(atom_text))
}

/// Parameter names from a lambda list, in order, skipping the lambda list
/// keywords themselves (`&optional`, `&rest`, ...) and reading the name out
/// of a `(name default)` or `(name default supplied-p)` form.
fn lambda_list_parameter_names(lambda_list: &ExpressionView) -> Vec<String> {
    lambda_list
        .children
        .iter()
        .filter_map(|parameter| match parameter.kind {
            ExpressionKind::Atom => {
                let text = atom_text(parameter)?;
                (!text.starts_with('&')).then(|| text.to_owned())
            }
            ExpressionKind::List => parameter
                .children
                .first()
                .and_then(atom_text)
                .map(ToOwned::to_owned),
            ExpressionKind::Root => None,
        })
        .collect()
}

fn template_text(name: &str, params: &[String]) -> String {
    if params.is_empty() {
        format!("TODO: document {name}.")
    } else {
        format!("TODO: document {name}. Parameters: {}.", params.join(", "))
    }
}

/// Plans a docstring insertion for `view`, or explains why it cannot.
#[must_use]
pub fn plan_docstring(view: &ExpressionView) -> DocstringOutcome {
    let Some(head) = list_head(view) else {
        return DocstringOutcome::Unsupported {
            reason: "not a definition form",
        };
    };
    let Some(place) = docstring_place(head) else {
        return DocstringOutcome::Unsupported {
            reason: "not a definition head this generator documents",
        };
    };
    let Some(name) = definition_name(view) else {
        return DocstringOutcome::Unsupported {
            reason: "the definition has no name",
        };
    };
    let name = name.to_owned();

    match place {
        DocstringPlace::BodyHead(start) => {
            let body = view.children.get(start..).unwrap_or(&[]);
            if body.len() > 1 && is_string(&body[0]) {
                return DocstringOutcome::AlreadyDocumented;
            }
            let Some(anchor) = view.children.get(start - 1) else {
                return DocstringOutcome::Unsupported {
                    reason: "the definition has no lambda list to anchor on",
                };
            };
            let params = lambda_list_parameter_names(anchor);
            let mut insertion_text = format!(" \"{}\"", template_text(&name, &params));
            if body.is_empty() {
                // A lone string body is a return value, not a docstring; a
                // trailing `nil` keeps the body at two forms so the inserted
                // string reads as documentation.
                insertion_text.push_str(" nil");
            }
            DocstringOutcome::Ready(DocstringPlan {
                name,
                insertion_offset: anchor.span.end().get(),
                insertion_text,
            })
        }
        DocstringPlace::Fixed(index) => {
            if view.children.get(index).is_some_and(is_string) {
                return DocstringOutcome::AlreadyDocumented;
            }
            let Some(anchor) = index.checked_sub(1).and_then(|i| view.children.get(i)) else {
                return DocstringOutcome::Unsupported {
                    reason: "the definition has nothing before the docstring position to anchor on",
                };
            };
            DocstringOutcome::Ready(DocstringPlan {
                name: name.clone(),
                insertion_offset: anchor.span.end().get(),
                insertion_text: format!(" \"{}\"", template_text(&name, &[])),
            })
        }
        DocstringPlace::Option => {
            if has_documentation_option(view) {
                return DocstringOutcome::AlreadyDocumented;
            }
            let close = view.span.end().get().saturating_sub(1);
            DocstringOutcome::Ready(DocstringPlan {
                name: name.clone(),
                insertion_offset: close,
                insertion_text: format!(" (:documentation \"{}\")", template_text(&name, &[])),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn target(input: &str) -> ExpressionView {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        tree.select_path(&Path::root_child(0))
            .expect("root form")
            .view()
    }

    fn apply(input: &str) -> String {
        let view = target(input);
        match plan_docstring(&view) {
            DocstringOutcome::Ready(plan) => {
                let mut rewritten = input.to_owned();
                rewritten.insert_str(plan.insertion_offset, &plan.insertion_text);
                rewritten
            }
            other => panic!("expected Ready, got {other:?}"),
        }
    }

    #[test]
    fn inserts_after_the_lambda_list_for_a_function() {
        let rewritten = apply("(defun f (x) (+ x 1))");
        assert_eq!(
            rewritten,
            "(defun f (x) \"TODO: document f. Parameters: x.\" (+ x 1))"
        );
        let tree = SyntaxTree::parse_with_dialect(&rewritten, Dialect::CommonLisp)
            .expect("rewritten parses");
        assert!(
            tree.root_view().children[0]
                .span
                .slice(&rewritten)
                .starts_with("(defun")
        );
    }

    #[test]
    fn an_empty_body_gets_a_trailing_nil_so_the_string_is_not_a_return_value() {
        let rewritten = apply("(defun f (x))");
        assert_eq!(
            rewritten,
            "(defun f (x) \"TODO: document f. Parameters: x.\" nil)"
        );
    }

    #[test]
    fn an_already_documented_function_is_reported_rather_than_double_documented() {
        let view = target(r#"(defun f (x) "Already." (+ x 1))"#);
        assert!(matches!(
            plan_docstring(&view),
            DocstringOutcome::AlreadyDocumented
        ));
    }

    #[test]
    fn a_lone_string_body_is_a_return_value_and_a_second_string_precedes_it() {
        let rewritten = apply(r#"(defun greeting () "hello")"#);
        assert_eq!(
            rewritten,
            r#"(defun greeting () "TODO: document greeting." "hello")"#
        );
    }

    #[test]
    fn inserts_after_the_value_for_a_variable() {
        let rewritten = apply("(defparameter *timeout* 30)");
        assert_eq!(
            rewritten,
            "(defparameter *timeout* 30 \"TODO: document *timeout*.\")"
        );
    }

    #[test]
    fn a_variable_with_no_value_is_refused_rather_than_placed_in_the_value_slot() {
        let view = target("(defvar *timeout*)");
        assert!(matches!(
            plan_docstring(&view),
            DocstringOutcome::Unsupported { .. }
        ));
    }

    #[test]
    fn inserts_a_documentation_option_for_a_class() {
        let rewritten = apply("(defclass point () (x y))");
        assert_eq!(
            rewritten,
            "(defclass point () (x y) (:documentation \"TODO: document point.\"))"
        );
    }

    #[test]
    fn a_class_with_a_documentation_option_is_already_documented() {
        let view = target(r#"(defclass point () (x y) (:documentation "A point."))"#);
        assert!(matches!(
            plan_docstring(&view),
            DocstringOutcome::AlreadyDocumented
        ));
    }

    #[test]
    fn inserts_before_the_slots_for_a_struct() {
        let rewritten = apply("(defstruct point x y)");
        assert_eq!(rewritten, "(defstruct point \"TODO: document point.\" x y)");
        SyntaxTree::parse_with_dialect(&rewritten, Dialect::CommonLisp)
            .expect("rewritten struct parses");
    }

    #[test]
    fn a_form_that_is_not_a_definition_is_unsupported() {
        let view = target("(let ((x 1)) x)");
        assert!(matches!(
            plan_docstring(&view),
            DocstringOutcome::Unsupported { .. }
        ));
    }
}
