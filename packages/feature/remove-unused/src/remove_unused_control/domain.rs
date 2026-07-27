//! Remove unused Common Lisp `block` names and `tagbody` tags.

use anyhow::{Context, Result, bail};

use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
use crate::domain::dialect::Dialect;
use crate::domain::extract_shared::replace_span;
use crate::domain::mutation_safety::reject_common_lisp_reader_conditionals;
use crate::domain::sexpr::reader::atom_symbol_text;
use crate::domain::sexpr::{ByteSpan, ExpressionKind, ExpressionView, Path, SyntaxTree};

#[derive(Debug, Clone)]
pub struct RemoveUnusedControlRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct RemoveUnusedControlPlan {
    pub dialect: Dialect,
    pub path: Path,
    pub form_span: ByteSpan,
    pub reference_count: usize,
    pub rewritten: String,
    pub changed: bool,
}

pub fn plan_remove_unused_block(
    request: RemoveUnusedControlRequest<'_>,
) -> Result<RemoveUnusedControlPlan> {
    prepare(&request, "remove-unused-block")?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("input is not valid")?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    require_known_expression_context(&tree, &request.path)?;
    let form = tree.select_path(&request.path)?.view();
    reject_unsafe(&tree, &form, "remove-unused-block")?;
    require_head(&form, "block", "remove-unused-block")?;
    let name = form
        .children
        .get(1)
        .and_then(plain_atom)
        .context("remove-unused-block requires a plain symbol block name")?;
    require_symbol(name, "remove-unused-block")?;
    if !symbol_eq(name, &request.name) {
        bail!("selected block name does not match --name");
    }
    let mut references = 0;
    for child in form.children.iter().skip(2) {
        count_block_references(child, &request.name, true, &mut references)?;
    }
    if references != 0 {
        bail!("remove-unused-block found {references} matching return-from reference(s)");
    }
    let replacement = body_replacement(request.input, &form.children[2..]);
    finish(request, form.span, references, &replacement)
}

pub fn plan_remove_unused_tag(
    request: RemoveUnusedControlRequest<'_>,
) -> Result<RemoveUnusedControlPlan> {
    prepare(&request, "remove-unused-tag")?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("input is not valid")?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    let form = tree.select_path(&request.path)?.view();
    reject_unsafe(&tree, &form, "remove-unused-tag")?;
    require_head(&form, "tagbody", "remove-unused-tag")?;
    let tags = direct_tags(&form);
    let matches = tags
        .iter()
        .filter(|tag| plain_atom(tag).is_some_and(|name| tag_eq(name, &request.name)))
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        bail!("remove-unused-tag requires exactly one matching tag definition");
    }
    let mut references = 0;
    for child in form
        .children
        .iter()
        .skip(1)
        .filter(|child| child.kind == ExpressionKind::List)
    {
        count_tag_references(child, &request.name, true, &mut references)?;
    }
    if references != 0 {
        bail!("remove-unused-tag found {references} matching go reference(s)");
    }
    finish(request, matches[0].span, references, "")
}

fn prepare(request: &RemoveUnusedControlRequest<'_>, operation: &str) -> Result<()> {
    if request.dialect != Dialect::CommonLisp {
        bail!("{operation} supports only Common Lisp");
    }
    if request.name.contains(':') {
        bail!("{operation} requires an unqualified symbol or integer tag");
    }
    Ok(())
}

fn finish(
    request: RemoveUnusedControlRequest<'_>,
    span: ByteSpan,
    reference_count: usize,
    replacement: &str,
) -> Result<RemoveUnusedControlPlan> {
    let rewritten = replace_span(request.input, span, replacement);
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect)
        .context("rewritten output is not valid")?;
    Ok(RemoveUnusedControlPlan {
        dialect: request.dialect,
        path: request.path,
        form_span: span,
        reference_count,
        changed: rewritten != request.input,
        rewritten,
    })
}

fn count_block_references(
    view: &ExpressionView,
    target: &str,
    enabled: bool,
    count: &mut usize,
) -> Result<()> {
    if view.kind == ExpressionKind::List {
        if head_is(view, "block") {
            let name = view
                .children
                .get(1)
                .and_then(plain_atom)
                .context("remove-unused-block found malformed nested block")?;
            require_symbol(name, "remove-unused-block")?;
            for child in view.children.iter().skip(2) {
                count_block_references(child, target, enabled && !symbol_eq(name, target), count)?;
            }
            return Ok(());
        }
        if head_is(view, "return-from") {
            let name = view
                .children
                .get(1)
                .and_then(plain_atom)
                .context("remove-unused-block found malformed return-from")?;
            require_symbol(name, "remove-unused-block")?;
            if enabled && symbol_eq(name, target) {
                *count += 1;
            }
        }
    }
    for child in &view.children {
        count_block_references(child, target, enabled, count)?;
    }
    Ok(())
}

