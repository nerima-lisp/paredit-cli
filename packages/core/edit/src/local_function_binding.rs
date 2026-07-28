//! Common Lisp local function binding conversions.

use crate::error::{
    BindingRefusal, ConservativeRefusal, DialectRefusal, DocumentRefusal, EditResult,
    LocalFunctionRefusal, ShapeRefusal,
};

use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, Path, SyntaxTree};

#[derive(Debug, Clone)]
pub struct ConvertFletToLabelsRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
}

#[derive(Debug, Clone)]
pub struct ConvertFletToLabelsPlan {
    pub dialect: Dialect,
    pub path: Path,
    pub form_span: ByteSpan,
    pub binding_count: usize,
    pub rewritten: String,
    pub changed: bool,
}

pub fn validate_convert_flet_to_labels_dialect(dialect: Dialect) -> EditResult<()> {
    validate_common_lisp_dialect(dialect, "convert-flet-to-labels")
}

pub fn validate_convert_labels_to_flet_dialect(dialect: Dialect) -> EditResult<()> {
    validate_common_lisp_dialect(dialect, "convert-labels-to-flet")
}

fn validate_common_lisp_dialect(dialect: Dialect, operation: &'static str) -> EditResult<()> {
    if dialect != Dialect::CommonLisp {
        return Err(DialectRefusal::CommonLispOnly { operation }.into());
    }
    Ok(())
}

pub fn plan_convert_flet_to_labels(
    request: ConvertFletToLabelsRequest<'_>,
) -> EditResult<ConvertFletToLabelsPlan> {
    let BindingAnalysis { form, head, names } =
        analyze_bindings(&request, "flet", "convert-flet-to-labels")?;
    for definition in form.children[1].children.iter() {
        for body in definition.children.iter().skip(2) {
            if contains_local_function_reference(body, &names) {
                return Err(LocalFunctionRefusal::WouldCaptureReferences {
                    operation: "convert-flet-to-labels",
                }
                .into());
            }
        }
    }
    let rewritten = replace_head(request.input, &form, replace_flet_name(&head));
    parse_output(&rewritten, request.dialect, "convert-flet-to-labels")?;
    Ok(ConvertFletToLabelsPlan {
        dialect: request.dialect,
        path: request.path,
        form_span: form.span,
        binding_count: names.len(),
        changed: rewritten != request.input,
        rewritten,
    })
}

#[derive(Debug, Clone)]
pub struct ConvertLabelsToFletRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
}

#[derive(Debug, Clone)]
pub struct ConvertLabelsToFletPlan {
    pub dialect: Dialect,
    pub path: Path,
    pub form_span: ByteSpan,
    pub binding_count: usize,
    pub rewritten: String,
    pub changed: bool,
}

pub fn plan_convert_labels_to_flet(
    request: ConvertLabelsToFletRequest<'_>,
) -> EditResult<ConvertLabelsToFletPlan> {
    let BindingAnalysis { form, head, names } =
        analyze_bindings(&request, "labels", "convert-labels-to-flet")?;
    for definition in form.children[1].children.iter() {
        for body in definition.children.iter().skip(2) {
            if contains_local_function_reference(body, &names) {
                return Err(LocalFunctionRefusal::Recursive {
                    operation: "convert-labels-to-flet",
                }
                .into());
            }
        }
    }
    let rewritten = replace_head(request.input, &form, replace_labels_name(&head));
    parse_output(&rewritten, request.dialect, "convert-labels-to-flet")?;
    Ok(ConvertLabelsToFletPlan {
        dialect: request.dialect,
        path: request.path,
        form_span: form.span,
        binding_count: names.len(),
        changed: rewritten != request.input,
        rewritten,
    })
}

struct BindingAnalysis {
    form: ExpressionView,
    head: String,
    names: Vec<String>,
}

