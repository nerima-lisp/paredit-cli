use super::*;

pub(super) fn print_unused_parameter_report(
    reports: &[UnusedParameterReportFile],
    policy: &UnusedParameterReportPolicy,
) {
    println!("files\t{}", reports.len());
    println!(
        "checked_definition_count\t{}",
        policy.checked_definition_count
    );
    println!("unused_parameter_count\t{}", policy.unused_parameter_count);
    if policy.fail_on_unused {
        println!("policy\tfail_on_unused=true\tpassed={}", policy.passed);
    }

    for report in reports {
        for item in &report.unused_parameters {
            println!(
                "unused-parameter\t{}\t{}\t{}\t{}\tparameter={}",
                safe_text!(report.path.display()),
                report.dialect.label(),
                item.category.label(),
                safe_text!(
                    item.definition_name
                        .as_deref()
                        .unwrap_or(item.head.as_str())
                ),
                safe_text!(item.parameter_name),
            );
        }
    }
}
