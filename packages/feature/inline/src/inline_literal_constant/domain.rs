//! Use case for inlining an immutable, self-evaluating Common Lisp constant.

use anyhow::{Context, Result, bail};

use super::mutation_safety::reject_common_lisp_reader_conditionals;
use super::rename::collect_define_symbol_macro_reference_renames;
use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::reader::atom_symbol_text;
use crate::domain::sexpr::{
    ByteOffset, ByteSpan, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree,
};

#[derive(Debug, Clone)]
pub struct InlineLiteralConstantRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineLiteralConstantPlan {
    pub dialect: Dialect,
    pub path: Path,
    pub definition_span: ByteSpan,
    pub constant_name: SymbolName,
    pub literal: String,
    pub reference_count: usize,
    pub rewritten: String,
    pub changed: bool,
}

pub fn plan_inline_literal_constant(
    request: InlineLiteralConstantRequest<'_>,
) -> Result<InlineLiteralConstantPlan> {
    if request.dialect != Dialect::CommonLisp {
        bail!("inline-literal-constant supports only Common Lisp");
    }
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("inline-literal-constant input is not a valid S-expression document")?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    let definition = tree.select_path(&request.path)?.view();
    require_top_level_path(&request.path)?;
    if tree.has_comment_in(definition.span) {
        bail!("inline-literal-constant cannot remove a definition containing comments");
    }
    let (constant_name, literal) = parse_definition(&definition, request.input)?;
    reject_duplicate_definitions(&tree, &constant_name)?;

    let placeholder = SymbolName::new("inline-literal-constant-replacement")?;
    let mut references = collect_define_symbol_macro_reference_renames(
        &tree,
        request.dialect,
        &constant_name,
        &placeholder,
    )?;
    references.retain(|reference| !contains_span(definition.span, reference.span));
    if references.is_empty() {
        bail!("inline-literal-constant requires at least one safe value reference");
    }
    let mut place_spans = Vec::new();
    for (index, _) in tree.root_children().iter().enumerate() {
        let child = tree.select_path(&Path::root_child(index))?.view();
        collect_place_spans(&child, &mut place_spans);
    }
    if references.iter().any(|reference| {
        place_spans
            .iter()
            .any(|place| contains_span(*place, reference.span))
    }) {
        bail!("inline-literal-constant rejects references used as mutation places");
    }

    let mut edits = references
        .iter()
        .map(|reference| (reference.span, literal.clone()))
        .collect::<Vec<_>>();
    edits.push((
        definition_removal_span(request.input, definition.span),
        String::new(),
    ));
    edits.sort_by_key(|(span, _)| std::cmp::Reverse(span.start()));
    let mut rewritten = request.input.to_owned();
    for (span, replacement) in edits {
        rewritten.replace_range(span.start().get()..span.end().get(), &replacement);
    }
    let rewritten = collapse_removed_definition_gap(&rewritten);
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect)
        .context("inline-literal-constant output is not a valid S-expression document")?;

    Ok(InlineLiteralConstantPlan {
        dialect: request.dialect,
        path: request.path,
        definition_span: definition.span,
        constant_name,
        literal,
        reference_count: references.len(),
        changed: rewritten != request.input,
        rewritten,
    })
}

fn require_top_level_path(path: &Path) -> Result<()> {
    if path.indexes().len() != 1 {
        bail!("inline-literal-constant requires a top-level defconstant path");
    }
    Ok(())
}

fn parse_definition(definition: &ExpressionView, input: &str) -> Result<(SymbolName, String)> {
    if definition.kind != ExpressionKind::List || definition.children.len() != 3 {
        bail!("inline-literal-constant requires (defconstant name literal)");
    }
    let head = plain_symbol(&definition.children[0])
        .context("inline-literal-constant requires a plain defconstant head")?;
    if !common_lisp_symbol_reference_eq(head, "defconstant") {
        bail!("inline-literal-constant selection must be a defconstant form");
    }
    let name_text = plain_symbol(&definition.children[1])
        .context("inline-literal-constant requires a plain constant name")?;
    if name_text.contains(':') || name_text.starts_with('&') {
        bail!("inline-literal-constant requires an unqualified constant name");
    }
    let name = SymbolName::new(name_text)?;
    let value = &definition.children[2];
    let literal = &input[value.span.start().get()..value.span.end().get()];
    if !is_safe_literal(value, literal) {
        bail!(
            "inline-literal-constant supports only immutable self-evaluating literals (numbers, characters, T, NIL, and keywords)"
        );
    }
    Ok((name, literal.to_owned()))
}

