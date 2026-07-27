use crate::domain::{
    common_lisp::CommonLispPackageDeclarationForm,
    dialect::Dialect,
    sexpr::{ExpressionKind, ExpressionView},
};

pub(super) fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

pub(super) fn package_atoms_match(left: &str, right: &str) -> bool {
    normalize_package_atom(left).eq_ignore_ascii_case(normalize_package_atom(right))
}

pub(super) fn normalize_package_atom(value: &str) -> &str {
    value
        .strip_prefix("#:")
        .or_else(|| value.strip_prefix(':'))
        .unwrap_or(value)
}

pub(super) fn is_package_head(
    dialect: Dialect,
    head: &str,
    expected: CommonLispPackageDeclarationForm,
) -> bool {
    dialect.common_lisp_package_declaration_form_for_head(head) == Some(expected)
}

pub(super) fn package_option_name(head: &str) -> String {
    head.trim_start_matches(':').to_ascii_lowercase()
}

pub(super) fn package_option_atoms(option: &ExpressionView) -> impl Iterator<Item = String> + '_ {
    option
        .children
        .iter()
        .filter_map(atom_text)
        .map(ToOwned::to_owned)
}
