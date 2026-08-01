//! Emacs Lisp's own declaration sources: `cl-defstruct` slot `:type` options
//! and `defcustom`'s `:type` option.
//!
//! Deliberately narrow, matching [`super::declarations`]'s shape without
//! sharing its CLHS-specific parsing:
//!
//! - [`collect_declared_returns`] is `declarations::collect_declared_returns`'s
//!   Emacs Lisp counterpart — a name-keyed accessor return-type table, built
//!   from `cl-defstruct` slots instead of `declaim`/`proclaim` `ftype`.
//! - [`defcustom_declared_type`] is closer to `(the TYPE form)`: it narrows
//!   one form (`defcustom`'s initial-value expression) rather than a binding,
//!   because a `defcustom` name has no [`crate::semantics::binding::BindingId`]
//!   this layer's narrowing can attach to — global Emacs Lisp bindings are
//!   tracked only as a name set (see
//!   `crate::semantics::binding::service::emacs_lisp`), not resolved
//!   occurrence by occurrence the way a lexical binding is.
//!
//! This does not attempt anything like CLHS-equivalent coverage of Emacs
//! Lisp's own standard library. A `:conc-name` override, `cl-deftype`, and a
//! compound Custom widget type (`(choice …)`, `(repeat …)`) are all left
//! unmodelled; everything outside the two functions below falls through to
//! "no declared type" rather than guessing.

use std::collections::HashMap;

use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView, Path, SyntaxTree};
use paredit_core_syntax::view_query::list_head;

use super::super::model::Ty;

/// The file's `cl-defstruct` slot `:type` declarations, keyed by the default
/// accessor name (`struct-slot`, lowercased) the way `calls::infer_call`
/// looks names up.
///
/// Scoped to top-level forms, mirroring
/// `declarations::collect_declared_returns`'s scoping of `declaim`: a
/// `cl-defstruct` sitting inside an unrecognized macro call is not
/// necessarily a real one.
pub(super) fn collect_declared_returns(tree: &SyntaxTree) -> HashMap<String, Ty> {
    let mut table = HashMap::new();
    for index in 0..tree.root_children().len() {
        let Ok(selection) = tree.select_path(&Path::root_child(index)) else {
            continue;
        };
        let root = selection.view();
        if list_head(&root) != Some("cl-defstruct") {
            continue;
        }
        record_defstruct_slots(&root, &mut table);
    }
    table
}

/// `(cl-defstruct NAME-OR-OPTIONS SLOT…)`'s slots, added under
/// `NAME-SLOT` — the default `cl-defstruct` accessor name. A struct whose
/// options override `:conc-name` is skipped entirely, since that changes the
/// accessor prefix in a way this layer does not model.
fn record_defstruct_slots(view: &ExpressionView, table: &mut HashMap<String, Ty>) {
    let Some(name_or_options) = view.children.get(1) else {
        return;
    };
    let Some(struct_name) = defstruct_name(name_or_options) else {
        return;
    };
    if has_custom_conc_name(name_or_options) {
        return;
    }

    for slot in view.children.get(2..).unwrap_or(&[]) {
        let Some((slot_name, ty)) = slot_declared_type(slot) else {
            continue;
        };
        table.insert(
            format!("{struct_name}-{slot_name}").to_ascii_lowercase(),
            ty,
        );
    }
}

/// The struct name out of `NAME-OR-OPTIONS`, which is either a bare symbol or
/// a `(NAME OPTION…)` list.
fn defstruct_name(view: &ExpressionView) -> Option<&str> {
    match view.kind {
        ExpressionKind::Atom => atom_symbol_text(view),
        ExpressionKind::List => view.children.first().and_then(atom_symbol_text),
        ExpressionKind::Root => None,
    }
}

fn has_custom_conc_name(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::List
        && view.children.iter().skip(1).any(|option| {
            option.kind == ExpressionKind::List
                && option.children.first().and_then(atom_symbol_text) == Some(":conc-name")
        })
}

