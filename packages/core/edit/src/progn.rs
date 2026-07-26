//! Pure `progn` transformations used by the application safety facade.

use anyhow::{Context, Result, bail};

use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::reader::atom_symbol_text;
use crate::domain::sexpr::{ByteSpan, ExpressionKind, ExpressionView, Path, SyntaxTree};

#[derive(Debug, Clone)]
pub struct FlattenPrognRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
}
#[derive(Debug, Clone)]
pub struct FlattenPrognPlan {
    pub dialect: Dialect,
    pub path: Path,
    pub form_span: ByteSpan,
    pub nested_count: usize,
    pub result_form_count: usize,
    pub replacement: String,
    pub rewritten: String,
    pub changed: bool,
}

pub fn plan_flatten_progn(request: FlattenPrognRequest<'_>) -> Result<FlattenPrognPlan> {
    require_supported(request.dialect, "flatten-progn")?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("flatten-progn input is not valid")?;
    let form = tree.select_path(&request.path)?.view();
    require_head(request.dialect, &form, "progn", "flatten-progn")?;
    if tree.has_comment_in(form.span) || contains_prefix(&form) {
        bail!("flatten-progn cannot rewrite comments or reader prefixes");
    }
    if contains_headed(request.dialect, &form, "declare") {
        bail!("flatten-progn rejects declarations");
    }
    let body = &form.children[1..];
    let nested_count = body
        .iter()
        .filter(|child| is_head(request.dialect, child, "progn"))
        .count();
    let flattened: Vec<&ExpressionView> = body
        .iter()
        .flat_map(|child| {
            if is_head(request.dialect, child, "progn") {
                child.children[1..].iter().collect()
            } else {
                vec![child]
            }
        })
        .collect();
    let replacement = match flattened.as_slice() {
        [] => "nil".to_owned(),
        [only] => source(request.input, only).to_owned(),
        forms => format!(
            "({} {})",
            source(request.input, &form.children[0]),
            forms
                .iter()
                .map(|child| source(request.input, child))
                .collect::<Vec<_>>()
                .join(" ")
        ),
    };
    let rewritten = replace_span(request.input, form.span, &replacement);
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect)
        .context("flatten-progn output is not valid")?;
    Ok(FlattenPrognPlan {
        dialect: request.dialect,
        path: request.path,
        form_span: form.span,
        nested_count,
        result_form_count: flattened.len(),
        changed: rewritten != request.input,
        replacement,
        rewritten,
    })
}

#[derive(Debug, Clone)]
pub struct EliminateEmptyBindingFormRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
}
#[derive(Debug, Clone)]
pub struct EliminateEmptyBindingFormPlan {
    pub dialect: Dialect,
    pub path: Path,
    pub form_span: ByteSpan,
    pub body_form_count: usize,
    pub introduced_progn: bool,
    pub rewritten: String,
    pub changed: bool,
}

pub fn plan_eliminate_empty_binding_form(
    request: EliminateEmptyBindingFormRequest<'_>,
) -> Result<EliminateEmptyBindingFormPlan> {
    require_supported(request.dialect, "eliminate-empty-binding-form")?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("eliminate-empty-binding-form input is not valid")?;
    let form = tree.select_path(&request.path)?.view();
    if form.kind != ExpressionKind::List
        || !form.reader_prefixes.is_empty()
        || form.children.len() < 2
    {
        bail!("eliminate-empty-binding-form requires a plain let or let* form");
    }
    let head = form
        .children
        .first()
        .and_then(atom_symbol_text)
        .context("missing binding form head")?;
    if !symbol_eq(request.dialect, head, "let") && !symbol_eq(request.dialect, head, "let*") {
        bail!("selected form is not let or let*");
    }
    if form.children[1].kind != ExpressionKind::List || !form.children[1].children.is_empty() {
        bail!("binding list must be empty");
    }
    if tree.has_comment_in(form.span)
        || contains_prefix(&form)
        || contains_headed(request.dialect, &form, "declare")
    {
        bail!("eliminate-empty-binding-form rejects comments, prefixes, or declarations");
    }
    let body = &form.children[2..];
    let replacement = match body {
        [] => "nil".to_owned(),
        [only] => source(request.input, only).to_owned(),
        many => format!(
            "(progn {})",
            many.iter()
                .map(|child| source(request.input, child))
                .collect::<Vec<_>>()
                .join(" ")
        ),
    };
    let rewritten = replace_span(request.input, form.span, &replacement);
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect)
        .context("eliminate-empty-binding-form output is not valid")?;
    Ok(EliminateEmptyBindingFormPlan {
        dialect: request.dialect,
        path: request.path,
        form_span: form.span,
        body_form_count: body.len(),
        introduced_progn: body.len() > 1,
        changed: rewritten != request.input,
        rewritten,
    })
}

