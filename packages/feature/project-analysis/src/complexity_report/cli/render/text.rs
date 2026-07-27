use super::*;

pub(super) fn print_complexity_report(
    reports: &[ComplexityReportFile],
    policy: &ComplexityReportPolicy,
    ranked: &[RankedComplexityEntry<'_>],
) {
    println!("files\t{}", reports.len());
    println!("definition_count\t{}", policy.definition_count);
    println!("max_depth_overall\t{}", policy.max_depth_overall);
    if let Some(threshold) = policy.fail_on_max_depth {
        println!(
            "policy\tfail_on_max_depth={threshold}\tpassed={}",
            policy.passed
        );
    }

    println!("ranked\t{}", ranked.len());
    for entry in ranked {
        println!(
            "\t{}\t{}\t{}\t{}\tscore={}\tdepth={}\tlists={}\tatoms={}",
            safe_text!(entry.file.display()),
            entry.dialect.label(),
            entry.item.category.label(),
            safe_text!(
                entry
                    .item
                    .name
                    .as_deref()
                    .unwrap_or(entry.item.head.as_str())
            ),
            entry.item.complexity_score,
            entry.item.max_depth,
            entry.item.list_count,
            entry.item.atom_count,
        );
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
                "\t{}\t{}\t{}\t{}..{}\tscore={}\tdepth={}\tlists={}\tatoms={}",
                definition.category.label(),
                safe_text!(definition.head),
                safe_text!(definition.name.as_deref().unwrap_or("")),
                definition.span.start().get(),
                definition.span.end().get(),
                definition.complexity_score,
                definition.max_depth,
                definition.list_count,
                definition.atom_count,
            );
        }
    }
}
