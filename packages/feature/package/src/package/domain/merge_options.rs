use anyhow::Result;

mod merge;
mod slots;

use crate::domain::{
    dialect::Dialect,
    sexpr::{ByteSpan, ExpressionView, Path, SymbolName, SyntaxTree},
};

use super::visit::visit_defpackage_forms;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OptionMergeEdit {
    pub(super) defpackage_path: String,
    pub(super) defpackage_span: ByteSpan,
    pub(super) package_name: String,
    pub(super) merges: Vec<OptionMerge>,
    pub(super) replacements: Vec<OptionReplacement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OptionMerge {
    pub(super) head: String,
    pub(super) key: Option<String>,
    pub(super) kept_path: String,
    pub(super) kept_span: ByteSpan,
    pub(super) removed_paths: Vec<String>,
    pub(super) removed_spans: Vec<ByteSpan>,
    pub(super) old_atoms: Vec<String>,
    pub(super) new_atoms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OptionReplacement {
    pub(super) span: ByteSpan,
    pub(super) replacement: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::domain::package) struct OptionSlot {
    pub(in crate::domain::package) path: String,
    pub(in crate::domain::package) span: ByteSpan,
    pub(in crate::domain::package) head_text: String,
    pub(in crate::domain::package) name: String,
    pub(in crate::domain::package) key: Option<String>,
    pub(in crate::domain::package) body_atoms: Vec<String>,
}

pub(super) fn defpackage_option_merge_edits(
    input: &str,
    tree: &SyntaxTree,
    dialect: Dialect,
    package: Option<&SymbolName>,
) -> Result<Vec<OptionMergeEdit>> {
    let mut edits = Vec::new();
    visit_defpackage_forms(tree, dialect, package, |view, path, package_name| {
        analyze_defpackage_options(tree, view, path, package_name, &mut edits)
    })?;

    let _ = input;
    Ok(edits)
}

fn analyze_defpackage_options(
    tree: &SyntaxTree,
    view: &ExpressionView,
    path: &Path,
    package_name: &str,
    edits: &mut Vec<OptionMergeEdit>,
) -> Result<()> {
    if view.children.len() <= 3 {
        return Ok(());
    }

    let slots = slots::collect_option_slots(view, path)?;
    let (merges, replacements) = merge::merge_slots(&slots, tree);
    if merges.is_empty() {
        return Ok(());
    }

    edits.push(OptionMergeEdit {
        defpackage_path: path.to_string(),
        defpackage_span: view.span,
        package_name: package_name.to_owned(),
        merges,
        replacements,
    });

    Ok(())
}