fn count_tag_references(
    view: &ExpressionView,
    target: &str,
    enabled: bool,
    count: &mut usize,
) -> Result<()> {
    if view.kind == ExpressionKind::List {
        if head_is(view, "tagbody") {
            let shadows = direct_tags(view)
                .iter()
                .any(|tag| plain_atom(tag).is_some_and(|name| tag_eq(name, target)));
            for child in view
                .children
                .iter()
                .skip(1)
                .filter(|child| child.kind == ExpressionKind::List)
            {
                count_tag_references(child, target, enabled && !shadows, count)?;
            }
            return Ok(());
        }
        if head_is(view, "go") {
            let name = view
                .children
                .get(1)
                .and_then(plain_atom)
                .context("remove-unused-tag found malformed go")?;
            if enabled && tag_eq(name, target) {
                *count += 1;
            }
        }
    }
    for child in &view.children {
        count_tag_references(child, target, enabled, count)?;
    }
    Ok(())
}

fn require_known_expression_context(tree: &SyntaxTree, path: &Path) -> Result<()> {
    let indexes = path.to_raw_indexes();
    if indexes.len() < 2 {
        bail!("remove-unused-block refuses top-level or unknown contexts");
    }
    for depth in 1..indexes.len() {
        let ancestor = tree
            .select_path(&Path::from_indexes(indexes[..depth].to_vec()))?
            .view();
        if !ancestor.reader_prefixes.is_empty() {
            bail!("remove-unused-block refuses reader-prefixed contexts");
        }
    }
    let index = *indexes
        .last()
        .context("remove-unused-block requires a non-empty path")?;
    let parent = tree
        .select_path(&Path::from_indexes(indexes[..indexes.len() - 1].to_vec()))?
        .view();
    let head = parent
        .children
        .first()
        .and_then(plain_atom)
        .context("remove-unused-block requires a known expression context")?;
    let known = (symbol_eq(head, "progn") && index >= 1)
        || (symbol_eq(head, "if") && (1..=3).contains(&index))
        || ((symbol_eq(head, "when") || symbol_eq(head, "unless")) && index >= 1)
        || ((symbol_eq(head, "let") || symbol_eq(head, "let*")) && index >= 2)
        || (symbol_eq(head, "lambda") && index >= 2)
        || (symbol_eq(head, "defun") && index >= 3);
    if !known {
        bail!("remove-unused-block requires a known expression position");
    }
    Ok(())
}

fn reject_unsafe(tree: &SyntaxTree, form: &ExpressionView, operation: &str) -> Result<()> {
    if tree.has_comment_in(form.span) {
        bail!("{operation} cannot rewrite a form containing comments");
    }
    if contains_prefix(form) || contains_head(form, "quote") || contains_head(form, "quasiquote") {
        bail!("{operation} conservatively rejects reader-prefixed or quoted forms");
    }
    if contains_head(form, "declare") {
        bail!("{operation} conservatively rejects declarations");
    }
    Ok(())
}

fn body_replacement(input: &str, body: &[ExpressionView]) -> String {
    match body {
        [] => "nil".to_owned(),
        [only] => only.span.slice(input).to_owned(),
        many => format!(
            "(progn {})",
            many.iter()
                .map(|view| view.span.slice(input))
                .collect::<Vec<_>>()
                .join(" ")
        ),
    }
}

