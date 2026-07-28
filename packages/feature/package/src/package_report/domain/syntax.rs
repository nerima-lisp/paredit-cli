use paredit_core_syntax::common_lisp::CommonLispPackageDeclarationForm;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView};

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

pub fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}
