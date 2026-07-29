//! Domain planning for Common Lisp conditional-sugar conversions.

use paredit_core_edit::{ConservativeRefusal, DialectRefusal, DocumentRefusal, ShapeRefusal};

use crate::error::{ConditionalConversionResult, ConditionalShapeError};

use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, Path, SyntaxTree};

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

pub fn require_supported_dialect(dialect: Dialect) -> ConditionalConversionResult<()> {
    if !matches!(dialect, Dialect::CommonLisp | Dialect::EmacsLisp) {
        return Err(DialectRefusal::CommonLispAndEmacsLisp {
            operation: "conditional conversion",
        }
        .into());
    }
    Ok(())
}

fn prepare<'a>(
    request: &ConditionalConversionRequest<'a>,
    head: &str,
) -> ConditionalConversionResult<(SyntaxTree, ExpressionView)> {
    require_supported_dialect(request.dialect)?;
    let tree =
        SyntaxTree::parse_with_dialect(request.input, request.dialect).map_err(|source| {
            DocumentRefusal::InputNotAnSexprDocument {
                operation: "conditional conversion",
                source,
            }
        })?;
    let form = tree.select_path(&request.path)?.view();
    if tree.has_comment_in(form.span) {
        return Err(ConservativeRefusal::Comments {
            operation: "conditional conversion",
        }
        .into());
    }
    if form.kind != ExpressionKind::List || !form.reader_prefixes.is_empty() {
        return Err(ShapeRefusal::UnnamedRoleNotPlainForm {
            role: "selected form".to_owned(),
            expected: head.to_owned(),
        }
        .into());
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
        return Err(ShapeRefusal::UnnamedRoleNotExpectedForm {
            role: "selected form".to_owned(),
            expected: head.to_owned(),
        }
        .into());
    }
    Ok((tree, form))
}

fn finish(
    request: ConditionalConversionRequest<'_>,
    form: &ExpressionView,
    body_count: usize,
    replacement: String,
) -> ConditionalConversionResult<ConditionalConversionPlan> {
    let rewritten = replace_span(request.input, form.span, &replacement);
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect).map_err(|source| {
        DocumentRefusal::OutputNotAnSexprDocument {
            operation: "conditional conversion",
            source,
        }
    })?;
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
) -> ConditionalConversionResult<ConditionalConversionPlan> {
    let (_tree, form) = prepare(&request, "when")?;
    if form.children.len() < 2 {
        return Err(ConditionalShapeError::WhenHasNoTest.into());
    }
    let test = form.children[1].span.slice(request.input);
    let body = sequenced_body(&form, request.input);
    finish(
        request,
        &form,
        form.children.len() - 2,
        format!("(if {test} {body})"),
    )
}

pub fn plan_convert_unless_to_if(
    request: ConditionalConversionRequest<'_>,
) -> ConditionalConversionResult<ConditionalConversionPlan> {
    let (_tree, form) = prepare(&request, "unless")?;
    if form.children.len() < 2 {
        return Err(ConditionalShapeError::UnlessHasNoTest.into());
    }
    let test = form.children[1].span.slice(request.input);
    let body = sequenced_body(&form, request.input);
    finish(
        request,
        &form,
        form.children.len() - 2,
        format!("(if {test} nil {body})"),
    )
}

/// Renders a `when`/`unless` body as the single form an `if` branch accepts.
///
/// `if` takes one form per branch and `when` takes a sequence, so a body of two
/// or more forms needs a `progn` to fit. A body of *one* form does not, and
/// wrapping it anyway had two costs worth naming:
///
/// - It generated code this tool's own `redundant-progn` rule reports. A
///   refactor whose output fails the linter in the same repository is telling
///   the user two different things.
/// - It made the conversion pair non-terminating under repeated use.
///   `if → when` keeps the `then` form verbatim, so `(if p (progn a))` became
///   `(when p (progn a))` became `(if p (progn (progn a)))`, gaining a level on
///   every round trip. An agent looping over conversions grew the file without
///   bound.
///
/// An empty body still yields `(progn)`, which is the only spelling of "no
/// forms" that an `if` branch can hold.
fn sequenced_body(form: &ExpressionView, input: &str) -> String {
    let body = &form.children[2..];
    match body {
        [single] => single.span.slice(input).to_owned(),
        _ => {
            let forms = body
                .iter()
                .map(|view| view.span.slice(input))
                .collect::<Vec<_>>()
                .join(" ");
            format!("(progn{}{forms})", if forms.is_empty() { "" } else { " " })
        }
    }
}

pub fn plan_convert_if_to_when(
    request: ConditionalConversionRequest<'_>,
) -> ConditionalConversionResult<ConditionalConversionPlan> {
    let (_tree, form) = prepare(&request, "if")?;
    if !(3..=4).contains(&form.children.len()) {
        return Err(ConditionalShapeError::IfIsNotWhenShaped.into());
    }
    if form.children.len() == 4 && !literal_nil(&form.children[3], request.dialect) {
        return Err(ConditionalShapeError::IfHasNonNilElse.into());
    }
    let test = form.children[1].span.slice(request.input);
    let then = form.children[2].span.slice(request.input);
    finish(request, &form, 1, format!("(when {test} {then})"))
}

pub fn plan_convert_if_to_unless(
    request: ConditionalConversionRequest<'_>,
) -> ConditionalConversionResult<ConditionalConversionPlan> {
    let (_tree, form) = prepare(&request, "if")?;
    if form.children.len() != 4 || !literal_nil(&form.children[2], request.dialect) {
        return Err(ConditionalShapeError::IfIsNotUnlessShaped.into());
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
