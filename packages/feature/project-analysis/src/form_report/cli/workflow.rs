use paredit_core_cli::CliResult;

use crate::form_report::cli::{args::FormReportArgs, render::print_form_report};
use crate::form_report::usecase::types::FormReportRequest;
use crate::form_report::usecase::workflow::build_form_report;
use paredit_core_cli::shared::{read_input_dialect_and_tree, resolve_one_target};

pub fn form_report(args: FormReportArgs) -> CliResult<()> {
    let (input, dialect, tree) = read_input_dialect_and_tree(args.file, args.dialect)?;
    let target = resolve_one_target(&tree, dialect, &args.selector, "inspect form")?;
    let selection = tree.select_path(&target.path)?;
    let report = build_form_report(FormReportRequest {
        input: &input.text,
        dialect,
        // The *resolved* path, not the one the caller typed: `--name foo`
        // and `--line-column 12:5` now report a path too, which is what makes
        // this command a way of turning a coordinate into one.
        path: Some(target.path),
        target: selection.view(),
        include_source: args.include_source,
    })?;

    print_form_report(&report, args.output)
}
