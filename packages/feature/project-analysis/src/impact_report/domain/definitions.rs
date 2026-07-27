use anyhow::Result;

use crate::domain::common_lisp::{
    CommonLispPackageDeclarationForm, common_lisp_symbol_reference_eq,
};
use crate::domain::definition::definition_shape;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{Path, SymbolName, SyntaxTree};

use super::syntax::{atom_child, list_head};
use super::types::ImpactDefinitionItem;

pub(super) fn collect_impact_definitions(
    tree: &SyntaxTree,
    dialect: Dialect,
) -> Result<(Option<String>, Vec<ImpactDefinitionItem>)> {
    let mut current_package = None;
    let mut definitions = Vec::new();

    for index in 0..tree.root_children().len() {
        let path = Path::root_child(index);
        let view = tree.select_path(&path)?.view();
        let Some(head) = list_head(&view) else {
            continue;
        };

        if dialect.common_lisp_package_declaration_form_for_head(head)
            == Some(CommonLispPackageDeclarationForm::InPackage)
        {
            if let Some(package_name) = atom_child(&view, 1) {
                current_package = Some(package_name.to_owned());
            }
            continue;
        }

        let Some(shape) = definition_shape(dialect, &view, head) else {
            continue;
        };

        definitions.push(ImpactDefinitionItem {
            path: path.to_string(),
            span: view.span,
            head: head.to_owned(),
            name: shape.name(&view).map(ToOwned::to_owned),
            category: shape.category,
            parameter_count: shape.lambda_parameter_count(&view),
            parameter_arity: shape.lambda_parameter_arity(&view),
            body_form_count: Some(shape.body_form_count(&view)),
            package: current_package.clone(),
        });
    }

    Ok((current_package, definitions))
}

pub(super) fn impact_definition_matches_signature(
    definition: &ImpactDefinitionItem,
    symbol: Option<&SymbolName>,
) -> bool {
    definition.parameter_count.is_some()
        && definition.category.is_callable()
        && definition.name.as_deref().is_some_and(|name| {
            symbol.is_none_or(|target| common_lisp_symbol_reference_eq(name, target.as_str()))
        })
}
