use anyhow::Result;

use crate::domain::common_lisp::{common_lisp_operator_head_eq, common_lisp_symbol_reference_eq};
use crate::domain::definition::{DefinitionCategory, definition_shape};
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{
    ExpressionKind, ExpressionView, NonEmptyExpressionPath, Path, SymbolName, SyntaxTree,
};

use super::super::RenameFunctionOccurrence;
use super::super::reader::{
    apply_reader_prefix_context, explicit_reader_form_kind,
    explicit_reader_function_lambda_body_children,
};
use super::shared::list_head;

#[derive(Clone, Copy)]
struct DefinitionPath(usize);

struct DefinitionPathNode {
    parent: Option<usize>,
    index: usize,
}

struct DefinitionPathArena {
    nodes: Vec<DefinitionPathNode>,
    edge_count: usize,
    materialized_index_count: usize,
}

impl DefinitionPathArena {
    fn from_path(path: NonEmptyExpressionPath<'_>) -> (Self, DefinitionPath) {
        let mut nodes = Vec::with_capacity(path.indexes().len());
        let mut parent = None;
        for index in path.indexes() {
            let node = nodes.len();
            nodes.push(DefinitionPathNode { parent, index });
            parent = Some(node);
        }
        let current = parent.unwrap_or_else(|| unreachable!("non-empty path has a root node"));
        (
            Self {
                nodes,
                edge_count: 0,
                materialized_index_count: 0,
            },
            DefinitionPath(current),
        )
    }

    fn child(&mut self, path: DefinitionPath, index: usize) -> DefinitionPath {
        let node = self.nodes.len();
        self.nodes.push(DefinitionPathNode {
            parent: Some(path.0),
            index,
        });
        self.edge_count += 1;
        DefinitionPath(node)
    }

    fn descendant(
        &mut self,
        mut path: DefinitionPath,
        indexes: impl IntoIterator<Item = usize>,
    ) -> DefinitionPath {
        for index in indexes {
            path = self.child(path, index);
        }
        path
    }

    fn materialize(&mut self, path: DefinitionPath) -> Path {
        let mut indexes = Vec::new();
        let mut cursor = Some(path.0);
        while let Some(node) = cursor {
            indexes.push(self.nodes[node].index);
            cursor = self.nodes[node].parent;
        }
        self.materialized_index_count += indexes.len();
        indexes.reverse();
        Path::from_indexes(indexes)
    }
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(not(test), allow(dead_code))]
struct DefinitionTraversalStats {
    visited_count: usize,
    edge_count: usize,
    materialized_index_count: usize,
}

pub fn collect_define_symbol_macro_definition_renames(
    tree: &SyntaxTree,
    dialect: Dialect,
    from: &SymbolName,
    to: &SymbolName,
) -> Result<Vec<RenameFunctionOccurrence>> {
    let mut renames = Vec::new();

    for (top_index, _) in tree.root_children().iter().enumerate() {
        collect_definition_renames_from_path(
            tree,
            Path::root_child(top_index),
            dialect,
            from,
            to,
            &mut renames,
        )?;
    }

    Ok(renames)
}

fn collect_definition_renames_from_path(
    tree: &SyntaxTree,
    path: Path,
    dialect: Dialect,
    from: &SymbolName,
    to: &SymbolName,
    renames: &mut Vec<RenameFunctionOccurrence>,
) -> Result<()> {
    let view = tree.select_path(&path)?.view();
    let _ = collect_definition_renames_from_view(&view, path, dialect, from, to, 0, renames);
    Ok(())
}

#[derive(Clone, Copy)]
struct DefinitionFrame<'a> {
    view: &'a ExpressionView,
    path: DefinitionPath,
    quasiquote_depth: usize,
}

#[allow(clippy::too_many_arguments)]
fn collect_definition_renames_from_view(
    view: &ExpressionView,
    path: Path,
    dialect: Dialect,
    from: &SymbolName,
    to: &SymbolName,
    quasiquote_depth: usize,
    renames: &mut Vec<RenameFunctionOccurrence>,
) -> Option<DefinitionTraversalStats> {
    let path = NonEmptyExpressionPath::try_from(&path).ok()?;
    let (mut paths, root_path) = DefinitionPathArena::from_path(path);
    let mut stack = vec![DefinitionFrame {
        view,
        path: root_path,
        quasiquote_depth,
    }];
    let mut visited_count = 0usize;

    while let Some(frame) = stack.pop() {
        visited_count += 1;
        let Some(quasiquote_depth) =
            apply_reader_prefix_context(frame.view, frame.quasiquote_depth)
        else {
            continue;
        };

        if collect_explicit_reader_form_definition_renames(
            frame,
            dialect,
            from,
            to,
            quasiquote_depth,
            &mut paths,
            renames,
            &mut stack,
        ) {
            continue;
        }

        if quasiquote_depth == 0 {
            collect_target_definition_rename(
                frame.view, frame.path, dialect, from, to, &mut paths, renames,
            );
        }

        push_children(
            frame.view,
            frame.path,
            quasiquote_depth,
            &mut paths,
            &mut stack,
        );
    }

    Some(DefinitionTraversalStats {
        visited_count,
        edge_count: paths.edge_count,
        materialized_index_count: paths.materialized_index_count,
    })
}

