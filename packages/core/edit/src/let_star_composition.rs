//! Pure planning rules for splitting sequential `let*` bindings.

use crate::domain::binding_index::BindingIndex;
use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::reader::atom_symbol_text;
use crate::domain::sexpr::{ByteSpan, ExpressionKind, ExpressionView, Path, SyntaxTree};
use anyhow::{Context, Result, bail};

#[derive(Debug, Clone)]
pub(crate) struct Request<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
    pub binding_index: BindingIndex,
}
#[derive(Debug, Clone)]
pub(crate) struct Plan {
    pub dialect: Dialect,
    pub path: Path,
    pub form_span: ByteSpan,
    pub binding_index: BindingIndex,
    pub outer_binding_count: usize,
    pub inner_binding_count: usize,
    pub rewritten: String,
    pub changed: bool,
}

pub(crate) fn plan(request: Request<'_>) -> Result<Plan> {
    validate_dialect(request.dialect)?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("split-let-star input is not a valid S-expression document")?;
    let form = tree.select_path(&request.path)?.view();
    require_let_star(request.dialect, &form)?;
    if tree.has_comment_in(form.span) || contains_reader_prefix(&form) {
        bail!("split-let-star conservatively rejects comments or reader prefixes");
    }
    if contains_headed_form(request.dialect, &form, "declare") {
        bail!("split-let-star conservatively rejects declarations");
    }
    if form.children.len() < 3 {
        bail!("split-let-star requires a body");
    }
    let bindings = form
        .children
        .get(1)
        .context("split-let-star requires a binding list")?;
    if bindings.kind != ExpressionKind::List || !bindings.reader_prefixes.is_empty() {
        bail!("split-let-star requires a plain binding list");
    }
    let binding_index = request.binding_index.get();
    if binding_index >= bindings.children.len() {
        bail!(
            "split-let-star --binding-index must be between 1 and {}",
            bindings.children.len().saturating_sub(1)
        );
    }
    let head = form.children[0].span.slice(request.input);
    let outer = bindings.children[..binding_index]
        .iter()
        .map(|b| b.span.slice(request.input))
        .collect::<Vec<_>>()
        .join(" ");
    let inner = bindings.children[binding_index..]
        .iter()
        .map(|b| b.span.slice(request.input))
        .collect::<Vec<_>>()
        .join(" ");
    let body = &request.input[bindings.span.end().get()..form.span.end().get() - 1];
    let replacement = format!("({head} ({outer}) ({head} ({inner}){body}))");
    let rewritten = replace_span(request.input, form.span, &replacement);
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect)
        .context("split-let-star output is not valid")?;
    Ok(Plan {
        dialect: request.dialect,
        path: request.path,
        form_span: form.span,
        binding_index: request.binding_index,
        outer_binding_count: binding_index,
        inner_binding_count: bindings.children.len() - binding_index,
        changed: rewritten != request.input,
        rewritten,
    })
}

pub(crate) fn validate_dialect(dialect: Dialect) -> Result<()> {
    if !matches!(dialect, Dialect::CommonLisp | Dialect::EmacsLisp) {
        bail!("split-let-star supports only Common Lisp and Emacs Lisp");
    }
    Ok(())
}

fn replace_span(input: &str, span: ByteSpan, replacement: &str) -> String {
    format!(
        "{}{}{}",
        &input[..span.start().get()],
        replacement,
        &input[span.end().get()..]
    )
}
fn require_let_star(dialect: Dialect, form: &ExpressionView) -> Result<()> {
    let matches = form.kind == ExpressionKind::List
        && form.reader_prefixes.is_empty()
        && form
            .children
            .first()
            .and_then(atom_symbol_text)
            .is_some_and(|head| {
                if dialect == Dialect::CommonLisp {
                    common_lisp_symbol_reference_eq(head, "let*")
                } else {
                    head == "let*"
                }
            });
    if !matches {
        bail!("split-let-star selected form must be a plain let* form");
    }
    Ok(())
}
fn contains_reader_prefix(view: &ExpressionView) -> bool {
    !view.reader_prefixes.is_empty() || view.children.iter().any(contains_reader_prefix)
}
fn contains_headed_form(dialect: Dialect, view: &ExpressionView, expected: &str) -> bool {
    (view.kind == ExpressionKind::List
        && view
            .children
            .first()
            .and_then(atom_symbol_text)
            .is_some_and(|head| {
                if dialect == Dialect::CommonLisp {
                    common_lisp_symbol_reference_eq(head, expected)
                } else {
                    head == expected
                }
            }))
        || view
            .children
            .iter()
            .any(|child| contains_headed_form(dialect, child, expected))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_in_both_dialects_with_reader_literals_outside_target() {
        for (dialect, reader_literal) in
            [(Dialect::CommonLisp, r"#\)"), (Dialect::EmacsLisp, r"?\)")]
        {
            let input =
                format!("(let* ((a 1) (b (+ a 1)) (c (+ b 1))) (+ a b c)) {reader_literal}");
            let plan = plan(Request {
                input: &input,
                dialect,
                path: "0".parse().expect("path"),
                binding_index: BindingIndex::new(1).expect("binding index"),
            })
            .expect("plan");
            assert_eq!(
                plan.rewritten,
                format!(
                    "(let* ((a 1)) (let* ((b (+ a 1)) (c (+ b 1))) (+ a b c))) {reader_literal}"
                )
            );
            SyntaxTree::parse_with_dialect(&plan.rewritten, dialect).expect("rewritten input");
        }
    }

    #[test]
    fn rejects_unsupported_dialects_before_parsing() {
        for dialect in [
            Dialect::Scheme,
            Dialect::Clojure,
            Dialect::Janet,
            Dialect::Fennel,
            Dialect::Unknown,
        ] {
            let error = plan(Request {
                input: ")",
                dialect,
                path: "0".parse().expect("path"),
                binding_index: BindingIndex::new(1).expect("binding index"),
            })
            .expect_err("dialect must be rejected");
            assert_eq!(
                error.to_string(),
                "split-let-star supports only Common Lisp and Emacs Lisp"
            );
        }
    }

    #[test]
    fn rejects_invalid_boundaries_and_declarations() {
        assert!(
            plan(Request {
                input: "(let* ((a 1) (b 2)) b)",
                dialect: Dialect::CommonLisp,
                path: "0".parse().unwrap(),
                binding_index: BindingIndex::new(2).expect("binding index")
            })
            .is_err()
        );
        assert!(
            plan(Request {
                input: "(let* ((a 1) (b 2)) (declare (special a)) b)",
                dialect: Dialect::CommonLisp,
                path: "0".parse().unwrap(),
                binding_index: BindingIndex::new(1).expect("binding index")
            })
            .is_err()
        );
    }
}
