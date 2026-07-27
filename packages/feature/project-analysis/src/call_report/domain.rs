use anyhow::Result;
use std::rc::Rc;

use crate::domain::common_lisp::{
    CommonLispBindingListShape, CommonLispBindingRefactorForm, CommonLispLocalCallableForm,
    CommonLispOperator, CommonLispSlotBindingForm, common_lisp_symbol_reference_eq,
    is_common_lisp_declaration_form,
};
use crate::domain::common_lisp::{
    common_lisp_local_callable_form, is_local_callable_bound, local_callable_binding_body_scope,
    local_callable_body_scope,
};
use crate::domain::definition::{DefinitionShape, definition_shape};
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::reader::apply_reader_prefix_context;
use crate::domain::sexpr::{
    ByteSpan, Delimiter, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree,
};

#[derive(Debug, Clone)]
pub struct CallReportItem {
    pub path: String,
    pub span: ByteSpan,
    pub head: String,
    pub argument_count: usize,
    pub category: Option<crate::domain::definition::DefinitionCategory>,
    pub enclosing_definition: Option<String>,
}

fn list_head(view: &ExpressionView) -> Option<&str> {
    atom_child(view, 0)
}

fn atom_child(view: &ExpressionView, index: usize) -> Option<&str> {
    view.children.get(index).and_then(atom_text)
}

fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

pub fn build_call_report(
    tree: &SyntaxTree,
    dialect: Dialect,
    symbol: Option<&SymbolName>,
    include_definitions: bool,
) -> Result<Vec<CallReportItem>> {
    let mut calls = Vec::new();
    let ctx = CallReportTraversal {
        dialect,
        symbol,
        include_definitions,
    };

    for index in 0..tree.root_children().len() {
        let path = Path::root_child(index);
        let view = tree.select_path(&path)?.view();
        let _ = collect_call_report_items_from_view(&view, path, &ctx, &mut calls);
    }

    calls.sort_by_key(|call| call.span.start());
    Ok(calls)
}

struct CallReportTraversal<'a> {
    dialect: Dialect,
    symbol: Option<&'a SymbolName>,
    include_definitions: bool,
}

#[derive(Clone, Copy)]
struct CallReportPath(usize);

struct CallReportPathNode {
    parent: Option<usize>,
    index: usize,
}

struct CallReportPathArena {
    nodes: Vec<CallReportPathNode>,
    edge_count: usize,
    materialized_path_count: usize,
}

impl CallReportPathArena {
    fn from_path(path: &Path) -> (Self, CallReportPath) {
        let indexes = path.to_raw_indexes();
        let mut nodes = Vec::with_capacity(indexes.len());
        let mut parent = None;
        for index in indexes {
            let node = nodes.len();
            nodes.push(CallReportPathNode { parent, index });
            parent = Some(node);
        }
        let root = parent.expect("call-report traversal starts at a root child");
        (
            Self {
                nodes,
                edge_count: 0,
                materialized_path_count: 0,
            },
            CallReportPath(root),
        )
    }

    fn child(&mut self, path: CallReportPath, index: usize) -> CallReportPath {
        let node = self.nodes.len();
        self.nodes.push(CallReportPathNode {
            parent: Some(path.0),
            index,
        });
        self.edge_count += 1;
        CallReportPath(node)
    }

    fn materialize(&mut self, path: CallReportPath) -> String {
        let mut indexes = Vec::new();
        let mut cursor = Some(path.0);
        while let Some(node) = cursor {
            indexes.push(self.nodes[node].index);
            cursor = self.nodes[node].parent;
        }
        indexes.reverse();
        self.materialized_path_count += 1;
        Path::from_indexes(indexes).to_string()
    }
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(not(test), allow(dead_code))]
struct CallReportTraversalStats {
    visited_count: usize,
    edge_count: usize,
    materialized_path_count: usize,
}

#[derive(Clone)]
struct CallReportFrame<'a> {
    view: &'a ExpressionView,
    path: CallReportPath,
    enclosing_definition: Option<Rc<str>>,
    local_callables: Rc<[String]>,
    quasiquote_depth: usize,
}