fn analyze_bindings<'a, R>(
    request: &'a R,
    expected_head: &str,
    operation: &'static str,
) -> EditResult<BindingAnalysis>
where
    R: BindingRequest<'a> + ?Sized,
{
    validate_common_lisp_dialect(request.dialect(), operation)?;
    let tree = SyntaxTree::parse_with_dialect(request.input(), request.dialect())
        .map_err(|source| DocumentRefusal::InputNotAnSexprDocument { operation, source })?;
    let form = tree.select_path(request.path())?.view();
    if tree.has_comment_in(form.span) {
        return Err(ConservativeRefusal::Comments { operation }.into());
    }
    if contains_reader_prefix(&form) {
        return Err(ConservativeRefusal::RequiresNoReaderPrefixes { operation }.into());
    }
    if form.kind != ExpressionKind::List || form.children.len() < 2 {
        return Err(ShapeRefusal::NotExpectedForm {
            operation,
            expected: expected_head.to_owned(),
        }
        .into());
    }
    let head = plain_atom(&form.children[0])
        .ok_or(ShapeRefusal::HeadNotPlain { operation })?
        .to_owned();
    if !common_lisp_symbol_reference_eq(&head, expected_head) {
        return Err(ShapeRefusal::NotExpectedForm {
            operation,
            expected: expected_head.to_owned(),
        }
        .into());
    }
    let bindings = &form.children[1];
    if bindings.kind != ExpressionKind::List {
        return Err(BindingRefusal::NotPlainBindingList { operation }.into());
    }
    let mut names: Vec<String> = Vec::with_capacity(bindings.children.len());
    for definition in &bindings.children {
        if definition.kind != ExpressionKind::List || definition.children.len() < 2 {
            return Err(LocalFunctionRefusal::NotPlainDefinitions { operation }.into());
        }
        let name = plain_atom(&definition.children[0])
            .ok_or(LocalFunctionRefusal::NotPlainName { operation })?;
        if definition.children[1].kind != ExpressionKind::List {
            return Err(LocalFunctionRefusal::NotPlainLambdaList { operation }.into());
        }
        if names
            .iter()
            .any(|existing| common_lisp_symbol_reference_eq(existing, name))
        {
            return Err(LocalFunctionRefusal::DuplicateNames { operation }.into());
        }
        names.push(name.to_owned());
    }
    Ok(BindingAnalysis { form, head, names })
}

trait BindingRequest<'a> {
    fn input(&self) -> &'a str;
    fn dialect(&self) -> Dialect;
    fn path(&self) -> &Path;
}

impl<'a> BindingRequest<'a> for ConvertFletToLabelsRequest<'a> {
    fn input(&self) -> &'a str {
        self.input
    }
    fn dialect(&self) -> Dialect {
        self.dialect
    }
    fn path(&self) -> &Path {
        &self.path
    }
}

impl<'a> BindingRequest<'a> for ConvertLabelsToFletRequest<'a> {
    fn input(&self) -> &'a str {
        self.input
    }
    fn dialect(&self) -> Dialect {
        self.dialect
    }
    fn path(&self) -> &Path {
        &self.path
    }
}

fn plain_atom(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom && view.reader_prefixes.is_empty())
        .then(|| atom_symbol_text(view))
        .flatten()
}

fn contains_reader_prefix(view: &ExpressionView) -> bool {
    !view.reader_prefixes.is_empty() || view.children.iter().any(contains_reader_prefix)
}

fn contains_local_function_reference(view: &ExpressionView, names: &[String]) -> bool {
    if view.kind == ExpressionKind::List {
        let head = view.children.first().and_then(plain_atom);
        if head.is_some_and(|head| {
            names
                .iter()
                .any(|name| common_lisp_symbol_reference_eq(name, head))
        }) {
            return true;
        }
        if head.is_some_and(|head| common_lisp_symbol_reference_eq(head, "function"))
            && view
                .children
                .get(1)
                .and_then(plain_atom)
                .is_some_and(|name| {
                    names
                        .iter()
                        .any(|local| common_lisp_symbol_reference_eq(local, name))
                })
        {
            return true;
        }
    }
    view.children
        .iter()
        .any(|child| contains_local_function_reference(child, names))
}

fn replace_flet_name(head: &str) -> String {
    replace_operator_name(head, "labels")
}
fn replace_labels_name(head: &str) -> String {
    replace_operator_name(head, "flet")
}

fn replace_operator_name(head: &str, replacement: &str) -> String {
    match head.rsplit_once(':') {
        Some((package, _)) => format!("{package}:{replacement}"),
        None => replacement.to_owned(),
    }
}

fn replace_head(input: &str, form: &ExpressionView, replacement: String) -> String {
    let span = form.children[0].span;
    let mut output = String::with_capacity(input.len() - span.len() + replacement.len());
    output.push_str(&input[..span.start().get()]);
    output.push_str(&replacement);
    output.push_str(&input[span.end().get()..]);
    output
}

