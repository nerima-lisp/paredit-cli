//! Pure planning rules for inlining a single-binding `let` form.

use anyhow::{Context, Result};

use paredit_core_semantics::lexical_scope::{collect_unshadowed_symbol_references, value_capture};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, Delimiter, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree,
};

#[derive(Debug, Clone)]
pub struct InlineLetRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Option<Path>,
    pub target: ExpressionView,
    pub allow_duplicate_evaluation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineLetPlan {
    pub dialect: Dialect,
    pub path: Option<Path>,
    pub let_span: ByteSpan,
    pub binding_name: SymbolName,
    pub binding_value: String,
    pub body_count: usize,
    pub reference_count: usize,
    pub replacement: String,
    pub rewritten: String,
    pub changed: bool,
}

#[must_use]
pub const fn supports_inline_let_dialect(dialect: Dialect) -> bool {
    matches!(
        dialect,
        Dialect::CommonLisp
            | Dialect::EmacsLisp
            | Dialect::Lfe
            | Dialect::Scheme
            | Dialect::Racket
            | Dialect::Clojure
            | Dialect::Hy
            | Dialect::Carp
            | Dialect::Janet
            | Dialect::Fennel
    )
}

fn require_supported_dialect(dialect: Dialect) -> Result<()> {
    if supports_inline_let_dialect(dialect) {
        return Ok(());
    }

    anyhow::bail!(
        "inline-let requires a known dialect because semantic safety cannot be verified for unknown input"
    )
}

pub fn plan_inline_let(request: InlineLetRequest<'_>) -> Result<InlineLetPlan> {
    require_supported_dialect(request.dialect)?;
    let input_tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("inline-let input is not a valid S-expression document")?;
    paredit_core_edit::mutation_safety::reject_common_lisp_reader_conditionals(
        &input_tree,
        request.dialect,
    )?;
    let target = select_target_from_tree(&input_tree, request.path.as_ref(), &request.target)?;
    let plan = plan(CoreRequest {
        input: request.input,
        dialect: request.dialect,
        path: request.path,
        target,
        allow_duplicate_evaluation: request.allow_duplicate_evaluation,
    })?;
    Ok(InlineLetPlan {
        dialect: plan.dialect,
        path: plan.path,
        let_span: plan.let_span,
        binding_name: plan.binding_name,
        binding_value: plan.binding_value,
        body_count: plan.body_count,
        reference_count: plan.reference_count,
        replacement: plan.replacement,
        rewritten: plan.rewritten,
        changed: plan.changed,
    })
}

#[derive(Debug, Clone)]
pub struct CoreRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Option<Path>,
    pub target: ExpressionView,
    pub allow_duplicate_evaluation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorePlan {
    pub dialect: Dialect,
    pub path: Option<Path>,
    pub let_span: ByteSpan,
    pub binding_name: SymbolName,
    pub binding_value: String,
    pub body_count: usize,
    pub reference_count: usize,
    pub replacement: String,
    pub rewritten: String,
    pub changed: bool,
}

fn select_target_from_tree(
    tree: &SyntaxTree,
    path: Option<&Path>,
    requested_target: &ExpressionView,
) -> Result<ExpressionView> {
    let selected = match path {
        Some(path) => tree.select_path(path)?,
        None => tree.select_at(requested_target.span.start().get())?,
    };
    let target = selected.view();
    if target.span != requested_target.span {
        anyhow::bail!("inline-let target does not match the dialect-aware input tree");
    }
    Ok(target)
}

