//! Scope-aware Common Lisp `block` and `tagbody` control-name renames.

use paredit_core_edit::{ConservativeRefusal, DialectRefusal, DocumentRefusal, ShapeRefusal};

use crate::error::{RenameControlError, RenameResult};

use paredit_core_edit::extract_shared::replace_span;
use paredit_core_edit::mutation_safety::reject_common_lisp_reader_conditionals;
use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree,
};

#[derive(Debug, Clone)]
pub struct RenameControlRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
    pub from: SymbolName,
    pub to: SymbolName,
}

#[derive(Debug, Clone)]
pub struct RenameControlPlan {
    pub dialect: Dialect,
    pub path: Path,
    pub form_span: ByteSpan,
    pub reference_count: usize,
    pub rewritten: String,
    pub changed: bool,
}

pub fn plan_rename_block(request: RenameControlRequest<'_>) -> RenameResult<RenameControlPlan> {
    plan(request, ControlKind::Block)
}

pub fn plan_rename_tag(request: RenameControlRequest<'_>) -> RenameResult<RenameControlPlan> {
    plan(request, ControlKind::Tag)
}

#[derive(Clone, Copy)]
enum ControlKind {
    Block,
    Tag,
}

fn plan(request: RenameControlRequest<'_>, kind: ControlKind) -> RenameResult<RenameControlPlan> {
    let operation = match kind {
        ControlKind::Block => "rename-block",
        ControlKind::Tag => "rename-tag",
    };
    if request.dialect != Dialect::CommonLisp {
        return Err(DialectRefusal::CommonLispOnly { operation }.into());
    }
    require_unqualified(request.from.as_str(), operation)?;
    require_unqualified(request.to.as_str(), operation)?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .map_err(|source| DocumentRefusal::UnnamedInputNotAnSexprDocument { source })?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    let form = tree.select_path(&request.path)?.view();
    if tree.has_comment_in(form.span) {
        return Err(ConservativeRefusal::Comments { operation }.into());
    }
    if contains_prefix(&form) || contains_quoted_form(&form) {
        return Err(ConservativeRefusal::CannotAnalyzeReaderPrefixedOrQuoted { operation }.into());
    }

    let mut edits = Vec::new();
    match kind {
        ControlKind::Block => collect_block(
            &form,
            request.from.as_str(),
            request.to.as_str(),
            &mut edits,
        )?,
        ControlKind::Tag => collect_tagbody(
            &form,
            request.from.as_str(),
            request.to.as_str(),
            &mut edits,
        )?,
    }
    let references = edits.len().saturating_sub(1);
    edits.sort_by_key(|span| std::cmp::Reverse(span.start().get()));
    let mut rewritten = request.input.to_owned();
    for span in edits {
        rewritten = replace_span(&rewritten, span, request.to.as_str());
    }
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect).map_err(|source| {
        DocumentRefusal::OutputNotAnSexprDocument {
            operation: "renamed",
            source,
        }
    })?;
    Ok(RenameControlPlan {
        dialect: request.dialect,
        path: request.path,
        form_span: form.span,
        reference_count: references,
        changed: rewritten != request.input,
        rewritten,
    })
}

fn collect_block(
    form: &ExpressionView,
    from: &str,
    to: &str,
    edits: &mut Vec<ByteSpan>,
) -> RenameResult<()> {
    require_head(form, "block", "rename-block")?;
    let name = form
        .children
        .get(1)
        .and_then(plain_atom)
        .ok_or(RenameControlError::BlockNameNotPlain)?;
    require_unqualified(name, "rename-block")?;
    if !eq(name, from) {
        return Err(RenameControlError::BlockNameMismatch.into());
    }
    edits.push(form.children[1].span);
    for child in form.children.iter().skip(2) {
        walk_block(child, from, to, edits)?;
    }
    Ok(())
}

fn walk_block(
    view: &ExpressionView,
    from: &str,
    to: &str,
    edits: &mut Vec<ByteSpan>,
) -> RenameResult<()> {
    if view.kind == ExpressionKind::List {
        if head_is(view, "block") {
            let name = view
                .children
                .get(1)
                .and_then(plain_atom)
                .ok_or(RenameControlError::MalformedNestedBlock)?;
            require_unqualified(name, "rename-block")?;
            if eq(name, to) && !eq(from, to) {
                return Err(RenameControlError::BlockCollides.into());
            }
            if eq(name, from) {
                return Ok(());
            }
            for child in view.children.iter().skip(2) {
                walk_block(child, from, to, edits)?;
            }
            return Ok(());
        }
        if head_is(view, "return-from") {
            let name = view
                .children
                .get(1)
                .and_then(plain_atom)
                .ok_or(RenameControlError::MalformedReturnFrom)?;
            require_unqualified(name, "rename-block")?;
            if eq(name, to) && !eq(from, to) {
                return Err(RenameControlError::BlockCaptures.into());
            }
            if eq(name, from) {
                edits.push(view.children[1].span);
            }
        }
    }
    for child in &view.children {
        walk_block(child, from, to, edits)?;
    }
    Ok(())
}

