use crate::domain::common_lisp::CommonLispLocalCallableForm;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ExpressionView, Path, SymbolName};

use crate::domain::rename::reader::apply_reader_prefix_context;

use super::super::RenameFunctionOccurrence;
use super::super::scope::{
    LocalCallableRenameKind, MacroletRenameScope, symbol_macrolet_shadowing_scope,
};
use super::local_callable::{LocalCallableTraversal, collect_local_callable_or_definition};
use super::reader::{collect_explicit_reader_form_renames, collect_reader_lambda_renames};
use super::state::{TraversalContext, TraversalState};

#[derive(Clone, Copy)]
pub(in crate::domain::rename::macrolet) struct TraversalPath(usize);

struct TraversalPathNode {
    parent: Option<usize>,
    index: usize,
}

pub(in crate::domain::rename::macrolet) struct TraversalPathArena {
    nodes: Vec<TraversalPathNode>,
    edge_count: usize,
    materialized_index_count: usize,
}

impl TraversalPathArena {
    pub(super) fn from_path(path: &Path) -> (Self, TraversalPath) {
        let mut arena = Self {
            nodes: vec![TraversalPathNode {
                parent: None,
                index: 0,
            }],
            edge_count: 0,
            materialized_index_count: 0,
        };
        let mut current = TraversalPath(0);
        for index in path.to_raw_indexes() {
            current = arena.child(current, index);
        }
        arena.edge_count = 0;
        (arena, current)
    }

    pub(in crate::domain::rename::macrolet) fn child(
        &mut self,
        parent: TraversalPath,
        index: usize,
    ) -> TraversalPath {
        self.nodes.push(TraversalPathNode {
            parent: Some(parent.0),
            index,
        });
        self.edge_count += 1;
        TraversalPath(self.nodes.len() - 1)
    }

    pub(super) fn descendant<const N: usize>(
        &mut self,
        mut path: TraversalPath,
        indexes: [usize; N],
    ) -> TraversalPath {
        for index in indexes {
            path = self.child(path, index);
        }
        path
    }

    pub(in crate::domain::rename::macrolet) fn materialize(&mut self, path: TraversalPath) -> Path {
        let mut indexes = Vec::new();
        let mut node_index = path.0;
        while let Some(parent) = self.nodes[node_index].parent {
            indexes.push(self.nodes[node_index].index);
            node_index = parent;
        }
        indexes.reverse();
        self.materialized_index_count += indexes.len();
        Path::from_indexes(indexes)
    }
}

#[derive(Clone, Copy)]
pub(super) struct TraversalFrame<'a> {
    pub(super) view: &'a ExpressionView,
    pub(super) path: TraversalPath,
    pub(super) state: TraversalState,
}