pub fn plan(request: CoreRequest<'_>) -> Result<CorePlan> {
    require_supported_dialect(request.dialect)?;
    let input_tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("inline-let input is not a valid S-expression document")?;
    let target = select_target_from_tree(&input_tree, request.path.as_ref(), &request.target)?;
    let parts = parts(request.dialect, request.input, &target)?;
    let reference_count = parts.reference_spans.len();
    if reference_count == 0 {
        anyhow::bail!("inline-let would drop an unused binding value");
    }
    if reference_count > 1 && !request.allow_duplicate_evaluation {
        anyhow::bail!(
            "inline-let would duplicate binding value evaluation; pass --allow-duplicate-evaluation to permit it"
        );
    }

    let replacement = replace_body_references(
        request.input,
        parts.body_span,
        &parts.reference_spans,
        &parts.binding_value,
    );
    let rewritten = replace_span(request.input, parts.let_span, &replacement);
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect)
        .context("inline-let output is not a valid S-expression document")?;

    Ok(CorePlan {
        dialect: request.dialect,
        path: request.path,
        let_span: parts.let_span,
        binding_name: parts.binding_name,
        binding_value: parts.binding_value,
        body_count: parts.body_count,
        reference_count,
        changed: rewritten != request.input,
        replacement,
        rewritten,
    })
}

struct Parts {
    let_span: ByteSpan,
    binding_name: SymbolName,
    binding_value: String,
    body_count: usize,
    body_span: ByteSpan,
    reference_spans: Vec<ByteSpan>,
}

fn parts(dialect: Dialect, input: &str, target: &ExpressionView) -> Result<Parts> {
    if target.kind != ExpressionKind::List {
        anyhow::bail!("inline-let selection must be a let list");
    }
    if target.children.len() < 3 {
        anyhow::bail!("inline-let requires one binding and at least one body expression");
    }
    let head = atom_text(&target.children[0]).context("inline-let form must start with an atom")?;
    if !dialect.supports_inline_let_refactor_head(head) {
        anyhow::bail!("inline-let selection must start with let");
    }

    let (binding_name, binding_value_view) = match dialect {
        Dialect::Clojure | Dialect::Hy | Dialect::Carp | Dialect::Janet | Dialect::Fennel => {
            vector_let_binding(&target.children[1])?
        }
        Dialect::CommonLisp
        | Dialect::EmacsLisp
        | Dialect::Lfe
        | Dialect::Scheme
        | Dialect::Racket
        | Dialect::Unknown => list_pair_let_binding(&target.children[1])?,
    };
    let binding_name = SymbolName::new(binding_name)?;
    let mut reference_spans = Vec::new();
    for body in &target.children[2..] {
        collect_unshadowed_symbol_references(
            dialect,
            body,
            &binding_name,
            input,
            &mut reference_spans,
        );
    }
    if let Some(name) = value_capture(
        dialect,
        input,
        target.span,
        &binding_name,
        binding_value_view,
        &reference_spans,
    )
    .first()
    {
        anyhow::bail!(
            "inline-let would capture variable `{name}`: it is free in the binding value but a nested binding in the let body would shadow it, changing the meaning of the code"
        );
    }

    let first_body = &target.children[2];
    let last_body = target
        .children
        .last()
        .context("inline-let body disappeared after validation")?;
    Ok(Parts {
        let_span: target.span,
        binding_name,
        binding_value: binding_value_view.span.slice(input).to_owned(),
        body_count: target.children.len() - 2,
        body_span: ByteSpan::new(first_body.span.start(), last_body.span.end()),
        reference_spans,
    })
}

fn vector_let_binding(binding_form: &ExpressionView) -> Result<(String, &ExpressionView)> {
    if binding_form.kind != ExpressionKind::List
        || binding_form.delimiter != Some(Delimiter::Bracket)
    {
        anyhow::bail!("dialect expects vector let bindings: [name value]");
    }
    if binding_form.children.len() != 2 {
        anyhow::bail!("inline-let currently supports exactly one vector binding");
    }
    let name = atom_text(&binding_form.children[0])
        .context("let binding name must be an atom")?
        .to_owned();
    Ok((name, &binding_form.children[1]))
}

