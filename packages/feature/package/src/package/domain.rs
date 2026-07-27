//! Use-case helpers for Common Lisp package refactorings.

use anyhow::{Context, Result};

use crate::domain::dialect::Dialect;
use crate::domain::mutation_safety::reject_common_lisp_reader_conditionals;
use crate::domain::sexpr::SyntaxTree;

mod export;
mod merge_options;
mod rename;
mod rewrite;
mod sort_exports;
mod sort_options;
mod syntax;
mod types;
mod visit;

use export::find_defpackage_export_edit;
use merge_options::defpackage_option_merge_edits;
use rename::package_rename_occurrences;
use rewrite::{
    SpanReplacement, expand_blanked_line_span, replace_span, rewrite_package_occurrences,
    rewrite_spans,
};
use sort_exports::defpackage_export_sort_edits;
use sort_options::defpackage_option_sort_edits;

pub use sort_options::PackageOptionSortOrder;
pub use types::*;

fn ensure_common_lisp_package_refactoring(dialect: Dialect) -> Result<()> {
    anyhow::ensure!(
        dialect == Dialect::CommonLisp,
        "package refactoring currently supports only Common Lisp"
    );
    Ok(())
}

pub fn plan_add_export(request: AddExportRequest<'_>) -> Result<AddExportPlan> {
    ensure_common_lisp_package_refactoring(request.dialect)?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("failed to parse input")?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    let edit =
        find_defpackage_export_edit(&tree, request.dialect, request.package, request.symbol)?;
    let rewritten = if edit.already_exported {
        request.input.to_owned()
    } else {
        replace_span(request.input, edit.insertion_span, &edit.replacement)
    };
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect)
        .context("add-export output is not a valid S-expression document")?;

    Ok(AddExportPlan {
        package: edit.package_name,
        symbol: request.symbol.clone(),
        defpackage_path: edit.defpackage_path,
        defpackage_span: edit.defpackage_span,
        export_span: edit.export_span,
        insertion_span: edit.insertion_span,
        already_exported: edit.already_exported,
        changed: rewritten != request.input,
        rewritten,
    })
}

pub fn plan_rename_package(request: RenamePackageRequest<'_>) -> Result<RenamePackagePlan> {
    ensure_common_lisp_package_refactoring(request.dialect)?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("failed to parse input")?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    let occurrences = package_rename_occurrences(&tree, request.dialect, request.from, request.to)?;
    let rewritten = rewrite_package_occurrences(request.input, &occurrences);
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect)
        .context("rename-package output is not a valid S-expression document")?;

    Ok(RenamePackagePlan {
        changed: rewritten != request.input,
        occurrences,
        rewritten,
    })
}

pub fn plan_sort_package_exports(
    request: SortPackageExportsRequest<'_>,
) -> Result<SortPackageExportsPlan> {
    ensure_common_lisp_package_refactoring(request.dialect)?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("failed to parse input")?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    let edits =
        defpackage_export_sort_edits(request.input, &tree, request.dialect, request.package)?;
    let replacements = edits
        .iter()
        .flat_map(|edit| {
            edit.replacements.iter().map(|replacement| SpanReplacement {
                span: replacement.span,
                replacement: replacement.replacement.clone(),
            })
        })
        .collect::<Vec<_>>();
    let rewritten = rewrite_spans(request.input, &replacements);
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect)
        .context("sort-package-exports output is not a valid S-expression document")?;

    let exports = edits
        .into_iter()
        .map(|edit| PackageExportSort {
            package: edit.package_name,
            defpackage_path: edit.defpackage_path,
            defpackage_span: edit.defpackage_span,
            export_path: edit.export_path,
            export_span: edit.export_span,
            changed: edit.old_symbols != edit.new_symbols,
            old_symbols: edit.old_symbols,
            new_symbols: edit.new_symbols,
        })
        .collect();

    Ok(SortPackageExportsPlan {
        changed: rewritten != request.input,
        exports,
        rewritten,
    })
}

pub fn plan_sort_package_options(
    request: SortPackageOptionsRequest<'_>,
) -> Result<SortPackageOptionsPlan> {
    ensure_common_lisp_package_refactoring(request.dialect)?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("failed to parse input")?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    let edits = defpackage_option_sort_edits(
        request.input,
        &tree,
        request.dialect,
        request.package,
        request.order,
    )?;
    let replacements = edits
        .iter()
        .flat_map(|edit| {
            edit.replacements.iter().map(|replacement| SpanReplacement {
                span: replacement.span,
                replacement: replacement.replacement.clone(),
            })
        })
        .collect::<Vec<_>>();
    let rewritten = rewrite_spans(request.input, &replacements);
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect)
        .context("sort-package-options output is not a valid S-expression document")?;

    let packages = edits
        .into_iter()
        .map(|edit| PackageOptionSort {
            package: edit.package_name,
            defpackage_path: edit.defpackage_path,
            defpackage_span: edit.defpackage_span,
            changed: edit.old_options != edit.new_options,
            old_options: edit.old_options,
            new_options: edit.new_options,
        })
        .collect();

    Ok(SortPackageOptionsPlan {
        changed: rewritten != request.input,
        packages,
        rewritten,
    })
}

