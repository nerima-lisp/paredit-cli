//! A `&rest` parameter declared to be the type of its *elements*.
//!
//! # What CLHS says
//!
//! CLHS 3.4.1.3: the `&rest` variable is bound to "a list of the remaining
//! arguments". Whatever the arguments are, the variable itself is always a list.
//! So `(defun f (&rest args) (declare (fixnum args)) …)` declares the list to be
//! a number — the author meant "the arguments are fixnums", which a `&rest`
//! declaration cannot express at all.
//!
//! # What SBCL 2.6.0 does
//!
//! A full `WARNING`, and its wording names the mechanism exactly:
//!
//! ```lisp
//! (defun bad-9 (&rest args) (declare (fixnum args)) (apply #'+ args))
//! ```
//!
//! ```text
//! ; caught WARNING:
//! ;   Derived type of (SB-C:%LISTIFY-REST-ARGS #:N-CONTEXT-2 #:N-COUNT-3) is
//! ;     (VALUES LIST &OPTIONAL),
//! ;   conflicting with its asserted type
//! ;     FIXNUM.
//! ```
//!
//! It is silent on `(declare (list args))` and on `(declare (dynamic-extent
//! args))`, the two things a `&rest` variable is legitimately declared to be.
//!
//! # Why this is a separate rule from the `let` one
//!
//! The contradiction here is not with a literal initial value — there is none —
//! but with the *binding mechanism*. The variable's type is known from the
//! lambda list alone, which is why the test is a type-versus-type question and
//! not a type-versus-literal one.

use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{is_paren_list, list_head};

use crate::support::{
    LiteralKind, body_start_of, declaration_section_end, is_declare, is_reader_conditional,
    normalized_symbol, symbol_name, type_declarations, type_excludes, type_text,
};

/// One `&rest` parameter with an impossible type declaration.
#[derive(Debug, Clone)]
pub struct RestTypeDeclaration {
    /// The offending declaration specifier.
    pub span: ByteSpan,
    pub variable: String,
    pub declared: String,
}

impl RestTypeDeclaration {
    #[must_use]
    pub fn message(&self) -> String {
        format!(
            "{} is a &rest parameter, so it is always bound to a list, but it is declared {}; \
             a &rest declaration describes the list itself and cannot describe its elements",
            self.variable, self.declared
        )
    }
}

/// The lambda list of a form this rule inspects.
///
/// `(defun name lambda-list . body)` and `(defmacro name lambda-list . body)`
/// put it at index 2; `(lambda lambda-list . body)` at index 1. `defmethod` is
/// excluded: a specialized lambda list may not contain `&rest` specializers, and
/// its body start is searched for rather than counted to, so the two indices
/// would have to be derived separately for no additional coverage.
fn lambda_list_index(head: &str) -> Option<usize> {
    match head {
        "defun" | "defmacro" | "define-compiler-macro" => Some(2),
        "lambda" => Some(1),
        _ => None,
    }
}

/// The variables bound by `&rest` or `&body` in a lambda list.
///
/// Only symbols: `defmacro` permits a destructuring pattern after `&rest`, and a
/// pattern binds several variables of which none is the list.
fn rest_variables(lambda_list: &ExpressionView) -> Vec<String> {
    let mut names = Vec::new();
    let mut expecting = false;
    for child in &lambda_list.children {
        let Some(name) = symbol_name(child) else {
            expecting = false;
            continue;
        };
        if expecting {
            if !name.starts_with('&') {
                names.push(name);
            }
            expecting = false;
            continue;
        }
        if name == "&rest" || name == "&body" {
            expecting = true;
        }
    }
    names
}

