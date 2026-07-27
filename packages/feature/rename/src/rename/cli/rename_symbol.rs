use anyhow::Result;

use super::super::read_input_dialect_and_tree;
use super::args::RenameSymbolArgs;
use super::render::symbol::print_rename_plan;
use super::shared::ensure_rename_changed;

pub(in crate::presentation::cli) fn rename_symbol(args: RenameSymbolArgs) -> Result<()> {
    let (input, dialect, tree) = read_input_dialect_and_tree(args.file, args.dialect)?;
    let rewritten = tree.rename_symbol(&args.from, &args.to);
    let changed = rewritten != input.text;
    if args.plan {
        print_rename_plan(&tree, dialect, &args.from, &args.to, args.output)?;
    } else {
        print!("{rewritten}");
    }
    ensure_rename_changed(args.fail_on_no_change, changed, "rename-symbol")
}
