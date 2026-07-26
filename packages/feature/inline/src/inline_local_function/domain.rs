//! Pure planning rules for inlining a single Common Lisp `flet` call.

use anyhow::{Context, Result, bail};

use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
use crate::domain::dialect::Dialect;
use crate::domain::lexical_scope::collect_unshadowed_symbol_references;
use crate::domain::sexpr::reader::atom_symbol_text;
use crate::domain::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree,
};

#[derive(Debug, Clone)]
pub(crate) struct Request<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
}

#[derive(Debug, Clone)]
pub(crate) struct ParameterPlan {
    pub name: SymbolName,
    pub argument: String,
    pub reference_count: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct Plan {
    pub dialect: Dialect,
    pub path: Path,
    pub form_span: ByteSpan,
    pub call_span: ByteSpan,
    pub function_name: SymbolName,
    pub parameters: Vec<ParameterPlan>,
    pub replacement: String,
    pub rewritten: String,
    pub changed: bool,
}

pub(crate) fn plan(request: Request<'_>) -> Result<Plan> {
    validate_dialect(request.dialect)?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("inline-local-function input is not a valid S-expression document")?;
    let form = tree.select_path(&request.path)?.view();
    if tree.has_comment_in(form.span) {
        bail!("inline-local-function cannot replace an flet form containing comments");
    }
    require_list_head(&form, "flet", "selected form must be an flet form")?;
    if form.children.len() != 3 {
        bail!("inline-local-function requires exactly one flet body expression");
    }
    let bindings = &form.children[1];
    if bindings.kind != ExpressionKind::List || !bindings.reader_prefixes.is_empty() {
        bail!("inline-local-function requires a plain flet binding list");
    }
    if bindings.children.len() != 1 {
        bail!("inline-local-function requires exactly one flet binding");
    }
    let definition = &bindings.children[0];
    if definition.kind != ExpressionKind::List
        || !definition.reader_prefixes.is_empty()
        || definition.children.len() != 3
    {
        bail!("inline-local-function requires one definition with a single body expression");
    }
    let function_name = plain_symbol(&definition.children[0], "local function name")?;
    let parameter_list = &definition.children[1];
    if parameter_list.kind != ExpressionKind::List || !parameter_list.reader_prefixes.is_empty() {
        bail!("inline-local-function requires a plain required-parameter list");
    }
    let mut parameter_names = Vec::with_capacity(parameter_list.children.len());
    for parameter in &parameter_list.children {
        let name = plain_symbol(parameter, "required parameter")?;
        if name.as_str().starts_with('&') {
            bail!("inline-local-function supports required parameters only");
        }
        if parameter_names.iter().any(|existing: &SymbolName| {
            common_lisp_symbol_reference_eq(existing.as_str(), name.as_str())
        }) {
            bail!("inline-local-function requires unique parameter names");
        }
        parameter_names.push(name);
    }
    let definition_body = &definition.children[2];
    reject_control_transfer(definition_body)?;
    reject_self_call(definition_body, function_name.as_str())?;
    let call = &form.children[2];
    require_list_head(
        call,
        function_name.as_str(),
        "flet body must be exactly one direct call to the local function",
    )?;
    if call.children.len() != parameter_names.len() + 1 {
        bail!("inline-local-function requires exact call arity");
    }
    let mut parameters = Vec::with_capacity(parameter_names.len());
    for (name, argument) in parameter_names.into_iter().zip(&call.children[1..]) {
        let mut references = Vec::new();
        collect_unshadowed_symbol_references(
            request.dialect,
            definition_body,
            &name,
            request.input,
            &mut references,
        );
        if references.len() != 1 {
            bail!(
                "inline-local-function requires parameter '{}' to be referenced exactly once; found {}",
                name,
                references.len()
            );
        }
        parameters.push(ParameterPlan {
            name,
            argument: argument.span.slice(request.input).to_owned(),
            reference_count: references.len(),
        });
    }
    let body = definition_body.span.slice(request.input);
    let bindings = parameters
        .iter()
        .map(|parameter| format!("({} {})", parameter.name, parameter.argument))
        .collect::<Vec<_>>()
        .join(" ");
    let replacement = format!("(let ({bindings}) {body})");
    let rewritten = replace_span(request.input, form.span, &replacement);
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect)
        .context("inline-local-function output is not a valid S-expression document")?;
    Ok(Plan {
        dialect: request.dialect,
        path: request.path,
        form_span: form.span,
        call_span: call.span,
        function_name,
        parameters,
        replacement,
        changed: rewritten != request.input,
        rewritten,
    })
}

