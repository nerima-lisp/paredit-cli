use anyhow::Result;

use crate::function_parameter::domain::calls::matches_function_call_view;
use crate::function_parameter::domain::list_edit::{list_head, spans_overlap};
use paredit_core_semantics::callable_scope::{
    common_lisp_local_callable_form, is_local_callable_bound, local_callable_names,
};
use paredit_core_syntax::common_lisp::{
    CommonLispLocalCallableForm, common_lisp_symbol_reference_eq,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, Delimiter, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree,
};

use super::SelectedLocalCallableTraversal;
use super::shared::matched_setf_place_call;

#[derive(Clone, Copy)]
struct PathSuffix {
    indexes: [usize; 3],
    len: usize,
}

impl PathSuffix {
    const EMPTY: Self = Self {
        indexes: [0; 3],
        len: 0,
    };

    const fn child(index: usize) -> Self {
        Self {
            indexes: [index, 0, 0],
            len: 1,
        }
    }

    const fn descendant(indexes: [usize; 3]) -> Self {
        Self { indexes, len: 3 }
    }

    const fn setf_place_child(index: usize) -> Self {
        Self {
            indexes: [1, index, 0],
            len: 2,
        }
    }

    fn append_to(self, path: &mut Vec<usize>) {
        path.extend_from_slice(&self.indexes[..self.len]);
    }
}

#[derive(Clone, Copy)]
struct Visit<'view> {
    view: &'view ExpressionView,
    parent_path_len: usize,
    path_suffix: PathSuffix,
    scope_index: usize,
    selected_binding_visible: bool,
}

struct CallableScope {
    parent: Option<usize>,
    names: Vec<String>,
}

struct CallableScopeArena {
    scopes: Vec<CallableScope>,
}

impl CallableScopeArena {
    fn new() -> Self {
        Self {
            scopes: vec![CallableScope {
                parent: None,
                names: Vec::new(),
            }],
        }
    }

    fn extend(&mut self, parent: usize, names: Vec<String>) -> usize {
        let index = self.scopes.len();
        self.scopes.push(CallableScope {
            parent: Some(parent),
            names,
        });
        index
    }

    fn is_bound(&self, scope_index: usize, target: &str) -> bool {
        let mut current = Some(scope_index);
        while let Some(index) = current {
            let scope = &self.scopes[index];
            if is_local_callable_bound(&scope.names, target) {
                return true;
            }
            current = scope.parent;
        }
        false
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.scopes.len()
    }

    #[cfg(test)]
    fn retained_name_count(&self) -> usize {
        self.scopes.iter().map(|scope| scope.names.len()).sum()
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct TraversalStats {
    #[cfg(test)]
    visited_nodes: usize,
    #[cfg(test)]
    copied_path_indexes: usize,
    #[cfg(test)]
    materialized_paths: usize,
    #[cfg(test)]
    retained_scope_count: usize,
    #[cfg(test)]
    retained_scope_names: usize,
}

impl TraversalStats {
    const fn record_visit(&mut self, copied_path_indexes: usize) {
        #[cfg(test)]
        {
            self.visited_nodes += 1;
            self.copied_path_indexes += copied_path_indexes;
        }
        #[cfg(not(test))]
        let _ = copied_path_indexes;
    }

    const fn record_match(&mut self) {
        #[cfg(test)]
        {
            self.materialized_paths += 1;
        }
    }

    // Const-eligible only outside `cfg(test)`: the test-only body calls
    // non-const `CallableScopeArena` methods, so clippy (which lints the lib
    // target and never sees that body) suggests a `const` that fails to
    // compile as E0015 in the `lib test` target.
    // `expect` cannot be used: the lint fires for the lib target but not for
    // the `lib test` target, so an expectation would go unfulfilled there.
    #[allow(
        clippy::missing_const_for_fn,
        reason = "cfg(test) body calls non-const methods"
    )]
    fn record_scope_retention(&mut self, scopes: &CallableScopeArena) {
        #[cfg(test)]
        {
            self.retained_scope_count = scopes.len();
            self.retained_scope_names = scopes.retained_name_count();
        }
        #[cfg(not(test))]
        let _ = scopes;
    }
}

pub fn discover_local_callable_binding_call_paths(
    tree: &SyntaxTree,
    dialect: Dialect,
    definition_span: ByteSpan,
    enclosing_form_span: ByteSpan,
    function_name: &SymbolName,
    form: CommonLispLocalCallableForm,
) -> Result<Vec<Path>> {
    let mut call_paths = Vec::new();
    let context = SelectedLocalCallableTraversal {
        dialect,
        definition_span,
        enclosing_form_span,
        function_name,
        form,
    };

    for index in 0..tree.root_children().len() {
        let path = Path::root_child(index);
        let selection = tree.select_path(&path)?;
        let view = selection.view();
        collect_selected_local_callable_binding_call_paths(&view, index, &context, &mut call_paths);
    }

    call_paths.sort_by_key(|path| {
        tree.select_path(path)
            .map(|selection| selection.span().start().get())
            .unwrap_or(usize::MAX)
    });
    Ok(call_paths)
}