fn list_pair_let_binding(binding_form: &ExpressionView) -> Result<(String, &ExpressionView)> {
    if binding_form.kind != ExpressionKind::List || binding_form.delimiter != Some(Delimiter::Paren)
    {
        anyhow::bail!("dialect expects list-pair let bindings: ((name value))");
    }
    if binding_form.children.len() != 1 {
        anyhow::bail!("inline-let currently supports exactly one list-pair binding");
    }
    let pair = &binding_form.children[0];
    if pair.kind != ExpressionKind::List || pair.delimiter != Some(Delimiter::Paren) {
        anyhow::bail!("let binding must be a (name value) pair");
    }
    if pair.children.len() != 2 {
        anyhow::bail!("let binding pair must contain a name and value");
    }
    let name = atom_text(&pair.children[0])
        .context("let binding name must be an atom")?
        .to_owned();
    Ok((name, &pair.children[1]))
}

fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

fn replace_body_references(
    input: &str,
    body_span: ByteSpan,
    reference_spans: &[ByteSpan],
    replacement: &str,
) -> String {
    let body_start = body_span.start().get();
    let mut output = body_span.slice(input).to_owned();
    let mut spans = reference_spans.to_vec();
    spans.sort_by_key(|span| span.start());
    for span in spans.into_iter().rev() {
        output.replace_range(
            span.start().get() - body_start..span.end().get() - body_start,
            replacement,
        );
    }
    output
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

    fn plan(input: &str, dialect: Dialect, allow_duplicate_evaluation: bool) -> Result<CorePlan> {
        let path = Path::from_indexes(vec![0]);
        let tree = SyntaxTree::parse_with_dialect(input, dialect)?;
        let target = tree.select_path(&path)?.view();
        super::plan(CoreRequest {
            input,
            dialect,
            path: Some(path),
            target,
            allow_duplicate_evaluation,
        })
    }

    #[test]
    fn all_known_dialects_rewrite_and_reparse_with_the_same_dialect() {
        for (dialect, input, expected) in [
            (
                Dialect::CommonLisp,
                "(let ((x 1)) (+ x y)) #\\)",
                "(+ 1 y) #\\)",
            ),
            (
                Dialect::EmacsLisp,
                r"(let ((x 1)) (+ x y)) ?\)",
                r"(+ 1 y) ?\)",
            ),
            (Dialect::Scheme, "(let ((x 1)) (+ x y))", "(+ 1 y)"),
            (Dialect::Clojure, "(let [x 1] (+ x y))", "(+ 1 y)"),
            (Dialect::Janet, "(let [x 1] (+ x y))", "(+ 1 y)"),
            (Dialect::Fennel, "(let [x 1] (+ x y))", "(+ 1 y)"),
        ] {
            assert!(supports_inline_let_dialect(dialect));
            let core = plan(input, dialect, false).unwrap();
            assert_eq!(core.rewritten, expected);
            SyntaxTree::parse_with_dialect(&core.rewritten, dialect).unwrap();

            let path = Path::from_indexes(vec![0]);
            let tree = SyntaxTree::parse_with_dialect(input, dialect).unwrap();
            let target = tree.select_path(&path).unwrap().view();
            let public = plan_inline_let(InlineLetRequest {
                input,
                dialect,
                path: Some(path),
                target,
                allow_duplicate_evaluation: false,
            })
            .unwrap();
            assert_eq!(public.rewritten, core.rewritten);
            SyntaxTree::parse_with_dialect(&public.rewritten, dialect).unwrap();
        }
    }

    #[test]
    fn support_predicate_rejects_only_unknown() {
        assert!(!supports_inline_let_dialect(Dialect::Unknown));
    }

    #[test]
    fn unknown_fails_closed_before_parsing_or_target_traversal() {
        let valid_input = "(let ((x 1)) x)";
        let path = Path::from_indexes(vec![0]);
        let tree = SyntaxTree::parse_with_dialect(valid_input, Dialect::CommonLisp).unwrap();
        let target = tree.select_path(&path).unwrap().view();

        let expected = "inline-let requires a known dialect because semantic safety cannot be verified for unknown input";
        let core_error = super::plan(CoreRequest {
            input: ")",
            dialect: Dialect::Unknown,
            path: Some(path.clone()),
            target: target.clone(),
            allow_duplicate_evaluation: false,
        })
        .unwrap_err();
        assert_eq!(core_error.to_string(), expected);

        let public_error = plan_inline_let(InlineLetRequest {
            input: ")",
            dialect: Dialect::Unknown,
            path: Some(path),
            target,
            allow_duplicate_evaluation: false,
        })
        .unwrap_err();
        assert_eq!(public_error.to_string(), expected);
    }

    #[test]
    fn all_known_dialects_reject_unused_or_duplicated_evaluation() {
        for (dialect, unused, duplicate) in [
            (
                Dialect::CommonLisp,
                "(let ((x (effect))) y)",
                "(let ((x (effect))) (+ x x))",
            ),
            (
                Dialect::EmacsLisp,
                "(let ((x (effect))) y)",
                "(let ((x (effect))) (+ x x))",
            ),
            (
                Dialect::Scheme,
                "(let ((x (effect))) y)",
                "(let ((x (effect))) (+ x x))",
            ),
            (
                Dialect::Clojure,
                "(let [x (effect)] y)",
                "(let [x (effect)] (+ x x))",
            ),
            (
                Dialect::Janet,
                "(let [x (effect)] y)",
                "(let [x (effect)] (+ x x))",
            ),
            (
                Dialect::Fennel,
                "(let [x (effect)] y)",
                "(let [x (effect)] (+ x x))",
            ),
        ] {
            assert!(plan(unused, dialect, false).is_err());
            assert!(plan(duplicate, dialect, false).is_err());
            assert!(plan(duplicate, dialect, true).is_ok());
        }
    }

    #[test]
    fn callable_bindings_are_not_rewritten_when_they_shadow_the_let_binding() {
        for (dialect, input, expected) in [
            (
                Dialect::CommonLisp,
                "(let ((x 1)) (list x (lambda (x) x)))",
                "(list 1 (lambda (x) x))",
            ),
            (
                Dialect::EmacsLisp,
                "(let ((x 1)) (list x (lambda (x) x)))",
                "(list 1 (lambda (x) x))",
            ),
            (
                Dialect::Scheme,
                "(let ((x 1)) (list x (lambda x x)))",
                "(list 1 (lambda x x))",
            ),
            (
                Dialect::Clojure,
                "(let [x 1] (list x (fn x [value] x)))",
                "(list 1 (fn x [value] x))",
            ),
            (
                Dialect::Clojure,
                "(let [x 1] (list x (fn ([x] x) ([x y] x))))",
                "(list 1 (fn ([x] x) ([x y] x)))",
            ),
            (
                Dialect::Janet,
                "(let [x 1] (list x (fn [x] x)))",
                "(list 1 (fn [x] x))",
            ),
            (
                Dialect::Fennel,
                "(let [x 1] (list x (fn [x] x)))",
                "(list 1 (fn [x] x))",
            ),
        ] {
            let shadowed = plan(input, dialect, false).unwrap();
            assert_eq!(shadowed.reference_count, 1);
            assert_eq!(shadowed.rewritten, expected);
            SyntaxTree::parse_with_dialect(&shadowed.rewritten, dialect).unwrap();
        }
    }

    #[test]
    fn rejects_inline_let_when_callable_bindings_would_capture_the_value() {
        for (dialect, input) in [
            (
                Dialect::CommonLisp,
                "(let ((target external)) (lambda (external) target))",
            ),
            (
                Dialect::EmacsLisp,
                "(let ((target external)) (lambda (external) target))",
            ),
            (
                Dialect::Scheme,
                "(let ((target external)) (lambda external target))",
            ),
            (
                Dialect::Clojure,
                "(let [target recur-name] (fn recur-name ([value] target)))",
            ),
            (
                Dialect::Clojure,
                "(let [target external] (fn ([external] target) ([other external] target)))",
            ),
            (
                Dialect::Janet,
                "(let [target external] (fn [external] target))",
            ),
            (
                Dialect::Fennel,
                "(let [target external] (fn [external] target))",
            ),
        ] {
            let error = plan(input, dialect, false).unwrap_err();
            assert!(
                error.to_string().contains("would capture variable"),
                "unexpected {dialect:?} error: {error:#}"
            );
        }
    }
}
