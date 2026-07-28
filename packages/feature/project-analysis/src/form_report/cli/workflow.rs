use anyhow::Result;

use crate::form_report::cli::{args::FormReportArgs, render::print_form_report};
use crate::form_report::usecase::types::FormReportRequest;
use crate::form_report::usecase::workflow::build_form_report;
use paredit_core_cli::shared::{read_input_dialect_and_tree, resolve_target};

pub fn form_report(args: FormReportArgs) -> Result<()> {
    let (input, dialect, tree) = read_input_dialect_and_tree(args.file, args.dialect)?;
    let selection = resolve_target(&tree, args.path.as_ref(), args.at)?;
    let report = build_form_report(FormReportRequest {
        input: &input.text,
        dialect,
        path: args.path,
        target: selection.view(),
        include_source: args.include_source,
    })?;

    print_form_report(&report, args.output)
}