fn collect_selected_local_callable_binding_call_paths(
    root: &ExpressionView,
    root_index: usize,
    context: &SelectedLocalCallableTraversal<'_>,
    output: &mut Vec<Path>,
) -> TraversalStats {
    let mut stats = TraversalStats::default();
    let mut path = vec![root_index];
    let mut scopes = CallableScopeArena::new();
    let mut visits = vec![Visit {
        view: root,
        parent_path_len: path.len(),
        path_suffix: PathSuffix::EMPTY,
        scope_index: 0,
        selected_binding_visible: false,
    }];

    while let Some(visit) = visits.pop() {
        path.truncate(visit.parent_path_len);
        visit.path_suffix.append_to(&mut path);
        stats.record_visit(visit.path_suffix.len);

        if visit.view.span == context.enclosing_form_span
            && list_head(visit.view).is_some_and(|head| {
                common_lisp_local_callable_form(context.dialect, head) == Some(context.form)
            })
        {
            let local_names = local_callable_names(visit.view)
                .into_iter()
                .filter(|name| {
                    !common_lisp_symbol_reference_eq(name, context.function_name.as_str())
                })
                .collect::<Vec<_>>();
            let body_scope_index = scopes.extend(visit.scope_index, local_names);
            let binding_body_visible = matches!(context.form, CommonLispLocalCallableForm::Labels);
            let binding_scope_index = if binding_body_visible {
                body_scope_index
            } else {
                visit.scope_index
            };
            schedule_local_callable_children(
                &mut visits,
                visit.view,
                path.len(),
                binding_scope_index,
                body_scope_index,
                binding_body_visible,
                true,
            );
            continue;
        }

        if let Some(head) = list_head(visit.view) {
            if let Some(form) = common_lisp_local_callable_form(context.dialect, head) {
                let body_scope_index =
                    scopes.extend(visit.scope_index, local_callable_names(visit.view));
                let binding_scope_index = if matches!(form, CommonLispLocalCallableForm::Labels) {
                    body_scope_index
                } else {
                    visit.scope_index
                };
                schedule_local_callable_children(
                    &mut visits,
                    visit.view,
                    path.len(),
                    binding_scope_index,
                    body_scope_index,
                    visit.selected_binding_visible,
                    visit.selected_binding_visible,
                );
                continue;
            }
        }

        if visit.selected_binding_visible
            && visit.view.kind == ExpressionKind::List
            && visit.view.delimiter == Some(Delimiter::Paren)
            && !spans_overlap(context.definition_span, visit.view.span)
            && matches_function_call_view(visit.view, context.function_name)
            && !scopes.is_bound(visit.scope_index, context.function_name.as_str())
        {
            output.push(Path::from_indexes(path.clone()));
            stats.record_match();
            schedule_matched_descendants(
                &mut visits,
                visit.view,
                path.len(),
                visit.scope_index,
                visit.selected_binding_visible,
                context.function_name,
            );
            continue;
        }

        schedule_children(
            &mut visits,
            visit.view,
            path.len(),
            visit.scope_index,
            visit.selected_binding_visible,
        );
    }

    stats.record_scope_retention(&scopes);
    stats
}

