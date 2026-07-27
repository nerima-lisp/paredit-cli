use anyhow::Result;

use crate::domain::common_lisp::{
    common_lisp_symbol_reference_eq, has_common_lisp_package_qualifier,
};
use crate::domain::definition::{definition_shape, is_macro_expander_definition};
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{
    Delimiter, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree,
};

use crate::domain::rename::reader::{
    apply_reader_prefix_context, atom_symbol_span, atom_symbol_text,
};

use super::super::super::selection::list_head;
use super::super::RenameFunctionOccurrence;
use super::modes::{
    collect_local_callable_function_call_renames, collect_symbol_macrolet_function_call_renames,
};
use super::reader::{
    collect_bare_lambda_call_renames, collect_explicit_reader_form_call_renames,
    collect_function_designator_renames,
};

pub(in crate::domain::rename::function) struct TraversalContext<'a> {
    pub(super) dialect: Dialect,
    pub(super) from: &'a SymbolName,
    pub(super) to: &'a SymbolName,
}

#[derive(Clone, Copy)]
pub(in crate::domain::rename::function) struct TraversalState {
    pub(super) path: TraversalPath,
    pub(super) local_callable_shadowed: bool,
    pub(super) local_function_shadowed: bool,
    pub(super) quasiquote_depth: usize,
    pub(super) in_macro_expander: bool,
    pub(super) shadowed_depth: usize,
}

#[derive(Clone, Copy)]
pub(super) struct TraversalPath(usize);

struct TraversalPathNode {
    parent: Option<usize>,
    index: usize,
}

pub(super) struct TraversalPathArena {
    nodes: Vec<TraversalPathNode>,
}

impl TraversalPathArena {
    fn root_child(index: usize) -> (Self, TraversalPath) {
        (
            Self {
                nodes: vec![TraversalPathNode {
                    parent: None,
                    index,
                }],
            },
            TraversalPath(0),
        )
    }

    pub(super) fn child(&mut self, path: TraversalPath, index: usize) -> TraversalPath {
        let next = self.nodes.len();
        self.nodes.push(TraversalPathNode {
            parent: Some(path.0),
            index,
        });
        TraversalPath(next)
    }

    pub(super) fn descendant(
        &mut self,
        mut path: TraversalPath,
        indexes: impl IntoIterator<Item = usize>,
    ) -> TraversalPath {
        for index in indexes {
            path = self.child(path, index);
        }
        path
    }

    pub(super) fn materialize(&self, path: TraversalPath) -> Path {
        let mut indexes = Vec::new();
        let mut cursor = Some(path.0);
        while let Some(node_index) = cursor {
            let node = &self.nodes[node_index];
            indexes.push(node.index);
            cursor = node.parent;
        }
        indexes.reverse();
        Path::from_indexes(indexes)
    }
}

impl TraversalState {
    pub(super) const fn with_path(&self, path: TraversalPath) -> Self {
        Self { path, ..*self }
    }

    pub(super) const fn with_quasiquote_depth(&self, quasiquote_depth: usize) -> Self {
        Self {
            quasiquote_depth,
            ..*self
        }
    }

    pub(super) const fn with_local_shadowing(
        &self,
        local_callable_shadowed: bool,
        local_function_shadowed: bool,
    ) -> Self {
        Self {
            local_callable_shadowed,
            local_function_shadowed,
            ..*self
        }
    }

    pub(super) const fn in_macro_expander(&self) -> Self {
        Self {
            in_macro_expander: true,
            ..*self
        }
    }
}

pub(super) struct TraversalFrame<'a> {
    pub(super) view: &'a ExpressionView,
    pub(super) state: TraversalState,
}

pub(in crate::domain::rename::function) fn allows_function_reference_rename(
    state: &TraversalState,
    target_text: &str,
) -> bool {
    if state.local_function_shadowed {
        return false;
    }

    (!state.local_callable_shadowed && state.shadowed_depth == 0)
        || has_common_lisp_package_qualifier(target_text)
}

pub fn collect_function_call_head_renames(
    tree: &SyntaxTree,
    dialect: Dialect,
    from: &SymbolName,
    to: &SymbolName,
) -> Result<Vec<RenameFunctionOccurrence>> {
    let context = TraversalContext { dialect, from, to };
    let mut renames = Vec::new();

    for (index, _) in tree.root_children().iter().enumerate() {
        let path = Path::root_child(index);
        let view = tree.select_path(&path)?.view();
        let (mut paths, traversal_path) = TraversalPathArena::root_child(index);
        collect_function_call_head_renames_from_view(
            &view,
            &context,
            TraversalState {
                path: traversal_path,
                local_callable_shadowed: false,
                local_function_shadowed: false,
                quasiquote_depth: 0,
                in_macro_expander: false,
                shadowed_depth: 0,
            },
            &mut paths,
            &mut renames,
        );
    }

    Ok(renames)
}

