use crate::domain::{
    common_lisp::CommonLispPackageDeclarationForm,
    dialect::Dialect,
    sexpr::{Delimiter, ExpressionKind, ExpressionView, Path, SymbolName},
};

use super::super::syntax::{atom_text, is_package_head, package_atoms_match, package_option_name};
use super::super::{PackageRenameKind, PackageRenameOccurrence};
use super::paths::{child_path, local_nickname_package_path, option_child_path};
use super::replacement::{package_designator_replacement, package_qualified_replacement};

pub(super) fn collect_package_rename_occurrences(
    view: &ExpressionView,
    path: Path,
    dialect: Dialect,
    from: &SymbolName,
    to: &SymbolName,
    occurrences: &mut Vec<PackageRenameOccurrence>,
) {
    if let Some(text) = atom_text(view) {
        if let Some(replacement) = package_qualified_replacement(text, from, to) {
            occurrences.push(PackageRenameOccurrence {
                kind: PackageRenameKind::QualifiedPrefix,
                path: path.to_string(),
                span: view.span,
                text: text.to_owned(),
                replacement,
            });
        }
        return;
    }

    collect_package_form_designators(view, &path, dialect, from, to, occurrences);

    for (index, child) in view.children.iter().enumerate() {
        collect_package_rename_occurrences(
            child,
            path.child(index),
            dialect,
            from,
            to,
            occurrences,
        );
    }
}

fn collect_package_form_designators(
    view: &ExpressionView,
    path: &Path,
    dialect: Dialect,
    from: &SymbolName,
    to: &SymbolName,
    occurrences: &mut Vec<PackageRenameOccurrence>,
) {
    if view.kind != ExpressionKind::List
        || view.delimiter != Some(Delimiter::Paren)
        || view.children.len() < 2
    {
        return;
    }

    let Some(head) = atom_text(&view.children[0]) else {
        return;
    };

    if is_package_head(dialect, head, CommonLispPackageDeclarationForm::Defpackage) {
        push_package_designator_occurrence(
            &view.children[1],
            child_path(path, 1),
            PackageRenameKind::DefpackageName,
            from,
            to,
            occurrences,
        );
        collect_defpackage_option_designators(view, path, from, to, occurrences);
    } else if is_package_head(dialect, head, CommonLispPackageDeclarationForm::InPackage) {
        push_package_designator_occurrence(
            &view.children[1],
            child_path(path, 1),
            PackageRenameKind::InPackageName,
            from,
            to,
            occurrences,
        );
    }
}

fn collect_defpackage_option_designators(
    view: &ExpressionView,
    path: &Path,
    from: &SymbolName,
    to: &SymbolName,
    occurrences: &mut Vec<PackageRenameOccurrence>,
) {
    for (option_index, option) in view.children.iter().enumerate().skip(2) {
        if option.kind != ExpressionKind::List || option.children.is_empty() {
            continue;
        }
        let Some(option_head) = atom_text(&option.children[0]) else {
            continue;
        };
        let option_name = package_option_name(option_head);
        match option_name.as_str() {
            "nicknames" | "use" => {
                for child_index in 1..option.children.len() {
                    push_package_designator_occurrence(
                        &option.children[child_index],
                        option_child_path(path, option_index, child_index),
                        PackageRenameKind::PackageOption,
                        from,
                        to,
                        occurrences,
                    );
                }
            }
            "import-from" | "shadowing-import-from" => {
                if let Some(package) = option.children.get(1) {
                    push_package_designator_occurrence(
                        package,
                        option_child_path(path, option_index, 1),
                        PackageRenameKind::PackageOption,
                        from,
                        to,
                        occurrences,
                    );
                }
            }
            "local-nicknames" => {
                for (pair_index, pair) in option.children.iter().enumerate().skip(1) {
                    if let Some(package) = pair.children.get(1) {
                        push_package_designator_occurrence(
                            package,
                            local_nickname_package_path(path, option_index, pair_index, 1),
                            PackageRenameKind::PackageOption,
                            from,
                            to,
                            occurrences,
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

fn push_package_designator_occurrence(
    view: &ExpressionView,
    path: Path,
    kind: PackageRenameKind,
    from: &SymbolName,
    to: &SymbolName,
    occurrences: &mut Vec<PackageRenameOccurrence>,
) {
    let Some(text) = atom_text(view) else {
        return;
    };
    if !package_atoms_match(text, from.as_str()) {
        return;
    }

    occurrences.push(PackageRenameOccurrence {
        kind,
        path: path.to_string(),
        span: view.span,
        text: text.to_owned(),
        replacement: package_designator_replacement(text, to),
    });
}
