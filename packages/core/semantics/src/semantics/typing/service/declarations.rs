//! `declare`/`declaim`/`proclaim` and `check-type`: the sources that narrow a
//! *binding* (as opposed to one occurrence) directly.
//!
//! Their variable-spec atoms are not references the binding builder resolves
//! — `BindingTableBuilder::body` skips a leading `declare` entirely, since a
//! declaration is not evaluated — so [`resolve_declared_name`] finds the
//! binding the same way lexical scoping does: by matching name inside the
//! smallest enclosing scope whose opener textually contains the declaration.
//! `check-type`'s place is an ordinary evaluated position, so its variable
//! *is* resolvable the normal way.

use std::collections::HashMap;

use paredit_core_syntax::common_lisp::common_lisp_symbol_name_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::{atom_symbol_span, atom_symbol_text};
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, Path, SyntaxTree};
use paredit_core_syntax::view_query::list_head;

use crate::semantics::NodeKey;
use crate::semantics::binding::{BindingId, BindingTable};

use super::super::model::{Ty, TypeTableBuilder};

/// Every decl-spec of a `(declare …)`/`(declaim …)`/`(proclaim …)` form — the
/// parenthesized items after its head, skipping any that are not lists (which
/// is not valid CLHS syntax, but nothing here should panic on it).
fn decl_specs(view: &ExpressionView) -> impl Iterator<Item = &ExpressionView> {
    view.children
        .iter()
        .skip(1)
        .filter(|child| child.kind == ExpressionKind::List)
}

/// Narrows every binding a `declare`/`declaim`/`proclaim` form's `(type TYPE
/// var…)` decl-specs — or the `(TYPE var…)` shorthand, when `TYPE` is itself
/// a modelled type name — name.
///
/// `ftype` decl-specs are deliberately skipped: a declared function has no
/// `Ty` to narrow onto (`Ty::Function` carries no return type), so its return
/// type lives in the name-keyed table [`collect_declared_returns`] builds
/// instead, which `calls` consults.
pub(super) fn narrow_declared_bindings(
    view: &ExpressionView,
    bindings: &BindingTable,
    builder: &mut TypeTableBuilder,
) {
    for spec in decl_specs(view) {
        let Some(head) = spec.children.first().and_then(atom_symbol_text) else {
            continue;
        };
        if common_lisp_symbol_name_eq(head, "ftype") {
            continue;
        }

        let (ty, vars) = if common_lisp_symbol_name_eq(head, "type") {
            let ty = spec
                .children
                .get(1)
                .and_then(atom_symbol_text)
                .and_then(Ty::from_name);
            (ty, spec.children.get(2..).unwrap_or(&[]))
        } else {
            (Ty::from_name(head), spec.children.get(1..).unwrap_or(&[]))
        };
        let Some(ty) = ty else { continue };

        for var in vars {
            let Some(name) = atom_symbol_text(var) else {
                continue;
            };
            if let Some(binding) = resolve_declared_name(bindings, name, var.span) {
                builder.narrow_binding(binding, ty);
            }
        }
    }
}

/// Narrows a `(check-type place TYPE [string])` form's place — only when
/// `place` is a bare, resolvable variable; a compound place (`(check-type
/// (car x) …)`) has no [`BindingId`] to narrow.
pub(super) fn narrow_check_type(
    view: &ExpressionView,
    bindings: &BindingTable,
    builder: &mut TypeTableBuilder,
) {
    let Some(place) = view.children.get(1) else {
        return;
    };
    let Some(type_spec) = view.children.get(2) else {
        return;
    };
    let Some(ty) = atom_symbol_text(type_spec).and_then(Ty::from_name) else {
        return;
    };
    let Some(span) = atom_symbol_span(place) else {
        return;
    };
    let Some(binding) = bindings.resolve(NodeKey::atom(span)) else {
        return;
    };
    builder.narrow_binding(binding, ty);
}

/// The binding named `name` whose lexical scope innermost-encloses `at`.
///
/// Purely span-based: among every binding named `name`, the candidate scopes
/// are those whose opener span contains `at`, and the innermost is the one
/// with the smallest such span. This reconstructs ordinary lexical shadowing
/// without re-walking the binder logic that already produced [`BindingTable`]
/// — Common Lisp binders nest textually, so span containment and scope
/// nesting agree.
fn resolve_declared_name(bindings: &BindingTable, name: &str, at: ByteSpan) -> Option<BindingId> {
    bindings
        .bindings()
        .filter(|(_, binding)| common_lisp_symbol_name_eq(binding.name().as_str(), name))
        .filter_map(|(id, binding)| {
            let opener = bindings.scope(binding.scope()).opener()?;
            opener.contains_span(at).then_some((id, opener.len()))
        })
        .min_by_key(|(_, len)| *len)
        .map(|(id, _)| id)
}

