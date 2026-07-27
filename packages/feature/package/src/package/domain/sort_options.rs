use anyhow::Result;

mod ordering;
mod slots;

use crate::domain::{
    dialect::Dialect,
    sexpr::{ByteSpan, ExpressionView, Path, SymbolName, SyntaxTree},
};

use super::visit::visit_defpackage_forms;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageOptionSortOrder {
    Canonical,
    Name,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OptionSortEdit {
    pub(super) defpackage_path: String,
    pub(super) defpackage_span: ByteSpan,
    pub(super) package_name: String,
    pub(super) old_options: Vec<String>,
    pub(super) new_options: Vec<String>,
    pub(super) replacements: Vec<OptionReplacement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OptionReplacement {
    pub(super) span: ByteSpan,
    pub(super) replacement: String,
}

/// `full_span`/`full_text` span from the newline that ends the previous
/// option's line up to this option's own end, so a leading `;;` comment (or
/// blank run) travels with the option below it when options are reordered.
/// The first option in a `defpackage` has no previous option to inherit
/// trivia from, so its slot starts right after the package name and
/// `has_leading_trivia` is `false`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::domain::package) struct OptionSlot {
    pub(in crate::domain::package) full_span: ByteSpan,
    pub(in crate::domain::package) full_text: String,
    pub(in crate::domain::package) has_leading_trivia: bool,
    pub(in crate::domain::package) label: String,
    pub(in crate::domain::package) sort_key: ordering::OptionSortKey,
}

pub(super) fn defpackage_option_sort_edits(
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
