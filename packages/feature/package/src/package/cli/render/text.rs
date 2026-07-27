use anyhow::Result;

use crate::domain::sexpr::SymbolName;

use super::super::types::{
    AddExportPlan, MergePackageOptionsPlan, PackageReportFile, RenamePackageFilePlan,
    SortPackageExportsPlan, SortPackageOptionsPlan,
};

pub(super) fn print_merge_package_options_plan(plan: &MergePackageOptionsPlan) -> Result<()> {
    let merge_count = plan.merges.len();
    let changed_merge_count = plan.merges.iter().filter(|merge| merge.changed).count();

    println!("file\t{}", safe_text!(plan.path.display()));
    println!("dialect\t{}", plan.dialect.label());
    println!("merge_count\t{merge_count}");
    println!("changed_merge_count\t{changed_merge_count}");
    println!("changed\t{}", plan.changed);
    println!("written\t{}", plan.written);
    for merge in &plan.merges {
        let key = merge.key.as_deref().unwrap_or("-");
        println!(
            "\tmerge\t{}\t{}\tkey={}\t{}..{}\tchanged={}",
            safe_text!(merge.package),
            safe_text!(merge.head),
            safe_text!(key),
            merge.kept_span.start().get(),
            merge.kept_span.end().get(),
            merge.changed
        );
        println!("\t\told\t{}", safe_text!(merge.old_atoms.join(" | ")));
        println!("\t\tnew\t{}", safe_text!(merge.new_atoms.join(" | ")));
        println!(
            "\t\tremoved\t{}",
            safe_text!(merge.removed_paths.join(" | "))
        );
    }
    println!("rewritten\t{}", safe_text!(plan.rewritten));
    Ok(())
}

pub(super) fn print_sort_package_options_plan(plan: &SortPackageOptionsPlan) -> Result<()> {
    let package_count = plan.packages.len();
    let changed_package_count = plan
        .packages
        .iter()
        .filter(|package| package.changed)
        .count();

    println!("file\t{}", safe_text!(plan.path.display()));
    println!("dialect\t{}", plan.dialect.label());
    println!("package_count\t{package_count}");
    println!("changed_package_count\t{changed_package_count}");
    println!("changed\t{}", plan.changed);
    println!("written\t{}", plan.written);
    for package in &plan.packages {
        println!(
            "\tpackage\t{}\t{}\t{}..{}\tchanged={}",
            safe_text!(package.package),
            safe_text!(package.defpackage_path),
            package.defpackage_span.start().get(),
            package.defpackage_span.end().get(),
            package.changed
        );
        println!("\t\told\t{}", safe_text!(package.old_options.join(" | ")));
        println!("\t\tnew\t{}", safe_text!(package.new_options.join(" | ")));
    }
    println!("rewritten\t{}", safe_text!(plan.rewritten));
    Ok(())
}

pub(super) fn print_sort_package_exports_plan(plan: &SortPackageExportsPlan) -> Result<()> {
    let export_count = plan.exports.len();
    let changed_export_count = plan.exports.iter().filter(|export| export.changed).count();

    println!("file\t{}", safe_text!(plan.path.display()));
    println!("dialect\t{}", plan.dialect.label());
    println!("export_count\t{export_count}");
    println!("changed_export_count\t{changed_export_count}");
    println!("changed\t{}", plan.changed);
    println!("written\t{}", plan.written);
    for export in &plan.exports {
        println!(
            "\texport\t{}\t{}\t{}..{}\tchanged={}",
            safe_text!(export.package),
            safe_text!(export.export_path),
            export.export_span.start().get(),
            export.export_span.end().get(),
            export.changed
        );
        println!("\t\told\t{}", safe_text!(export.old_symbols.join(" ")));
        println!("\t\tnew\t{}", safe_text!(export.new_symbols.join(" ")));
    }
    println!("rewritten\t{}", safe_text!(plan.rewritten));
    Ok(())
}

