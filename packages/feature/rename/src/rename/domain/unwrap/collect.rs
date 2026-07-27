use anyhow::Result;

use crate::domain::callable_scope::{
    common_lisp_local_callable_form, local_callable_binding_body_scope, local_callable_body_scope,
    local_callable_names, local_callable_scope_at_path,
};
use crate::domain::common_lisp::CommonLispLocalCallableForm;
use crate::domain::definition::macro_expander_body_range;
use crate::domain::dialect::Dialect;
use crate::domain::rename::call_identity::is_local_call_bound;
use crate::domain::rename::reader::{
    apply_reader_prefix_context, executable_reader_context_at_path,
};
use crate::domain::rename::selection::list_head;
use crate::domain::sexpr::{ExpressionView, Path, SymbolName, SyntaxTree};

use super::UnwrapFunctionCallSite;
use super::call_site::{UnwrapCandidate, unwrap_call_site_from_view};
use super::choose::select_outermost_unwrap_call_sites;

pub(super) fn collect_unwrap_all_call_sites(
    tree: &SyntaxTree,
    dialect: Dialect,
    input: &str,
    function: &SymbolName,
    wrapper: &SymbolName,
) -> Result<(
    Vec<UnwrapFunctionCallSite>,
    Vec<UnwrapFunctionCallSite>,
    Vec<UnwrapFunctionCallSite>,
)> {
    let mut collection = UnwrapCollection::new(dialect, input, function, wrapper);

    for (index, _) in tree.root_children().iter().enumerate() {
        let path = Path::root_child(index);
        let view = tree.select_path(&path)?.view();
        collection.collect_from_view(&view, path, 0, false);
    }

    let (calls, skipped_nested) = select_outermost_unwrap_call_sites(collection.candidates);
    Ok((calls, collection.skipped_non_unary_wrapper, skipped_nested))
}

pub(super) fn collect_unwrap_explicit_call_sites(
    tree: &SyntaxTree,
    dialect: Dialect,
    input: &str,
    paths: &[Path],
    function: &SymbolName,
    wrapper: &SymbolName,
) -> Result<(
    Vec<UnwrapFunctionCallSite>,
    Vec<UnwrapFunctionCallSite>,
    Vec<UnwrapFunctionCallSite>,
)> {
    let mut calls = Vec::new();
    let mut skipped_non_unary_wrapper = Vec::new();

    for path in paths {
        let view = tree.select_path(path)?.view();
        if !executable_reader_context_at_path(tree, dialect, path)? {
            anyhow::bail!("call-path {path} is not in an executable reader context");
        }
        let local_callables = local_callable_scope_at_path(tree, dialect, path)?;
        if is_local_call_bound(dialect, &local_callables, function.as_str()) {
            anyhow::bail!("call-path {path} is shadowed by a local callable named {function}");
        }
        match unwrap_call_site_from_view(
            &view,
            dialect,
            input,
            || path.to_string(),
            function,
            wrapper,
        ) {
            UnwrapCandidate::Selected(site) => calls.push(site),
            UnwrapCandidate::NonUnaryWrapper(site) => skipped_non_unary_wrapper.push(site),
            UnwrapCandidate::NotMatched => {
                anyhow::bail!("call-path {path} is not a unary {wrapper} wrapper around {function}")
            }
        }
    }

    Ok((calls, skipped_non_unary_wrapper, Vec::new()))
}

struct UnwrapCollection<'a> {
    dialect: Dialect,
    input: &'a str,
    function: &'a SymbolName,
    wrapper: &'a SymbolName,
    candidates: Vec<UnwrapFunctionCallSite>,
    skipped_non_unary_wrapper: Vec<UnwrapFunctionCallSite>,
    traversal_stats: UnwrapTraversalStats,
}

#[derive(Default)]
struct UnwrapTraversalStats {
    visited: usize,
    path_edges: usize,
    materialized_paths: usize,
}

enum UnwrapTraversalTask<'view> {
    Visit {
        view: &'view ExpressionView,
        quasiquote_depth: usize,
        in_macro_expander: bool,
    },
    EnterPath(usize),
    ExitPath,
    EnterScope(Vec<String>),
    ExitScope(usize),
}

struct ScopedVisit<'view> {
    view: &'view ExpressionView,
    path: Vec<usize>,
    in_macro_expander: bool,
}