fn collect_call_report_items_from_view(
    view: &ExpressionView,
    path: Path,
    ctx: &CallReportTraversal<'_>,
    calls: &mut Vec<CallReportItem>,
) -> CallReportTraversalStats {
    let (mut paths, root_path) = CallReportPathArena::from_path(&path);
    let mut stack = vec![CallReportFrame {
        view,
        path: root_path,
        enclosing_definition: None,
        local_callables: Rc::from(Vec::<String>::new().into_boxed_slice()),
        quasiquote_depth: 0,
    }];
    let mut visited_count = 0;

    while let Some(frame) = stack.pop() {
        visited_count += 1;
        let Some(quasiquote_depth) =
            apply_reader_prefix_context(frame.view, frame.quasiquote_depth)
        else {
            continue;
        };

        if quasiquote_depth > 0 {
            push_children_from(
                &mut stack,
                &mut paths,
                &frame,
                0,
                quasiquote_depth,
                frame.local_callables.clone(),
            );
            continue;
        }

        if frame.view.kind == ExpressionKind::List && frame.view.delimiter == Some(Delimiter::Paren)
        {
            if let Some(head) = list_head(frame.view) {
                if let Some(form) = common_lisp_local_callable_form(ctx.dialect, head) {
                    push_local_callable_form_children(
                        &mut stack,
                        &mut paths,
                        &frame,
                        quasiquote_depth,
                        form,
                    );
                    continue;
                }

                if is_common_lisp_declaration_form(head) {
                    continue;
                }

                if CommonLispOperator::from_head(head)
                    .is_some_and(|operator| operator == CommonLispOperator::Locally)
                {
                    push_children_from(
                        &mut stack,
                        &mut paths,
                        &frame,
                        2,
                        quasiquote_depth,
                        frame.local_callables.clone(),
                    );
                    continue;
                }

                if let Some(refactor_form) = ctx
                    .dialect
                    .common_lisp_binding_refactor_form_for_head(head)
                    .filter(|form| form.binding_list_shape().is_some())
                {
                    let shape = definition_shape(ctx.dialect, frame.view, head);
                    collect_call_if_matched(&frame, head, shape, ctx, &mut paths, calls);
                    push_binding_refactor_form_children(
                        &mut stack,
                        &mut paths,
                        &frame,
                        quasiquote_depth,
                        refactor_form,
                    );
                    continue;
                }

                let shape = definition_shape(ctx.dialect, frame.view, head);
                collect_call_if_matched(&frame, head, shape, ctx, &mut paths, calls);

                let child_enclosing_definition = match shape {
                    Some(shape) => shape.name(frame.view).map(Rc::<str>::from),
                    None => frame.enclosing_definition.clone(),
                };
                let definition_body_range = shape.map(|shape| shape.body_range());
                for (index, child) in frame.view.children.iter().enumerate().rev() {
                    if definition_body_range.is_some_and(|range| !range.contains_child(index)) {
                        continue;
                    }
                    let child_path = paths.child(frame.path, index);
                    stack.push(CallReportFrame {
                        view: child,
                        path: child_path,
                        enclosing_definition: child_enclosing_definition.clone(),
                        local_callables: frame.local_callables.clone(),
                        quasiquote_depth,
                    });
                }
                continue;
            }
        }

        push_children_from(
            &mut stack,
            &mut paths,
            &frame,
            0,
            quasiquote_depth,
            frame.local_callables.clone(),
        );
    }

    CallReportTraversalStats {
        visited_count,
        edge_count: paths.edge_count,
        materialized_path_count: paths.materialized_path_count,
    }
}

