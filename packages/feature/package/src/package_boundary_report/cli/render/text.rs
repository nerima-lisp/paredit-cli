use super::*;
use paredit_core_cli::safe_text;

pub fn print_package_boundary_report(
    reports: &[PackageBoundaryReportFile],
    policy: &PackageBoundaryPolicy,
) {
    println!("files\t{}", reports.len());
    println!("qualified_symbol_count\t{}", policy.qualified_symbol_count);
    println!("violation_count\t{}", policy.violation_count);
    if policy.fail_on_violation {
        println!("policy\tfail_on_violation=true\tpassed={}", policy.passed);
    }

    for report in reports {
        for item in &report.violations {
            println!(
                "violation\t{}\t{}\treference={}\ttarget={}\tcurrent={}",
                safe_text!(report.path.display()),
                report.dialect.label(),
                safe_text!(item.reference),
                safe_text!(item.target_package),
                safe_text!(item.current_package.as_deref().unwrap_or("<none>")),
            );
        }
    }
}