fn collect_tagbody(
    form: &ExpressionView,
    from: &str,
    to: &str,
    edits: &mut Vec<ByteSpan>,
) -> RenameResult<()> {
    require_head(form, "tagbody", "rename-tag")?;
    let tags = direct_tags(form);
    let matches: Vec<_> = tags
        .iter()
        .filter(|tag| plain_atom(tag).is_some_and(|name| eq(name, from)))
        .collect();
    if matches.len() != 1 {
        return Err(RenameControlError::TagNotUnique.into());
    }
    if !eq(from, to)
        && tags
            .iter()
            .any(|tag| plain_atom(tag).is_some_and(|name| eq(name, to)))
    {
        return Err(RenameControlError::TagDuplicates.into());
    }
    edits.push(matches[0].span);
    for child in form
        .children
        .iter()
        .skip(1)
        .filter(|v| v.kind == ExpressionKind::List)
    {
        walk_tag(child, from, to, true, edits)?;
    }
    Ok(())
}

fn walk_tag(
    view: &ExpressionView,
    from: &str,
    to: &str,
    rename_enabled: bool,
    edits: &mut Vec<ByteSpan>,
) -> RenameResult<()> {
    if view.kind == ExpressionKind::List {
        if head_is(view, "tagbody") {
            let tags = direct_tags(view);
            if !eq(from, to)
                && tags
                    .iter()
                    .any(|tag| plain_atom(tag).is_some_and(|name| eq(name, to)))
            {
                return Err(RenameControlError::TagCollides.into());
            }
            let shadows = tags
                .iter()
                .any(|tag| plain_atom(tag).is_some_and(|name| eq(name, from)));
            for child in view
                .children
                .iter()
                .skip(1)
                .filter(|v| v.kind == ExpressionKind::List)
            {
                walk_tag(child, from, to, rename_enabled && !shadows, edits)?;
            }
            return Ok(());
        }
        if head_is(view, "go") {
            let name = view
                .children
                .get(1)
                .and_then(plain_atom)
                .ok_or(RenameControlError::MalformedGo)?;
            require_unqualified(name, "rename-tag")?;
            if eq(name, to) && !eq(from, to) {
                return Err(RenameControlError::TagCaptures.into());
            }
            if rename_enabled && eq(name, from) {
                edits.push(view.children[1].span);
            }
        }
    }
    for child in &view.children {
        walk_tag(child, from, to, rename_enabled, edits)?;
    }
    Ok(())
}