fn collect_call_if_matched(
    frame: &CallReportFrame<'_>,
    head: &str,
    shape: Option<DefinitionShape>,
    ctx: &CallReportTraversal<'_>,
    paths: &mut CallReportPathArena,
    calls: &mut Vec<CallReportItem>,
) {
    let matches_symbol = ctx
        .symbol
        .is_none_or(|target| common_lisp_symbol_reference_eq(head, target.as_str()));

    if matches_symbol
        && (ctx.include_definitions || shape.is_none())
        && !is_local_callable_bound(frame.local_callables.as_ref(), head)
    {
        calls.push(CallReportItem {
            path: paths.materialize(frame.path),
            span: frame.view.span,
            head: head.to_owned(),
            argument_count: frame.view.children.len().saturating_sub(1),
            category: shape.map(|shape| shape.category),
            enclosing_definition: frame.enclosing_definition.as_deref().map(str::to_owned),
        });
    }
}

fn push_children_from<'a>(
    stack: &mut Vec<CallReportFrame<'a>>,
    paths: &mut CallReportPathArena,
    frame: &CallReportFrame<'a>,
    start_index: usize,
    quasiquote_depth: usize,
    local_callables: Rc<[String]>,
) {
    for (index, child) in frame
        .view
        .children
        .iter()
        .enumerate()
        .skip(start_index)
        .rev()
    {
        let child_path = paths.child(frame.path, index);
        stack.push(CallReportFrame {
            view: child,
            path: child_path,
            enclosing_definition: frame.enclosing_definition.clone(),
            local_callables: local_callables.clone(),
            quasiquote_depth,
        });
    }
}