/// Every impossible `&rest` type declaration in one form.
#[must_use]
pub fn examine_form(view: &ExpressionView) -> Vec<RestTypeDeclaration> {
    let Some(head) = list_head(view) else {
        return Vec::new();
    };
    let Some(index) = lambda_list_index(&normalized_symbol(head)) else {
        return Vec::new();
    };
    let Some(lambda_list) = view.children.get(index).filter(|list| is_paren_list(list)) else {
        return Vec::new();
    };
    let rest = rest_variables(lambda_list);
    if rest.is_empty() {
        // The common case, and the one that keeps this rule off the benchmark
        // path: no `&rest`, no declaration section walk.
        return Vec::new();
    }
    let Some(start) = body_start_of(view) else {
        return Vec::new();
    };
    let section_end = declaration_section_end(view, start);

    let mut found = Vec::new();
    for child in view.children.iter().take(section_end).skip(start) {
        if !is_declare(child) {
            continue;
        }
        for declaration in type_declarations(child) {
            if is_reader_conditional(declaration.type_spec) {
                continue;
            }
            // A `&rest` variable is a list, so its declared type must admit both
            // the empty list and a non-empty one. A type that excludes either is
            // a type no `&rest` variable can ever have.
            let excludes_list = type_excludes(declaration.type_spec, LiteralKind::Null)
                || type_excludes(declaration.type_spec, LiteralKind::Cons);
            if !excludes_list {
                continue;
            }
            let declared = type_text(declaration.type_spec);
            for name in declaration.variables() {
                if !rest.contains(&name) {
                    continue;
                }
                found.push(RestTypeDeclaration {
                    span: declaration.form.span,
                    variable: name,
                    declared: declared.clone(),
                });
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    fn findings(source: &str) -> Vec<RestTypeDeclaration> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        examine_form(&tree.root_view().children[0])
    }

    #[test]
    fn flags_the_references_own_example() {
        let found = findings("(defun f (&rest args) (declare (fixnum args)) (apply #'+ args))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].variable, "args");
        assert_eq!(found[0].declared, "fixnum");
    }

    #[test]
    fn flags_the_long_hand_spelling() {
        assert_eq!(
            findings("(defun f (&rest args) (declare (type string args)) args)").len(),
            1
        );
    }

    #[test]
    fn flags_it_after_required_parameters() {
        assert_eq!(
            findings("(defun f (a b &rest args) (declare (fixnum args)) args)").len(),
            1
        );
    }

    #[test]
    fn flags_it_in_a_lambda_and_a_defmacro_body_variable() {
        assert_eq!(
            findings("(lambda (&rest args) (declare (fixnum args)) args)").len(),
            1
        );
        assert_eq!(
            findings("(defmacro m (&body forms) (declare (fixnum forms)) forms)").len(),
            1
        );
    }

    // -- the correct declarations --------------------------------------------

    /// The two things a `&rest` variable is legitimately declared to be. SBCL
    /// accepts both without a word.
    #[test]
    fn accepts_the_declarations_a_rest_variable_can_have() {
        assert!(findings("(defun f (&rest args) (declare (list args)) args)").is_empty());
        assert!(findings("(defun f (&rest args) (declare (type list args)) args)").is_empty());
        assert!(findings("(defun f (&rest args) (declare (dynamic-extent args)) args)").is_empty());
        assert!(findings("(defun f (&rest args) (declare (ignore args)) 1)").is_empty());
        assert!(findings("(defun f (&rest args) (declare (ignorable args)) 1)").is_empty());
    }

    /// `sequence` and `t` are not modelled, so they are silence rather than a
    /// guess — and both are in fact correct for a list.
    #[test]
    fn accepts_a_wider_or_unmodelled_type() {
        assert!(findings("(defun f (&rest args) (declare (sequence args)) args)").is_empty());
        assert!(findings("(defun f (&rest args) (declare (t args)) args)").is_empty());
        assert!(findings("(defun f (&rest args) (declare (my-type args)) args)").is_empty());
    }

    /// `null` admits the empty list but not a non-empty one, so it is still an
    /// impossible declaration for a `&rest` variable — but `list` and `cons` are
    /// not symmetric here, and only `list` is right.
    #[test]
    fn flags_a_type_that_admits_only_part_of_what_a_rest_list_can_be() {
        assert_eq!(
            findings("(defun f (&rest args) (declare (null args)) args)").len(),
            1
        );
        assert_eq!(
            findings("(defun f (&rest args) (declare (cons args)) args)").len(),
            1
        );
    }

    #[test]
    fn accepts_a_declaration_on_a_different_parameter() {
        assert!(findings("(defun f (a &rest args) (declare (fixnum a)) args)").is_empty());
    }

    #[test]
    fn accepts_a_lambda_list_with_no_rest_parameter() {
        assert!(findings("(defun f (a b) (declare (fixnum a b)) (+ a b))").is_empty());
    }

    #[test]
    fn declines_a_destructuring_pattern_after_rest() {
        assert!(findings("(defmacro m (&rest (a b)) (declare (fixnum a)) a)").is_empty());
    }

    #[test]
    fn declines_a_form_with_no_lambda_list() {
        assert!(findings("(let ((args 1)) (declare (fixnum args)) args)").is_empty());
        assert!(findings("(defun f)").is_empty());
    }

    #[test]
    fn accepts_a_compound_type_specifier() {
        assert!(
            findings("(defun f (&rest args) (declare (type (or null cons) args)) args)").is_empty()
        );
    }
}
