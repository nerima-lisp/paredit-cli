//! A `let` binding initialised to a literal its own declared type excludes.
//!
//! # What CLHS says
//!
//! CLHS 3.3.4 and the `type` declaration's page: a `type` declaration is a
//! promise that the variable's value is always of that type. Binding it to a
//! value outside the type breaks the promise, and CLHS says the consequences are
//! undefined — a safe implementation may signal, and an optimising one may
//! generate code that simply assumes the declaration.
//!
//! # What SBCL 2.6.0 does
//!
//! A full `WARNING` on each:
//!
//! ```lisp
//! (defun bad-3a () (let ((x 0))    (declare (string x)) x))
//! (defun bad-3b () (let ((s "hi")) (declare (fixnum s)) s))
//! (defun bad-3c () (let ((n nil))  (declare (fixnum n)) n))
//! ```
//!
//! ```text
//! ; caught WARNING:
//! ;   Constant 0 conflicts with its asserted type STRING.
//! ```
//!
//! # The idiom this must not fire on
//!
//! A placeholder initial value that a later `setf` replaces is *correct* code
//! and extremely common:
//!
//! ```lisp
//! (let ((total 0))  (declare (fixnum total))            (incf total 1) total)
//! (let ((cache nil)) (declare (type (or null hash-table) cache)) cache)
//! ```
//!
//! Both stay silent, and SBCL agrees: the first because `0` *is* a `fixnum`, the
//! second because `(or null hash-table)` is a compound specifier and
//! [`crate::support::type_excludes`] declines every compound specifier by
//! construction. That decision is what keeps this rule off the widening idiom,
//! and it is worth more than the findings it costs.

use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_is};

use crate::support::{
    LiteralKind, body_start_of, declaration_section_end, is_declare, is_reader_conditional,
    kind_text, literal_kind, symbol_name, type_declarations, type_excludes, type_text,
};

/// One binding whose declared type excludes its initial value.
#[derive(Debug, Clone)]
pub struct ContradictedBinding {
    /// The binding form `(var initform)`.
    pub span: ByteSpan,
    pub variable: String,
    pub declared: String,
    kind: LiteralKind,
}

impl ContradictedBinding {
    #[must_use]
    pub fn message(&self) -> String {
        format!(
            "{} is declared {} but is bound to {}, which that type cannot contain; the \
             declaration is a promise the binding already breaks",
            self.variable,
            self.declared,
            kind_text(self.kind)
        )
    }
}

/// Reads one binding of a `let`/`let*` bindings list as `(name, initform)`.
///
/// A bare symbol and a `(var)` binding both initialise to `nil`, but neither is
/// reported: a declaration on a placeholder the author never wrote a value for
/// is far more likely to be a deliberate "assigned later" than a mistake, and
/// the finding is not worth the argument.
fn binding_with_initform(binding: &ExpressionView) -> Option<(String, &ExpressionView)> {
    if !is_paren_list(binding) || binding.children.len() < 2 {
        return None;
    }
    let name = symbol_name(binding.children.first()?)?;
    Some((name, binding.children.get(1)?))
}