/// The file's top-level `(declaim (ftype (function (…) RETURN) name…))` (and
/// `proclaim`) declarations, keyed by lowercased function name.
///
/// Scoped to top-level forms only, mirroring
/// `value::service::propagation::collect_constants`'s scoping of
/// `defconstant`: a `declaim` is conventionally top-level, and restricting
/// the scan there sidesteps the question of whether one sitting inside an
/// unrecognized macro call was really evaluated as a declaration at all.
///
/// Only the unambiguous three-element `(function (arg-types…) RETURN)` shape
/// is modelled; a `function` type-specifier with no return type, or a
/// compound return type, is skipped rather than guessed at.
pub(super) fn collect_declared_returns(dialect: Dialect, tree: &SyntaxTree) -> HashMap<String, Ty> {
    let mut table = HashMap::new();
    if dialect != Dialect::CommonLisp {
        return table;
    }

    for index in 0..tree.root_children().len() {
        let Ok(selection) = tree.select_path(&Path::root_child(index)) else {
            continue;
        };
        let root = selection.view();
        let Some(head) = list_head(&root) else {
            continue;
        };
        if !(common_lisp_symbol_name_eq(head, "declaim")
            || common_lisp_symbol_name_eq(head, "proclaim"))
        {
            continue;
        }
        for spec in decl_specs(&root) {
            record_ftype(spec, &mut table);
        }
    }

    table
}

fn record_ftype(spec: &ExpressionView, table: &mut HashMap<String, Ty>) {
    let Some(head) = spec.children.first().and_then(atom_symbol_text) else {
        return;
    };
    if !common_lisp_symbol_name_eq(head, "ftype") {
        return;
    }
    let Some(function_type) = spec.children.get(1) else {
        return;
    };
    let [fn_head, _arg_types, return_spec] = function_type.children.as_slice() else {
        return;
    };
    let Some(fn_head_text) = atom_symbol_text(fn_head) else {
        return;
    };
    if !common_lisp_symbol_name_eq(fn_head_text, "function") {
        return;
    }
    let Some(return_ty) = atom_symbol_text(return_spec).and_then(Ty::from_name) else {
        return;
    };

    for name in &spec.children[2..] {
        if let Some(text) = atom_symbol_text(name) {
            table.insert(text.to_ascii_lowercase(), return_ty);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantics::binding::build_binding_table;

    #[test]
    fn a_non_common_lisp_file_declares_nothing() {
        let tree = SyntaxTree::parse_with_dialect(
            "(declaim (ftype (function (integer) string) f))",
            Dialect::Clojure,
        )
        .expect("parse");
        assert!(collect_declared_returns(Dialect::Clojure, &tree).is_empty());
    }

    #[test]
    fn an_ftype_with_a_compound_return_is_skipped() {
        let input = "(declaim (ftype (function (integer) (or string null)) f))";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        assert!(collect_declared_returns(Dialect::CommonLisp, &tree).is_empty());
    }

    #[test]
    fn an_ftype_with_an_unmodelled_return_is_skipped() {
        let input = "(declaim (ftype (function (integer) simple-array) f))";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        assert!(collect_declared_returns(Dialect::CommonLisp, &tree).is_empty());
    }

    #[test]
    fn a_declaimed_ftype_is_recorded_by_lowercased_name() {
        let input = "(declaim (ftype (function (integer) string) MyFn))";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let table = collect_declared_returns(Dialect::CommonLisp, &tree);
        assert_eq!(table.get("myfn"), Some(&Ty::String));
    }

    #[test]
    fn resolve_declared_name_finds_the_innermost_matching_scope() {
        let input = "(let ((x 1)) (let ((x 2)) (declare (type integer x))))";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let bindings = build_binding_table(Dialect::CommonLisp, &tree, input);
        let declare_at = input.find("x))))").expect("declare's x");
        let span = ByteSpan::new(
            paredit_core_syntax::sexpr::ByteOffset::new(declare_at),
            paredit_core_syntax::sexpr::ByteOffset::new(declare_at + 1),
        );
        let outer = bindings
            .bindings()
            .find(|(_, b)| b.definition().start().get() == input.find("(x 1)").unwrap() + 1)
            .map(|(id, _)| id);
        let inner = bindings
            .bindings()
            .find(|(_, b)| b.definition().start().get() == input.find("(x 2)").unwrap() + 1)
            .map(|(id, _)| id);
        let resolved = resolve_declared_name(&bindings, "x", span);
        assert_ne!(resolved, outer);
        assert_eq!(resolved, inner);
    }
}