/// A `(SLOT-NAME [DEFAULT-VALUE] [:type TYPE] …)` slot spec's own `:type`
/// option, when it names a modelled type. A bare `SLOT-NAME` — no default, no
/// options — carries no type and is skipped, the same as one whose `:type` is
/// absent or unmodelled.
fn slot_declared_type(slot: &ExpressionView) -> Option<(&str, Ty)> {
    if slot.kind != ExpressionKind::List {
        return None;
    }
    let name = slot.children.first().and_then(atom_symbol_text)?;
    let ty = keyword_type_value(&slot.children, ":type")?;
    Some((name, ty))
}

/// `(defcustom NAME VALUE DOC …)`'s own `:type` option, when it is a bare,
/// modelled type name (`'integer`, `'string`, …). A compound Custom widget
/// type (`(choice …)`, `(repeat …)`) is not modelled.
pub(super) fn defcustom_declared_type(view: &ExpressionView) -> Option<Ty> {
    keyword_type_value(&view.children, ":type")
}

/// Scans `children` for a `keyword` marker atom and reads the modelled type
/// name immediately after it. Position-independent by design: a slot spec's
/// default-value form is optional, so `:type` may sit at different indices
/// depending on whether one was written.
fn keyword_type_value(children: &[ExpressionView], keyword: &str) -> Option<Ty> {
    let index = children
        .iter()
        .position(|child| atom_symbol_text(child) == Some(keyword))?;
    children
        .get(index + 1)
        .and_then(atom_symbol_text)
        .and_then(Ty::from_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn parse(input: &str) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(input, Dialect::EmacsLisp).expect("parse")
    }

    #[test]
    fn a_defstruct_slot_type_is_recorded_under_its_default_accessor_name() {
        let tree = parse("(cl-defstruct point (x 0 :type integer) (y 0 :type integer))");
        let table = collect_declared_returns(&tree);
        assert_eq!(table.get("point-x"), Some(&Ty::Integer));
        assert_eq!(table.get("point-y"), Some(&Ty::Integer));
    }

    #[test]
    fn a_slot_with_no_default_value_still_reads_its_type() {
        let tree = parse("(cl-defstruct point (x :type integer))");
        assert_eq!(
            collect_declared_returns(&tree).get("point-x"),
            Some(&Ty::Integer)
        );
    }

    #[test]
    fn a_bare_slot_with_no_options_declares_nothing() {
        let tree = parse("(cl-defstruct point x y)");
        assert!(collect_declared_returns(&tree).is_empty());
    }

    #[test]
    fn a_custom_conc_name_is_not_guessed_at() {
        let tree = parse("(cl-defstruct (point (:conc-name pt-)) (x 0 :type integer))");
        assert!(collect_declared_returns(&tree).is_empty());
    }

    #[test]
    fn a_non_defstruct_top_level_form_declares_nothing() {
        let tree = parse("(defun f (x) x)");
        assert!(collect_declared_returns(&tree).is_empty());
    }

    #[test]
    fn a_defcustom_type_is_read_from_a_quoted_symbol() {
        let tree = parse("(defcustom my-count 0 \"doc\" :type 'integer)");
        let selection = tree.select_path(&Path::root_child(0)).expect("select");
        assert_eq!(
            defcustom_declared_type(&selection.view()),
            Some(Ty::Integer)
        );
    }

    #[test]
    fn a_compound_defcustom_type_is_not_modelled() {
        let tree = parse("(defcustom my-choice nil \"doc\" :type '(choice integer string))");
        let selection = tree.select_path(&Path::root_child(0)).expect("select");
        assert_eq!(defcustom_declared_type(&selection.view()), None);
    }

    #[test]
    fn a_defcustom_with_no_type_option_declares_nothing() {
        let tree = parse("(defcustom my-count 0 \"doc\")");
        let selection = tree.select_path(&Path::root_child(0)).expect("select");
        assert_eq!(defcustom_declared_type(&selection.view()), None);
    }
}
