//! Use case for expanding one conservative Common Lisp `symbol-macrolet` binding.

use anyhow::{Context, Result, bail};

use super::mutation_safety::reject_common_lisp_reader_conditionals;
use crate::domain::common_lisp::{
    common_lisp_symbol_reference_eq, is_common_lisp_declaration_form,
};
use crate::domain::dialect::Dialect;
use crate::domain::inline_let::{InlineLetRequest, plan_inline_let};
use crate::domain::lexical_scope::collect_unshadowed_symbol_references;
use crate::domain::sexpr::reader::atom_symbol_text;
use crate::domain::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree,
};

#[derive(Debug, Clone)]
pub struct InlineSymbolMacroRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineSymbolMacroPlan {
    pub dialect: Dialect,
    pub path: Path,
    pub form_span: ByteSpan,
    pub binding_name: SymbolName,
    pub binding_value: String,
    pub reference_count: usize,
    pub replacement: String,
    pub rewritten: String,
    pub changed: bool,
}

pub fn plan_inline_symbol_macro(
    request: InlineSymbolMacroRequest<'_>,
) -> Result<InlineSymbolMacroPlan> {
    if request.dialect != Dialect::CommonLisp {
        bail!("inline-symbol-macro currently supports only Common Lisp");
    }
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("inline-symbol-macro input is not a valid S-expression document")?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    let form = tree.select_path(&request.path)?.view();
    if tree.has_comment_in(form.span) {
        bail!("inline-symbol-macro cannot replace a form containing comments");
    }
    if contains_reader_prefix(&form) {
        bail!("inline-symbol-macro requires a form without reader prefixes");
    }
    require_head(&form, "symbol-macrolet")?;
    if form.children.len() != 3 {
        bail!("inline-symbol-macro requires exactly one body expression");
    }

    let bindings = &form.children[1];
    if bindings.kind != ExpressionKind::List || bindings.children.len() != 1 {
        bail!("inline-symbol-macro requires exactly one binding");
    }
    let binding = &bindings.children[0];
    if binding.kind != ExpressionKind::List || binding.children.len() != 2 {
        bail!("inline-symbol-macro binding must be a (name expansion) pair");
    }
    let binding_name = plain_symbol(&binding.children[0])?;
    let body = &form.children[2];
    reject_declarations(body)?;

    let mut references = Vec::new();
    collect_unshadowed_symbol_references(
        request.dialect,
        body,
        &binding_name,
        request.input,
        &mut references,
    );
    let mut place_spans = Vec::new();
    collect_place_spans(body, &mut place_spans);
    if references.iter().any(|reference| {
        place_spans.iter().any(|place| {
            place.start().get() <= reference.start().get()
                && reference.end().get() <= place.end().get()
        })
    }) {
        bail!("inline-symbol-macro rejects references used as mutation places");
    }

    let inline = plan_inline_let(InlineLetRequest {
        input: request.input,
        dialect: request.dialect,
        path: Some(request.path.clone()),
        target: form,
        allow_duplicate_evaluation: true,
    })?;
    Ok(InlineSymbolMacroPlan {
        dialect: inline.dialect,
        path: request.path,
        form_span: inline.let_span,
        binding_name: inline.binding_name,
        binding_value: inline.binding_value,
        reference_count: inline.reference_count,
        replacement: inline.replacement,
        rewritten: inline.rewritten,
        changed: inline.changed,
    })
}

fn require_head(view: &ExpressionView, expected: &str) -> Result<()> {
    let matches = view.kind == ExpressionKind::List
        && view
            .children
            .first()
            .and_then(atom_symbol_text)
            .is_some_and(|head| common_lisp_symbol_reference_eq(head, expected));
    if !matches {
        bail!("inline-symbol-macro selection must be a symbol-macrolet form");
    }
    Ok(())
}

fn plain_symbol(view: &ExpressionView) -> Result<SymbolName> {
    if view.kind != ExpressionKind::Atom {
        bail!("inline-symbol-macro binding name must be a plain symbol");
    }
    SymbolName::new(
        atom_symbol_text(view)
            .context("inline-symbol-macro binding name must be a plain symbol")?,
    )
}