fn push_binding_refactor_form_children<'a>(
    stack: &mut Vec<CallReportFrame<'a>>,
    paths: &mut CallReportPathArena,
    frame: &CallReportFrame<'a>,
    quasiquote_depth: usize,
    refactor_form: CommonLispBindingRefactorForm,
) {
    push_children_from(
        stack,
        paths,
        frame,
        refactor_form.remove_unused_body_start_index(),
        quasiquote_depth,
        frame.local_callables.clone(),
    );

    let Some(bindings) = frame.view.children.get(1) else {
        return;
    };
    let Some(shape) = refactor_form.binding_list_shape() else {
        return;
    };
    let bindings_path = paths.child(frame.path, 1);

    match shape {
        CommonLispBindingListShape::NameValuePairs
            if bindings.kind == ExpressionKind::List
                && bindings.delimiter == Some(Delimiter::Bracket) =>
        {
            for (index, child) in bindings.children.iter().enumerate().rev() {
                if index % 2 == 0 {
                    continue;
                }
                push_scheduled_child(
                    stack,
                    paths,
                    child,
                    bindings_path,
                    index,
                    frame,
                    quasiquote_depth,
                    frame.local_callables.clone(),
                );
            }
        }
        CommonLispBindingListShape::NameValuePairs => {
            push_binding_children(
                stack,
                paths,
                bindings,
                bindings_path,
                frame,
                quasiquote_depth,
                1,
                false,
            );
        }
        CommonLispBindingListShape::LocalCallableDefinitions(_) => {}
        CommonLispBindingListShape::VariableSpecs(_) => {
            push_binding_children(
                stack,
                paths,
                bindings,
                bindings_path,
                frame,
                quasiquote_depth,
                1,
                true,
            );
        }
        CommonLispBindingListShape::SlotBindings(form) => {
            push_slot_binding_children(
                stack,
                paths,
                bindings,
                bindings_path,
                frame,
                quasiquote_depth,
                form,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_binding_children<'a>(
    stack: &mut Vec<CallReportFrame<'a>>,
    paths: &mut CallReportPathArena,
    bindings: &'a ExpressionView,
    bindings_path: CallReportPath,
    frame: &CallReportFrame<'a>,
    quasiquote_depth: usize,
    start_index: usize,
    skip_atoms: bool,
) {
    for (binding_index, binding) in bindings.children.iter().enumerate().rev() {
        if skip_atoms && binding.kind == ExpressionKind::Atom {
            continue;
        }
        let binding_path = paths.child(bindings_path, binding_index);
        for (child_index, child) in binding.children.iter().enumerate().skip(start_index).rev() {
            push_scheduled_child(
                stack,
                paths,
                child,
                binding_path,
                child_index,
                frame,
                quasiquote_depth,
                frame.local_callables.clone(),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_slot_binding_children<'a>(
    stack: &mut Vec<CallReportFrame<'a>>,
    paths: &mut CallReportPathArena,
    bindings: &'a ExpressionView,
    bindings_path: CallReportPath,
    frame: &CallReportFrame<'a>,
    quasiquote_depth: usize,
    form: CommonLispSlotBindingForm,
) {
    for (binding_index, binding) in bindings.children.iter().enumerate().rev() {
        if form == CommonLispSlotBindingForm::WithSlots && binding.kind == ExpressionKind::Atom {
            continue;
        }
        let binding_path = paths.child(bindings_path, binding_index);
        for (child_index, child) in binding.children.iter().enumerate().skip(1).rev() {
            push_scheduled_child(
                stack,
                paths,
                child,
                binding_path,
                child_index,
                frame,
                quasiquote_depth,
                frame.local_callables.clone(),
            );
        }
    }
}

fn push_local_callable_form_children<'a>(
    stack: &mut Vec<CallReportFrame<'a>>,
    paths: &mut CallReportPathArena,
    frame: &CallReportFrame<'a>,
    quasiquote_depth: usize,
    form: CommonLispLocalCallableForm,
) {
    let body_scope = local_callable_body_scope(frame.local_callables.as_ref(), frame.view);
    let binding_body_scope =
        local_callable_binding_body_scope(form, frame.local_callables.as_ref(), &body_scope);
    let binding_body_scope: Rc<[String]> = Rc::from(binding_body_scope.to_vec().into_boxed_slice());
    let body_scope: Rc<[String]> = Rc::from(body_scope.into_boxed_slice());

    push_children_from(stack, paths, frame, 2, quasiquote_depth, body_scope);

    let Some(bindings) = frame.view.children.get(1) else {
        return;
    };
    let bindings_path = paths.child(frame.path, 1);
    for (binding_index, binding) in bindings.children.iter().enumerate().rev() {
        let binding_path = paths.child(bindings_path, binding_index);
        for (child_index, child) in binding.children.iter().enumerate().skip(2).rev() {
            push_scheduled_child(
                stack,
                paths,
                child,
                binding_path,
                child_index,
                frame,
                quasiquote_depth,
                binding_body_scope.clone(),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_scheduled_child<'a>(
    stack: &mut Vec<CallReportFrame<'a>>,
    paths: &mut CallReportPathArena,
    child: &'a ExpressionView,
    parent_path: CallReportPath,
    child_index: usize,
    frame: &CallReportFrame<'a>,
    quasiquote_depth: usize,
    local_callables: Rc<[String]>,
) {
    let child_path = paths.child(parent_path, child_index);
    stack.push(CallReportFrame {
        view: child,
        path: child_path,
        enclosing_definition: frame.enclosing_definition.clone(),
        local_callables,
        quasiquote_depth,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deeply_nested_no_match_traversal_has_linear_path_work() {
        const DEPTH: usize = 6_000;
        let mut input = String::with_capacity(DEPTH * 2 + 1);
        for _ in 0..DEPTH {
            input.push('(');
        }
        input.push('x');
        for _ in 0..DEPTH {
            input.push(')');
        }

        let tree = SyntaxTree::parse(&input).expect("deep input parses");
        let path = Path::root_child(0);
        let view = tree.select_path(&path).expect("root exists").view();
        let target = SymbolName::new("missing").expect("symbol");
        let ctx = CallReportTraversal {
            dialect: Dialect::CommonLisp,
            symbol: Some(&target),
            include_definitions: false,
        };
        let mut calls = Vec::new();

        let stats = collect_call_report_items_from_view(&view, path, &ctx, &mut calls);

        assert!(calls.is_empty());
        assert_eq!(stats.visited_count, DEPTH + 1);
        assert_eq!(stats.edge_count, DEPTH);
        assert_eq!(stats.materialized_path_count, 0);
    }
}
