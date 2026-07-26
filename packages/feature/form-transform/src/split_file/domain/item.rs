use anyhow::Result;

use crate::domain::common_lisp::CommonLispPackageDeclarationForm;
use crate::domain::definition::definition_shape;
use crate::domain::dialect::Dialect;
use crate::domain::leading_trivia::first_newline_or;
use crate::domain::sexpr::{ByteOffset, ByteSpan, Path, SyntaxTree};

use super::syntax::{atom_child, list_head};
use super::{SplitFileDefinition, SplitFileItem};

pub(super) fn build_split_file_item(
    from_tree: &SyntaxTree,
    from_input: &str,
    from_dialect: Dialect,
    path: Path,
    target_index: usize,
) -> Result<SplitFileItem> {
    let selection = from_tree.select_path(&path)?;
    let view = selection.view();
    let span = selection.span();
    let Some(head) = list_head(&view) else {
        anyhow::bail!("selected top-level form is not a list definition: {path}");
    };
    let Some(shape) = definition_shape(from_dialect, &view, head) else {
        anyhow::bail!(
            "selected top-level form is not recognized as a definition at {path}: {head}"
        );
    };

    // A leading own-line comment (or blank run) describing this definition
    // lives outside its own span. Fold it into the moved text and the
    // removal span so it travels with the definition instead of being
    // orphaned in the source file. The very first top-level form has no
    // preceding sibling to draw a boundary from, so a file-header comment
    // above it is left in place rather than assumed to belong to it.
    let leading_start = if target_index == 0 {
        span.start().get()
    } else {
        let previous_end = from_tree
            .select_path(&Path::root_child(target_index - 1))?
            .span()
            .end()
            .get();
        first_newline_or(from_input, previous_end, span.start().get())
    };
    let move_span = ByteSpan::new(ByteOffset::new(leading_start), span.end());

    // Destination placement (`append_top_level_definitions`, and package
    // injection) already supplies its own separating blank line before this
    // text, so drop the leading newline captured above instead of stacking a
    // second blank line in the destination file. A captured comment's own
    // indentation is untouched; only the boundary newline(s) are dropped.
    let definition_text = move_span
        .slice(from_input)
        .trim_start_matches('\n')
        .to_owned();
    let definition = SplitFileDefinition {
        path: path.to_string(),
        span,
        head: head.to_owned(),
        name: shape.name(&view).map(ToOwned::to_owned),
        category: shape.category,
        parameter_count: shape.lambda_parameter_count(&view),
        body_form_count: Some(shape.body_form_count(&view)),
        package: package_context_before_top_level(from_tree, from_dialect, target_index)?,
    };

    Ok(SplitFileItem {
        path,
        span,
        removal_span: move_span,
        definition,
        definition_text,
    })
}

pub(super) fn package_context_before_top_level(
    tree: &SyntaxTree,
    dialect: Dialect,
    target_index: usize,
) -> Result<Option<String>> {
    let mut current_package = None;
    for index in 0..target_index {
        let path = Path::root_child(index);
        let view = tree.select_path(&path)?.view();
        if list_head(&view)
            .and_then(|head| dialect.common_lisp_package_declaration_form_for_head(head))
            == Some(CommonLispPackageDeclarationForm::InPackage)
        {
            if let Some(package_name) = atom_child(&view, 1) {
                current_package = Some(package_name.to_owned());
            }
        }
    }
    Ok(current_package)
}