fn direct_tags(form: &ExpressionView) -> Vec<&ExpressionView> {
    form.children
        .iter()
        .skip(1)
        .filter(|v| plain_atom(v).is_some())
        .collect()
}
fn require_head<'a>(
    form: &'a ExpressionView,
    expected: &str,
    op: &'static str,
) -> RenameResult<&'a str> {
    if form.kind != ExpressionKind::List {
        return Err(ShapeRefusal::NotExpectedForm {
            operation: op,
            expected: expected.to_owned(),
        }
        .into());
    }
    let head = form
        .children
        .first()
        .and_then(plain_atom)
        .ok_or(RenameControlError::HeadNotPlain)?;
    require_unqualified(head, op)?;
    if !eq(head, expected) {
        return Err(ShapeRefusal::NotExpectedForm {
            operation: op,
            expected: expected.to_owned(),
        }
        .into());
    }
    Ok(head)
}
fn plain_atom(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom && view.reader_prefixes.is_empty())
        .then(|| atom_symbol_text(view))
        .flatten()
}
fn head_is(view: &ExpressionView, name: &str) -> bool {
    view.children
        .first()
        .and_then(plain_atom)
        .is_some_and(|head| eq(head, name))
}
fn eq(left: &str, right: &str) -> bool {
    common_lisp_symbol_reference_eq(left, right)
}
fn require_unqualified(name: &str, op: &'static str) -> RenameResult<()> {
    if name.contains(':') {
        return Err(RenameControlError::NotUnqualified { operation: op }.into());
    }
    Ok(())
}
fn contains_prefix(view: &ExpressionView) -> bool {
    !view.reader_prefixes.is_empty() || view.children.iter().any(contains_prefix)
}
fn contains_quoted_form(view: &ExpressionView) -> bool {
    (view.kind == ExpressionKind::List
        && view
            .children
            .first()
            .and_then(plain_atom)
            .is_some_and(|h| eq(h, "quote") || eq(h, "quasiquote")))
        || view.children.iter().any(contains_quoted_form)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIALECTS: [Dialect; 7] = [
        Dialect::CommonLisp,
        Dialect::EmacsLisp,
        Dialect::Scheme,
        Dialect::Clojure,
        Dialect::Janet,
        Dialect::Fennel,
        Dialect::Unknown,
    ];

    fn req_for_dialect<'a>(
        input: &'a str,
        dialect: Dialect,
        from: &str,
        to: &str,
    ) -> RenameControlRequest<'a> {
        RenameControlRequest {
            input,
            dialect,
            path: "0".parse().unwrap(),
            from: from.parse().unwrap(),
            to: to.parse().unwrap(),
        }
    }

    fn req<'a>(input: &'a str, from: &str, to: &str) -> RenameControlRequest<'a> {
        req_for_dialect(input, Dialect::CommonLisp, from, to)
    }

    fn assert_support_error(result: RenameResult<RenameControlPlan>, operation: &'static str) {
        let error = result.expect_err("unsupported dialect must fail");
        assert_eq!(
            error.to_string(),
            format!("{operation} supports only Common Lisp")
        );
    }

    #[test]
    fn renames_block_references_but_not_shadowed_ones() {
        let p = plan_rename_block(req(
            "(block out (return-from out 1) (block out (return-from out 2)))",
            "out",
            "done",
        ))
        .unwrap();
        assert_eq!(
            p.rewritten,
            "(block done (return-from done 1) (block out (return-from out 2)))"
        );
    }

    #[test]
    fn rejects_block_capture() {
        assert!(plan_rename_block(req("(block out (return-from done 1))", "out", "done")).is_err());
    }

    #[test]
    fn renames_tag_and_go_but_not_shadowed_go() {
        let p = plan_rename_tag(req(
            "(tagbody start (go start) (tagbody start (go start)))",
            "start",
            "next",
        ))
        .unwrap();
        assert_eq!(
            p.rewritten,
            "(tagbody next (go next) (tagbody start (go start)))"
        );
    }

    #[test]
    fn rejects_duplicate_tags() {
        assert!(plan_rename_tag(req("(tagbody x x (go x))", "x", "y")).is_err());
    }

    #[test]
    fn support_matrix_is_common_lisp_only_for_both_operations() {
        for dialect in DIALECTS {
            let block = plan_rename_block(req_for_dialect(
                "(block out (return-from out 1))",
                dialect,
                "out",
                "done",
            ));
            let tag = plan_rename_tag(req_for_dialect(
                "(tagbody start (go start))",
                dialect,
                "start",
                "next",
            ));
            if dialect == Dialect::CommonLisp {
                assert!(block.is_ok(), "rename-block must support {dialect:?}");
                assert!(tag.is_ok(), "rename-tag must support {dialect:?}");
            } else {
                assert_support_error(block, "rename-block");
                assert_support_error(tag, "rename-tag");
            }
        }
    }

    #[test]
    fn unsupported_dialect_gate_precedes_parsing_for_both_operations() {
        for dialect in DIALECTS
            .into_iter()
            .filter(|dialect| *dialect != Dialect::CommonLisp)
        {
            assert_support_error(
                plan_rename_block(req_for_dialect(")", dialect, "out", "done")),
                "rename-block",
            );
            assert_support_error(
                plan_rename_tag(req_for_dialect(")", dialect, "start", "next")),
                "rename-tag",
            );
        }
    }

    #[test]
    fn preserves_common_lisp_delimiter_character_literals() {
        let plan = plan_rename_block(req(
            "(block out #\\) (return-from out #\\)))",
            "out",
            "done",
        ))
        .expect("rename block containing character literals");
        assert_eq!(plan.rewritten, "(block done #\\) (return-from done #\\)))");
        SyntaxTree::parse_with_dialect(&plan.rewritten, Dialect::CommonLisp)
            .expect("rewritten output must parse with the request dialect");
    }
}