fn plain_symbol(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom && view.reader_prefixes.is_empty())
        .then(|| atom_symbol_text(view))
        .flatten()
}

fn is_safe_literal(view: &ExpressionView, source: &str) -> bool {
    if view.kind != ExpressionKind::Atom || !view.children.is_empty() {
        return false;
    }
    if source.starts_with("#\\") {
        return source.len() > 2 && !source.chars().any(char::is_whitespace);
    }
    if !view.reader_prefixes.is_empty() {
        return false;
    }
    let Some(atom) = atom_symbol_text(view) else {
        return false;
    };
    common_lisp_symbol_reference_eq(atom, "t")
        || common_lisp_symbol_reference_eq(atom, "nil")
        || is_keyword(atom)
        || is_conservative_number(atom)
}

fn is_keyword(atom: &str) -> bool {
    atom.starts_with(':') && atom.len() > 1 && !atom[1..].contains(':')
}

fn is_conservative_number(atom: &str) -> bool {
    let unsigned = atom.strip_prefix(['+', '-']).unwrap_or(atom);
    if unsigned.is_empty() {
        return false;
    }
    if let Some((numerator, denominator)) = unsigned.split_once('/') {
        return !numerator.is_empty()
            && !denominator.is_empty()
            && numerator.bytes().all(|byte| byte.is_ascii_digit())
            && denominator.bytes().all(|byte| byte.is_ascii_digit())
            && denominator.bytes().any(|byte| byte != b'0');
    }
    if unsigned.bytes().all(|byte| byte.is_ascii_digit()) {
        return true;
    }
    unsigned.contains(['.', 'e', 'E'])
        && unsigned.parse::<f64>().is_ok()
        && unsigned.bytes().any(|byte| byte.is_ascii_digit())
}

fn reject_duplicate_definitions(tree: &SyntaxTree, name: &SymbolName) -> Result<()> {
    let mut count = 0;
    for (index, _) in tree.root_children().iter().enumerate() {
        let view = tree.select_path(&Path::root_child(index))?.view();
        if view.kind == ExpressionKind::List
            && view.children.len() >= 2
            && view
                .children
                .first()
                .and_then(plain_symbol)
                .is_some_and(|head| common_lisp_symbol_reference_eq(head, "defconstant"))
            && view
                .children
                .get(1)
                .and_then(plain_symbol)
                .is_some_and(|candidate| common_lisp_symbol_reference_eq(candidate, name.as_str()))
        {
            count += 1;
        }
    }
    if count != 1 {
        bail!("inline-literal-constant requires exactly one matching defconstant definition");
    }
    Ok(())
}

fn contains_span(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start() <= inner.start() && inner.end() <= outer.end()
}

fn definition_removal_span(input: &str, span: ByteSpan) -> ByteSpan {
    let suffix = &input[span.end().get()..];
    let end = if suffix.starts_with("\n\n") {
        span.end().get() + 1
    } else if suffix.starts_with("\r\n\r\n") {
        span.end().get() + 2
    } else {
        span.end().get()
    };
    ByteSpan::new(span.start(), ByteOffset::new(end))
}

