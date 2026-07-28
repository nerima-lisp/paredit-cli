//! Semantics-preserving composition and decomposition of parallel/sequential `let` forms.

use crate::error::{
    BindingRefusal, ConservativeRefusal, DialectRefusal, DocumentRefusal, EditResult, ShapeRefusal,
};

use paredit_core_semantics::binding_index::BindingIndex;
use paredit_core_semantics::lexical_scope::collect_unshadowed_symbol_references;
use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree,
};

#[derive(Debug, Clone)]
pub struct MergeNestedLetRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
}
#[derive(Debug, Clone)]
pub struct MergeNestedLetStarRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
}
#[derive(Debug, Clone)]
pub struct SplitLetRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
    pub binding_index: BindingIndex,
}

#[derive(Debug, Clone)]
pub struct LetCompositionPlan {
    pub dialect: Dialect,
    pub path: Path,
    pub form_span: ByteSpan,
    pub outer_binding_count: usize,
    pub inner_binding_count: usize,
    pub rewritten: String,
    pub changed: bool,
}

#[derive(Debug, Clone)]
pub struct SplitLetPlan {
    pub dialect: Dialect,
    pub path: Path,
    pub form_span: ByteSpan,
    pub outer_binding_count: usize,
    pub inner_binding_count: usize,
    pub binding_index: BindingIndex,
    pub rewritten: String,
    pub changed: bool,
}

pub fn plan_merge_nested_let(request: MergeNestedLetRequest<'_>) -> EditResult<LetCompositionPlan> {
    let (tree, outer) = select(
        request.input,
        request.dialect,
        &request.path,
        "merge-nested-let",
    )?;
    require_form(
        request.dialect,
        &outer,
        "let",
        "merge-nested-let selected form",
    )?;
    reject_unsafe(&tree, request.dialect, &outer, "merge-nested-let")?;
    if outer.children.len() != 3 {
        return Err(BindingRefusal::OuterBodyNotSingleForm {
            operation: "merge-nested-let",
        }
        .into());
    }
    let outer_bindings = binding_list(&outer.children[1], "merge-nested-let outer")?;
    let inner = &outer.children[2];
    require_form(request.dialect, inner, "let", "merge-nested-let outer body")?;
    if inner.children.len() < 3 {
        return Err(BindingRefusal::InnerHasNoBody {
            operation: "merge-nested-let",
            inner: "let",
        }
        .into());
    }
    let inner_bindings = binding_list(&inner.children[1], "merge-nested-let inner")?;
    let outer_parsed = parse_bindings(outer_bindings, "merge-nested-let")?;
    let inner_parsed = parse_bindings(inner_bindings, "merge-nested-let")?;
    let outer_names: Vec<_> = outer_parsed.iter().map(|(name, _)| name.clone()).collect();
    unique(request.dialect, &outer_names, "merge-nested-let")?;
    let mut names = outer_names.clone();
    for (name, initializer) in &inner_parsed {
        if names
            .iter()
            .any(|old| equal(request.dialect, old.as_str(), name.as_str()))
        {
            return Err(BindingRefusal::DuplicateBindingNames {
                operation: "merge-nested-let",
            }
            .into());
        }
        if let Some(initializer) = initializer {
            for outer_name in &outer_names {
                let mut refs = Vec::new();
                collect_unshadowed_symbol_references(
                    request.dialect,
                    initializer,
                    outer_name,
                    request.input,
                    &mut refs,
                );
                if !refs.is_empty() {
                    return Err(BindingRefusal::InnerReferencesOuterBinding {
                        name: name.to_string(),
                        outer_name: outer_name.to_string(),
                    }
                    .into());
                }
            }
        }
        names.push(name.clone());
    }
    let replacement =
        merge_replacement(request.input, &outer, outer_bindings, inner, inner_bindings);
    finish(
        request.input,
        request.dialect,
        request.path,
        outer.span,
        outer_bindings.children.len(),
        inner_bindings.children.len(),
        replacement,
        "merge-nested-let",
    )
}

