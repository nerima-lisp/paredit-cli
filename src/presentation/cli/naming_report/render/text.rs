use super::*;

pub(super) fn print_naming_report(reports: &[NamingReportFile], policy: &NamingReportPolicy) {
    println!("files\t{}", reports.len());
    println!("named_definition_count\t{}", policy.named_definition_count);
    println!("non_idiomatic_count\t{}", policy.non_idiomatic_count);
    if policy.fail_on_non_idiomatic {
        println!(
            "policy\tfail_on_non_idiomatic=true\tpassed={}",
            policy.passed
        );
    }

    for report in reports {
        for item in report.non_idiomatic() {
            println!(
                "non-idiomatic\t{}\t{}\t{}\t{}\tstyle={}",
                safe_text!(report.path.display()),
                report.dialect.label(),
                item.category.label(),
                safe_text!(item.name),
                item.style.label(),
            );
        }
    }

    for report in reports {
        println!(
            "{}\t{}\tdefinitions={}",
            safe_text!(report.path.display()),
            report.dialect.label(),
            report.definitions.len(),
        );
        for definition in &report.definitions {
            println!(
                "\t{}\t{}\t{}..{}\tstyle={}\tidiomatic={}",
                definition.category.label(),
                safe_text!(definition.name.as_str()),
                definition.span.start().get(),
                definition.span.end().get(),
                definition.style.label(),
                definition.style.is_idiomatic(),
            );
        }
    }
}