fn direct_tags(form: &ExpressionView) -> Vec<&ExpressionView> {
    form.children
        .iter()
        .skip(1)
        .filter(|view| plain_atom(view).is_some())
        .collect()
}
fn require_head(form: &ExpressionView, expected: &str, operation: &str) -> Result<()> {
    if form.kind != ExpressionKind::List
        || form
            .children
            .first()
            .and_then(plain_atom)
            .is_none_or(|head| !symbol_eq(head, expected))
    {
        bail!("{operation} selected form must be a {expected} form");
    }
    Ok(())
}
fn plain_atom(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom && view.reader_prefixes.is_empty())
        .then(|| atom_symbol_text(view))
        .flatten()
}
fn head_is(view: &ExpressionView, expected: &str) -> bool {
    view.children
        .first()
        .and_then(plain_atom)
        .is_some_and(|head| symbol_eq(head, expected))
}
fn symbol_eq(left: &str, right: &str) -> bool {
    common_lisp_symbol_reference_eq(left, right)
}
fn tag_eq(left: &str, right: &str) -> bool {
    match (left.parse::<i128>(), right.parse::<i128>()) {
        (Ok(left), Ok(right)) => left == right,
        (Err(_), Err(_)) => symbol_eq(left, right),
        _ => false,
    }
}
fn require_symbol(name: &str, operation: &str) -> Result<()> {
    if name.contains(':') || name.parse::<i128>().is_ok() {
        bail!("{operation} requires an unqualified symbol name");
    }
    Ok(())
}
fn contains_prefix(view: &ExpressionView) -> bool {
    !view.reader_prefixes.is_empty() || view.children.iter().any(contains_prefix)
}
fn contains_head(view: &ExpressionView, expected: &str) -> bool {
    (view.kind == ExpressionKind::List && head_is(view, expected))
        || view
            .children
            .iter()
            .any(|child| contains_head(child, expected))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request<'a>(input: &'a str, path: &str, name: &str) -> RemoveUnusedControlRequest<'a> {
        request_with_dialect(input, Dialect::CommonLisp, path, name)
    }

    fn request_with_dialect<'a>(
        input: &'a str,
        dialect: Dialect,
        path: &str,
        name: &str,
    ) -> RemoveUnusedControlRequest<'a> {
        RemoveUnusedControlRequest {
            input,
            dialect,
            path: path.parse().expect("path"),
            name: name.to_owned(),
        }
    }

    #[test]
    fn removes_unused_block_with_common_lisp_reader_literal_outside_target() {
        let plan = plan_remove_unused_block(request(
            r"(if ok (block out (first) (second)) nil) #\)",
            "0.2",
            "out",
        ))
        .expect("plan");
        assert_eq!(plan.rewritten, r"(if ok (progn (first) (second)) nil) #\)");
        SyntaxTree::parse_with_dialect(&plan.rewritten, Dialect::CommonLisp)
            .expect("rewritten input");
    }

    #[test]
    fn ignores_shadowed_return_from() {
        plan_remove_unused_block(request(
            "(progn (block out (block out (return-from out 1))))",
            "0.1",
            "out",
        ))
        .unwrap();
    }

    #[test]
    fn rejects_referenced_block_and_unknown_context() {
        assert!(
            plan_remove_unused_block(request(
                "(progn (block out (return-from out 1)))",
                "0.1",
                "out"
            ))
            .is_err()
        );
        assert!(plan_remove_unused_block(request("(list (block out 1))", "0.1", "out")).is_err());
    }

    #[test]
    fn removes_symbol_and_integer_tags() {
        assert_eq!(
            plan_remove_unused_tag(request("(tagbody start (print 1))", "0", "start"))
                .unwrap()
                .rewritten,
            "(tagbody  (print 1))"
        );
        assert_eq!(
            plan_remove_unused_tag(request("(tagbody 01 (print 1))", "0", "1"))
                .unwrap()
                .rewritten,
            "(tagbody  (print 1))"
        );
    }

    #[test]
    fn removes_unused_tag_with_common_lisp_reader_literal_outside_target() {
        let plan = plan_remove_unused_tag(request(r"(tagbody start (print 1)) #\)", "0", "start"))
            .expect("plan");
        assert_eq!(plan.rewritten, r"(tagbody  (print 1)) #\)");
        SyntaxTree::parse_with_dialect(&plan.rewritten, Dialect::CommonLisp)
            .expect("rewritten input");
    }

    #[test]
    fn rejects_referenced_tag_but_ignores_nested_shadow() {
        assert!(plan_remove_unused_tag(request("(tagbody x (go x))", "0", "x")).is_err());
        plan_remove_unused_tag(request("(tagbody x (tagbody x (go x)))", "0", "x")).unwrap();
    }

    #[test]
    fn rejects_unsupported_dialects_before_parsing_for_both_operations() {
        for dialect in [
            Dialect::EmacsLisp,
            Dialect::Scheme,
            Dialect::Clojure,
            Dialect::Janet,
            Dialect::Fennel,
            Dialect::Unknown,
        ] {
            let block_error =
                plan_remove_unused_block(request_with_dialect(")", dialect, "0", "out"))
                    .expect_err("dialect must be rejected");
            assert_eq!(
                block_error.to_string(),
                "remove-unused-block supports only Common Lisp"
            );

            let tag_error =
                plan_remove_unused_tag(request_with_dialect(")", dialect, "0", "start"))
                    .expect_err("dialect must be rejected");
            assert_eq!(
                tag_error.to_string(),
                "remove-unused-tag supports only Common Lisp"
            );
        }
    }
}