pub(crate) fn require_supported(dialect: Dialect, operation: &str) -> Result<()> {
    if matches!(dialect, Dialect::CommonLisp | Dialect::EmacsLisp) {
        Ok(())
    } else {
        bail!("{operation} supports only Common Lisp and Emacs Lisp")
    }
}
fn symbol_eq(dialect: Dialect, left: &str, right: &str) -> bool {
    dialect == Dialect::CommonLisp && common_lisp_symbol_reference_eq(left, right)
        || dialect == Dialect::EmacsLisp && left == right
}
fn is_head(dialect: Dialect, view: &ExpressionView, expected: &str) -> bool {
    view.kind == ExpressionKind::List
        && view.reader_prefixes.is_empty()
        && view
            .children
            .first()
            .and_then(atom_symbol_text)
            .is_some_and(|head| symbol_eq(dialect, head, expected))
}
fn require_head(
    dialect: Dialect,
    view: &ExpressionView,
    expected: &str,
    operation: &str,
) -> Result<()> {
    if is_head(dialect, view, expected) {
        Ok(())
    } else {
        bail!("{operation} selected form must be a plain {expected}")
    }
}
fn contains_prefix(view: &ExpressionView) -> bool {
    !view.reader_prefixes.is_empty() || view.children.iter().any(contains_prefix)
}
fn contains_headed(dialect: Dialect, view: &ExpressionView, expected: &str) -> bool {
    is_head(dialect, view, expected)
        || view
            .children
            .iter()
            .any(|child| contains_headed(dialect, child, expected))
}
fn source<'a>(input: &'a str, view: &ExpressionView) -> &'a str {
    &input[view.span.start().get()..view.span.end().get()]
}
fn replace_span(input: &str, span: ByteSpan, replacement: &str) -> String {
    let mut output = String::with_capacity(input.len() - span.len() + replacement.len());
    output.push_str(&input[..span.start().get()]);
    output.push_str(replacement);
    output.push_str(&input[span.end().get()..]);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatten_round_trips_across_dialects_with_reader_literals_outside_target() {
        for (dialect, reader_literal) in
            [(Dialect::CommonLisp, r"#\)"), (Dialect::EmacsLisp, r"?\)")]
        {
            let input = format!("(progn a (progn b c)) {reader_literal}");
            let plan = plan_flatten_progn(FlattenPrognRequest {
                input: &input,
                dialect,
                path: "0".parse().expect("path"),
            })
            .expect("plan");
            assert_eq!(plan.result_form_count, 3);
            assert_eq!(plan.rewritten, format!("(progn a b c) {reader_literal}"));
            SyntaxTree::parse_with_dialect(&plan.rewritten, dialect).expect("rewritten input");
        }
    }

    #[test]
    fn eliminate_round_trips_across_dialects_with_reader_literals_outside_target() {
        for (dialect, reader_literal) in
            [(Dialect::CommonLisp, r"#\)"), (Dialect::EmacsLisp, r"?\)")]
        {
            let input = format!("(let () a b) {reader_literal}");
            let plan = plan_eliminate_empty_binding_form(EliminateEmptyBindingFormRequest {
                input: &input,
                dialect,
                path: "0".parse().expect("path"),
            })
            .expect("plan");
            assert_eq!(plan.body_form_count, 2);
            assert!(plan.introduced_progn);
            assert_eq!(plan.rewritten, format!("(progn a b) {reader_literal}"));
            SyntaxTree::parse_with_dialect(&plan.rewritten, dialect).expect("rewritten input");
        }
    }

    #[test]
    fn rejects_unsupported_dialects_before_parsing_for_both_operations() {
        for dialect in [
            Dialect::Scheme,
            Dialect::Clojure,
            Dialect::Janet,
            Dialect::Fennel,
            Dialect::Unknown,
        ] {
            let flatten_error = plan_flatten_progn(FlattenPrognRequest {
                input: ")",
                dialect,
                path: "0".parse().expect("path"),
            })
            .expect_err("dialect must be rejected");
            assert_eq!(
                flatten_error.to_string(),
                "flatten-progn supports only Common Lisp and Emacs Lisp"
            );

            let eliminate_error =
                plan_eliminate_empty_binding_form(EliminateEmptyBindingFormRequest {
                    input: ")",
                    dialect,
                    path: "0".parse().expect("path"),
                })
                .expect_err("dialect must be rejected");
            assert_eq!(
                eliminate_error.to_string(),
                "eliminate-empty-binding-form supports only Common Lisp and Emacs Lisp"
            );
        }
    }

    #[test]
    fn rejects_unsupported_forms() {
        assert!(
            plan_flatten_progn(FlattenPrognRequest {
                input: "(list (quote x))",
                dialect: Dialect::CommonLisp,
                path: "0.1".parse().unwrap()
            })
            .is_err()
        );
        assert!(
            plan_eliminate_empty_binding_form(EliminateEmptyBindingFormRequest {
                input: "(let ((x 1)) x)",
                dialect: Dialect::CommonLisp,
                path: "0".parse().unwrap()
            })
            .is_err()
        );
    }
}
