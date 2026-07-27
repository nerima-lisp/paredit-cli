//! Domain planning for Common Lisp conditional-sugar conversions.

use anyhow::{Context, Result, bail};

use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::reader::atom_symbol_text;
use crate::domain::sexpr::{ByteSpan, ExpressionKind, ExpressionView, Path, SyntaxTree};

#[derive(Debug, Clone)]
pub struct ConditionalConversionRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
}

#[derive(Debug, Clone)]
pub struct ConditionalConversionPlan {
    pub dialect: Dialect,
    pub path: Path,
    pub form_span: ByteSpan,
    pub body_count: usize,
    pub rewritten: String,
    pub changed: bool,
}

fn replace_span(input: &str, span: ByteSpan, replacement: &str) -> String {
    let mut output = String::with_capacity(input.len() + replacement.len());
    output.push_str(&input[..span.start().get()]);
    output.push_str(replacement);
    output.push_str(&input[span.end().get()..]);
    output
}

pub(crate) fn require_supported_dialect(dialect: Dialect) -> Result<()> {
    if !matches!(dialect, Dialect::CommonLisp | Dialect::EmacsLisp) {
        bail!("conditional conversion supports only Common Lisp and Emacs Lisp");
    }
    Ok(())
}

fn prepare<'a>(
    request: &ConditionalConversionRequest<'a>,
    head: &str,
) -> Result<(SyntaxTree, ExpressionView)> {
    require_supported_dialect(request.dialect)?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("conditional conversion input is not a valid S-expression document")?;
    let form = tree.select_path(&request.path)?.view();
    if tree.has_comment_in(form.span) {
        bail!("conditional conversion cannot rewrite a form containing comments");
    }
    if form.kind != ExpressionKind::List || !form.reader_prefixes.is_empty() {
        bail!("selected form must be a plain {head} form");
    }
    let matches = form
        .children
        .first()
        .filter(|view| view.reader_prefixes.is_empty())
        .and_then(atom_symbol_text)
        .is_some_and(|actual| match request.dialect {
            Dialect::CommonLisp => common_lisp_symbol_reference_eq(actual, head),
            Dialect::EmacsLisp => actual == head,
            _ => false,
        });
    if !matches {
        bail!("selected form must be a {head} form");
    }
    Ok((tree, form))
}

fn finish(
    request: ConditionalConversionRequest<'_>,
    form: &ExpressionView,
    body_count: usize,
    replacement: String,
) -> Result<ConditionalConversionPlan> {
    let rewritten = replace_span(request.input, form.span, &replacement);
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect)
        .context("conditional conversion output is not a valid S-expression document")?;
    Ok(ConditionalConversionPlan {
        dialect: request.dialect,
        path: request.path,
        form_span: form.span,
        body_count,
        changed: rewritten != request.input,
        rewritten,
    })
}

fn literal_nil(view: &ExpressionView, dialect: Dialect) -> bool {
    view.kind == ExpressionKind::Atom
        && view.reader_prefixes.is_empty()
        && atom_symbol_text(view).is_some_and(|text| match dialect {
            Dialect::CommonLisp => common_lisp_symbol_reference_eq(text, "nil"),
            Dialect::EmacsLisp => text == "nil",
            _ => false,
        })
}

pub fn plan_convert_when_to_if(
    request: ConditionalConversionRequest<'_>,
) -> Result<ConditionalConversionPlan> {
    let (_tree, form) = prepare(&request, "when")?;
    if form.children.len() < 2 {
        bail!("convert-when-to-if requires a test");
    }
    let test = form.children[1].span.slice(request.input);
    let body = form.children[2..]
        .iter()
        .map(|view| view.span.slice(request.input))
        .collect::<Vec<_>>()
        .join(" ");
    finish(
        request,
        &form,
        form.children.len() - 2,
        format!(
            "(if {test} (progn{}{body}))",
            if body.is_empty() { "" } else { " " }
        ),
    )
}

