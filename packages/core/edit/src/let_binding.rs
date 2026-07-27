//! Dependency-preserving conversions between Common Lisp `let` forms.

use crate::error::{
    BindingRefusal, ConservativeRefusal, DialectRefusal, DocumentRefusal, EditResult, ShapeRefusal,
};

use paredit_core_semantics::lexical_scope::collect_unshadowed_symbol_references;
use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree,
};

#[derive(Debug, Clone)]
pub struct ConvertLetToLetStarRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
}
#[derive(Debug, Clone)]
pub struct ConvertLetToLetStarPlan {
    pub dialect: Dialect,
    pub path: Path,
    pub form_span: ByteSpan,
    pub binding_names: Vec<SymbolName>,
    pub rewritten: String,
    pub changed: bool,
}

pub fn validate_convert_let_to_let_star_dialect(dialect: Dialect) -> EditResult<()> {
    if !matches!(dialect, Dialect::CommonLisp | Dialect::EmacsLisp) {
        return Err(DialectRefusal::CommonLispAndEmacsLisp {
            operation: "convert-let-to-let-star",
        }
        .into());
    }
    Ok(())
}

pub fn validate_convert_let_star_to_let_dialect(dialect: Dialect) -> EditResult<()> {
    if dialect != Dialect::CommonLisp {
        return Err(DialectRefusal::CurrentlyCommonLispOnly {
            operation: "convert-let-star-to-let",
        }
        .into());
    }
    Ok(())
}

pub fn plan_convert_let_to_let_star(
    request: ConvertLetToLetStarRequest<'_>,
) -> EditResult<ConvertLetToLetStarPlan> {
    validate_convert_let_to_let_star_dialect(request.dialect)?;
    let tree =
        SyntaxTree::parse_with_dialect(request.input, request.dialect).map_err(|source| {
            DocumentRefusal::InputInvalid {
                operation: "convert-let-to-let-star",
                source,
            }
        })?;
    let form = tree.select_path(&request.path)?.view();
    validate_form(
        &form,
        &tree,
        request.dialect,
        "let",
        "convert-let-to-let-star",
    )?;
    let (names, initializers) =
        analyze_bindings(&form, request.dialect, "convert-let-to-let-star")?;
    reject_dependencies(&names, &initializers, &request, "convert-let-to-let-star")?;
    let rewritten = replace_head(request.input, form.children[0].span, "let*");
    parse_output(&rewritten, request.dialect, "convert-let-to-let-star")?;
    Ok(ConvertLetToLetStarPlan {
        dialect: request.dialect,
        path: request.path,
        form_span: form.span,
        binding_names: names,
        changed: rewritten != request.input,
        rewritten,
    })
}

#[derive(Debug, Clone)]
pub struct ConvertLetStarToLetRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
}
#[derive(Debug, Clone)]
pub struct ConvertLetStarToLetPlan {
    pub dialect: Dialect,
    pub path: Path,
    pub form_span: ByteSpan,
    pub binding_names: Vec<SymbolName>,
    pub rewritten: String,
    pub changed: bool,
}

pub fn plan_convert_let_star_to_let(
    request: ConvertLetStarToLetRequest<'_>,
) -> EditResult<ConvertLetStarToLetPlan> {
    validate_convert_let_star_to_let_dialect(request.dialect)?;
    let tree =
        SyntaxTree::parse_with_dialect(request.input, request.dialect).map_err(|source| {
            DocumentRefusal::InputNotAnSexprDocument {
                operation: "convert-let-star-to-let",
                source,
            }
        })?;
    let form = tree.select_path(&request.path)?.view();
    validate_form(
        &form,
        &tree,
        request.dialect,
        "let*",
        "convert-let-star-to-let",
    )?;
    let (names, initializers) =
        analyze_bindings(&form, request.dialect, "convert-let-star-to-let")?;
    reject_dependencies(&names, &initializers, &request, "convert-let-star-to-let")?;
    let rewritten = replace_head(request.input, form.children[0].span, "let");
    parse_output(&rewritten, request.dialect, "convert-let-star-to-let")?;
    Ok(ConvertLetStarToLetPlan {
        dialect: request.dialect,
        path: request.path,
        form_span: form.span,
        binding_names: names,
        changed: rewritten != request.input,
        rewritten,
    })
}