fn parse_output(rewritten: &str, dialect: Dialect, operation: &'static str) -> EditResult<()> {
    SyntaxTree::parse_with_dialect(rewritten, dialect)
        .map_err(|source| DocumentRefusal::OutputNotAnSexprDocument { operation, source })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIALECTS: [Dialect; 7] = [
        Dialect::CommonLisp,
        Dialect::EmacsLisp,
        Dialect::Scheme,
        Dialect::Clojure,
        Dialect::Janet,
        Dialect::Fennel,
        Dialect::Unknown,
    ];

    fn flet_request(input: &str, dialect: Dialect) -> ConvertFletToLabelsRequest<'_> {
        ConvertFletToLabelsRequest {
            input,
            dialect,
            path: "0".parse().expect("path"),
        }
    }

    fn labels_request(input: &str, dialect: Dialect) -> ConvertLabelsToFletRequest<'_> {
        ConvertLabelsToFletRequest {
            input,
            dialect,
            path: "0".parse().expect("path"),
        }
    }

    fn assert_support_error<T>(result: EditResult<T>, operation: &str) {
        let error = result.err().expect("unsupported dialect must fail");
        assert_eq!(
            error.to_string(),
            format!("{operation} supports only Common Lisp")
        );
    }

    #[test]
    fn converts_capture_free_and_non_recursive_forms() {
        let flet = plan_convert_flet_to_labels(flet_request(
            "(flet ((work () 1)) (work))",
            Dialect::CommonLisp,
        ))
        .expect("flet plan");
        assert_eq!(flet.rewritten, "(labels ((work () 1)) (work))");

        let labels = plan_convert_labels_to_flet(labels_request(
            "(labels ((work () 1)) (work))",
            Dialect::CommonLisp,
        ))
        .expect("labels plan");
        assert_eq!(labels.rewritten, "(flet ((work () 1)) (work))");
    }

    #[test]
    fn rejects_recursion_duplicates_and_malformed_forms() {
        for input in [
            "(labels ((walk () (walk))) (walk))",
            "(labels ((walk () (function walk))) (walk))",
            "(flet ((work () 1) (WORK () 2)) (work))",
            "(flet (work) (work))",
            "(flet ((work value 1)) (work))",
            "(flet ((work () ; keep\n 1)) (work))",
        ] {
            assert!(
                plan_convert_labels_to_flet(labels_request(input, Dialect::CommonLisp)).is_err()
                    || plan_convert_flet_to_labels(flet_request(input, Dialect::CommonLisp))
                        .is_err()
            );
        }
    }

    #[test]
    fn support_matrix_is_common_lisp_only_for_both_conversions() {
        for dialect in DIALECTS {
            let flet =
                plan_convert_flet_to_labels(flet_request("(flet ((work () 1)) (work))", dialect));
            let labels = plan_convert_labels_to_flet(labels_request(
                "(labels ((work () 1)) (work))",
                dialect,
            ));

            if dialect == Dialect::CommonLisp {
                assert!(flet.is_ok(), "Common Lisp flet conversion must succeed");
                assert!(labels.is_ok(), "Common Lisp labels conversion must succeed");
            } else {
                assert_support_error(flet, "convert-flet-to-labels");
                assert_support_error(labels, "convert-labels-to-flet");
            }
        }
    }

    #[test]
    fn unsupported_dialect_gate_precedes_parsing_for_both_conversions() {
        for dialect in DIALECTS
            .into_iter()
            .filter(|dialect| *dialect != Dialect::CommonLisp)
        {
            assert_support_error(
                plan_convert_flet_to_labels(flet_request(")", dialect)),
                "convert-flet-to-labels",
            );
            assert_support_error(
                plan_convert_labels_to_flet(labels_request(")", dialect)),
                "convert-labels-to-flet",
            );
        }
    }

    #[test]
    fn preserves_common_lisp_delimiter_character_literals() {
        let flet = plan_convert_flet_to_labels(flet_request(
            "(flet ((work () #\\))) (work))",
            Dialect::CommonLisp,
        ))
        .expect("flet character literal plan");
        assert_eq!(flet.rewritten, "(labels ((work () #\\))) (work))");
        SyntaxTree::parse_with_dialect(&flet.rewritten, Dialect::CommonLisp)
            .expect("flet output must reparse as Common Lisp");

        let labels = plan_convert_labels_to_flet(labels_request(
            "(labels ((work () #\\))) (work))",
            Dialect::CommonLisp,
        ))
        .expect("labels character literal plan");
        assert_eq!(labels.rewritten, "(flet ((work () #\\))) (work))");
        SyntaxTree::parse_with_dialect(&labels.rewritten, Dialect::CommonLisp)
            .expect("labels output must reparse as Common Lisp");
    }
}
