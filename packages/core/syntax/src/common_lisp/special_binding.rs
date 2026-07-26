use crate::domain::sexpr::{ExpressionKind, ExpressionView, ReaderPrefix, SymbolName};

use super::{
    CommonLispDeclarationScope, common_lisp_operator_head_eq, common_lisp_symbol_reference_eq,
};

/// Returns the first body child covered by a leading `(declare (special ...))`
/// declaration for `name`.
pub(crate) fn common_lisp_special_declaration_body_start(
    view: &ExpressionView,
    scope: CommonLispDeclarationScope,
    name: &str,
) -> Option<usize> {
    let declaration_start = scope.declaration_start_index();
    let declaration_and_body = view.children.get(declaration_start..)?;
    let body_start = declaration_and_body
        .iter()
        .position(|child| !is_declare_form(child))
        .map(|offset| declaration_start + offset)?;

    declaration_and_body[..body_start - declaration_start]
        .iter()
        .any(|declaration| is_special_declaration_name(declaration, "declare", name))
        .then_some(body_start)
}

/// Returns whether a binding may have dynamic-scope effects through a
/// `declaim`, `proclaim`, `defvar`, `defparameter`, or an enclosing lexical
/// `declare (special ...)` declaration, including one at the start of the
/// binding form's own body.
pub(crate) fn common_lisp_dynamic_binding_is_declared(
    document: &ExpressionView,
    target: &ExpressionView,
    symbol: &SymbolName,
) -> bool {
    contains_global_special_declaration(document, symbol)
        || target_declares_special(target, symbol)
        || ancestor_declares_special(document, target, symbol)
}

fn target_declares_special(target: &ExpressionView, symbol: &SymbolName) -> bool {
    target
        .children
        .get(2..)
        .into_iter()
        .flatten()
        .take_while(|child| is_declare_form(child))
        .any(|declaration| is_special_declaration(declaration, "declare", symbol))
}

fn contains_global_special_declaration(view: &ExpressionView, symbol: &SymbolName) -> bool {
    is_special_declaration(view, "declaim", symbol)
        || is_special_proclamation(view, symbol)
        || is_special_variable_definition(view, symbol)
        || view
            .children
            .iter()
            .any(|child| contains_global_special_declaration(child, symbol))
}

fn ancestor_declares_special(
    view: &ExpressionView,
    target: &ExpressionView,
    symbol: &SymbolName,
) -> bool {
    let Some(target_child_index) = view
        .children
        .iter()
        .position(|child| contains_span(child, target))
    else {
        return false;
    };

    view.children[..target_child_index]
        .iter()
        .any(|child| contains_special_declaration(child, symbol))
        || ancestor_declares_special(&view.children[target_child_index], target, symbol)
}

fn contains_span(view: &ExpressionView, target: &ExpressionView) -> bool {
    view.span.start() <= target.span.start() && view.span.end() >= target.span.end()
}

fn contains_special_declaration(view: &ExpressionView, symbol: &SymbolName) -> bool {
    is_special_declaration(view, "declare", symbol)
        || view
            .children
            .iter()
            .any(|child| is_special_declaration(child, "declare", symbol))
}

fn is_special_proclamation(view: &ExpressionView, symbol: &SymbolName) -> bool {
    view.kind == ExpressionKind::List
        && view
            .children
            .first()
            .and_then(atom_text)
            .is_some_and(|head| common_lisp_operator_head_eq(head, "proclaim"))
        && view.children[1..]
            .iter()
            .filter(|argument| argument.reader_prefixes.contains(&ReaderPrefix::Quote))
            .any(|argument| is_special_specifier(argument, symbol))
}

fn is_special_variable_definition(view: &ExpressionView, symbol: &SymbolName) -> bool {
    view.kind == ExpressionKind::List
        && view
            .children
            .first()
            .and_then(atom_text)
            .is_some_and(|head| {
                common_lisp_operator_head_eq(head, "defvar")
                    || common_lisp_operator_head_eq(head, "defparameter")
            })
        && view.children.get(1).is_some_and(|name| {
            name.kind == ExpressionKind::Atom
                && name
                    .text
                    .as_deref()
                    .is_some_and(|name| common_lisp_symbol_reference_eq(name, symbol.as_str()))
        })
}

fn is_special_declaration(
    view: &ExpressionView,
    declaration_head: &str,
    symbol: &SymbolName,
) -> bool {
    is_special_declaration_name(view, declaration_head, symbol.as_str())
}

fn is_special_declaration_name(view: &ExpressionView, declaration_head: &str, name: &str) -> bool {
    view.kind == ExpressionKind::List
        && view
            .children
            .first()
            .and_then(atom_text)
            .is_some_and(|head| common_lisp_operator_head_eq(head, declaration_head))
        && view.children[1..]
            .iter()
            .any(|specifier| is_special_specifier_name(specifier, name))
}

fn is_special_specifier(view: &ExpressionView, symbol: &SymbolName) -> bool {
    is_special_specifier_name(view, symbol.as_str())
}

fn is_special_specifier_name(view: &ExpressionView, name: &str) -> bool {
    view.kind == ExpressionKind::List
        && view
            .children
            .first()
            .and_then(atom_text)
            .is_some_and(|head| common_lisp_operator_head_eq(head, "special"))
        && view.children[1..].iter().any(|declared| {
            declared.kind == ExpressionKind::Atom
                && declared
                    .text
                    .as_deref()
                    .is_some_and(|candidate| common_lisp_symbol_reference_eq(candidate, name))
        })
}

fn is_declare_form(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::List
        && view
            .children
            .first()
            .and_then(atom_text)
            .is_some_and(|head| common_lisp_operator_head_eq(head, "declare"))
}

fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}
