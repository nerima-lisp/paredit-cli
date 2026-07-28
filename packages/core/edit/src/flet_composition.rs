//! Scope-safe composition of nested Common Lisp `flet` forms.

use crate::error::{
    BindingRefusal, ConservativeRefusal, DialectRefusal, DocumentRefusal, EditResult,
    LocalFunctionRefusal, ShapeRefusal,
};
use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, Path, SyntaxTree};

#[derive(Debug, Clone)]
pub struct Request<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
}
#[derive(Debug, Clone)]
pub struct Plan {
    pub dialect: Dialect,
    pub path: Path,
    pub form_span: ByteSpan,
    pub outer_binding_count: usize,
    pub inner_binding_count: usize,
    pub rewritten: String,
    pub changed: bool,
}

pub fn plan(request: Request<'_>) -> EditResult<Plan> {
    validate_dialect(request.dialect)?;
    let tree =
        SyntaxTree::parse_with_dialect(request.input, request.dialect).map_err(|source| {
            DocumentRefusal::InputInvalid {
                operation: "merge-nested-flet",
                source,
            }
        })?;
    let outer = tree.select_path(&request.path)?.view();
    require_flet(&outer, "selected form")?;
    reject_unsafe(&tree, &outer)?;
    if outer.children.len() != 3 {
        return Err(BindingRefusal::OuterBodyNotSingleForm {
            operation: "merge-nested-flet",
        }
        .into());
    }
    let outer_defs = definitions(&outer.children[1], "outer")?;
    let inner = &outer.children[2];
    require_flet(inner, "outer body")?;
    if inner.children.len() < 3 {
        return Err(BindingRefusal::InnerHasNoBody {
            operation: "merge-nested-flet",
            inner: "flet",
        }
        .into());
    }
    let inner_defs = definitions(&inner.children[1], "inner")?;
    let outer_names = names(outer_defs)?;
    let inner_names = names(inner_defs)?;
    let mut all = Vec::new();
    for name in outer_names.iter().chain(inner_names.iter()) {
        if all
            .iter()
            .any(|old: &&str| common_lisp_symbol_reference_eq(old, name))
        {
            return Err(LocalFunctionRefusal::DuplicateNames {
                operation: "merge-nested-flet",
            }
            .into());
        }
        all.push(name);
    }
    for definition in &inner_defs.children {
        for body in definition.children.iter().skip(2) {
            if references_local(body, &outer_names) {
                return Err(LocalFunctionRefusal::WouldEscapeOuterScope {
                    operation: "merge-nested-flet",
                }
                .into());
            }
        }
    }
    let left = list_contents(request.input, outer_defs);
    let right = list_contents(request.input, inner_defs);
    let separator = if left.trim().is_empty() || right.trim().is_empty() {
        ""
    } else {
        " "
    };
    let head = outer.children[0].span.slice(request.input);
    let body = &request.input[inner_defs.span.end().get()..inner.span.end().get() - 1];
    let replacement = format!("({head} ({left}{separator}{right}){body})");
    let rewritten = replace_span(request.input, outer.span, &replacement);
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect).map_err(|source| {
        DocumentRefusal::OutputInvalid {
            operation: "merge-nested-flet",
            source,
        }
    })?;
    Ok(Plan {
        dialect: request.dialect,
        path: request.path,
        form_span: outer.span,
        outer_binding_count: outer_defs.children.len(),
        inner_binding_count: inner_defs.children.len(),
        changed: rewritten != request.input,
        rewritten,
    })
}