pub(super) fn print_package_report(reports: &[PackageReportFile]) -> Result<()> {
    let defpackage_count = reports
        .iter()
        .map(|report| report.report.defpackages.len())
        .sum::<usize>();
    let in_package_count = reports
        .iter()
        .map(|report| report.report.in_packages.len())
        .sum::<usize>();

    println!("files\t{}", reports.len());
    println!("defpackage_count\t{defpackage_count}");
    println!("in_package_count\t{in_package_count}");
    for report in reports {
        println!(
            "{}\t{}\tdefpackages={}\tin_packages={}",
            safe_text!(report.path.display()),
            report.dialect.label(),
            report.report.defpackages.len(),
            report.report.in_packages.len()
        );
        for defpackage in &report.report.defpackages {
            println!(
                "\tdefpackage\t{}\t{}..{}\t{}",
                safe_text!(defpackage.path),
                defpackage.span.start().get(),
                defpackage.span.end().get(),
                safe_text!(defpackage.name)
            );
        }
        for in_package in &report.report.in_packages {
            println!(
                "\tin-package\t{}\t{}..{}\t{}",
                safe_text!(in_package.path),
                in_package.span.start().get(),
                in_package.span.end().get(),
                safe_text!(in_package.name)
            );
        }
    }
    Ok(())
}

pub(super) fn print_rename_package_plan(
    plans: &[RenamePackageFilePlan],
    from: &SymbolName,
    to: &SymbolName,
    write: bool,
) -> Result<()> {
    let occurrence_count = plans
        .iter()
        .map(|plan| plan.occurrences.len())
        .sum::<usize>();
    let changed_count = plans.iter().filter(|plan| plan.changed).count();
    let written_count = plans.iter().filter(|plan| plan.written).count();

    println!("from\t{}", safe_text!(from));
    println!("to\t{}", safe_text!(to));
    println!("write\t{write}");
    println!("file_count\t{}", plans.len());
    println!("occurrence_count\t{occurrence_count}");
    println!("changed_count\t{changed_count}");
    println!("written_count\t{written_count}");
    for plan in plans {
        println!(
            "{}\t{}\toccurrences={}\tchanged={}\twritten={}",
            safe_text!(plan.path.display()),
            plan.dialect.label(),
            plan.occurrences.len(),
            plan.changed,
            plan.written
        );
        for occurrence in &plan.occurrences {
            println!(
                "\t{}\t{}\t{}..{}\t{}\t=>\t{}",
                occurrence.kind.label(),
                safe_text!(occurrence.path),
                occurrence.span.start().get(),
                occurrence.span.end().get(),
                safe_text!(occurrence.text),
                safe_text!(occurrence.replacement)
            );
        }
    }
    Ok(())
}

pub(super) fn print_add_export_plan(plan: &AddExportPlan) -> Result<()> {
    println!("file\t{}", safe_text!(plan.path.display()));
    println!("dialect\t{}", plan.dialect.label());
    println!("package\t{}", safe_text!(plan.package));
    println!("symbol\t{}", safe_text!(plan.symbol));
    println!("defpackage_path\t{}", safe_text!(plan.defpackage_path));
    println!(
        "defpackage_span\t{}..{}",
        plan.defpackage_span.start().get(),
        plan.defpackage_span.end().get()
    );
    if let Some(export_span) = plan.export_span {
        println!(
            "export_span\t{}..{}",
            export_span.start().get(),
            export_span.end().get()
        );
    } else {
        println!("export_span\tmissing");
    }
    println!(
        "insertion_span\t{}..{}",
        plan.insertion_span.start().get(),
        plan.insertion_span.end().get()
    );
    println!("already_exported\t{}", plan.already_exported);
    println!("changed\t{}", plan.changed);
    println!("written\t{}", plan.written);
    println!("rewritten\t{}", safe_text!(plan.rewritten));
    Ok(())
}
