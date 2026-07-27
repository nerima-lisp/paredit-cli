//! Domain planning for safe Common Lisp sequential-binding conversions.

use paredit_core_edit::{ConservativeRefusal, DialectRefusal, DocumentRefusal, ShapeRefusal};

use crate::error::{BindingCaptureError, BindingFormShapeError, BindingResult};

use paredit_core_semantics::lexical_scope::collect_unshadowed_symbol_references;
use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree,
};

#[derive(Debug, Clone)]
pub struct ConvertSequentialBindingRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
}

#[derive(Debug, Clone)]
pub struct ConvertSequentialBindingPlan {
    pub dialect: Dialect,
    pub path: Path,
    pub form_span: ByteSpan,
    pub binding_names: Vec<SymbolName>,
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

pub fn require_supported_dialect(dialect: Dialect, command: &'static str) -> BindingResult<()> {
    if dialect != Dialect::CommonLisp {
        return Err(DialectRefusal::CurrentlyCommonLispOnly { operation: command }.into());
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Conversion {
    Do,
    Prog,
}

impl Conversion {
    const fn command(self) -> &'static str {
        match self {
            Self::Do => "convert-do-star-to-do",
            Self::Prog => "convert-prog-star-to-prog",
        }
    }
    const fn source_head(self) -> &'static str {
        match self {
            Self::Do => "do*",
            Self::Prog => "prog*",
        }
    }
    const fn target_head(self) -> &'static str {
        match self {
            Self::Do => "do",
            Self::Prog => "prog",
        }
    }
    const fn max_binding_parts(self) -> usize {
        match self {
            Self::Do => 3,
            Self::Prog => 2,
        }
    }
}

pub fn plan_convert_do_star_to_do(
    request: ConvertSequentialBindingRequest<'_>,
) -> BindingResult<ConvertSequentialBindingPlan> {
    plan_conversion(request, Conversion::Do)
}

pub fn plan_convert_prog_star_to_prog(
    request: ConvertSequentialBindingRequest<'_>,
) -> BindingResult<ConvertSequentialBindingPlan> {
    plan_conversion(request, Conversion::Prog)
}

fn plan_conversion(
    request: ConvertSequentialBindingRequest<'_>,
    conversion: Conversion,
) -> BindingResult<ConvertSequentialBindingPlan> {
    let command = conversion.command();
    require_supported_dialect(request.dialect, command)?;
    let tree =
        SyntaxTree::parse_with_dialect(request.input, request.dialect).map_err(|source| {
            DocumentRefusal::InputNotAnSexprDocument {
                operation: command,
                source,
            }
        })?;
    let form = tree.select_path(&request.path)?.view();
    if tree.has_comment_in(form.span) {
        return Err(ConservativeRefusal::Comments { operation: command }.into());
    }
    if contains_reader_prefix(&form) {
        return Err(ConservativeRefusal::ReaderPrefixedSyntax { operation: command }.into());
    }
    require_head(&form, conversion)?;
    if contains_headed_form(&form, "declare") {
        return Err(ConservativeRefusal::Declarations { operation: command }.into());
    }
    let minimum_children = match conversion {
        Conversion::Do => 3,
        Conversion::Prog => 2,
    };
    if form.children.len() < minimum_children {
        return Err(BindingFormShapeError::MalformedForm { command }.into());
    }
    let bindings = &form.children[1];
    if bindings.kind != ExpressionKind::List || !bindings.reader_prefixes.is_empty() {
        return Err(BindingFormShapeError::NotPlainBindingList { command }.into());
    }
    if matches!(conversion, Conversion::Do) && form.children[2].kind != ExpressionKind::List {
        return Err(BindingFormShapeError::MissingTerminationClause { command }.into());
    }
    let mut names = Vec::with_capacity(bindings.children.len());
    let mut initializers = Vec::with_capacity(bindings.children.len());
    let mut steps = Vec::with_capacity(bindings.children.len());
    for binding in &bindings.children {
        let (name, initializer, step) = parse_binding(binding, conversion)?;
        if names.iter().any(|existing: &SymbolName| {
            common_lisp_symbol_reference_eq(existing.as_str(), name.as_str())
        }) {
            return Err(BindingFormShapeError::DuplicateBindingNames { command }.into());
        }
        names.push(name);
        initializers.push(initializer);
        steps.push(step);
    }
    reject_dependencies(
        request.input,
        request.dialect,
        &names,
        &initializers,
        "initializer",
    )?;
    if matches!(conversion, Conversion::Do) {
        reject_dependencies(
            request.input,
            request.dialect,
            &names,
            &steps,
            "step expression",
        )?;
    }
    let head = &form.children[0];
    let rewritten = replace_span(request.input, head.span, conversion.target_head());
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect).map_err(|source| {
        DocumentRefusal::OutputNotAnSexprDocument {
            operation: command,
            source,
        }
    })?;
    Ok(ConvertSequentialBindingPlan {
        dialect: request.dialect,
        path: request.path,
        form_span: form.span,
        binding_names: names,
        changed: rewritten != request.input,
        rewritten,
    })
}

