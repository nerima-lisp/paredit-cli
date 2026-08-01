//! Scheme's and Racket's own declaration sources this layer models: R7RS
//! `define-record-type` predicates (both dialects) and Typed Racket's
//! `(: name (-> Args… Return))` function-type annotations (Racket only).
//!
//! Deliberately narrow, matching [`super::emacs_lisp_declarations`]'s shape:
//! this does not attempt a [`Ty`] for a record's constructor or accessors —
//! the coarse type lattice has no "record of type NAME" member, and
//! inventing one would ripple into every dialect that shares the lattice,
//! Common Lisp included — but a record's own predicate always returns a
//! boolean by construction, which is a real, sound fact this layer can
//! record under the same name-keyed lookup
//! [`super::calls::infer_call`] already consults for a declared `ftype`.
//! Typed Racket's non-function `(: name Type)` (a plain value annotation, as
//! opposed to a function type) is left unmodelled: this layer's binding
//! narrowing has no [`crate::semantics::binding::BindingId`] to attach a
//! file-level declaration to for a name whose own `define` lives in a
//! separate top-level form, unlike Common Lisp's lexically-scoped `declare`.

use std::collections::HashMap;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ExpressionView, Path, SyntaxTree};
use paredit_core_syntax::view_query::list_head;

use super::super::model::Ty;

/// The file's `define-record-type` predicates (Scheme and Racket) and, for
/// Racket only, its Typed Racket `(: name (-> …))` function annotations.
///
/// Scoped to top-level forms, mirroring
/// `declarations::collect_declared_returns`'s scoping of `declaim`.
pub(super) fn collect_declared_returns(dialect: Dialect, tree: &SyntaxTree) -> HashMap<String, Ty> {
    let mut table = HashMap::new();
    for index in 0..tree.root_children().len() {
        let Ok(selection) = tree.select_path(&Path::root_child(index)) else {
            continue;
        };
        let root = selection.view();
        match list_head(&root) {
            Some("define-record-type") => record_define_record_type_predicate(&root, &mut table),
            Some(":") if dialect == Dialect::Racket => {
                record_typed_racket_function_annotation(&root, &mut table);
            }
            _ => {}
        }
    }
    table
}

/// `(define-record-type NAME (CTOR FIELD…) PRED (FIELD ACCESSOR [MODIFIER])…)`'s
/// `PRED`: a record predicate takes one argument and answers a boolean, by
/// construction, regardless of what the record type itself is.
fn record_define_record_type_predicate(view: &ExpressionView, table: &mut HashMap<String, Ty>) {
    let Some(predicate) = view.children.get(3).and_then(atom_symbol_text) else {
        return;
    };
    table.insert(predicate.to_ascii_lowercase(), Ty::Boolean);
}

/// Typed Racket's `(: name (-> Arg… Return))`, added under `name` the way a
/// Common Lisp `declaim ftype` is. A non-function `(: name Type)` is left
/// unmodelled — see the module doc.
fn record_typed_racket_function_annotation(view: &ExpressionView, table: &mut HashMap<String, Ty>) {
    let Some(name) = view.children.get(1).and_then(atom_symbol_text) else {
        return;
    };
    let Some(type_spec) = view.children.get(2) else {
        return;
    };
    if type_spec.children.first().and_then(atom_symbol_text) != Some("->") {
        return;
    }
    let Some(return_ty) = type_spec
        .children
        .last()
        .and_then(atom_symbol_text)
        .and_then(Ty::from_name)
    else {
        return;
    };
    table.insert(name.to_ascii_lowercase(), return_ty);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str, dialect: Dialect) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(input, dialect).expect("parse")
    }

    #[test]
    fn a_scheme_record_predicate_returns_boolean() {
        let tree = parse(
            "(define-record-type point (make-point x y) point? (x point-x) (y point-y))",
            Dialect::Scheme,
        );
        let table = collect_declared_returns(Dialect::Scheme, &tree);
        assert_eq!(table.get("point?"), Some(&Ty::Boolean));
        // The accessors themselves are not modelled: there is no `Ty` for
        // "whatever a `point`'s `x` field holds".
        assert!(!table.contains_key("point-x"));
    }

    #[test]
    fn a_racket_record_predicate_returns_boolean() {
        let tree = parse(
            "(define-record-type point (make-point x y) point? (x point-x) (y point-y))",
            Dialect::Racket,
        );
        assert_eq!(
            collect_declared_returns(Dialect::Racket, &tree).get("point?"),
            Some(&Ty::Boolean)
        );
    }

    #[test]
    fn a_typed_racket_function_annotation_is_recorded_by_its_return_type() {
        let tree = parse("(: convert (-> Integer String))", Dialect::Racket);
        let table = collect_declared_returns(Dialect::Racket, &tree);
        assert_eq!(table.get("convert"), Some(&Ty::String));
    }

    #[test]
    fn a_typed_racket_annotation_is_scheme_only_ignored() {
        // `:` is not a Scheme special form, so this is inert there — the same
        // way an unrecognized head anywhere else is.
        let tree = parse("(: convert (-> Integer String))", Dialect::Scheme);
        assert!(collect_declared_returns(Dialect::Scheme, &tree).is_empty());
    }

    #[test]
    fn a_non_function_typed_racket_annotation_is_not_modelled() {
        let tree = parse("(: my-count Integer)", Dialect::Racket);
        assert!(collect_declared_returns(Dialect::Racket, &tree).is_empty());
    }

    #[test]
    fn a_multi_argument_typed_racket_annotation_uses_the_last_type_as_the_return() {
        let tree = parse("(: add (-> Integer Integer Integer))", Dialect::Racket);
        assert_eq!(
            collect_declared_returns(Dialect::Racket, &tree).get("add"),
            Some(&Ty::Integer)
        );
    }
}
