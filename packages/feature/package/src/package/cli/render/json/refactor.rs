use anyhow::Result;

use crate::domain::sexpr::SymbolName;

use super::super::super::types::{
    AddExportPlan, MergePackageOptionsPlan, RenamePackageFilePlan, SortPackageExportsPlan,
    SortPackageOptionsPlan,
};
use crate::presentation::cli::package::render::json::shared::span_json;
use serde_json::json;

pub(in crate::presentation::cli::package::render) fn print_merge_package_options_plan(
    plan: &MergePackageOptionsPlan,
) -> Result<()> {
    let merge_count = plan.merges.len();
    let changed_merge_count = plan.merges.iter().filter(|merge| merge.changed).count();

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "path": plan.path.display().to_string(),
            "dialect": plan.dialect.label(),
            "merge_count": merge_count,
            "changed_merge_count": changed_merge_count,
            "changed": plan.changed,
            "written": plan.written,
            "merges": plan
                .merges
                .iter()
                .map(|merge| json!({
                    "package": merge.package.as_str(),
                    "defpackage": {
                        "path": merge.defpackage_path.as_str(),
                        "span": span_json(merge.defpackage_span),
                    },
                    "head": merge.head.as_str(),
                    "key": merge.key.as_deref(),
                    "kept": {
                        "path": merge.kept_path.as_str(),
                        "span": span_json(merge.kept_span),
                    },
                    "removed": merge
                        .removed_paths
                        .iter()
                        .zip(merge.removed_spans.iter())
                        .map(|(path, span)| json!({
                            "path": path.as_str(),
                            "span": span_json(*span),
                        }))
                        .collect::<Vec<_>>(),
                    "old_atoms": merge.old_atoms.as_slice(),
                    "new_atoms": merge.new_atoms.as_slice(),
                    "changed": merge.changed,
                }))
                .collect::<Vec<_>>(),
            "rewritten": plan.rewritten.as_str(),
        }))?
    );
    Ok(())
}

pub(in crate::presentation::cli::package::render) fn print_sort_package_options_plan(
    plan: &SortPackageOptionsPlan,
) -> Result<()> {
    let package_count = plan.packages.len();
    let changed_package_count = plan
        .packages
        .iter()
        .filter(|package| package.changed)
        .count();

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "path": plan.path.display().to_string(),
            "dialect": plan.dialect.label(),
            "package_count": package_count,
            "changed_package_count": changed_package_count,
            "changed": plan.changed,
            "written": plan.written,
            "packages": plan
                .packages
                .iter()
                .map(|package| json!({
                    "package": package.package.as_str(),
                    "defpackage": {
                        "path": package.defpackage_path.as_str(),
                        "span": span_json(package.defpackage_span),
                    },
                    "old_options": package.old_options.as_slice(),
                    "new_options": package.new_options.as_slice(),
                    "changed": package.changed,
                }))
                .collect::<Vec<_>>(),
            "rewritten": plan.rewritten.as_str(),
        }))?
    );
    Ok(())
}

pub(in crate::presentation::cli::package::render) fn print_sort_package_exports_plan(
    plan: &SortPackageExportsPlan,
) -> Result<()> {
    let export_count = plan.exports.len();
    let changed_export_count = plan.exports.iter().filter(|export| export.changed).count();

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "path": plan.path.display().to_string(),
            "dialect": plan.dialect.label(),
            "export_count": export_count,
            "changed_export_count": changed_export_count,
            "changed": plan.changed,
            "written": plan.written,
            "exports": plan
                .exports
                .iter()
                .map(|export| json!({
                    "package": export.package.as_str(),
                    "defpackage": {
                        "path": export.defpackage_path.as_str(),
                        "span": span_json(export.defpackage_span),
                    },
                    "export": {
                        "path": export.export_path.as_str(),
                        "span": span_json(export.export_span),
                    },
                    "old_symbols": export.old_symbols.as_slice(),
                    "new_symbols": export.new_symbols.as_slice(),
                    "changed": export.changed,
                }))
                .collect::<Vec<_>>(),
            "rewritten": plan.rewritten.as_str(),
        }))?
    );
    Ok(())
}

pub(in crate::presentation::cli::package::render) fn print_rename_package_plan(
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

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "from": from.as_str(),
            "to": to.as_str(),
            "write": write,
            "file_count": plans.len(),
            "occurrence_count": occurrence_count,
            "changed_count": changed_count,
            "written_count": written_count,
            "files": plans
                .iter()
                .map(|plan| json!({
                    "path": plan.path.display().to_string(),
                    "dialect": plan.dialect.label(),
                    "count": plan.occurrences.len(),
                    "changed": plan.changed,
                    "written": plan.written,
                    "occurrences": plan
                        .occurrences
                        .iter()
                        .map(|occurrence| json!({
                            "kind": occurrence.kind.label(),
                            "path": occurrence.path,
                            "span": span_json(occurrence.span),
                            "text": occurrence.text,
                            "replacement": occurrence.replacement,
                        }))
                        .collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>(),
        }))?
    );
    Ok(())
}

pub(in crate::presentation::cli::package::render) fn print_add_export_plan(
    plan: &AddExportPlan,
) -> Result<()> {
    let export_span = plan.export_span.map(span_json);

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "path": plan.path.display().to_string(),
            "dialect": plan.dialect.label(),
            "package": plan.package.as_str(),
            "symbol": plan.symbol.as_str(),
            "defpackage": {
                "path": plan.defpackage_path.as_str(),
                "span": span_json(plan.defpackage_span),
            },
            "export_span": export_span,
            "insertion_span": span_json(plan.insertion_span),
            "already_exported": plan.already_exported,
            "changed": plan.changed,
            "written": plan.written,
            "rewritten": plan.rewritten.as_str(),
        }))?
    );
    Ok(())
}
