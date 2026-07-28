//! Common Lisp syntax helpers shared across semantic refactoring passes.

mod forms;
mod operator;
mod reader_condition;
mod reader_label;
mod reader_literal;
mod scope;
mod special_binding;

pub use forms::{
    CommonLispBindingListShape, CommonLispBindingRefactorForm, CommonLispBindingReferenceScope,
    CommonLispDeclarationScope, CommonLispHandlerBindingForm, CommonLispLambdaListShape,
    CommonLispLetBindingForm, CommonLispLocalCallableForm, CommonLispPackageDeclarationForm,
    CommonLispResourceBindingForm, CommonLispRuntimeDependencyForm, CommonLispSlotBindingForm,
    CommonLispValueScopeForm, CommonLispVariableBindingForm, CommonLispVariableSpecForm,
};
pub use operator::{
    CommonLispOperator, common_lisp_binding_refactor_form_for_head, common_lisp_operator_head_eq,
    common_lisp_symbol_identity_eq, common_lisp_symbol_name_eq, common_lisp_symbol_reference_eq,
    common_lisp_symbol_reference_needle, has_common_lisp_package_qualifier,
    is_common_lisp_declaration_form, is_common_lisp_earmuffed_special_variable_name,
    normalize_common_lisp_operator_head, normalize_common_lisp_package_designator,
};
#[cfg(test)]
pub use reader_condition::common_lisp_reader_conditional_dispatches;
pub use reader_condition::{
    CommonLispReaderConditionalKind, common_lisp_reader_conditional_forms,
    reader_conditional_kind as common_lisp_reader_conditional_kind,
};
#[cfg(test)]
pub use reader_label::common_lisp_reader_label_dispatches;
pub use reader_label::{
    CommonLispReaderLabelKind, common_lisp_reader_label_forms,
    reader_label_kind as common_lisp_reader_label_kind,
};
pub use reader_literal::{
    CommonLispReaderLiteralKind, common_lisp_reader_literals,
    reader_literal_kind as common_lisp_reader_literal_kind,
};
pub use scope::{
    common_lisp_local_callable_form, common_lisp_macro_expander_path, is_local_callable_bound,
    is_macro_callable_form, local_callable_binding_body_scope, local_callable_body_scope,
    local_callable_definition_reference_scope, local_callable_names, local_callable_scope_at_path,
};
pub use special_binding::{
    common_lisp_dynamic_binding_is_declared, common_lisp_special_declaration_body_start,
};

#[cfg(test)]
mod tests;