fn collect_place_spans(view: &ExpressionView, spans: &mut Vec<ByteSpan>) {
    if view.kind == ExpressionKind::List {
        if let Some(head) = view.children.first().and_then(plain_symbol) {
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
                spans.extend(view.children.get(1).map(|child| child.span));
            } else if ["incf", "decf", "pop", "remf"]
                .iter()
                .any(|name| common_lisp_symbol_reference_eq(head, name))
            {
                spans.extend(view.children.get(1).map(|child| child.span));
            } else if ["push", "pushnew"]
                .iter()
                .any(|name| common_lisp_symbol_reference_eq(head, name))
            {
                spans.extend(view.children.get(2).map(|child| child.span));
            } else if common_lisp_symbol_reference_eq(head, "shiftf") {
                spans.extend(
                    view.children
                        .iter()
                        .skip(1)
                        .take(view.children.len().saturating_sub(2))
                        .map(|child| child.span),
                );
            } else if common_lisp_symbol_reference_eq(head, "rotatef") {
                spans.extend(view.children.iter().skip(1).map(|child| child.span));
            }
        }
    }
    for child in &view.children {
        collect_place_spans(child, spans);
    }
}

fn collapse_removed_definition_gap(input: &str) -> String {
    let mut output = input.to_owned();
    while output.contains("\n\n\n") {
        output = output.replace("\n\n\n", "\n\n");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(input: &str) -> Result<InlineLiteralConstantPlan> {
        plan_inline_literal_constant(InlineLiteralConstantRequest {
            input,
            dialect: Dialect::CommonLisp,
            path: "0".parse()?,
        })
    }

    #[test]
    fn inlines_safe_references_and_removes_definition() {
        let result = plan("(defconstant +answer+ 42)\n\n(defun answer () +answer+)\n").unwrap();
        assert_eq!(result.reference_count, 1);
        assert_eq!(result.rewritten, "\n(defun answer () 42)\n");
    }

    #[test]
    fn skips_quoted_and_shadowed_occurrences() {
        let result =
            plan("(defconstant +x+ :ok)\n(defun f (+x+) (list +x+ '+x+))\n(defun g () +x+)")
                .unwrap();
        assert_eq!(result.reference_count, 1);
        assert!(result.rewritten.contains("(defun g () :ok)"));
        assert!(result.rewritten.contains("(defun f (+x+) (list +x+ '+x+))"));
    }

    #[test]
    fn rejects_mutable_or_computed_values_and_places() {
        for input in [
            "(defconstant +x+ \"text\") (+ +x+ 1)",
            "(defconstant +x+ '(1 2)) (+ +x+ 1)",
            "(defconstant +x+ (+ 1 2)) (+ +x+ 1)",
            "(defconstant +x+ 1) (setq +x+ 2)",
        ] {
            assert!(plan(input).is_err(), "unexpectedly accepted {input}");
        }
    }

    #[test]
    fn rejects_no_reference_and_duplicate_definition() {
        assert!(plan("(defconstant +x+ 1) (print 2)").is_err());
        assert!(plan("(defconstant +x+ 1) (defconstant +x+ 2) (print +x+)").is_err());
    }

    #[test]
    fn permits_literal_reference_in_non_place_argument() {
        let result = plan("(defconstant +step+ 2) (incf value +step+)").unwrap();
        assert_eq!(result.rewritten, " (incf value 2)");
    }

    #[test]
    fn dialect_support_matrix_is_enforced_before_parsing_and_reparses_output() {
        let plan = plan_inline_literal_constant(InlineLiteralConstantRequest {
            input: "#\\)\n(defconstant +x+ 1)\n(print +x+)",
            dialect: Dialect::CommonLisp,
            path: "1".parse().expect("path"),
        })
        .expect("Common Lisp");
        assert!(plan.rewritten.starts_with("#\\)"));
        SyntaxTree::parse_with_dialect(&plan.rewritten, Dialect::CommonLisp)
            .expect("Common Lisp output");

        for dialect in [
            Dialect::EmacsLisp,
            Dialect::Scheme,
            Dialect::Clojure,
            Dialect::Janet,
            Dialect::Fennel,
            Dialect::Unknown,
        ] {
            let error = plan_inline_literal_constant(InlineLiteralConstantRequest {
                input: ")",
                dialect,
                path: "0".parse().expect("path"),
            })
            .expect_err("unsupported dialect");
            assert!(
                error.to_string().contains("supports only Common Lisp"),
                "{dialect:?}: {error:#}"
            );
        }
    }
}
