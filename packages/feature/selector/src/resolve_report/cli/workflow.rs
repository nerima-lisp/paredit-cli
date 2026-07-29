use paredit_core_cli::CommandResult;

use paredit_core_cli::shared::read_input_dialect_and_tree;
use paredit_core_syntax::selector::{SelectorError, SelectorTarget, resolve};

use crate::resolve_report::cli::args::ResolveReportArgs;
use crate::resolve_report::cli::render::print_resolve_report;
use crate::resolve_report::usecase::build_resolve_report;

pub fn resolve_report(args: ResolveReportArgs) -> CommandResult {
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
        // A gate, not a malfunction: the report above was printed, and
        // `--fail-on-empty` is the caller asking for a non-zero exit when it
        // comes back empty. `gate.rs` reserves exit 3 for exactly this, "so
        // automation can distinguish gate tripped as designed from hard
        // errors" — which is what reaching this through `bail!` prevented.
        return Err(paredit_core_cli::gate::gate_failure(format!(
            "no form matches {}",
            request.describe()
        )));
    }
    Ok(())
}
