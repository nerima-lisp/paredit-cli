//! Semantics-preserving inlining of immediately invoked Common Lisp lambdas.

use paredit_core_edit::{DialectRefusal, DocumentRefusal};

use crate::error::{
    CallBindingError, InlineError, InlineResult, InlineSafetyError, InlineSelectionError,
};
use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree,
};

#[derive(Debug, Clone)]
pub struct Request<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
}
#[derive(Debug, Clone)]
pub struct Binding {
    pub name: SymbolName,
    pub argument: String,
}
#[derive(Debug, Clone)]
pub struct Plan {
    pub dialect: Dialect,
    pub path: Path,
    pub call_span: ByteSpan,
    pub lambda_span: ByteSpan,
    pub bindings: Vec<Binding>,
    pub replacement: String,
    pub rewritten: String,
    pub changed: bool,
}

pub fn plan(request: Request<'_>) -> InlineResult<Plan> {
    validate_dialect(request.dialect)?;
    let tree =
        SyntaxTree::parse_with_dialect(request.input, request.dialect).map_err(|source| {
            DocumentRefusal::InputInvalid {
                operation: "inline-lambda",
                source,
            }
        })?;
    let call = tree.select_path(&request.path)?.view();
    if tree.has_comment_in(call.span) {
        return Err(InlineSelectionError::Shape {
            operation: "inline-lambda",
            problem: "cannot replace a call containing comments".to_owned(),
        }
        .into());
    }
    if call.kind != ExpressionKind::List || !call.reader_prefixes.is_empty() {
        return Err(InlineSelectionError::Shape {
            operation: "inline-lambda",
            problem: "selected form must be a plain call list".to_owned(),
        }
        .into());
    }
    let lambda = call.children.first().ok_or_else(|| {
        InlineError::from(InlineSelectionError::Shape {
            operation: "inline-lambda",
            problem: "selected call has no operator".to_owned(),
        })
    })?;
    require_head(lambda, "lambda", "call operator must be a lambda form")?;
    if lambda.children.len() != 3 {
        return Err(InlineSelectionError::Shape {
            operation: "inline-lambda",
            problem: "requires exactly one lambda body expression".to_owned(),
        }
        .into());
    }
    let parameters = &lambda.children[1];
    if parameters.kind != ExpressionKind::List || !parameters.reader_prefixes.is_empty() {
        return Err(InlineSelectionError::Shape {
            operation: "inline-lambda",
            problem: "requires a plain required-parameter list".to_owned(),
        }
        .into());
    }
    let mut names = Vec::with_capacity(parameters.children.len());
    for parameter in &parameters.children {
        let name = plain_symbol(parameter, "required parameter")?;
        if name.as_str().starts_with('&') {
            return Err(InlineSelectionError::Shape {
                operation: "inline-lambda",
                problem: "supports required parameters only".to_owned(),
            }
            .into());
        }
        if names.iter().any(|existing: &SymbolName| {
            common_lisp_symbol_reference_eq(existing.as_str(), name.as_str())
        }) {
            return Err(InlineSelectionError::Shape {
                operation: "inline-lambda",
                problem: "requires unique parameter names".to_owned(),
            }
            .into());
        }
        names.push(name);
    }
    if call.children.len() != names.len() + 1 {
        return Err(CallBindingError::ExactArityRequired {
            operation: "inline-lambda",
        }
        .into());
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
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect).map_err(|source| {
        DocumentRefusal::OutputInvalid {
            operation: "inline-lambda",
            source,
        }
    })?;
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

pub fn validate_dialect(dialect: Dialect) -> InlineResult<()> {
    if dialect != Dialect::CommonLisp {
        return Err(DialectRefusal::CurrentlyCommonLispOnly {
            operation: "inline-lambda",
        }
        .into());
    }
    Ok(())
}

fn plain_symbol(view: &ExpressionView, role: &str) -> InlineResult<SymbolName> {
    if view.kind != ExpressionKind::Atom || !view.reader_prefixes.is_empty() {
        return Err(InlineSelectionError::NotPlain {
            operation: "inline-lambda",
            role: role.to_owned(),
        }
        .into());
    }
    let text = atom_symbol_text(view).ok_or_else(|| InlineSelectionError::NotPlain {
        operation: "inline-lambda",
        role: role.to_owned(),
    })?;
    SymbolName::new(text).map_err(|_| {
        InlineSelectionError::Invalid {
            operation: "inline-lambda",
            role: role.to_owned(),
        }
        .into()
    })
}
fn require_head(view: &ExpressionView, expected: &str, message: &str) -> InlineResult<()> {
    if view.kind != ExpressionKind::List
        || !view.reader_prefixes.is_empty()
        || !view
            .children
            .first()
            .and_then(atom_symbol_text)
            .is_some_and(|head| common_lisp_symbol_reference_eq(head, expected))
    {
        return Err(InlineSelectionError::Shape {
            operation: "inline-lambda",
            problem: message.to_owned(),
        }
        .into());
    }
    Ok(())
}
fn reject_boundary(view: &ExpressionView) -> InlineResult<()> {
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
        return Err(InlineSafetyError::LambdaControlTransfer.into());
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
