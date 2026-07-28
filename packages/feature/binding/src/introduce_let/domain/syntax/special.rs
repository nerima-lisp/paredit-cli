use paredit_core_syntax::common_lisp::{
    CommonLispDeclarationScope, CommonLispValueScopeForm, common_lisp_operator_head_eq,
    common_lisp_special_declaration_body_start,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::ExpressionView;

use super::list_head;

pub fn special_declaration_shadows_child(
    dialect: Dialect,
    view: &ExpressionView,
    form: Option<CommonLispValueScopeForm>,
    name: &str,
    child_index: usize,
) -> bool {
    matches!(dialect, Dialect::CommonLisp | Dialect::Unknown)
        && declaration_scope(view, form).is_some_and(|scope| {
            common_lisp_special_declaration_body_start(view, scope, name)
                .is_some_and(|body_start| child_index >= body_start)
        })
}

fn declaration_scope(
    view: &ExpressionView,
    form: Option<CommonLispValueScopeForm>,
) -> Option<CommonLispDeclarationScope> {
    if list_head(view).is_some_and(|head| common_lisp_operator_head_eq(head, "locally")) {
        return Some(CommonLispDeclarationScope::new(1));
    }

    form.and_then(CommonLispValueScopeForm::declaration_scope)
}
