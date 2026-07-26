use crate::domain::common_lisp::CommonLispRuntimeDependencyForm;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ExpressionView, Path};

use crate::domain::dependency_report::syntax::{dependency_designator_text, list_head};
use crate::domain::dependency_report::types::{DependencyKind, DependencyReportItem};

pub(super) fn collect_list_dependency_items(
    view: &ExpressionView,
    dialect: Dialect,
    path: &Path,
    dependencies: &mut Vec<DependencyReportItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };

    let Some(form) = dialect.common_lisp_runtime_dependency_form_for_head(head) else {
        return;
    };
    let kind = runtime_dependency_kind(form);

    push_dependency_from_child(view, path, 1, kind, Some(head.to_owned()), dependencies);
}

const fn runtime_dependency_kind(form: CommonLispRuntimeDependencyForm) -> DependencyKind {
    match form {
        CommonLispRuntimeDependencyForm::Require => DependencyKind::Require,
        CommonLispRuntimeDependencyForm::Provide => DependencyKind::Provide,
        CommonLispRuntimeDependencyForm::Load => DependencyKind::Load,
        CommonLispRuntimeDependencyForm::LoadFile => DependencyKind::LoadFile,
        CommonLispRuntimeDependencyForm::LoadLibrary => DependencyKind::LoadLibrary,
        CommonLispRuntimeDependencyForm::UsePackage => DependencyKind::UsePackage,
        CommonLispRuntimeDependencyForm::Import => DependencyKind::Import,
    }
}

fn push_dependency_from_child(
    view: &ExpressionView,
    path: &Path,
    child_index: usize,
    kind: DependencyKind,
    source: Option<String>,
    dependencies: &mut Vec<DependencyReportItem>,
) {
    let Some(child) = view.children.get(child_index) else {
        return;
    };
    let Some(target) = dependency_designator_text(child) else {
        return;
    };

    dependencies.push(DependencyReportItem::new(
        kind,
        target,
        path.child(child_index).to_string(),
        child.span,
        source,
    ));
}