pub fn validate_dialect(dialect: Dialect) -> EditResult<()> {
    if dialect != Dialect::CommonLisp {
        return Err(DialectRefusal::CommonLispOnly {
            operation: "merge-nested-flet",
        }
        .into());
    }
    Ok(())
}
fn require_flet(view: &ExpressionView, role: &str) -> EditResult<()> {
    if view.kind != ExpressionKind::List
        || !view.reader_prefixes.is_empty()
        || !view
            .children
            .first()
            .and_then(atom_symbol_text)
            .is_some_and(|head| common_lisp_symbol_reference_eq(head, "flet"))
    {
        return Err(ShapeRefusal::RoleNotPlainFlet {
            operation: "merge-nested-flet",
            role: role.to_owned(),
        }
        .into());
    }
    Ok(())
}
fn definitions<'a>(view: &'a ExpressionView, role: &str) -> EditResult<&'a ExpressionView> {
    if view.kind != ExpressionKind::List || !view.reader_prefixes.is_empty() {
        return Err(LocalFunctionRefusal::NotPlainDefinitionList {
            operation: "merge-nested-flet",
            role: role.to_owned(),
        }
        .into());
    }
    for definition in &view.children {
        if definition.kind != ExpressionKind::List
            || !definition.reader_prefixes.is_empty()
            || definition.children.len() < 2
            || plain_atom(&definition.children[0]).is_none()
            || definition.children[1].kind != ExpressionKind::List
            || !definition.children[1].reader_prefixes.is_empty()
        {
            return Err(LocalFunctionRefusal::NotPlainDefinitions {
                operation: "merge-nested-flet",
            }
            .into());
        }
    }
    Ok(view)
}
fn names(view: &ExpressionView) -> EditResult<Vec<&str>> {
    view.children
        .iter()
        .map(|definition| {
            plain_atom(&definition.children[0])
                .ok_or_else(|| LocalFunctionRefusal::NameNotPlain.into())
        })
        .collect()
}
fn plain_atom(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom && view.reader_prefixes.is_empty())
        .then(|| atom_symbol_text(view))
        .flatten()
}
fn reject_unsafe(tree: &SyntaxTree, form: &ExpressionView) -> EditResult<()> {
    if tree.has_comment_in(form.span) {
        return Err(ConservativeRefusal::Comments {
            operation: "merge-nested-flet",
        }
        .into());
    }
    if contains_prefix(form) {
        return Err(ConservativeRefusal::ReaderPrefixes {
            operation: "merge-nested-flet",
        }
        .into());
    }
    if contains_headed(form, "declare") {
        return Err(ConservativeRefusal::Declarations {
            operation: "merge-nested-flet",
        }
        .into());
    }
    Ok(())
}
fn contains_prefix(view: &ExpressionView) -> bool {
    !view.reader_prefixes.is_empty() || view.children.iter().any(contains_prefix)
}
fn contains_headed(view: &ExpressionView, expected: &str) -> bool {
    (view.kind == ExpressionKind::List
        && view
            .children
            .first()
            .and_then(plain_atom)
            .is_some_and(|head| common_lisp_symbol_reference_eq(head, expected)))
        || view
            .children
            .iter()
            .any(|child| contains_headed(child, expected))
}
fn references_local(view: &ExpressionView, names: &[&str]) -> bool {
    if view.kind == ExpressionKind::List {
        let head = view.children.first().and_then(plain_atom);
        if head.is_some_and(|head| {
            names
                .iter()
                .any(|name| common_lisp_symbol_reference_eq(name, head))
        }) {
            return true;
        }
        if head.is_some_and(|head| common_lisp_symbol_reference_eq(head, "function"))
            && view
                .children
                .get(1)
                .and_then(plain_atom)
                .is_some_and(|name| {
                    names
                        .iter()
                        .any(|local| common_lisp_symbol_reference_eq(local, name))
                })
        {
            return true;
        }
    }
    view.children
        .iter()
        .any(|child| references_local(child, names))
}
fn list_contents<'a>(input: &'a str, view: &ExpressionView) -> &'a str {
    &input[view.span.start().get() + 1..view.span.end().get() - 1]
}
fn replace_span(input: &str, span: ByteSpan, replacement: &str) -> String {
    let mut output = String::with_capacity(input.len() + replacement.len());
    output.push_str(&input[..span.start().get()]);
    output.push_str(replacement);
    output.push_str(&input[span.end().get()..]);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(input: &str, dialect: Dialect) -> Request<'_> {
        Request {
            input,
            dialect,
            path: "0".parse().expect("path"),
        }
    }

    #[test]
    fn merges_independent_definitions_with_common_lisp_reader_literal() {
        let input =
            r"(flet ((parse (x) (list x))) (flet ((emit (x) (print x))) (emit (parse value)))) #\)";
        let plan = plan(request(input, Dialect::CommonLisp)).expect("plan");
        assert_eq!(
            plan.rewritten,
            r"(flet ((parse (x) (list x)) (emit (x) (print x))) (emit (parse value))) #\)"
        );
        SyntaxTree::parse_with_dialect(&plan.rewritten, Dialect::CommonLisp)
            .expect("rewritten input");
    }

    #[test]
    fn rejects_unsupported_dialects_before_parsing() {
        for dialect in [
            Dialect::EmacsLisp,
            Dialect::Scheme,
            Dialect::Clojure,
            Dialect::Janet,
            Dialect::Fennel,
            Dialect::Unknown,
        ] {
            let error = plan(request(")", dialect)).expect_err("dialect must be rejected");
            assert_eq!(
                error.to_string(),
                "merge-nested-flet supports only Common Lisp"
            );
        }
    }

    #[test]
    fn rejects_scope_capture_duplicates_and_unsafe_syntax() {
        for input in [
            "(flet ((parse (x) x)) (flet ((emit (x) (parse x))) (emit value)))",
            "(flet ((work () 1)) (flet ((work () 2)) (work)))",
            "(flet ((left () ; note\n 1)) (flet ((right () 2)) (right)))",
        ] {
            assert!(plan(request(input, Dialect::CommonLisp)).is_err());
        }
    }
}
