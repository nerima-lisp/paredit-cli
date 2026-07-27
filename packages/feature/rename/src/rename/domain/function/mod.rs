use anyhow::Result;

use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_eq;
use paredit_core_syntax::definition::definition_shape;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{SymbolName, SyntaxTree};

use super::RenameFunctionOccurrence;
use super::selection::list_head;

pub mod target;
mod traversal;

pub use traversal::collect_function_call_head_renames;

pub fn collect_callable_definition_renames(
    tree: &SyntaxTree,
    dialect: Dialect,
    from: &SymbolName,
    to: &SymbolName,
) -> Result<Vec<RenameFunctionOccurrence>> {
    let mut renames = Vec::new();

    for (top_index, _) in tree.root_children().iter().enumerate() {
        let form_path = paredit_core_syntax::sexpr::Path::root_child(top_index);
        let view = tree.select_path(&form_path)?.view();
        let Some(head) = list_head(&view) else {
            continue;
        };
        let Some(shape) = definition_shape(dialect, &view, head) else {
            continue;
        };
        let Some(name_target) = shape.name_target(&view, &form_path) else {
            continue;
        };
        if !common_lisp_symbol_reference_eq(name_target.text, from.as_str()) {
            continue;
        }
        renames.push(RenameFunctionOccurrence {
            path: name_target.path.to_string(),
            span: name_target.span,
            text: from.as_str().to_owned(),
            replacement: to.as_str().to_owned(),
        });
    }

    Ok(renames)
}