pub(crate) fn validate_dialect(dialect: Dialect) -> Result<()> {
    if dialect != Dialect::CommonLisp {
        bail!("inline-local-function currently supports only Common Lisp");
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

fn plain_symbol(view: &ExpressionView, role: &str) -> Result<SymbolName> {
    if view.kind != ExpressionKind::Atom || !view.reader_prefixes.is_empty() {
        bail!("inline-local-function requires a plain {role}");
    }
    let text = atom_symbol_text(view)
        .with_context(|| format!("inline-local-function requires a plain {role}"))?;
    SymbolName::new(text).with_context(|| format!("inline-local-function has invalid {role}"))
}

fn require_list_head(view: &ExpressionView, expected: &str, message: &str) -> Result<()> {
    let matches = view.kind == ExpressionKind::List
        && view.reader_prefixes.is_empty()
        && view
            .children
            .first()
            .and_then(atom_symbol_text)
            .is_some_and(|head| common_lisp_symbol_reference_eq(head, expected));
    if !matches {
        bail!("inline-local-function {message}");
    }
    Ok(())
}

fn reject_self_call(view: &ExpressionView, name: &str) -> Result<()> {
    if view.kind == ExpressionKind::List
        && view
            .children
            .first()
            .and_then(atom_symbol_text)
            .is_some_and(|head| common_lisp_symbol_reference_eq(head, name))
    {
        bail!("inline-local-function rejects recursive or same-name calls in the definition body");
    }
    for child in &view.children {
        reject_self_call(child, name)?;
    }
    Ok(())
}

fn reject_control_transfer(view: &ExpressionView) -> Result<()> {
    if view.kind == ExpressionKind::List
        && view
            .children
            .first()
            .and_then(atom_symbol_text)
            .is_some_and(|head| {
                ["go", "return-from", "declare"]
                    .iter()
                    .any(|form| common_lisp_symbol_reference_eq(head, form))
            })
    {
        bail!("inline-local-function rejects non-local control transfer or declarations");
    }
    for child in &view.children {
        reject_control_transfer(child)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(input: &str) -> Result<Plan> {
        super::plan(Request {
            input,
            dialect: Dialect::CommonLisp,
            path: Path::from_indexes(vec![0]),
        })
    }

    #[test]
    fn inlines_single_flet_call_through_parallel_let() {
        let plan = plan("(flet ((sum (x y) (+ x y))) (sum (next-x) (next-y)))").unwrap();
        assert_eq!(plan.rewritten, "(let ((x (next-x)) (y (next-y))) (+ x y))");
        assert_eq!(plan.parameters.len(), 2);
    }

    #[test]
    fn rejects_unsafe_calls_and_dialects() {
        assert!(plan("(flet ((f (x) 1)) (f (effect)))").is_err());
        assert!(plan("(flet ((f (x) (+ x x))) (f (effect)))").is_err());
        assert!(plan("(flet ((f (x) (go done))) (f 1))").is_err());
        assert!(plan("(labels ((f (x) x)) (f 1))").is_err());
    }

    #[test]
    fn supports_only_common_lisp_before_input_validation() {
        let cases = [
            (Dialect::CommonLisp, true),
            (Dialect::EmacsLisp, false),
            (Dialect::Scheme, false),
            (Dialect::Clojure, false),
            (Dialect::Janet, false),
            (Dialect::Fennel, false),
            (Dialect::Unknown, false),
        ];

        for (dialect, supported) in cases {
            let input = if supported {
                "(flet ((identity (x) x)) (identity value))"
            } else {
                ")"
            };
            let result = super::plan(Request {
                input,
                dialect,
                path: Path::from_indexes(vec![0]),
            });

            if supported {
                let plan = result.expect("Common Lisp should be supported");
                assert_eq!(plan.dialect, dialect);
                SyntaxTree::parse_with_dialect(&plan.rewritten, dialect)
                    .expect("rewritten Common Lisp should parse");
            } else {
                let error = match result {
                    Ok(_) => panic!("{dialect:?} should be rejected"),
                    Err(error) => error,
                };
                assert_eq!(
                    error.to_string(),
                    "inline-local-function currently supports only Common Lisp"
                );
            }
        }
    }

    #[test]
    fn preserves_common_lisp_reader_atoms() {
        let input = "(flet ((render (x) (list x #\\) #:done #x2a))) (render (next)))";
        let plan = plan(input).unwrap();

        assert_eq!(
            plan.rewritten,
            "(let ((x (next))) (list x #\\) #:done #x2a))"
        );
        SyntaxTree::parse_with_dialect(&plan.rewritten, Dialect::CommonLisp)
            .expect("rewritten Common Lisp reader atoms should parse");
    }
}
