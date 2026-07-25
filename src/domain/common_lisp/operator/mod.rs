use super::CommonLispBindingRefactorForm;

mod binding_forms;
mod classify;
mod definition;
mod kind;
mod normalize;
mod table;

pub(crate) use kind::CommonLispOperator;

pub(crate) use normalize::{
    common_lisp_operator_head_eq, common_lisp_symbol_identity_eq, common_lisp_symbol_name_eq,
    common_lisp_symbol_reference_eq, common_lisp_symbol_reference_needle,
    has_common_lisp_package_qualifier, is_common_lisp_declaration_form,
    is_common_lisp_earmuffed_special_variable_name, normalize_common_lisp_operator_head,
    normalize_common_lisp_package_designator,
};

impl CommonLispOperator {
    pub(crate) fn from_head(head: &str) -> Option<Self> {
        table::common_lisp_operator_from_head(head)
    }
}

pub(crate) fn common_lisp_binding_refactor_form_for_head(
    head: &str,
) -> Option<CommonLispBindingRefactorForm> {
    CommonLispOperator::from_head(head)?.binding_refactor_form()
}