#[allow(clippy::too_many_arguments)]
fn collect_target_definition_rename(
    view: &ExpressionView,
    path: DefinitionPath,
    dialect: Dialect,
    from: &SymbolName,
    to: &SymbolName,
    paths: &mut DefinitionPathArena,
    renames: &mut Vec<RenameFunctionOccurrence>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !common_lisp_operator_head_eq(head, "define-symbol-macro") {
        return;
    }
    let Some(shape) = definition_shape(dialect, view, head)
        .filter(|shape| shape.category == DefinitionCategory::Variable)
    else {
        return;
    };
    let Some(name) = shape.name(view) else {
        return;
    };
    if !common_lisp_symbol_reference_eq(name, from.as_str()) {
        return;
    }
    let Some(name_target) = shape.name_target(view, &Path::from_indexes(Vec::new())) else {
        return;
    };
    let name_path = paths.descendant(path, name_target.path.to_raw_indexes());
    renames.push(RenameFunctionOccurrence {
        path: paths.materialize(name_path).to_string(),
        span: name_target.span,
        text: from.as_str().to_owned(),
        replacement: to.as_str().to_owned(),
    });
}

fn push_children<'a>(
    view: &'a ExpressionView,
    path: DefinitionPath,
    quasiquote_depth: usize,
    paths: &mut DefinitionPathArena,
    stack: &mut Vec<DefinitionFrame<'a>>,
) {
    for (child_index, child) in view.children.iter().enumerate().rev() {
        let child_path = paths.child(path, child_index);
        stack.push(DefinitionFrame {
            view: child,
            path: child_path,
            quasiquote_depth,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_explicit_reader_form_definition_renames<'a>(
    frame: DefinitionFrame<'a>,
    _dialect: Dialect,
    _from: &SymbolName,
    _to: &SymbolName,
    quasiquote_depth: usize,
    paths: &mut DefinitionPathArena,
    _renames: &mut Vec<RenameFunctionOccurrence>,
    stack: &mut Vec<DefinitionFrame<'a>>,
) -> bool {
    if frame.view.kind != ExpressionKind::List || frame.view.children.len() < 2 {
        return false;
    }

    let Some(kind_name) = explicit_reader_form_kind(frame.view) else {
        return false;
    };

    match kind_name.as_str() {
        "quote" => true,
        "function" if quasiquote_depth == 0 => {
            if let Some(children) = explicit_reader_function_lambda_body_children(frame.view) {
                let children = children.collect::<Vec<_>>();
                for (child_index, child) in children.into_iter().rev() {
                    let child_path = paths.descendant(frame.path, [1, child_index]);
                    stack.push(DefinitionFrame {
                        view: child,
                        path: child_path,
                        quasiquote_depth,
                    });
                }
            }
            true
        }
        "function" => true,
        "quasiquote" => {
            for (child_index, child) in frame.view.children.iter().enumerate().skip(1).rev() {
                let child_path = paths.child(frame.path, child_index);
                stack.push(DefinitionFrame {
                    view: child,
                    path: child_path,
                    quasiquote_depth: quasiquote_depth + 1,
                });
            }
            true
        }
        "unquote" | "unquote-splicing" if quasiquote_depth > 0 => {
            for (child_index, child) in frame.view.children.iter().enumerate().skip(1).rev() {
                let child_path = paths.child(frame.path, child_index);
                stack.push(DefinitionFrame {
                    view: child,
                    path: child_path,
                    quasiquote_depth: quasiquote_depth - 1,
                });
            }
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_definition_walk_uses_one_path_node_per_edge() {
        let depth = 6_000usize;
        let input = format!(
            "{}(define-symbol-macro target 1){}",
            "(".repeat(depth),
            ")".repeat(depth)
        );
        let tree = SyntaxTree::parse(&input).expect("deep input should parse");
        let path = Path::root_child(0);
        let view = tree.select_path(&path).expect("root form").view();
        let from = SymbolName::new("target").expect("symbol");
        let to = SymbolName::new("replacement").expect("symbol");
        let mut renames = Vec::new();

        let stats = collect_definition_renames_from_view(
            &view,
            path,
            Dialect::CommonLisp,
            &from,
            &to,
            0,
            &mut renames,
        )
        .expect("root-child path is non-empty");

        assert_eq!(renames.len(), 1);
        assert_eq!(stats.visited_count, depth + 4);
        assert_eq!(stats.edge_count, stats.visited_count - 1 + 1);
        assert_eq!(
            stats.materialized_index_count,
            renames[0].path.split('.').count()
        );
    }
}