pub(super) enum TraversalTask<'a> {
    Visit(TraversalFrame<'a>),
    ExplicitFunctionLambdaAtom(TraversalFrame<'a>),
    ReaderQuotedLambdaAtom(TraversalFrame<'a>),
    BindingName {
        binding: &'a ExpressionView,
        binding_index: usize,
        path: TraversalPath,
        form: CommonLispLocalCallableForm,
        state: TraversalState,
    },
}

pub(in crate::domain::rename::macrolet) trait RenameTraversalMode {
    fn collect_pre_reader_renames(
        _view: &ExpressionView,
        _path: TraversalPath,
        _paths: &mut TraversalPathArena,
        _context: TraversalContext<'_>,
        _state: TraversalState,
        _renames: &mut Vec<RenameFunctionOccurrence>,
    ) -> bool {
        false
    }

    fn collect_function_reader_target_renames(
        _view: &ExpressionView,
        _path: TraversalPath,
        _paths: &mut TraversalPathArena,
        _context: TraversalContext<'_>,
        _state: TraversalState,
        _renames: &mut Vec<RenameFunctionOccurrence>,
    ) {
    }

    fn collect_list_head_renames(
        _view: &ExpressionView,
        _path: TraversalPath,
        _paths: &mut TraversalPathArena,
        _context: TraversalContext<'_>,
        _state: TraversalState,
        _renames: &mut Vec<RenameFunctionOccurrence>,
    ) {
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "binding callbacks need traversal context"
    )]
    fn collect_binding_name_renames(
        _binding: &ExpressionView,
        _binding_index: usize,
        _path: TraversalPath,
        _paths: &mut TraversalPathArena,
        _form: CommonLispLocalCallableForm,
        _context: TraversalContext<'_>,
        _state: TraversalState,
        _renames: &mut Vec<RenameFunctionOccurrence>,
    ) {
    }

    fn collect_explicit_function_lambda_atom_renames(
        _child: &ExpressionView,
        _child_path: TraversalPath,
        _paths: &mut TraversalPathArena,
        _context: TraversalContext<'_>,
        _state: TraversalState,
        _renames: &mut Vec<RenameFunctionOccurrence>,
    ) -> bool {
        false
    }

    fn collect_reader_quoted_lambda_atom_renames(
        _child: &ExpressionView,
        _child_path: TraversalPath,
        _paths: &mut TraversalPathArena,
        _context: TraversalContext<'_>,
        _state: TraversalState,
        _renames: &mut Vec<RenameFunctionOccurrence>,
    ) -> bool {
        false
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "traversal setup carries scope, quasiquote state, and accumulator"
)]
pub(in crate::domain::rename::macrolet) fn collect_renames_from_view<M: RenameTraversalMode>(
    view: &ExpressionView,
    path: Path,
    dialect: Dialect,
    from: &SymbolName,
    to: &SymbolName,
    kind: LocalCallableRenameKind,
    scope: MacroletRenameScope,
    reader_lambda_body_scope: MacroletRenameScope,
    quasiquote_depth: usize,
    renames: &mut Vec<RenameFunctionOccurrence>,
) {
    let context = TraversalContext {
        dialect,
        from,
        to,
        kind,
    };
    let state = TraversalState {
        scope,
        reader_lambda_body_scope,
        quasiquote_depth,
    };
    collect_renames_from_view_with_mode::<M>(view, path, context, state, renames);
}

#[derive(Debug, Default, PartialEq, Eq)]
struct TraversalStats {
    visited_count: usize,
    path_edge_count: usize,
    materialized_index_count: usize,
}

fn collect_renames_from_view_with_mode<M: RenameTraversalMode>(
    view: &ExpressionView,
    path: Path,
    context: TraversalContext<'_>,
    state: TraversalState,
    renames: &mut Vec<RenameFunctionOccurrence>,
) -> TraversalStats {
    let (mut paths, path) = TraversalPathArena::from_path(&path);
    let mut tasks = vec![TraversalTask::Visit(TraversalFrame { view, path, state })];
    let mut visited_count = 0;

    while let Some(task) = tasks.pop() {
        match task {
            TraversalTask::Visit(frame) => {
                visited_count += 1;
                schedule_view::<M>(frame, context, &mut paths, &mut tasks, renames);
            }
            TraversalTask::ExplicitFunctionLambdaAtom(frame) => {
                if !M::collect_explicit_function_lambda_atom_renames(
                    frame.view,
                    frame.path,
                    &mut paths,
                    context,
                    frame.state,
                    renames,
                ) {
                    tasks.push(TraversalTask::Visit(frame));
                }
            }
            TraversalTask::ReaderQuotedLambdaAtom(frame) => {
                if !M::collect_reader_quoted_lambda_atom_renames(
                    frame.view,
                    frame.path,
                    &mut paths,
                    context,
                    frame.state,
                    renames,
                ) {
                    tasks.push(TraversalTask::Visit(frame));
                }
            }
            TraversalTask::BindingName {
                binding,
                binding_index,
                path,
                form,
                state,
            } => M::collect_binding_name_renames(
                binding,
                binding_index,
                path,
                &mut paths,
                form,
                context,
                state,
                renames,
            ),
        }
    }

    TraversalStats {
        visited_count,
        path_edge_count: paths.edge_count,
        materialized_index_count: paths.materialized_index_count,
    }
}

fn schedule_view<'a, M: RenameTraversalMode>(
    frame: TraversalFrame<'a>,
    context: TraversalContext<'_>,
    paths: &mut TraversalPathArena,
    tasks: &mut Vec<TraversalTask<'a>>,
    renames: &mut Vec<RenameFunctionOccurrence>,
) {
    let scope = symbol_macrolet_shadowing_scope(frame.state.scope, frame.view, context.from);
    let frame = TraversalFrame {
        state: frame.state.with_scope(scope),
        ..frame
    };

    if M::collect_pre_reader_renames(frame.view, frame.path, paths, context, frame.state, renames) {
        return;
    }

    let Some(quasiquote_depth) =
        apply_reader_prefix_context(frame.view, frame.state.quasiquote_depth)
    else {
        return;
    };
    let frame = TraversalFrame {
        state: frame.state.with_quasiquote_depth(quasiquote_depth),
        ..frame
    };

    if collect_explicit_reader_form_renames::<M>(frame, context, paths, tasks, renames) {
        return;
    }
    if collect_reader_lambda_renames::<M>(frame, context, paths, tasks) {
        return;
    }

    if frame.state.quasiquote_depth > 0 {
        schedule_children(frame, paths, tasks);
        return;
    }

    let definition_body_range =
        match collect_local_callable_or_definition::<M>(frame, context, paths, tasks, renames) {
            LocalCallableTraversal::Handled => return,
            LocalCallableTraversal::DefinitionBody(range) => range,
        };

    for (index, child) in frame.view.children.iter().enumerate().rev() {
        if let Some(range) = definition_body_range {
            if !range.contains_child(index) {
                continue;
            }
        }
        let child_path = paths.child(frame.path, index);
        tasks.push(TraversalTask::Visit(TraversalFrame {
            view: child,
            path: child_path,
            state: frame.state.with_quasiquote_depth(0),
        }));
    }
}

