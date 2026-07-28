use crate::error::PackageRefactorResult;

use paredit_core_syntax::common_lisp::{
    CommonLispOperator, common_lisp_local_callable_form, local_callable_binding_body_scope,
    local_callable_body_scope,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::apply_reader_prefix_context;
use paredit_core_syntax::sexpr::{Delimiter, ExpressionKind, ExpressionView, Path, SyntaxTree};

use super::types::DependencyReportItem;

mod asdf;
mod forms;
mod qualified;

pub fn collect_dependency_items(
    tree: &SyntaxTree,
    dialect: Dialect,
) -> PackageRefactorResult<Vec<DependencyReportItem>> {
    let mut dependencies = Vec::new();

    for index in 0..tree.root_children().len() {
        let path = Path::root_child(index);
        let view = tree.select_path(&path)?.view();
        collect_dependency_items_from_view(&view, dialect, path, &[], 0, &mut dependencies);
    }

    Ok(dependencies)
}

/// Collects `(declaring_system, depended_on_system)` edges from every
/// top-level `defsystem` form. Unlike [`collect_dependency_items`], this
/// only looks at top-level forms — real `.asd` files always declare
/// `defsystem` there — so it does not need the full quasiquote/local-binding
/// aware recursive walk that general dependency collection requires.
pub fn collect_system_dependency_edges(
    tree: &SyntaxTree,
    dialect: Dialect,
) -> PackageRefactorResult<Vec<(String, String)>> {
    let mut edges = Vec::new();

    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&Path::root_child(index))?.view();
        asdf::collect_system_definition_edges(&view, dialect, &mut edges);
    }

    Ok(edges)
}

fn collect_dependency_items_from_view(
    view: &ExpressionView,
    dialect: Dialect,
    path: Path,
    local_bindings: &[String],
    quasiquote_depth: usize,
    dependencies: &mut Vec<DependencyReportItem>,
) {
    let Some(quasiquote_depth) = apply_reader_prefix_context(view, quasiquote_depth) else {
        return;
    };

    if quasiquote_depth > 0 {
        for (index, child) in view.children.iter().enumerate() {
            collect_dependency_items_from_view(
                child,
                dialect,
                path.child(index),
                local_bindings,
                quasiquote_depth,
                dependencies,
            );
        }
        return;
    }

    if view.kind == ExpressionKind::List && view.delimiter == Some(Delimiter::Paren) {
        forms::collect_list_dependency_items(view, dialect, &path, dependencies);
        asdf::collect_system_dependency_items(view, dialect, &path, dependencies);
    }

    qualified::collect_qualified_symbol_dependency(view, &path, local_bindings, dependencies);

    if collect_local_callable_dependency_items(
        view,
        dialect,
        &path,
        local_bindings,
        quasiquote_depth,
        dependencies,
    ) {
        return;
    }

    if collect_symbol_macrolet_dependency_items(
        view,
        dialect,
        &path,
        local_bindings,
        quasiquote_depth,
        dependencies,
    ) {
        return;
    }

    for (index, child) in view.children.iter().enumerate() {
        let child_path = path.child(index);
        collect_dependency_items_from_view(
            child,
            dialect,
            child_path,
            local_bindings,
            quasiquote_depth,
            dependencies,
        );
    }
}

fn collect_local_callable_dependency_items(
    view: &ExpressionView,
    dialect: Dialect,
    path: &Path,
    local_bindings: &[String],
    quasiquote_depth: usize,
    dependencies: &mut Vec<DependencyReportItem>,
) -> bool {
    if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
        return false;
    }

    let Some(head) = view
        .children
        .first()
        .and_then(|child| child.text.as_deref())
    else {
        return false;
    };
    let Some(form) = common_lisp_local_callable_form(dialect, head) else {
        return false;
    };

    if let Some(head_view) = view.children.first() {
        collect_dependency_items_from_view(
            head_view,
            dialect,
            path.child(0),
            local_bindings,
            quasiquote_depth,
            dependencies,
        );
    }

    let body_scope = local_callable_body_scope(local_bindings, view);

    if let Some(bindings) = view.children.get(1) {
        let binding_body_scope =
            local_callable_binding_body_scope(form, local_bindings, &body_scope);
        for (binding_index, binding) in bindings.children.iter().enumerate() {
            if binding.kind != ExpressionKind::List || binding.delimiter != Some(Delimiter::Paren) {
                continue;
            }

            for (child_index, child) in binding.children.iter().enumerate().skip(2) {
                collect_dependency_items_from_view(
                    child,
                    dialect,
                    path.child(1).child(binding_index).child(child_index),
                    binding_body_scope,
                    quasiquote_depth,
                    dependencies,
                );
            }
        }
    }

    for (index, child) in view.children.iter().enumerate().skip(2) {
        collect_dependency_items_from_view(
            child,
            dialect,
            path.child(index),
            &body_scope,
            quasiquote_depth,
            dependencies,
        );
    }

    true
}

fn collect_symbol_macrolet_dependency_items(
    view: &ExpressionView,
    dialect: Dialect,
    path: &Path,
    local_bindings: &[String],
    quasiquote_depth: usize,
    dependencies: &mut Vec<DependencyReportItem>,
) -> bool {
    if dialect != Dialect::CommonLisp
        || view.kind != ExpressionKind::List
        || view.delimiter != Some(Delimiter::Paren)
    {
        return false;
    }

    let Some(head) = view
        .children
        .first()
        .and_then(|child| child.text.as_deref())
    else {
        return false;
    };
    if CommonLispOperator::from_head(head) != Some(CommonLispOperator::SymbolMacrolet) {
        return false;
    }

    if let Some(head_view) = view.children.first() {
        collect_dependency_items_from_view(
            head_view,
            dialect,
            path.child(0),
            local_bindings,
            quasiquote_depth,
            dependencies,
        );
    }

    let Some(bindings) = view.children.get(1) else {
        return true;
    };

    let binding_names = bindings
        .children
        .iter()
        .filter_map(symbol_macrolet_binding_name)
        .collect::<Vec<_>>();

    for (binding_index, binding) in bindings.children.iter().enumerate() {
        if binding.kind != ExpressionKind::List {
            continue;
        }

        for (child_index, child) in binding.children.iter().enumerate().skip(1) {
            collect_dependency_items_from_view(
                child,
                dialect,
                path.child(1).child(binding_index).child(child_index),
                local_bindings,
                quasiquote_depth,
                dependencies,
            );
        }
    }

    let mut body_scope = local_bindings.to_vec();
    body_scope.extend(binding_names);
    for (index, child) in view.children.iter().enumerate().skip(2) {
        collect_dependency_items_from_view(
            child,
            dialect,
            path.child(index),
            &body_scope,
            quasiquote_depth,
            dependencies,
        );
    }

    true
}

fn symbol_macrolet_binding_name(binding: &ExpressionView) -> Option<String> {
    binding
        .children
        .first()
        .and_then(|child| child.text.as_ref())
        .cloned()
}