fn schedule_children<'view>(
    visits: &mut Vec<Visit<'view>>,
    view: &'view ExpressionView,
    parent_path_len: usize,
    scope_index: usize,
    selected_binding_visible: bool,
) {
    for (index, child) in view.children.iter().enumerate().rev() {
        visits.push(Visit {
            view: child,
            parent_path_len,
            path_suffix: PathSuffix::child(index),
            scope_index,
            selected_binding_visible,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn schedule_local_callable_children<'view>(
    visits: &mut Vec<Visit<'view>>,
    view: &'view ExpressionView,
    parent_path_len: usize,
    binding_scope_index: usize,
    body_scope_index: usize,
    binding_selected_visible: bool,
    body_selected_visible: bool,
) {
    for (index, child) in view.children.iter().enumerate().skip(2).rev() {
        visits.push(Visit {
            view: child,
            parent_path_len,
            path_suffix: PathSuffix::child(index),
            scope_index: body_scope_index,
            selected_binding_visible: body_selected_visible,
        });
    }

    if let Some(bindings) = view.children.get(1) {
        for (binding_index, binding) in bindings.children.iter().enumerate().rev() {
            for (binding_child_index, binding_child) in
                binding.children.iter().enumerate().skip(2).rev()
            {
                visits.push(Visit {
                    view: binding_child,
                    parent_path_len,
                    path_suffix: PathSuffix::descendant([1, binding_index, binding_child_index]),
                    scope_index: binding_scope_index,
                    selected_binding_visible: binding_selected_visible,
                });
            }
        }
    }
}

fn schedule_matched_descendants<'view>(
    visits: &mut Vec<Visit<'view>>,
    view: &'view ExpressionView,
    parent_path_len: usize,
    scope_index: usize,
    selected_binding_visible: bool,
    function_name: &SymbolName,
) {
    if let Some(place) = matched_setf_place_call(view, function_name) {
        for (index, child) in place.children.iter().enumerate().skip(1).rev() {
            visits.push(Visit {
                view: child,
                parent_path_len,
                path_suffix: PathSuffix::setf_place_child(index),
                scope_index,
                selected_binding_visible,
            });
        }
        for (index, child) in view.children.iter().enumerate().rev() {
            if index != 1 {
                visits.push(Visit {
                    view: child,
                    parent_path_len,
                    path_suffix: PathSuffix::child(index),
                    scope_index,
                    selected_binding_visible,
                });
            }
        }
        return;
    }

    schedule_children(
        visits,
        view,
        parent_path_len,
        scope_index,
        selected_binding_visible,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iterative_local_traversal_handles_thirty_thousand_levels_with_one_path_copy() {
        const DEPTH: usize = 30_000;

        let input = format!(
            "(flet ((target (x) x)) {}target{})",
            "(".repeat(DEPTH),
            ")".repeat(DEPTH)
        );
        let tree = SyntaxTree::parse(&input).expect("parse deep tree");
        let enclosing_form_span = tree
            .select_path(&Path::root_child(0))
            .expect("select enclosing form")
            .span();
        let definition_span = tree
            .select_path(&Path::from_indexes(vec![0, 1, 0]))
            .expect("select binding")
            .span();
        let root_view = tree
            .select_path(&Path::root_child(0))
            .expect("select root")
            .view();
        let function_name = SymbolName::new("target").expect("valid symbol name");
        let context = SelectedLocalCallableTraversal {
            dialect: Dialect::CommonLisp,
            definition_span,
            enclosing_form_span,
            function_name: &function_name,
            form: CommonLispLocalCallableForm::Flet,
        };
        let mut output = Vec::new();

        let stats = collect_selected_local_callable_binding_call_paths(
            &root_view,
            0,
            &context,
            &mut output,
        );

        assert_eq!(output.len(), 1);
        assert_eq!(output[0].to_raw_indexes().len(), DEPTH + 1);
        assert_eq!(stats.materialized_paths, 1);
    }

    #[test]
    fn deep_local_scopes_with_many_siblings_retain_only_introduced_names() {
        const DEPTH: usize = 128;
        const SIBLINGS: usize = 512;

        let mut input = String::from("(flet ((target () nil)) ");
        for index in 0..DEPTH {
            input.push_str(&format!("(flet ((outer-{index} () nil)) "));
        }
        input.push_str("(progn");
        for index in 0..SIBLINGS {
            input.push_str(&format!(" (flet ((sibling-{index} () nil)) (target))"));
        }
        input.push(')');
        input.push_str(&")".repeat(DEPTH + 1));

        let tree = SyntaxTree::parse(&input).expect("parse scoped siblings");
        let enclosing_form_span = tree
            .select_path(&Path::root_child(0))
            .expect("select enclosing form")
            .span();
        let definition_span = tree
            .select_path(&Path::from_indexes(vec![0, 1, 0]))
            .expect("select binding")
            .span();
        let root_view = tree
            .select_path(&Path::root_child(0))
            .expect("select root")
            .view();
        let function_name = SymbolName::new("target").expect("valid symbol name");
        let context = SelectedLocalCallableTraversal {
            dialect: Dialect::CommonLisp,
            definition_span,
            enclosing_form_span,
            function_name: &function_name,
            form: CommonLispLocalCallableForm::Flet,
        };
        let mut output = Vec::new();

        let stats = collect_selected_local_callable_binding_call_paths(
            &root_view,
            0,
            &context,
            &mut output,
        );

        assert_eq!(output.len(), SIBLINGS);
        assert_eq!(stats.retained_scope_count, 2 + DEPTH + SIBLINGS);
        assert_eq!(stats.retained_scope_names, DEPTH + SIBLINGS);
    }
}