pub fn plan_merge_nested_let_star(
    request: MergeNestedLetStarRequest<'_>,
) -> EditResult<LetCompositionPlan> {
    let (tree, outer) = select(
        request.input,
        request.dialect,
        &request.path,
        "merge-nested-let-star",
    )?;
    require_form(
        request.dialect,
        &outer,
        "let*",
        "merge-nested-let-star selected form",
    )?;
    reject_unsafe(&tree, request.dialect, &outer, "merge-nested-let-star")?;
    if outer.children.len() != 3 {
        return Err(BindingRefusal::OuterBodyNotSingleForm {
            operation: "merge-nested-let-star",
        }
        .into());
    }
    let outer_bindings = binding_list(&outer.children[1], "merge-nested-let-star outer")?;
    let inner = &outer.children[2];
    require_form(
        request.dialect,
        inner,
        "let*",
        "merge-nested-let-star outer body",
    )?;
    if inner.children.len() < 3 {
        return Err(BindingRefusal::InnerHasNoBody {
            operation: "merge-nested-let-star",
            inner: "let*",
        }
        .into());
    }
    let inner_bindings = binding_list(&inner.children[1], "merge-nested-let-star inner")?;
    let replacement =
        merge_replacement(request.input, &outer, outer_bindings, inner, inner_bindings);
    finish(
        request.input,
        request.dialect,
        request.path,
        outer.span,
        outer_bindings.children.len(),
        inner_bindings.children.len(),
        replacement,
        "merge-nested-let-star",
    )
}

pub fn plan_split_let(request: SplitLetRequest<'_>) -> EditResult<SplitLetPlan> {
    let (tree, form) = select(request.input, request.dialect, &request.path, "split-let")?;
    require_form(request.dialect, &form, "let", "split-let selected form")?;
    reject_unsafe(&tree, request.dialect, &form, "split-let")?;
    if form.children.len() < 3 {
        return Err(BindingRefusal::MissingBody {
            operation: "split-let",
        }
        .into());
    }
    let bindings = binding_list(&form.children[1], "split-let")?;
    let binding_index = request.binding_index.get();
    if binding_index >= bindings.children.len() {
        return Err(BindingRefusal::BindingIndexOutOfRange {
            operation: "split-let",
            maximum: bindings.children.len().saturating_sub(1),
        }
        .into());
    }
    let parsed = parse_bindings(bindings, "split-let")?;
    let outer_names: Vec<_> = parsed[..binding_index]
        .iter()
        .map(|(name, _)| name)
        .collect();
    for (name, initializer) in &parsed[binding_index..] {
        if let Some(initializer) = initializer {
            for outer_name in &outer_names {
                let mut refs = Vec::new();
                collect_unshadowed_symbol_references(
                    request.dialect,
                    initializer,
                    outer_name,
                    request.input,
                    &mut refs,
                );
                if !refs.is_empty() {
                    return Err(BindingRefusal::SplitWouldCapture {
                        outer_name: outer_name.to_string(),
                        name: name.to_string(),
                    }
                    .into());
                }
            }
        }
    }
    let head = form.children[0].span.slice(request.input);
    let outer = bindings.children[..binding_index]
        .iter()
        .map(|v| v.span.slice(request.input))
        .collect::<Vec<_>>()
        .join(" ");
    let inner = bindings.children[binding_index..]
        .iter()
        .map(|v| v.span.slice(request.input))
        .collect::<Vec<_>>()
        .join(" ");
    let body = &request.input[bindings.span.end().get()..form.span.end().get() - 1];
    let replacement = format!("({head} ({outer}) ({head} ({inner}){body}))");
    finish_split(
        request.input,
        request.dialect,
        request.path,
        form.span,
        request.binding_index.get(),
        bindings.children.len() - binding_index,
        request.binding_index,
        replacement,
        "split-let",
    )
}

fn select(
    input: &str,
    dialect: Dialect,
    path: &Path,
    operation: &'static str,
) -> EditResult<(SyntaxTree, ExpressionView)> {
    validate_dialect(dialect, operation)?;
    let tree = SyntaxTree::parse_with_dialect(input, dialect)
        .map_err(|source| DocumentRefusal::UnnamedInputNotAnSexprDocument { source })?;
    let view = tree.select_path(path)?.view();
    Ok((tree, view))
}

