use anyhow::Result;

use paredit_core_cli::shared::read_input_dialect_and_tree;
use paredit_core_syntax::selector::{SelectorError, SelectorTarget, resolve};

use crate::resolve_report::cli::args::ResolveReportArgs;
use crate::resolve_report::cli::render::print_resolve_report;
use crate::resolve_report::usecase::build_resolve_report;

pub fn resolve_report(args: ResolveReportArgs) -> Result<()> {
    let (_, dialect, tree) = read_input_dialect_and_tree(args.file, args.dialect)?;
    // Resolution is always fan-out here. Reporting is exactly the operation
    // that should show every match rather than refuse an ambiguous selector:
    // seeing all twelve is how a caller decides whether to narrow.
    let mut request = args.selector.to_request(dialect)?;
    request.all = true;

    let targets: Vec<SelectorTarget> = match resolve(&tree, dialect, &request) {
        Ok(targets) => targets,
        // "Nothing matched" is a result, not a failure, for a command whose
        // whole job is to say what a selector names.
        Err(SelectorError::NoMatch { .. }) => Vec::new(),
        Err(error) => return Err(error.into()),
    };

    let report = build_resolve_report(
        &tree,
        dialect,
        request.describe(),
        &targets,
        args.preview_bytes,
    );
    let empty = report.is_empty();
    print_resolve_report(&report, args.output)?;

    if empty && args.fail_on_empty {
        anyhow::bail!("no form matches {}", request.describe());
    }
    Ok(())
}