impl<'a> UnwrapCollection<'a> {
    fn new(
        dialect: Dialect,
        input: &'a str,
        function: &'a SymbolName,
        wrapper: &'a SymbolName,
    ) -> Self {
        Self {
            dialect,
            input,
            function,
            wrapper,
            candidates: Vec::new(),
            skipped_non_unary_wrapper: Vec::new(),
            traversal_stats: UnwrapTraversalStats::default(),
        }
    }

    fn collect_from_view(
        &mut self,
        view: &ExpressionView,
        path: Path,
        quasiquote_depth: usize,
        in_macro_expander: bool,
    ) {
        self.collect_from_view_with_scope(view, path, quasiquote_depth, in_macro_expander);
    }

    fn collect_from_view_with_scope(
        &mut self,
        view: &ExpressionView,
        path: Path,
        quasiquote_depth: usize,
        in_macro_expander: bool,
    ) {
        let mut path = path.to_raw_indexes();
        let mut local_callables = Vec::new();
        let mut tasks = vec![UnwrapTraversalTask::Visit {
            view,
            quasiquote_depth,
            in_macro_expander,
        }];

        while let Some(task) = tasks.pop() {
            match task {
                UnwrapTraversalTask::EnterPath(index) => {
                    path.push(index);
                    self.traversal_stats.path_edges += 1;
                }
                UnwrapTraversalTask::ExitPath => {
                    path.pop().expect("entered unwrap path has a component");
                }
                UnwrapTraversalTask::EnterScope(names) => {
                    local_callables.extend(names);
                }
                UnwrapTraversalTask::ExitScope(name_count) => {
                    local_callables.truncate(
                        local_callables
                            .len()
                            .checked_sub(name_count)
                            .expect("entered unwrap scope has all callable names"),
                    );
                }
                UnwrapTraversalTask::Visit {
                    view,
                    quasiquote_depth,
                    in_macro_expander,
                } => {
                    self.traversal_stats.visited += 1;
                    let Some(quasiquote_depth) =
                        apply_reader_prefix_context(view, quasiquote_depth)
                    else {
                        continue;
                    };
                    if quasiquote_depth > 0 && !in_macro_expander {
                        schedule_children(
                            &mut tasks,
                            view.children.iter().enumerate(),
                            quasiquote_depth,
                            in_macro_expander,
                        );
                        continue;
                    }

                    if let Some(form) = list_head(view)
                        .and_then(|head| common_lisp_local_callable_form(self.dialect, head))
                    {
                        schedule_local_callable(
                            &mut tasks,
                            view,
                            &local_callables,
                            form,
                            quasiquote_depth,
                            in_macro_expander,
                        );
                        continue;
                    }

                    if !is_local_call_bound(self.dialect, &local_callables, self.function.as_str())
                    {
                        let materialized_paths = &mut self.traversal_stats.materialized_paths;
                        match unwrap_call_site_from_view(
                            view,
                            self.dialect,
                            self.input,
                            || {
                                *materialized_paths += 1;
                                Path::from_indexes(path.clone()).to_string()
                            },
                            self.function,
                            self.wrapper,
                        ) {
                            UnwrapCandidate::Selected(site) => self.candidates.push(site),
                            UnwrapCandidate::NonUnaryWrapper(site) => {
                                self.skipped_non_unary_wrapper.push(site);
                            }
                            UnwrapCandidate::NotMatched => {}
                        }
                    }

                    let macro_expander_body = list_head(view)
                        .and_then(|head| macro_expander_body_range(self.dialect, view, head));
                    schedule_children_with_macro_expander(
                        &mut tasks,
                        view.children.iter().enumerate(),
                        quasiquote_depth,
                        in_macro_expander,
                        macro_expander_body,
                    );
                }
            }
        }
    }
}

fn schedule_visit<'view>(
    tasks: &mut Vec<UnwrapTraversalTask<'view>>,
    view: &'view ExpressionView,
    path: &[usize],
    quasiquote_depth: usize,
    in_macro_expander: bool,
) {
    tasks.extend(path.iter().map(|_| UnwrapTraversalTask::ExitPath));
    tasks.push(UnwrapTraversalTask::Visit {
        view,
        quasiquote_depth,
        in_macro_expander,
    });
    tasks.extend(
        path.iter()
            .rev()
            .copied()
            .map(UnwrapTraversalTask::EnterPath),
    );
}

