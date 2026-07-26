//! Semantics-preserving inlining of immediately invoked Common Lisp lambdas.

use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::reader::atom_symbol_text;
use crate::domain::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree,
};
use anyhow::{Context, Result, bail};

#[derive(Debug, Clone)]
pub(crate) struct Request<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
}
#[derive(Debug, Clone)]
pub(crate) struct Binding {
    pub name: SymbolName,
    pub argument: String,
}
#[derive(Debug, Clone)]
pub(crate) struct Plan {
    pub dialect: Dialect,
    pub path: Path,
    pub call_span: ByteSpan,
    pub lambda_span: ByteSpan,
    pub bindings: Vec<Binding>,
    pub replacement: String,
    pub rewritten: String,
    pub changed: bool,
}

pub(crate) fn plan(request: Request<'_>) -> Result<Plan> {
    validate_dialect(request.dialect)?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("inline-lambda input is not valid")?;
    let call = tree.select_path(&request.path)?.view();
    if tree.has_comment_in(call.span) {
        bail!("inline-lambda cannot replace a call containing comments");
    }
    if call.kind != ExpressionKind::List || !call.reader_prefixes.is_empty() {
        bail!("inline-lambda selected form must be a plain call list");
    }
    let lambda = call
        .children
        .first()
        .context("inline-lambda selected call has no operator")?;
    require_head(lambda, "lambda", "call operator must be a lambda form")?;
    if lambda.children.len() != 3 {
        bail!("inline-lambda requires exactly one lambda body expression");
    }
    let parameters = &lambda.children[1];
    if parameters.kind != ExpressionKind::List || !parameters.reader_prefixes.is_empty() {
        bail!("inline-lambda requires a plain required-parameter list");
    }
    let mut names = Vec::with_capacity(parameters.children.len());
    for parameter in &parameters.children {
        let name = plain_symbol(parameter, "required parameter")?;
        if name.as_str().starts_with('&') {
            bail!("inline-lambda supports required parameters only");
        }
        if names.iter().any(|existing: &SymbolName| {
            common_lisp_symbol_reference_eq(existing.as_str(), name.as_str())
        }) {
            bail!("inline-lambda requires unique parameter names");
        }
        names.push(name);
    }
    if call.children.len() != names.len() + 1 {
        bail!("inline-lambda requires exact call arity");
    }
    let body = &lambda.children[2];
    reject_boundary(body)?;
    let bindings = names
        .into_iter()
        .zip(&call.children[1..])
        .map(|(name, argument)| Binding {
            name,
            argument: argument.span.slice(request.input).to_owned(),
        })
        .collect::<Vec<_>>();
    let rendered = bindings
        .iter()
        .map(|binding| format!("({} {})", binding.name, binding.argument))
        .collect::<Vec<_>>()
        .join(" ");
    let replacement = format!("(let ({rendered}) {})", body.span.slice(request.input));
    let rewritten = replace_span(request.input, call.span, &replacement);
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect)
        .context("inline-lambda output is not valid")?;
    Ok(Plan {
        dialect: request.dialect,
        path: request.path,
        call_span: call.span,
        lambda_span: lambda.span,
        bindings,
        replacement,
        changed: rewritten != request.input,
        rewritten,
    })
}

pub(crate) fn validate_dialect(dialect: Dialect) -> Result<()> {
    if dialect != Dialect::CommonLisp {
        bail!("inline-lambda currently supports only Common Lisp");
    }
    Ok(())
}

fn plain_symbol(view: &ExpressionView, role: &str) -> Result<SymbolName> {
    if view.kind != ExpressionKind::Atom || !view.reader_prefixes.is_empty() {
        bail!("inline-lambda requires a plain {role}");
    }
    SymbolName::new(
        atom_symbol_text(view).with_context(|| format!("inline-lambda requires a plain {role}"))?,
    )
    .with_context(|| format!("inline-lambda has invalid {role}"))
}
fn require_head(view: &ExpressionView, expected: &str, message: &str) -> Result<()> {
    if view.kind != ExpressionKind::List
        || !view.reader_prefixes.is_empty()
        || !view
            .children
            .first()
            .and_then(atom_symbol_text)
            .is_some_and(|head| common_lisp_symbol_reference_eq(head, expected))
    {
        bail!("inline-lambda {message}");
    }
    Ok(())
}
fn reject_boundary(view: &ExpressionView) -> Result<()> {
    if view.kind == ExpressionKind::List
        && view
            .children
            .first()
            .and_then(atom_symbol_text)
            .is_some_and(|head| {
                ["go", "return", "return-from", "declare"]
                    .iter()
                    .any(|form| common_lisp_symbol_reference_eq(head, form))
            })
    {
        bail!("inline-lambda rejects control transfer or declarations tied to a function boundary");
    }
    for child in &view.children {
        reject_boundary(child)?;
    }
    Ok(())
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
    fn request(input: &str) -> Request<'_> {
        Request {
            input,
            dialect: Dialect::CommonLisp,
            path: "0".parse().expect("path"),
        }
    }
    #[test]
    fn inlines_required_parameters_and_preserves_arguments() {
        let plan = plan(request("((lambda (x y) (+ x y)) (next-x) (next-y))")).expect("plan");
        assert_eq!(plan.rewritten, "(let ((x (next-x)) (y (next-y))) (+ x y))");
    }
    #[test]
    fn rejects_extended_lists_wrong_arity_and_boundary_forms() {
        for input in [
            "((lambda (&optional x) x) 1)",
            "((lambda (x) x) 1 2)",
            "((lambda () (return 1)))",
            "((lambda () (declare (optimize speed))))",
        ] {
            assert!(plan(request(input)).is_err(), "accepted {input}");
        }
    }
    #[test]
    fn rejects_non_common_lisp_dialect() {
        assert!(
            plan(Request {
                input: "((lambda (x) x) 1)",
                dialect: Dialect::EmacsLisp,
                path: "0".parse().expect("path")
            })
            .is_err()
        );
    }

    #[test]
    fn dialect_support_matrix_is_enforced_before_parsing_and_reparses_output() {
        let result = plan(Request {
            input: "#\\) ((lambda (x) x) 1)",
            dialect: Dialect::CommonLisp,
            path: "1".parse().expect("path"),
        })
        .expect("Common Lisp");
        assert!(result.rewritten.starts_with("#\\)"));
        SyntaxTree::parse_with_dialect(&result.rewritten, Dialect::CommonLisp)
            .expect("Common Lisp output");

        for dialect in [
            Dialect::EmacsLisp,
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
            })
            .expect_err("unsupported dialect");
            assert!(
                error
                    .to_string()
                    .contains("currently supports only Common Lisp"),
                "{dialect:?}: {error:#}"
            );
        }
    }
}
