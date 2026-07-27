use crate::domain::rename::macrolet::RenameFunctionOccurrence;
use crate::domain::sexpr::{ExpressionKind, ExpressionView};

use super::super::super::reader::{CallableTarget, callable_target, push_atom_rename_if_match};
use super::super::super::scope::MacroletRenameScope;
use super::super::core::{TraversalPath, TraversalPathArena};
use super::super::state::{TraversalContext, TraversalState};

pub(super) fn collect_active_atom_rename(
    child: &ExpressionView,
    child_path: TraversalPath,
    paths: &mut TraversalPathArena,
    context: TraversalContext<'_>,
    state: TraversalState,
    scope: MacroletRenameScope,
    renames: &mut Vec<RenameFunctionOccurrence>,
) -> bool {
    if child.kind == ExpressionKind::Atom && state.allows_active_rename(scope) {
        return push_atom_rename_if_match(
            child,
            child_path,
            paths,
            context.from,
            context.to,
            renames,
        );
    }
    false
}

pub(super) fn callable_binding_name_target(binding: &ExpressionView) -> Option<CallableTarget<'_>> {
    callable_target(binding.children.first()?)
}

pub(super) fn callable_list_head_target(view: &ExpressionView) -> Option<CallableTarget<'_>> {
    let head = view.children.first()?;
    if head.kind != ExpressionKind::List {
        return None;
    }
    callable_target(head)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::rename::macrolet::scope::{LocalCallableRenameKind, MacroletRenameScope};
    use crate::domain::rename::macrolet::traversal::BindingTraversal;
    use crate::domain::rename::macrolet::traversal::core::{
        RenameTraversalMode, TraversalPathArena,
    };
    use crate::domain::rename::macrolet::traversal::state::TraversalState;
    use crate::domain::sexpr::{Path, SyntaxTree};

    #[test]
    fn extracts_setf_callable_binding_name_target() {
        let tree = SyntaxTree::parse("(flet (((setf foo) (value object) value)) foo)\n")
            .expect("test input should parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        let bindings = view.children.get(1).expect("bindings list");
        let binding = bindings.children.first().expect("binding");

        let target = callable_binding_name_target(binding).expect("setf binding target");

        assert_eq!(target.text, "foo");
    }

    #[test]
    fn collects_setf_callable_binding_name_rename() {
        let tree = SyntaxTree::parse("(flet (((setf foo) (value object) value)) foo)\n")
            .expect("test input should parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        let bindings = view.children.get(1).expect("bindings list");
        let binding = bindings.children.first().expect("binding");
        let from = crate::domain::sexpr::SymbolName::new("foo").expect("symbol");
        let to = crate::domain::sexpr::SymbolName::new("bar").expect("symbol");
        let context = TraversalContext {
            dialect: crate::domain::dialect::Dialect::CommonLisp,
            from: &from,
            to: &to,
            kind: LocalCallableRenameKind::Function,
        };
        let state = TraversalState {
            scope: MacroletRenameScope::default(),
            reader_lambda_body_scope: MacroletRenameScope::default(),
            quasiquote_depth: 0,
        };
        let (mut paths, path) = TraversalPathArena::from_path(&Path::from_indexes(vec![]));
        let mut renames = Vec::new();

        BindingTraversal::collect_binding_name_renames(
            binding,
            0,
            path,
            &mut paths,
            crate::domain::common_lisp::CommonLispLocalCallableForm::Flet,
            context,
            state,
            &mut renames,
        );

        assert_eq!(renames.len(), 1);
        assert_eq!(renames[0].path, "1.0.0.1");
    }

    #[test]
    fn extracts_setf_callable_list_head_target() {
        let tree =
            SyntaxTree::parse("(flet (((setf foo) (value object) value)) ((setf foo) 1 thing))\n")
                .expect("test input should parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        let body = view.children.get(2).expect("body form");

        let target = callable_list_head_target(body).expect("setf callable head");

        assert_eq!(target.text, "foo");
    }
}