fn parse_binding(
    binding: &ExpressionView,
    conversion: Conversion,
) -> BindingResult<(SymbolName, Option<ExpressionView>, Option<ExpressionView>)> {
    let command = conversion.command();
    if binding.kind == ExpressionKind::Atom {
        return Ok((plain_symbol(binding, command)?, None, None));
    }
    if binding.kind != ExpressionKind::List || !binding.reader_prefixes.is_empty() {
        return Err(BindingFormShapeError::NotPlainVariableBindings { command }.into());
    }
    if !(1..=conversion.max_binding_parts()).contains(&binding.children.len()) {
        return Err(BindingFormShapeError::Destructuring { command }.into());
    }
    let name = plain_symbol(&binding.children[0], command)?;
    Ok((
        name,
        binding.children.get(1).cloned(),
        binding.children.get(2).cloned(),
    ))
}

fn reject_dependencies(
    input: &str,
    dialect: Dialect,
    names: &[SymbolName],
    expressions: &[Option<ExpressionView>],
    role: &str,
) -> BindingResult<()> {
    for (index, expression) in expressions.iter().enumerate() {
        let Some(expression) = expression else {
            continue;
        };
        for earlier in &names[..index] {
            let mut references = Vec::new();
            collect_unshadowed_symbol_references(
                dialect,
                expression,
                earlier,
                input,
                &mut references,
            );
            if !references.is_empty() {
                return Err(BindingCaptureError::ReferencesEarlier {
                    role: role.to_owned(),
                    name: names[index].to_string(),
                    earlier: earlier.to_string(),
                }
                .into());
            }
        }
    }
    Ok(())
}

fn plain_symbol(view: &ExpressionView, command: &'static str) -> BindingResult<SymbolName> {
    if view.kind != ExpressionKind::Atom || !view.reader_prefixes.is_empty() {
        return Err(BindingFormShapeError::NotPlainBindingName { command }.into());
    }
    let text =
        atom_symbol_text(view).ok_or(BindingFormShapeError::NotPlainBindingName { command })?;
    SymbolName::new(text)
        .map_err(|source| BindingFormShapeError::InvalidBindingName { source }.into())
}

fn require_head(view: &ExpressionView, conversion: Conversion) -> BindingResult<()> {
    let command = conversion.command();
    let expected = conversion.source_head();
    if view.kind != ExpressionKind::List || !view.reader_prefixes.is_empty() {
        return Err(ShapeRefusal::NotPlainExpectedForm {
            operation: command,
            expected: expected.to_owned(),
        }
        .into());
    }
    if !view
        .children
        .first()
        .and_then(atom_symbol_text)
        .is_some_and(|head| common_lisp_symbol_reference_eq(head, expected))
    {
        return Err(ShapeRefusal::NotExpectedForm {
            operation: command,
            expected: expected.to_owned(),
        }
        .into());
    }
    Ok(())
}

fn contains_reader_prefix(view: &ExpressionView) -> bool {
    !view.reader_prefixes.is_empty() || view.children.iter().any(contains_reader_prefix)
}

fn contains_headed_form(view: &ExpressionView, expected: &str) -> bool {
    let matches = view.kind == ExpressionKind::List
        && view.reader_prefixes.is_empty()
        && view
            .children
            .first()
            .and_then(atom_symbol_text)
            .is_some_and(|head| common_lisp_symbol_reference_eq(head, expected));
    matches
        || view
            .children
            .iter()
            .any(|child| contains_headed_form(child, expected))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(input: &str, dialect: Dialect) -> ConvertSequentialBindingRequest<'_> {
        ConvertSequentialBindingRequest {
            input,
            dialect,
            path: "0".parse().expect("path"),
        }
    }

    #[test]
    fn independent_bindings_preserve_reader_forms_and_validate_as_common_lisp() {
        let input =
            "(do* ((x (first) (next-x)) (y (second) (next-y))) ((done-p x y) y) (work x)) #\\)";
        let plan = plan_convert_do_star_to_do(request(input, Dialect::CommonLisp)).unwrap();
        assert_eq!(plan.binding_names.len(), 2);
        SyntaxTree::parse_with_dialect(&plan.rewritten, Dialect::CommonLisp).unwrap();
    }

    #[test]
    fn unsupported_dialects_fail_before_parsing_input() {
        for dialect in [
            Dialect::EmacsLisp,
            Dialect::Scheme,
            Dialect::Clojure,
            Dialect::Janet,
            Dialect::Fennel,
            Dialect::Unknown,
        ] {
            let error = plan_convert_do_star_to_do(request(")", dialect)).unwrap_err();
            assert_eq!(
                error.to_string(),
                "convert-do-star-to-do currently supports only Common Lisp"
            );
        }
    }

    #[test]
    fn rejects_dependency_duplicates_and_malformed_forms() {
        assert!(
            plan_convert_do_star_to_do(request(
                "(do* ((x 1) (y (+ x 1))) ((done-p)))",
                Dialect::CommonLisp
            ))
            .is_err()
        );
        assert!(
            plan_convert_do_star_to_do(request(
                "(do* ((x 1) (X 2)) ((done-p)))",
                Dialect::CommonLisp
            ))
            .is_err()
        );
        assert!(
            plan_convert_prog_star_to_prog(request(
                "(prog* ((x 1)) (declare (special x)) (return x))",
                Dialect::CommonLisp
            ))
            .is_err()
        );
    }
}