pub fn validate_dialect(dialect: Dialect, operation: &'static str) -> EditResult<()> {
    if !matches!(dialect, Dialect::CommonLisp | Dialect::EmacsLisp) {
        return Err(DialectRefusal::CommonLispAndEmacsLisp { operation }.into());
    }
    Ok(())
}
fn require_form(
    dialect: Dialect,
    view: &ExpressionView,
    expected: &str,
    message: &str,
) -> EditResult<()> {
    if view.kind != ExpressionKind::List
        || !view.reader_prefixes.is_empty()
        || !view
            .children
            .first()
            .and_then(atom_symbol_text)
            .is_some_and(|head| equal(dialect, head, expected))
    {
        return Err(ShapeRefusal::UnnamedRoleNotPlainForm {
            role: message.to_owned(),
            expected: expected.to_owned(),
        }
        .into());
    }
    Ok(())
}
fn binding_list<'a>(
    view: &'a ExpressionView,
    operation: &'static str,
) -> EditResult<&'a ExpressionView> {
    if view.kind != ExpressionKind::List || !view.reader_prefixes.is_empty() {
        return Err(BindingRefusal::NotPlainBindingList { operation }.into());
    }
    Ok(view)
}
fn parse_bindings(
    bindings: &ExpressionView,
    operation: &'static str,
) -> EditResult<Vec<(SymbolName, Option<ExpressionView>)>> {
    bindings
        .children
        .iter()
        .map(|binding| {
            if binding.kind == ExpressionKind::Atom {
                return Ok((plain_symbol(binding, operation)?, None));
            }
            if binding.kind == ExpressionKind::List
                && binding.reader_prefixes.is_empty()
                && (1..=2).contains(&binding.children.len())
            {
                return Ok((
                    plain_symbol(&binding.children[0], operation)?,
                    binding.children.get(1).cloned(),
                ));
            }
            Err(BindingRefusal::Destructuring { operation }.into())
        })
        .collect()
}
fn plain_symbol(view: &ExpressionView, operation: &'static str) -> EditResult<SymbolName> {
    if view.kind != ExpressionKind::Atom || !view.reader_prefixes.is_empty() {
        return Err(BindingRefusal::NotPlainBindingName { operation }.into());
    }
    SymbolName::new(atom_symbol_text(view).ok_or(BindingRefusal::BindingNameNotASymbol)?)
        .map_err(|source| BindingRefusal::InvalidBindingName { source }.into())
}
fn unique(dialect: Dialect, names: &[SymbolName], operation: &'static str) -> EditResult<()> {
    for (i, name) in names.iter().enumerate() {
        if names[..i]
            .iter()
            .any(|old| equal(dialect, old.as_str(), name.as_str()))
        {
            return Err(BindingRefusal::DuplicateBindingNames { operation }.into());
        }
    }
    Ok(())
}
fn equal(dialect: Dialect, left: &str, right: &str) -> bool {
    dialect != Dialect::CommonLisp && left == right
        || dialect == Dialect::CommonLisp && common_lisp_symbol_reference_eq(left, right)
}
fn reject_unsafe(
    tree: &SyntaxTree,
    dialect: Dialect,
    form: &ExpressionView,
    operation: &'static str,
) -> EditResult<()> {
    if tree.has_comment_in(form.span) {
        return Err(ConservativeRefusal::Comments { operation }.into());
    }
    if contains_prefix(form) {
        return Err(ConservativeRefusal::ReaderPrefixes { operation }.into());
    }
    if contains_headed(dialect, form, "declare") {
        return Err(ConservativeRefusal::Declarations { operation }.into());
    }
    Ok(())
}
fn contains_prefix(view: &ExpressionView) -> bool {
    !view.reader_prefixes.is_empty() || view.children.iter().any(contains_prefix)
}
fn contains_headed(dialect: Dialect, view: &ExpressionView, expected: &str) -> bool {
    (view.kind == ExpressionKind::List
        && view
            .children
            .first()
            .and_then(atom_symbol_text)
            .is_some_and(|head| equal(dialect, head, expected)))
        || view
            .children
            .iter()
            .any(|child| contains_headed(dialect, child, expected))
}
fn list_contents<'a>(input: &'a str, view: &ExpressionView) -> &'a str {
    &input[view.span.start().get() + 1..view.span.end().get() - 1]
}
fn merge_replacement(
    input: &str,
    outer: &ExpressionView,
    outer_bindings: &ExpressionView,
    inner: &ExpressionView,
    inner_bindings: &ExpressionView,
) -> String {
    let left = list_contents(input, outer_bindings);
    let right = list_contents(input, inner_bindings);
    let separator = if left.trim().is_empty() || right.trim().is_empty() {
        ""
    } else {
        " "
    };
    let head = outer.children[0].span.slice(input);
    let body = &input[inner_bindings.span.end().get()..inner.span.end().get() - 1];
    format!("({head} ({left}{separator}{right}){body})")
}
#[allow(clippy::too_many_arguments)]
fn finish(
    input: &str,
    dialect: Dialect,
    path: Path,
    span: ByteSpan,
    outer_count: usize,
    inner_count: usize,
    replacement: String,
    operation: &'static str,
) -> EditResult<LetCompositionPlan> {
    let rewritten = replace_span(input, span, &replacement);
    SyntaxTree::parse_with_dialect(&rewritten, dialect)
        .map_err(|source| DocumentRefusal::OutputInvalid { operation, source })?;
    Ok(LetCompositionPlan {
        dialect,
        path,
        form_span: span,
        outer_binding_count: outer_count,
        inner_binding_count: inner_count,
        changed: rewritten != input,
        rewritten,
    })
}