fn validate_form(
    form: &ExpressionView,
    tree: &SyntaxTree,
    dialect: Dialect,
    expected: &str,
    operation: &'static str,
) -> EditResult<()> {
    if tree.has_comment_in(form.span) {
        return Err(ConservativeRefusal::Comments { operation }.into());
    }
    if form.kind != ExpressionKind::List
        || !form.reader_prefixes.is_empty()
        || !form
            .children
            .first()
            .and_then(atom_symbol_text)
            .is_some_and(|head| symbol_eq(dialect, head, expected))
    {
        return Err(ShapeRefusal::NotPlainExpectedForm {
            operation,
            expected: expected.to_owned(),
        }
        .into());
    }
    if contains_headed_form(dialect, form, "declare") {
        return Err(ConservativeRefusal::Declarations { operation }.into());
    }
    let bindings = form
        .children
        .get(1)
        .ok_or(BindingRefusal::MissingBindingList { operation })?;
    if bindings.kind != ExpressionKind::List || !bindings.reader_prefixes.is_empty() {
        return Err(BindingRefusal::NotPlainBindingList { operation }.into());
    }
    Ok(())
}

fn analyze_bindings(
    form: &ExpressionView,
    dialect: Dialect,
    operation: &'static str,
) -> EditResult<(Vec<SymbolName>, Vec<Option<ExpressionView>>)> {
    let bindings = &form.children[1];
    let mut names = Vec::with_capacity(bindings.children.len());
    let mut initializers = Vec::with_capacity(bindings.children.len());
    for binding in &bindings.children {
        let (name, initializer) = parse_binding(binding, operation)?;
        if names
            .iter()
            .any(|old: &SymbolName| symbol_eq(dialect, old.as_str(), name.as_str()))
        {
            return Err(BindingRefusal::DuplicateBindingNames { operation }.into());
        }
        names.push(name);
        initializers.push(initializer);
    }
    Ok((names, initializers))
}

fn parse_binding(
    binding: &ExpressionView,
    operation: &'static str,
) -> EditResult<(SymbolName, Option<ExpressionView>)> {
    if binding.kind == ExpressionKind::Atom {
        return Ok((plain_symbol(binding, operation)?, None));
    }
    if binding.kind != ExpressionKind::List
        || !binding.reader_prefixes.is_empty()
        || !(1..=2).contains(&binding.children.len())
    {
        return Err(BindingRefusal::Destructuring { operation }.into());
    }
    Ok((
        plain_symbol(&binding.children[0], operation)?,
        binding.children.get(1).cloned(),
    ))
}

fn plain_symbol(view: &ExpressionView, operation: &'static str) -> EditResult<SymbolName> {
    if view.kind != ExpressionKind::Atom || !view.reader_prefixes.is_empty() {
        return Err(BindingRefusal::NotPlainBindingName { operation }.into());
    }
    SymbolName::new(atom_symbol_text(view).ok_or(BindingRefusal::BindingName)?)
        .map_err(|source| BindingRefusal::InvalidBindingName { source }.into())
}

fn reject_dependencies<R>(
    names: &[SymbolName],
    initializers: &[Option<ExpressionView>],
    request: &R,
    operation: &'static str,
) -> EditResult<()>
where
    R: LetRequest + ?Sized,
{
    for (index, initializer) in initializers.iter().enumerate() {
        let Some(initializer) = initializer else {
            continue;
        };
        for earlier in &names[..index] {
            let mut references = Vec::new();
            collect_unshadowed_symbol_references(
                request.dialect(),
                initializer,
                earlier,
                request.input(),
                &mut references,
            );
            if !references.is_empty() {
                return Err(BindingRefusal::ReferencesEarlierBinding {
                    operation,
                    earlier: earlier.to_string(),
                }
                .into());
            }
        }
    }
    Ok(())
}

trait LetRequest {
    fn input(&self) -> &str;
    fn dialect(&self) -> Dialect;
}
impl<'a> LetRequest for ConvertLetToLetStarRequest<'a> {
    fn input(&self) -> &str {
        self.input
    }
    fn dialect(&self) -> Dialect {
        self.dialect
    }
}
impl<'a> LetRequest for ConvertLetStarToLetRequest<'a> {
    fn input(&self) -> &str {
        self.input
    }
    fn dialect(&self) -> Dialect {
        self.dialect
    }
}

