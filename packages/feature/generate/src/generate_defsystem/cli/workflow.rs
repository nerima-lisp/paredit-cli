use anyhow::{Context, Result};

use paredit_core_cli::shared::{
    expand_input_files, read_input_dialect_and_tree, write_file_with_rollback,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

use crate::generate_defsystem::cli::args::GenerateDefsystemArgs;
use crate::generate_defsystem::cli::render::print_defsystem_plan;
use crate::generate_defsystem::usecase::plan_defsystem;

pub fn generate_defsystem(args: GenerateDefsystemArgs) -> Result<()> {
    let system_name = args
        .name
        .clone()
        .unwrap_or_else(|| default_system_name(&args.directory));

    let files = expand_input_files(std::slice::from_ref(&args.directory), args.dialect)?;
    let mut parsed = Vec::new();
    let mut skipped_dialect = Vec::new();
    for file in &files {
        let (_, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        if dialect != Dialect::CommonLisp {
            skipped_dialect.push(file.clone());
            continue;
        }
        parsed.push((file.clone(), tree));
    }

    let plan = plan_defsystem(&system_name, &parsed);
    SyntaxTree::parse(&plan.generated).context("the generated defsystem would not be parseable")?;

    let mut written = false;
    if args.write {
        let target = args.directory.join(format!("{system_name}.asd"));
        if target.exists() && !args.force {
            anyhow::bail!("refusing to overwrite {} without --force", target.display());
        }
        write_file_with_rollback(target, plan.generated.clone())?;
        written = true;
    }

    print_defsystem_plan(&plan, &skipped_dialect, written, args.output)
}

fn default_system_name(directory: &std::path::Path) -> String {
    directory
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.replace('_', "-").to_ascii_lowercase())
        .unwrap_or_else(|| "app".to_owned())
}