fn schedule_children<'view>(
    tasks: &mut Vec<UnwrapTraversalTask<'view>>,
    children: impl DoubleEndedIterator<Item = (usize, &'view ExpressionView)>,
    quasiquote_depth: usize,
    in_macro_expander: bool,
) {
    for (index, child) in children.rev() {
        schedule_visit(tasks, child, &[index], quasiquote_depth, in_macro_expander);
    }
}

fn schedule_children_with_macro_expander<'view>(
    tasks: &mut Vec<UnwrapTraversalTask<'view>>,
    children: impl DoubleEndedIterator<Item = (usize, &'view ExpressionView)>,
    quasiquote_depth: usize,
    in_macro_expander: bool,
    macro_expander_body: Option<crate::domain::definition::DefinitionBodyRange>,
) {
    for (index, child) in children.rev() {
        schedule_visit(
            tasks,
            child,
            &[index],
            quasiquote_depth,
            in_macro_expander
                || macro_expander_body.is_some_and(|body_range| body_range.contains_child(index)),
        );
    }
}

fn schedule_local_callable<'view>(
    tasks: &mut Vec<UnwrapTraversalTask<'view>>,
    view: &'view ExpressionView,
    local_callables: &[String],
    form: CommonLispLocalCallableForm,
    quasiquote_depth: usize,
    in_macro_expander: bool,
) {
    let names = local_callable_names(view);
    let body_scope = local_callable_body_scope(local_callables, view);
    let binding_has_body_scope =
        local_callable_binding_body_scope(form, local_callables, &body_scope).len()
            > local_callables.len();
    let mut binding_visits = Vec::new();
    if let Some(bindings) = view.children.get(1) {
        for (binding_index, binding) in bindings.children.iter().enumerate() {
            for (child_index, child) in binding.children.iter().enumerate().skip(2) {
                binding_visits.push(ScopedVisit {
                    view: child,
                    path: vec![1, binding_index, child_index],
                    in_macro_expander: in_macro_expander || form.is_macro(),
                });
            }
        }
    }
    let body_visits = view
        .children
        .iter()
        .enumerate()
        .skip(2)
        .map(|(index, child)| ScopedVisit {
            view: child,
            path: vec![index],
            in_macro_expander,
        })
        .collect::<Vec<_>>();

    if binding_has_body_scope {
        binding_visits.extend(body_visits);
        schedule_scoped_visits(tasks, binding_visits, names, quasiquote_depth);
    } else {
        schedule_scoped_visits(tasks, body_visits, names, quasiquote_depth);
        schedule_scoped_visits(tasks, binding_visits, Vec::new(), quasiquote_depth);
    }
}

fn schedule_scoped_visits<'view>(
    tasks: &mut Vec<UnwrapTraversalTask<'view>>,
    visits: Vec<ScopedVisit<'view>>,
    names: Vec<String>,
    quasiquote_depth: usize,
) {
    if visits.is_empty() {
        return;
    }
    if !names.is_empty() {
        tasks.push(UnwrapTraversalTask::ExitScope(names.len()));
    }
    for visit in visits.into_iter().rev() {
        schedule_visit(
            tasks,
            visit.view,
            &visit.path,
            quasiquote_depth,
            visit.in_macro_expander,
        );
    }
    if !names.is_empty() {
        tasks.push(UnwrapTraversalTask::EnterScope(names));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_unwrap_walk_uses_one_path_operation_per_edge() {
        const DEPTH: usize = 6_000;

        let mut input = "(".repeat(DEPTH);
        input.push_str("leaf");
        input.push_str(&")".repeat(DEPTH));
        let tree = SyntaxTree::parse_with_dialect(&input, Dialect::CommonLisp)
            .expect("deep expression parses");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root expression")
            .view();
        let function = SymbolName::new("target").expect("function symbol");
        let wrapper = SymbolName::new("wrapper").expect("wrapper symbol");
        let mut collection =
            UnwrapCollection::new(Dialect::CommonLisp, &input, &function, &wrapper);

        collection.collect_from_view(&view, Path::root_child(0), 0, false);

        assert_eq!(collection.traversal_stats.visited, DEPTH + 1);
        assert_eq!(collection.traversal_stats.path_edges, DEPTH);
        assert_eq!(collection.traversal_stats.materialized_paths, 0);
        assert!(collection.candidates.is_empty());
    }
}
