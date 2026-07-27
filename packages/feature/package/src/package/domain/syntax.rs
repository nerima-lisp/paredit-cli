use paredit_core_syntax::common_lisp::CommonLispPackageDeclarationForm;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView};

pub fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

pub fn package_atoms_match(left: &str, right: &str) -> bool {
    normalize_package_atom(left).eq_ignore_ascii_case(normalize_package_atom(right))
}

pub fn normalize_package_atom(value: &str) -> &str {
    value
        .strip_prefix("#:")
        .or_else(|| value.strip_prefix(':'))
        .unwrap_or(value)
}

pub fn is_package_head(
    dialect: Dialect,
    head: &str,
    expected: CommonLispPackageDeclarationForm,
) -> bool {
    dialect.common_lisp_package_declaration_form_for_head(head) == Some(expected)
}

pub fn package_option_name(head: &str) -> String {
    head.trim_start_matches(':').to_ascii_lowercase()
}

pub fn package_option_atoms(option: &ExpressionView) -> impl Iterator<Item = String> + '_ {
    option
        .children
        .iter()
        .filter_map(atom_text)
        .map(ToOwned::to_owned)
}