/// Every contradicted binding of one `let` or `let*`.
#[must_use]
pub fn examine_let(view: &ExpressionView) -> Vec<ContradictedBinding> {
    if !list_head(view).is_some_and(|head| symbol_is(head, "let") || symbol_is(head, "let*")) {
        return Vec::new();
    }
    let Some(start) = body_start_of(view) else {
        return Vec::new();
    };
    let Some(bindings) = view.children.get(1).filter(|list| is_paren_list(list)) else {
        return Vec::new();
    };
    let section_end = declaration_section_end(view, start);
    if section_end == start {
        // No declaration section at all: nothing to contradict, and no walk.
        return Vec::new();
    }

    let mut found = Vec::new();
    for child in view.children.iter().take(section_end).skip(start) {
        if !is_declare(child) {
            continue;
        }
        for declaration in type_declarations(child) {
            if is_reader_conditional(declaration.type_spec) {
                continue;
            }
            let declared = type_text(declaration.type_spec);
            for name in declaration.variables() {
                let Some((_, initform)) = bindings
                    .children
                    .iter()
                    .filter_map(binding_with_initform)
                    .find(|(bound, _)| *bound == name)
                else {
                    continue;
                };
                let Some(kind) = literal_kind(initform) else {
                    continue;
                };
                if !type_excludes(declaration.type_spec, kind) {
                    continue;
                }
                let span = bindings
                    .children
                    .iter()
                    .find(|binding| {
                        binding_with_initform(binding).is_some_and(|(bound, _)| bound == name)
                    })
                    .map_or(initform.span, |binding| binding.span);
                found.push(ContradictedBinding {
                    span,
                    variable: name,
                    declared: declared.clone(),
                    kind,
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

    fn findings(source: &str) -> Vec<ContradictedBinding> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        examine_let(&tree.root_view().children[0])
    }

    #[test]
    fn flags_the_references_own_example() {
        let found = findings("(let ((x 0)) (declare (string x)) x)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].variable, "x");
        assert_eq!(found[0].declared, "string");
    }

    #[test]
    fn flags_a_string_bound_where_a_fixnum_is_declared() {
        assert_eq!(
            findings("(let ((s \"hi\")) (declare (fixnum s)) s)").len(),
            1
        );
    }

    #[test]
    fn flags_nil_bound_where_a_number_is_declared() {
        assert_eq!(findings("(let ((n nil)) (declare (fixnum n)) n)").len(), 1);
    }

    #[test]
    fn flags_the_long_hand_type_spelling_too() {
        assert_eq!(
            findings("(let ((x 0)) (declare (type string x)) x)").len(),
            1
        );
    }

    #[test]
    fn flags_a_let_star_the_same_way() {
        assert_eq!(findings("(let* ((x 0)) (declare (string x)) x)").len(), 1);
    }

    // -- the correct idioms --------------------------------------------------

    /// The placeholder-then-assign idiom, which is what makes this rule
    /// dangerous if it is done carelessly.
    #[test]
    fn accepts_a_placeholder_whose_type_contains_it() {
        assert!(
            findings("(let ((total 0)) (declare (fixnum total)) (incf total) total)").is_empty()
        );
        assert!(findings("(let ((s \"\")) (declare (string s)) s)").is_empty());
    }

    /// The widening idiom. A compound specifier is declined outright.
    #[test]
    fn accepts_nil_under_a_widened_compound_type() {
        assert!(
            findings("(let ((cache nil)) (declare (type (or null hash-table) cache)) cache)")
                .is_empty()
        );
        assert!(findings("(let ((x nil)) (declare (type (or null string) x)) x)").is_empty());
    }

    #[test]
    fn accepts_a_binding_with_no_type_declaration() {
        assert!(findings("(let ((x 0)) (declare (ignore x)) 1)").is_empty());
        assert!(findings("(let ((x 0)) x)").is_empty());
    }

    #[test]
    fn accepts_an_initform_whose_type_cannot_be_read() {
        assert!(findings("(let ((x (compute))) (declare (string x)) x)").is_empty());
        assert!(findings("(let ((x y)) (declare (fixnum x)) x)").is_empty());
    }

    #[test]
    fn accepts_an_unmodelled_declared_type() {
        assert!(findings("(let ((x 0)) (declare (my-widget x)) x)").is_empty());
        assert!(findings("(let ((x 0)) (declare (t x)) x)").is_empty());
    }

    /// `nil` is a `list`, a `symbol` and a `boolean`; none of those may fire.
    #[test]
    fn accepts_nil_under_every_type_that_contains_it() {
        for declared in ["list", "symbol", "boolean", "null"] {
            assert!(
                findings(&format!("(let ((x nil)) (declare ({declared} x)) x)")).is_empty(),
                "{declared} contains NIL"
            );
        }
    }

    #[test]
    fn accepts_a_declaration_naming_a_variable_bound_elsewhere() {
        assert!(findings("(let ((x 0)) (declare (string y)) x)").is_empty());
    }

    #[test]
    fn declines_a_binding_with_no_initform() {
        assert!(findings("(let (x) (declare (fixnum x)) x)").is_empty());
        assert!(findings("(let ((x)) (declare (fixnum x)) x)").is_empty());
    }

    #[test]
    fn flags_each_contradicted_binding() {
        assert_eq!(
            findings("(let ((x 0) (s \"a\")) (declare (string x) (fixnum s)) 1)").len(),
            2
        );
    }
}
