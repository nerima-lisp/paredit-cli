use paredit_core_cli::CommandResult;

use crate::dotfiles_report::cli::args::DotfilesReportArgs;
use crate::dotfiles_report::cli::render::print_report;
use crate::dotfiles_report::usecase::build;

pub fn dotfiles_report(args: DotfilesReportArgs) -> CommandResult {
    let report = build(&args.root)?;
    print_report(&report, args.output)?;
    if !report.complete {
        return Err(paredit_core_cli::gate::gate_failure(
            "inspect dotfiles inventory is incomplete; see diagnostics",
        ));
    }
    Ok(())
}