fn contains_reader_prefix(view: &ExpressionView) -> bool {
    !view.reader_prefixes.is_empty() || view.children.iter().any(contains_reader_prefix)
}

fn reject_declarations(view: &ExpressionView) -> Result<()> {
    if view.kind == ExpressionKind::List
        && view
            .children
            .first()
            .and_then(atom_symbol_text)
            .is_some_and(is_common_lisp_declaration_form)
    {
        bail!("inline-symbol-macro rejects declarations");
    }
    for child in &view.children {
        reject_declarations(child)?;
    }
    Ok(())
}

fn collect_place_spans(view: &ExpressionView, spans: &mut Vec<ByteSpan>) {
    if view.kind == ExpressionKind::List {
        if let Some(head) = view.children.first().and_then(atom_symbol_text) {
            if ["setq", "psetq", "setf", "psetf"]
                .iter()
                .any(|name| common_lisp_symbol_reference_eq(head, name))
            {
                spans.extend(
                    view.children
                        .iter()
                        .skip(1)
                        .step_by(2)
                        .map(|child| child.span),
                );
            } else if common_lisp_symbol_reference_eq(head, "multiple-value-setq") {
                if let Some(place) = view.children.get(1) {
                    spans.push(place.span);
                }
            } else if [
                "incf", "decf", "push", "pushnew", "pop", "remf", "shiftf", "rotatef",
            ]
            .iter()
            .any(|name| common_lisp_symbol_reference_eq(head, name))
            {
                spans.push(view.span);
            }
        }
    }
    for child in &view.children {
        collect_place_spans(child, spans);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(input: &str) -> Result<InlineSymbolMacroPlan> {
        plan_inline_symbol_macro(InlineSymbolMacroRequest {
            input,
            dialect: Dialect::CommonLisp,
            path: "0".parse()?,
        })
    }

    #[test]
    fn expands_only_unshadowed_value_references() {
        let plan =
            plan("(symbol-macrolet ((x (+ a 1))) (list x (let ((x 2)) x)))").expect("plan inline");
        assert_eq!(plan.reference_count, 1);
        assert_eq!(plan.rewritten, "(list (+ a 1) (let ((x 2)) x))");
    }

    #[test]
    fn rejects_mutation_place_reference() {
        let error =
            plan("(symbol-macrolet ((x (car cell))) (setq x 1))").expect_err("reject place");
        assert!(error.to_string().contains("mutation places"));
    }

    #[test]
    fn rejects_capture_caused_by_expansion() {
        let error =
            plan("(symbol-macrolet ((x (list y))) (let ((y 2)) x))").expect_err("reject capture");
        assert!(error.to_string().contains("capture variable"));
    }

    #[test]
    fn rejects_reader_prefixes() {
        let error = plan("(symbol-macrolet ((x 1)) (list x 'x))").expect_err("reject quote");
        assert!(error.to_string().contains("reader prefixes"));
    }

    #[test]
    fn dialect_matrix_preserves_common_lisp_gate_precedence() {
        let cases = [
            (Dialect::CommonLisp, r"(symbol-macrolet ((x #\))) (list x))"),
            (Dialect::EmacsLisp, ")"),
            (Dialect::Scheme, ")"),
            (Dialect::Clojure, ")"),
            (Dialect::Janet, ")"),
            (Dialect::Fennel, ")"),
            (Dialect::Unknown, ")"),
        ];

        for (dialect, input) in cases {
            let result = plan_inline_symbol_macro(InlineSymbolMacroRequest {
                input,
                dialect,
                path: "0".parse().unwrap(),
            });

            if dialect == Dialect::CommonLisp {
                let plan = result.unwrap();
                assert_eq!(plan.rewritten, r"(list #\))");
                assert!(
                    SyntaxTree::parse_with_dialect(&plan.rewritten, Dialect::CommonLisp).is_ok()
                );
            } else {
                assert_eq!(
                    result.unwrap_err().to_string(),
                    "inline-symbol-macro currently supports only Common Lisp"
                );
            }
        }
    }
}