pub(super) fn schedule_children<'a>(
    frame: TraversalFrame<'a>,
    paths: &mut TraversalPathArena,
    tasks: &mut Vec<TraversalTask<'a>>,
) {
    for (index, child) in frame.view.children.iter().enumerate().rev() {
        let child_path = paths.child(frame.path, index);
        tasks.push(TraversalTask::Visit(TraversalFrame {
            view: child,
            path: child_path,
            state: frame.state,
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::rename::macrolet::traversal::CallTraversal;
    use crate::domain::sexpr::SyntaxTree;

    #[test]
    fn deep_walk_uses_one_path_node_per_edge_without_materializing_paths() {
        let depth = 6_000;
        let source = format!("{}other{}", "(".repeat(depth), ")".repeat(depth));
        let tree = SyntaxTree::parse(&source).expect("deep input should parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        let from = SymbolName::new("target").expect("symbol");
        let to = SymbolName::new("renamed").expect("symbol");
        let context = TraversalContext {
            dialect: Dialect::CommonLisp,
            from: &from,
            to: &to,
            kind: LocalCallableRenameKind::Function,
        };
        let state = TraversalState {
            scope: MacroletRenameScope::default(),
            reader_lambda_body_scope: MacroletRenameScope::default(),
            quasiquote_depth: 0,
        };
        let mut renames = Vec::new();

        let stats = collect_renames_from_view_with_mode::<CallTraversal>(
            &view,
            Path::root_child(0),
            context,
            state,
            &mut renames,
        );

        assert_eq!(stats.visited_count, depth + 1);
        assert_eq!(stats.path_edge_count, depth);
        assert_eq!(stats.materialized_index_count, 0);
        assert!(renames.is_empty());
    }
}