pub fn plan_convert_unless_to_if(
    request: ConditionalConversionRequest<'_>,
) -> Result<ConditionalConversionPlan> {
    let (_tree, form) = prepare(&request, "unless")?;
    if form.children.len() < 2 {
        bail!("convert-unless-to-if requires a test");
    }
    let test = form.children[1].span.slice(request.input);
    let body = form.children[2..]
        .iter()
        .map(|view| view.span.slice(request.input))
        .collect::<Vec<_>>()
        .join(" ");
    finish(
        request,
        &form,
        form.children.len() - 2,
        format!(
            "(if {test} nil (progn{}{body}))",
            if body.is_empty() { "" } else { " " }
        ),
    )
}

pub fn plan_convert_if_to_when(
    request: ConditionalConversionRequest<'_>,
) -> Result<ConditionalConversionPlan> {
    let (_tree, form) = prepare(&request, "if")?;
    if !(3..=4).contains(&form.children.len()) {
        bail!("convert-if-to-when requires (if test then [nil])");
    }
    if form.children.len() == 4 && !literal_nil(&form.children[3], request.dialect) {
        bail!("convert-if-to-when requires no else form or a literal nil else");
    }
    let test = form.children[1].span.slice(request.input);
    let then = form.children[2].span.slice(request.input);
    finish(request, &form, 1, format!("(when {test} {then})"))
}

pub fn plan_convert_if_to_unless(
    request: ConditionalConversionRequest<'_>,
) -> Result<ConditionalConversionPlan> {
    let (_tree, form) = prepare(&request, "if")?;
    if form.children.len() != 4 || !literal_nil(&form.children[2], request.dialect) {
        bail!("convert-if-to-unless requires (if test nil else)");
    }
    let test = form.children[1].span.slice(request.input);
    let otherwise = form.children[3].span.slice(request.input);
    finish(request, &form, 1, format!("(unless {test} {otherwise})"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(input: &str, dialect: Dialect) -> ConditionalConversionRequest<'_> {
        ConditionalConversionRequest {
            input,
            dialect,
            path: "0".parse().expect("path"),
        }
    }

    #[test]
    fn supported_dialects_preserve_reader_forms_and_validate_with_the_same_dialect() {
        for (dialect, input) in [
            (Dialect::CommonLisp, "(when ok one two) #\\)"),
            (Dialect::EmacsLisp, "(when ok one two) ?\\)"),
        ] {
            let plan = plan_convert_when_to_if(request(input, dialect)).unwrap();
            assert!(plan.changed);
            assert_eq!(plan.body_count, 2);
            SyntaxTree::parse_with_dialect(&plan.rewritten, dialect).unwrap();
        }
    }

    #[test]
    fn unsupported_dialects_fail_before_parsing_input() {
        for dialect in [
            Dialect::Scheme,
            Dialect::Clojure,
            Dialect::Janet,
            Dialect::Fennel,
            Dialect::Unknown,
        ] {
            let error = plan_convert_when_to_if(request(")", dialect)).unwrap_err();
            assert_eq!(
                error.to_string(),
                "conditional conversion supports only Common Lisp and Emacs Lisp"
            );
        }
    }

    #[test]
    fn rejects_malformed_or_ambiguous_forms() {
        assert!(plan_convert_when_to_if(request("(when)", Dialect::CommonLisp)).is_err());
        assert!(plan_convert_if_to_when(request("(if x y z)", Dialect::CommonLisp)).is_err());
        assert!(plan_convert_if_to_unless(request("(if x y z)", Dialect::EmacsLisp)).is_err());
        assert!(plan_convert_when_to_if(request("(when x ; c\n y)", Dialect::CommonLisp)).is_err());
        assert!(plan_convert_if_to_when(request("'(if x y)", Dialect::EmacsLisp)).is_err());
    }
}