#[allow(clippy::too_many_arguments)]
fn finish_split(
    input: &str,
    dialect: Dialect,
    path: Path,
    span: ByteSpan,
    outer_count: usize,
    inner_count: usize,
    binding_index: BindingIndex,
    replacement: String,
    operation: &'static str,
) -> EditResult<SplitLetPlan> {
    let plan = finish(
        input,
        dialect,
        path,
        span,
        outer_count,
        inner_count,
        replacement,
        operation,
    )?;
    Ok(SplitLetPlan {
        dialect: plan.dialect,
        path: plan.path,
        form_span: plan.form_span,
        outer_binding_count: plan.outer_binding_count,
        inner_binding_count: plan.inner_binding_count,
        binding_index,
        rewritten: plan.rewritten,
        changed: plan.changed,
    })
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

    fn path() -> Path {
        "0".parse().expect("path")
    }

    #[test]
    fn merge_and_split_preserve_parseability_in_both_dialects() {
        for dialect in [Dialect::CommonLisp, Dialect::EmacsLisp] {
            let merged = plan_merge_nested_let(MergeNestedLetRequest {
                input: "(let ((x 1)) (let ((y 2)) (+ x y)))",
                dialect,
                path: path(),
            })
            .expect("merge");
            SyntaxTree::parse_with_dialect(&merged.rewritten, dialect).expect("merged output");
            let split = plan_split_let(SplitLetRequest {
                input: "(let ((x 1) (y 2)) (+ x y))",
                dialect,
                path: path(),
                binding_index: BindingIndex::new(1).expect("binding index"),
            })
            .expect("split");
            assert_eq!(split.binding_index.get(), 1);
            assert_eq!(split.outer_binding_count, 1);
            assert_eq!(split.inner_binding_count, 1);
            SyntaxTree::parse_with_dialect(&split.rewritten, dialect).expect("split output");
        }
    }

    #[test]
    fn split_rejects_initializer_capture_and_invalid_boundaries() {
        assert!(
            plan_split_let(SplitLetRequest {
                input: "(let ((x 1) (y (+ x 1))) y)",
                dialect: Dialect::CommonLisp,
                path: path(),
                binding_index: BindingIndex::new(1).expect("binding index")
            })
            .is_err()
        );
        assert!(
            plan_split_let(SplitLetRequest {
                input: "(let ((x 1) (y 2)) y)",
                dialect: Dialect::CommonLisp,
                path: path(),
                binding_index: BindingIndex::new(2).expect("binding index")
            })
            .is_err()
        );
    }

    #[test]
    fn merge_rejects_duplicate_names_comments_and_declarations() {
        assert!(
            plan_merge_nested_let(MergeNestedLetRequest {
                input: "(let ((x 1)) (let ((X 2)) x))",
                dialect: Dialect::CommonLisp,
                path: path()
            })
            .is_err()
        );
        assert!(
            plan_merge_nested_let(MergeNestedLetRequest {
                input: "(let ((x 1)) ; note\n (let ((y 2)) y))",
                dialect: Dialect::EmacsLisp,
                path: path()
            })
            .is_err()
        );
        assert!(
            plan_merge_nested_let_star(MergeNestedLetStarRequest {
                input: "(let* ((x 1)) (let* ((y 2)) (declare (special y)) y))",
                dialect: Dialect::CommonLisp,
                path: path()
            })
            .is_err()
        );
    }

    #[test]
    fn let_star_merge_preserves_sequential_initializers() {
        let plan = plan_merge_nested_let_star(MergeNestedLetStarRequest {
            input: "(let* ((x 1)) (let* ((y (+ x 1))) y))",
            dialect: Dialect::CommonLisp,
            path: path(),
        })
        .expect("let* merge");
        assert_eq!(plan.rewritten, "(let* ((x 1) (y (+ x 1))) y)");
    }

    #[test]
    fn supported_dialects_handle_reader_collisions_and_reparse_output() {
        for (dialect, prefix) in [(Dialect::CommonLisp, "#\\)"), (Dialect::EmacsLisp, "?\\)")] {
            let path: Path = "1".parse().expect("path");

            let merged = plan_merge_nested_let(MergeNestedLetRequest {
                input: &format!("{prefix} (let ((x 1)) (let ((y 2)) (+ x y)))"),
                dialect,
                path: path.clone(),
            })
            .expect("merge");
            SyntaxTree::parse_with_dialect(&merged.rewritten, dialect).expect("merged output");

            let merged_star = plan_merge_nested_let_star(MergeNestedLetStarRequest {
                input: &format!("{prefix} (let* ((x 1)) (let* ((y (+ x 1))) y))"),
                dialect,
                path: path.clone(),
            })
            .expect("let* merge");
            SyntaxTree::parse_with_dialect(&merged_star.rewritten, dialect)
                .expect("let* merged output");

            let split = plan_split_let(SplitLetRequest {
                input: &format!("{prefix} (let ((x 1) (y 2)) (+ x y))"),
                dialect,
                path,
                binding_index: BindingIndex::new(1).expect("binding index"),
            })
            .expect("split");
            SyntaxTree::parse_with_dialect(&split.rewritten, dialect).expect("split output");
        }
    }

    #[test]
    fn unsupported_dialects_are_rejected_before_parsing() {
        for dialect in [
            Dialect::Scheme,
            Dialect::Clojure,
            Dialect::Janet,
            Dialect::Fennel,
            Dialect::Unknown,
        ] {
            let merge_error = plan_merge_nested_let(MergeNestedLetRequest {
                input: ")",
                dialect,
                path: path(),
            })
            .expect_err("unsupported dialect");
            assert!(
                merge_error
                    .to_string()
                    .contains("supports only Common Lisp and Emacs Lisp")
            );

            let merge_star_error = plan_merge_nested_let_star(MergeNestedLetStarRequest {
                input: ")",
                dialect,
                path: path(),
            })
            .expect_err("unsupported dialect");
            assert!(
                merge_star_error
                    .to_string()
                    .contains("supports only Common Lisp and Emacs Lisp")
            );

            let split_error = plan_split_let(SplitLetRequest {
                input: ")",
                dialect,
                path: path(),
                binding_index: BindingIndex::new(1).expect("binding index"),
            })
            .expect_err("unsupported dialect");
            assert!(
                split_error
                    .to_string()
                    .contains("supports only Common Lisp and Emacs Lisp")
            );
        }
    }
}
