use anyhow::Result;

mod ordering;
mod slots;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path, SymbolName, SyntaxTree};

use super::visit::visit_defpackage_forms;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageOptionSortOrder {
    Canonical,
    Name,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionSortEdit {
    pub defpackage_path: String,
    pub defpackage_span: ByteSpan,
    pub package_name: String,
    pub old_options: Vec<String>,
    pub new_options: Vec<String>,
    pub replacements: Vec<OptionReplacement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionReplacement {
    pub span: ByteSpan,
    pub replacement: String,
}

/// `full_span`/`full_text` span from the newline that ends the previous
/// option's line up to this option's own end, so a leading `;;` comment (or
/// blank run) travels with the option below it when options are reordered.
/// The first option in a `defpackage` has no previous option to inherit
/// trivia from, so its slot starts right after the package name and
/// `has_leading_trivia` is `false`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionSlot {
    pub full_span: ByteSpan,
    pub full_text: String,
    pub has_leading_trivia: bool,
    pub label: String,
    pub sort_key: ordering::OptionSortKey,
}

pub fn defpackage_option_sort_edits(
    input: &str,
    tree: &SyntaxTree,
    dialect: Dialect,
    package: Option<&SymbolName>,
    order: PackageOptionSortOrder,
) -> Result<Vec<OptionSortEdit>> {
    let mut traversal = OptionSortTraversal {
        input,
        order,
        edits: Vec::new(),
    };

    visit_defpackage_forms(tree, dialect, package, |view, path, package_name| {
        analyze_defpackage_options(&mut traversal, view, path, package_name)
    })?;

    Ok(traversal.edits)
}

struct OptionSortTraversal<'a> {
    input: &'a str,
    order: PackageOptionSortOrder,
    edits: Vec<OptionSortEdit>,
}

fn analyze_defpackage_options(
    traversal: &mut OptionSortTraversal<'_>,
    view: &ExpressionView,
    path: &Path,
    package_name: &str,
) -> Result<()> {
    if view.children.len() <= 3 {
        return Ok(());
    }

    let slots = slots::collect_option_slots(traversal.input, view, path, traversal.order)?;
    let (new_options, replacements) = ordering::sort_slots(&slots);
    let old_options = slots
        .iter()
        .map(|slot| slot.label.clone())
        .collect::<Vec<_>>();

    traversal.edits.push(OptionSortEdit {
        defpackage_path: path.to_string(),
        defpackage_span: view.span,
        package_name: package_name.to_owned(),
        old_options,
        new_options,
        replacements,
    });

    Ok(())
}