pub fn plan_merge_package_options(
    request: MergePackageOptionsRequest<'_>,
) -> Result<MergePackageOptionsPlan> {
    ensure_common_lisp_package_refactoring(request.dialect)?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("failed to parse input")?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    let edits =
        defpackage_option_merge_edits(request.input, &tree, request.dialect, request.package)?;
    let replacements = edits
        .iter()
        .flat_map(|edit| {
            edit.replacements.iter().map(|replacement| {
                let span = if replacement.replacement.is_empty() {
                    expand_blanked_line_span(request.input, replacement.span)
                } else {
                    replacement.span
                };
                SpanReplacement {
                    span,
                    replacement: replacement.replacement.clone(),
                }
            })
        })
        .collect::<Vec<_>>();
    let rewritten = rewrite_spans(request.input, &replacements);
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect)
        .context("merge-package-options output is not a valid S-expression document")?;

    let merges = edits
        .into_iter()
        .flat_map(|edit| {
            edit.merges.into_iter().map(move |merge| {
                let changed = !merge.removed_spans.is_empty() || merge.old_atoms != merge.new_atoms;
                PackageOptionMerge {
                    package: edit.package_name.clone(),
                    defpackage_path: edit.defpackage_path.clone(),
                    defpackage_span: edit.defpackage_span,
                    head: merge.head,
                    key: merge.key,
                    kept_path: merge.kept_path,
                    kept_span: merge.kept_span,
                    removed_paths: merge.removed_paths,
                    removed_spans: merge.removed_spans,
                    changed,
                    old_atoms: merge.old_atoms,
                    new_atoms: merge.new_atoms,
                }
            })
        })
        .collect::<Vec<_>>();

    Ok(MergePackageOptionsPlan {
        changed: rewritten != request.input,
        merges,
        rewritten,
    })
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod dialect_tests {
    use anyhow::Result;

    use super::*;
    use crate::domain::sexpr::SymbolName;

    const VALID_INPUT: &str =
        "(defpackage demo (:use cl) (:export z) (:export a))\n(in-package demo)\n";
    const DIALECT_MATRIX: [(Dialect, bool); 7] = [
        (Dialect::CommonLisp, true),
        (Dialect::EmacsLisp, false),
        (Dialect::Scheme, false),
        (Dialect::Clojure, false),
        (Dialect::Janet, false),
        (Dialect::Fennel, false),
        (Dialect::Unknown, false),
    ];

    #[derive(Clone, Copy, Debug)]
    enum PackageOperation {
        AddExport,
        RenamePackage,
        SortExports,
        SortOptions,
        MergeOptions,
    }

    impl PackageOperation {
        fn run(self, input: &str, dialect: Dialect) -> Result<String> {
            let package = SymbolName::new("demo").expect("valid package name");

            match self {
                Self::AddExport => {
                    let symbol = SymbolName::new("z").expect("valid export name");
                    plan_add_export(AddExportRequest {
                        input,
                        dialect,
                        package: Some(&package),
                        symbol: &symbol,
                    })
                    .map(|plan| plan.rewritten)
                }
                Self::RenamePackage => {
                    let renamed = SymbolName::new("renamed").expect("valid package name");
                    plan_rename_package(RenamePackageRequest {
                        input,
                        dialect,
                        from: &package,
                        to: &renamed,
                    })
                    .map(|plan| plan.rewritten)
                }
                Self::SortExports => plan_sort_package_exports(SortPackageExportsRequest {
                    input,
                    dialect,
                    package: Some(&package),
                })
                .map(|plan| plan.rewritten),
                Self::SortOptions => plan_sort_package_options(SortPackageOptionsRequest {
                    input,
                    dialect,
                    package: Some(&package),
                    order: PackageOptionSortOrder::Canonical,
                })
                .map(|plan| plan.rewritten),
                Self::MergeOptions => plan_merge_package_options(MergePackageOptionsRequest {
                    input,
                    dialect,
                    package: Some(&package),
                })
                .map(|plan| plan.rewritten),
            }
        }
    }

    const OPERATIONS: [PackageOperation; 5] = [
        PackageOperation::AddExport,
        PackageOperation::RenamePackage,
        PackageOperation::SortExports,
        PackageOperation::SortOptions,
        PackageOperation::MergeOptions,
    ];

    fn assert_common_lisp_support_error(
        operation: PackageOperation,
        dialect: Dialect,
        error: anyhow::Error,
    ) {
        assert!(
            error.to_string().contains("supports only Common Lisp"),
            "{operation:?} returned the wrong error for {dialect:?}: {error:#}"
        );
    }

    #[test]
    fn package_operations_follow_the_dialect_support_matrix() {
        for operation in OPERATIONS {
            for (dialect, supported) in DIALECT_MATRIX {
                let result = operation.run(VALID_INPUT, dialect);

                if supported {
                    let rewritten = result.unwrap_or_else(|error| {
                        panic!("{operation:?} should support {dialect:?}: {error:#}")
                    });
                    SyntaxTree::parse_with_dialect(&rewritten, dialect).unwrap_or_else(|error| {
                        panic!("{operation:?} output should reparse with {dialect:?}: {error:#}")
                    });
                } else {
                    assert_common_lisp_support_error(
                        operation,
                        dialect,
                        result.expect_err("unsupported dialect should fail"),
                    );
                }
            }
        }
    }

    #[test]
    fn unsupported_package_operations_reject_before_parsing() {
        for operation in OPERATIONS {
            for (dialect, supported) in DIALECT_MATRIX {
                if supported {
                    continue;
                }

                let error = operation
                    .run(")", dialect)
                    .expect_err("unsupported dialect should fail before parsing");
                assert_common_lisp_support_error(operation, dialect, error);
            }
        }
    }

    #[test]
    fn rename_package_preserves_common_lisp_closing_paren_character_literal() {
        let input = "#\\)\n(defpackage demo (:export old))\n(in-package demo)\n";
        let rewritten = PackageOperation::RenamePackage
            .run(input, Dialect::CommonLisp)
            .expect("Common Lisp character literal should parse");

        assert!(rewritten.starts_with("#\\)\n"));
        assert!(rewritten.contains("(defpackage renamed"));
        SyntaxTree::parse_with_dialect(&rewritten, Dialect::CommonLisp)
            .expect("rewritten output should reparse as Common Lisp");
    }
}