fn symbol_eq(dialect: Dialect, left: &str, right: &str) -> bool {
    dialect != Dialect::CommonLisp && left == right
        || dialect == Dialect::CommonLisp && common_lisp_symbol_reference_eq(left, right)
}
fn contains_headed_form(dialect: Dialect, view: &ExpressionView, expected: &str) -> bool {
    (view.kind == ExpressionKind::List
        && view.reader_prefixes.is_empty()
        && view
            .children
            .first()
            .and_then(atom_symbol_text)
            .is_some_and(|head| symbol_eq(dialect, head, expected)))
        || view
            .children
            .iter()
            .any(|child| contains_headed_form(dialect, child, expected))
}
fn replace_head(input: &str, span: ByteSpan, replacement: &str) -> String {
    let mut output = String::with_capacity(input.len() - span.len() + replacement.len());
    output.push_str(&input[..span.start().get()]);
    output.push_str(replacement);
    output.push_str(&input[span.end().get()..]);
    output
}
fn parse_output(output: &str, dialect: Dialect, operation: &'static str) -> EditResult<()> {
    SyntaxTree::parse_with_dialect(output, dialect)
        .map_err(|source| DocumentRefusal::OutputInvalid { operation, source })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_safe_bindings_and_rejects_dependencies() {
        let path: Path = "0".parse().expect("path");
        for dialect in [Dialect::CommonLisp, Dialect::EmacsLisp] {
            let plan = plan_convert_let_to_let_star(ConvertLetToLetStarRequest {
                input: "(let ((x 1) (y 2)) (+ x y))",
                dialect,
                path: path.clone(),
            })
            .expect("plan");
            assert_eq!(plan.rewritten, "(let* ((x 1) (y 2)) (+ x y))");
            assert!(
                plan_convert_let_to_let_star(ConvertLetToLetStarRequest {
                    input: "(let ((x 1) (y (+ x 2))) y)",
                    dialect,
                    path: path.clone()
                })
                .is_err()
            );
        }
        assert!(
            plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
                input: "(let* ((x 1) (y 2)) (+ x y))",
                dialect: Dialect::CommonLisp,
                path
            })
            .is_ok()
        );
    }
    #[test]
    fn rejects_shadowing_ambiguity_comments_declarations_and_dialects() {
        let path: Path = "0".parse().expect("path");
        assert!(
            plan_convert_let_to_let_star(ConvertLetToLetStarRequest {
                input: "(let ((x 1) (X 2)) x)",
                dialect: Dialect::CommonLisp,
                path: path.clone()
            })
            .is_err()
        );
        assert!(
            plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
                input: "(let* ((x 1)) ; c\n x)",
                dialect: Dialect::CommonLisp,
                path: path.clone()
            })
            .is_err()
        );
        assert!(
            plan_convert_let_to_let_star(ConvertLetToLetStarRequest {
                input: "(let ((x 1)) (declare (special x)) x)",
                dialect: Dialect::CommonLisp,
                path: path.clone()
            })
            .is_err()
        );
        assert!(
            plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
                input: "(let* ((x 1)) x)",
                dialect: Dialect::EmacsLisp,
                path
            })
            .is_err()
        );
    }

    #[test]
    fn dialect_support_matrix_is_enforced_before_parsing_and_reparses_output() {
        for (dialect, input) in [
            (Dialect::CommonLisp, "#\\) (let ((x 1) (y 2)) (+ x y))"),
            (Dialect::EmacsLisp, "?\\) (let ((x 1) (y 2)) (+ x y))"),
        ] {
            let plan = plan_convert_let_to_let_star(ConvertLetToLetStarRequest {
                input,
                dialect,
                path: "1".parse().expect("path"),
            })
            .expect("supported dialect");
            SyntaxTree::parse_with_dialect(&plan.rewritten, dialect)
                .expect("dialect-specific let output");
        }

        for dialect in [
            Dialect::Scheme,
            Dialect::Clojure,
            Dialect::Janet,
            Dialect::Fennel,
            Dialect::Unknown,
        ] {
            let error = plan_convert_let_to_let_star(ConvertLetToLetStarRequest {
                input: ")",
                dialect,
                path: "0".parse().expect("path"),
            })
            .expect_err("unsupported dialect");
            assert!(
                error
                    .to_string()
                    .contains("supports only Common Lisp and Emacs Lisp"),
                "{dialect:?}: {error:#}"
            );
        }

        let plan = plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
            input: "#\\) (let* ((x 1) (y 2)) (+ x y))",
            dialect: Dialect::CommonLisp,
            path: "1".parse().expect("path"),
        })
        .expect("Common Lisp");
        SyntaxTree::parse_with_dialect(&plan.rewritten, Dialect::CommonLisp)
            .expect("Common Lisp let output");

        for dialect in [
            Dialect::EmacsLisp,
            Dialect::Scheme,
            Dialect::Clojure,
            Dialect::Janet,
            Dialect::Fennel,
            Dialect::Unknown,
        ] {
            let error = plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
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