pub(in crate::domain::rename::function) fn collect_function_call_head_renames_from_view(
    view: &ExpressionView,
    context: &TraversalContext<'_>,
    state: TraversalState,
    paths: &mut TraversalPathArena,
    renames: &mut Vec<RenameFunctionOccurrence>,
) {
    let mut stack = vec![TraversalFrame { view, state }];
    while let Some(frame) = stack.pop() {
        collect_function_call_head_renames_from_frame(frame, context, paths, renames, &mut stack);
    }
}

fn collect_function_call_head_renames_from_frame<'a>(
    frame: TraversalFrame<'a>,
    context: &TraversalContext<'_>,
    paths: &mut TraversalPathArena,
    renames: &mut Vec<RenameFunctionOccurrence>,
    stack: &mut Vec<TraversalFrame<'a>>,
) {
    let TraversalFrame { view, state } = frame;
    let Some(quasiquote_depth) = apply_reader_prefix_context(view, state.quasiquote_depth) else {
        return;
    };

    let reader_state = state.with_quasiquote_depth(quasiquote_depth);
    if collect_function_designator_renames(view, &reader_state, paths, context, renames) {
        return;
    }

    if collect_bare_lambda_call_renames(view, &reader_state, paths, stack) {
        return;
    }

    if collect_explicit_reader_form_call_renames(view, context, reader_state, paths, renames, stack)
    {
        return;
    }

    if quasiquote_depth > 0 && !state.in_macro_expander {
        push_children(
            view,
            state.with_quasiquote_depth(quasiquote_depth),
            paths,
            stack,
        );
        return;
    }

    let mut definition_body_range = None;
    let mut macro_expander_body_range = None;

    if view.kind == ExpressionKind::List && view.delimiter == Some(Delimiter::Paren) {
        if let Some(head) = list_head(view) {
            if collect_local_callable_function_call_renames(
                view,
                head,
                context,
                state.with_quasiquote_depth(0),
                paths,
                renames,
                stack,
            ) {
                return;
            }

            if collect_symbol_macrolet_function_call_renames(
                view,
                head,
                context,
                state.with_quasiquote_depth(0),
                paths,
                renames,
                stack,
            ) {
                return;
            }

            let shape = definition_shape(context.dialect, view, head);
            if common_lisp_symbol_reference_eq(head, context.from.as_str())
                && shape.is_none()
                && allows_function_reference_rename(&state, head)
            {
                if let Some(head_view) = view.children.first() {
                    renames.push(RenameFunctionOccurrence {
                        path: paths.child(state.path, 0).to_string(paths),
                        span: atom_symbol_span(head_view).unwrap_or(head_view.span),
                        text: atom_symbol_text(head_view).unwrap_or(head).to_owned(),
                        replacement: context.to.as_str().to_owned(),
                    });
                }
            }
            if let Some(shape) = shape {
                definition_body_range = Some(shape.body_range());
                if is_macro_expander_definition(context.dialect, head) {
                    macro_expander_body_range = definition_body_range;
                }
            }
        }
    }

    for (index, child) in view.children.iter().enumerate().rev() {
        if let Some(range) = definition_body_range {
            if !range.contains_child(index) {
                continue;
            }
        }
        let child_path = paths.child(state.path, index);
        let child_state = state.with_quasiquote_depth(0).with_path(child_path);
        let child_state =
            if macro_expander_body_range.is_some_and(|range| range.contains_child(index)) {
                child_state.in_macro_expander()
            } else {
                child_state
            };
        stack.push(TraversalFrame {
            view: child,
            state: child_state,
        });
    }
}

fn push_children<'a>(
    view: &'a ExpressionView,
    state: TraversalState,
    paths: &mut TraversalPathArena,
    stack: &mut Vec<TraversalFrame<'a>>,
) {
    for (index, child) in view.children.iter().enumerate().rev() {
        let child_path = paths.child(state.path, index);
        stack.push(TraversalFrame {
            view: child,
            state: state.with_path(child_path),
        });
    }
}

impl TraversalPath {
    fn to_string(self, paths: &TraversalPathArena) -> String {
        paths.materialize(self).to_string()
    }
}
