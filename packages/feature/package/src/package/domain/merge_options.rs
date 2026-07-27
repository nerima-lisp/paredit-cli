use anyhow::Result;

mod merge;
mod slots;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path, SymbolName, SyntaxTree};

use super::visit::visit_defpackage_forms;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionMergeEdit {
    pub defpackage_path: String,
    pub defpackage_span: ByteSpan,
    pub package_name: String,
    pub merges: Vec<OptionMerge>,
    pub replacements: Vec<OptionReplacement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionMerge {
    pub head: String,
    pub key: Option<String>,
    pub kept_path: String,
    pub kept_span: ByteSpan,
    pub removed_paths: Vec<String>,
    pub removed_spans: Vec<ByteSpan>,
    pub old_atoms: Vec<String>,
    pub new_atoms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionReplacement {
    pub span: ByteSpan,
    pub replacement: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionSlot {
    pub path: String,
    pub span: ByteSpan,
    pub head_text: String,
    pub name: String,
    pub key: Option<String>,
    pub body_atoms: Vec<String>,
}

pub fn defpackage_option_merge_edits(
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
