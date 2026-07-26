//! Dialect-aware conversion between `if` and `cond` forms.

use anyhow::{Context, Result, anyhow, bail};

use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::reader::atom_symbol_text;
use crate::domain::sexpr::{ByteSpan, ExpressionKind, ExpressionView, Path, SyntaxTree};

#[derive(Debug, Clone)]
pub struct ConvertIfToCondRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
}

#[derive(Debug, Clone)]
pub struct ConvertIfToCondPlan {
    pub dialect: Dialect,
    pub path: Path,
    pub form_span: ByteSpan,
    pub has_else: bool,
    pub rewritten: String,
    pub changed: bool,
}

pub fn plan_convert_if_to_cond(request: ConvertIfToCondRequest<'_>) -> Result<ConvertIfToCondPlan> {
    require_supported_dialect(request.dialect, "convert-if-to-cond")?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("convert-if-to-cond input is not a valid S-expression document")?;
    let form = tree.select_path(&request.path)?.view();
    if tree.has_comment_in(form.span) {
        bail!("convert-if-to-cond cannot rewrite a form containing comments");
    }
    require_named_form(&form, request.dialect, "if", "convert-if-to-cond")?;
    if !(3..=4).contains(&form.children.len()) {
        bail!("convert-if-to-cond requires (if test then [else])");
    }

    let test = form.children[1].span.slice(request.input);
    let then = form.children[2].span.slice(request.input);
    let replacement = match form.children.get(3) {
        Some(else_form) => format!(
            "(cond ({test} {then}) ((quote t) {}))",
            else_form.span.slice(request.input)
        ),
        None => format!("(cond ({test} {then}))"),
    };
    let rewritten = replace_span(request.input, form.span, &replacement);
    parse_output(&rewritten, request.dialect, "convert-if-to-cond")?;

    Ok(ConvertIfToCondPlan {
        dialect: request.dialect,
        path: request.path,
        form_span: form.span,
        has_else: form.children.len() == 4,
        changed: rewritten != request.input,
        rewritten,
    })
}

#[derive(Debug, Clone)]
pub struct ConvertCondToIfRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
}

#[derive(Debug, Clone)]
pub struct ConvertCondToIfPlan {
    pub dialect: Dialect,
    pub path: Path,
    pub form_span: ByteSpan,
    pub clause_count: usize,
    pub rewritten: String,
    pub changed: bool,
}

pub fn plan_convert_cond_to_if(request: ConvertCondToIfRequest<'_>) -> Result<ConvertCondToIfPlan> {
    require_supported_dialect(request.dialect, "convert-cond-to-if")?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("convert-cond-to-if input is not a valid S-expression document")?;
    let form = tree.select_path(&request.path)?.view();
    if tree.has_comment_in(form.span) {
        bail!("convert-cond-to-if cannot rewrite a form containing comments");
    }
    require_named_form(&form, request.dialect, "cond", "convert-cond-to-if")?;
    let clauses = &form.children[1..];
    if clauses.is_empty() {
        bail!("convert-cond-to-if requires at least one clause");
    }
    for clause in clauses {
        if clause.kind != ExpressionKind::List
            || !clause.reader_prefixes.is_empty()
            || clause.children.len() != 2
        {
            bail!("convert-cond-to-if requires each clause to contain exactly test and consequent");
        }
    }

    let mut replacement = None;
    for clause in clauses.iter().rev() {
        let test = clause.children[0].span.slice(request.input);
        let consequent = clause.children[1].span.slice(request.input);
        replacement = Some(match replacement {
            Some(else_form) => format!("(if {test} {consequent} {else_form})"),
            None => format!("(if {test} {consequent})"),
        });
    }
    let replacement = replacement.ok_or_else(|| anyhow!("convert-cond-to-if has no clauses"))?;
    let rewritten = replace_span(request.input, form.span, &replacement);
    parse_output(&rewritten, request.dialect, "convert-cond-to-if")?;

    Ok(ConvertCondToIfPlan {
        dialect: request.dialect,
        path: request.path,
        form_span: form.span,
        clause_count: clauses.len(),
        changed: rewritten != request.input,
        rewritten,
    })
}

pub(crate) fn require_supported_dialect(dialect: Dialect, operation: &str) -> Result<()> {
    if !matches!(dialect, Dialect::CommonLisp | Dialect::EmacsLisp) {
        bail!("{operation} currently supports only Common Lisp and Emacs Lisp");
    }
    Ok(())
}

fn require_named_form(
    form: &ExpressionView,
    dialect: Dialect,
    name: &str,
    operation: &str,
) -> Result<()> {
    if form.kind != ExpressionKind::List || !form.reader_prefixes.is_empty() {
        bail!("{operation} selected form must be a plain {name} form");
    }
    let matches = form
        .children
        .first()
        .filter(|head| head.reader_prefixes.is_empty())
        .and_then(atom_symbol_text)
        .is_some_and(|head| match dialect {
            Dialect::CommonLisp => common_lisp_symbol_reference_eq(head, name),
            Dialect::EmacsLisp => head == name,
            _ => false,
        });
    if !matches {
        bail!("{operation} selected form must be a {name} form");
    }
    Ok(())
}

fn replace_span(input: &str, span: ByteSpan, replacement: &str) -> String {
    let mut rewritten = String::with_capacity(input.len() + replacement.len());
    rewritten.push_str(&input[..span.start().get()]);
    rewritten.push_str(replacement);
    rewritten.push_str(&input[span.end().get()..]);
    rewritten
}

fn parse_output(rewritten: &str, dialect: Dialect, operation: &str) -> Result<()> {
    SyntaxTree::parse_with_dialect(rewritten, dialect)
        .with_context(|| format!("{operation} output is not a valid S-expression document"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn if_cond_round_trip_preserves_parseability_for_both_dialects() {
        for dialect in [Dialect::CommonLisp, Dialect::EmacsLisp] {
            let if_plan = plan_convert_if_to_cond(ConvertIfToCondRequest {
                input: "(if ready yes no)",
                dialect,
                path: "0".parse().expect("path"),
            })
            .expect("if plan");
            let cond_plan = plan_convert_cond_to_if(ConvertCondToIfRequest {
                input: &if_plan.rewritten,
                dialect,
                path: "0".parse().expect("path"),
            })
            .expect("cond plan");
            assert_eq!(cond_plan.rewritten, "(if ready yes (if (quote t) no))");
        }
    }

    #[test]
    fn rejects_unsupported_dialect_and_non_plain_forms() {
        assert!(
            plan_convert_if_to_cond(ConvertIfToCondRequest {
                input: "(if test then)",
                dialect: Dialect::Clojure,
                path: "0".parse().expect("path"),
            })
            .is_err()
        );
        assert!(
            plan_convert_cond_to_if(ConvertCondToIfRequest {
                input: "'(cond (test body))",
                dialect: Dialect::EmacsLisp,
                path: "0".parse().expect("path"),
            })
            .is_err()
        );
    }

    #[test]
    fn rejects_malformed_clauses_comments_and_arity() {
        for input in [
            "(cond)",
            "(cond (test))",
            "(cond (test one two))",
            "(cond test)",
        ] {
            assert!(
                plan_convert_cond_to_if(ConvertCondToIfRequest {
                    input,
                    dialect: Dialect::CommonLisp,
                    path: "0".parse().expect("path"),
                })
                .is_err()
            );
        }
        assert!(
            plan_convert_if_to_cond(ConvertIfToCondRequest {
                input: "(if test ; keep\n then)",
                dialect: Dialect::CommonLisp,
                path: "0".parse().expect("path"),
            })
            .is_err()
        );
        assert!(
            plan_convert_if_to_cond(ConvertIfToCondRequest {
                input: "(if test then else extra)",
                dialect: Dialect::EmacsLisp,
                path: "0".parse().expect("path"),
            })
            .is_err()
        );
    }

    #[test]
    fn dialect_support_matrix_is_enforced_before_parsing_and_reparses_output() {
        for (dialect, prefix) in [(Dialect::CommonLisp, "#\\)"), (Dialect::EmacsLisp, "?\\)")] {
            let if_input = format!("{prefix} (if ready yes no)");
            let if_plan = plan_convert_if_to_cond(ConvertIfToCondRequest {
                input: &if_input,
                dialect,
                path: "1".parse().expect("path"),
            })
            .expect("supported if conversion");
            SyntaxTree::parse_with_dialect(&if_plan.rewritten, dialect)
                .expect("dialect-specific cond output");

            let cond_input = format!("{prefix} (cond (ready yes) ((quote t) no))");
            let cond_plan = plan_convert_cond_to_if(ConvertCondToIfRequest {
                input: &cond_input,
                dialect,
                path: "1".parse().expect("path"),
            })
            .expect("supported cond conversion");
            SyntaxTree::parse_with_dialect(&cond_plan.rewritten, dialect)
                .expect("dialect-specific if output");
        }

        for dialect in [
            Dialect::Scheme,
            Dialect::Clojure,
            Dialect::Janet,
            Dialect::Fennel,
            Dialect::Unknown,
        ] {
            let if_error = plan_convert_if_to_cond(ConvertIfToCondRequest {
                input: ")",
                dialect,
                path: "0".parse().expect("path"),
            })
            .expect_err("unsupported if conversion");
            assert!(
                if_error
                    .to_string()
                    .contains("currently supports only Common Lisp and Emacs Lisp"),
                "{dialect:?}: {if_error:#}"
            );

            let cond_error = plan_convert_cond_to_if(ConvertCondToIfRequest {
                input: ")",
                dialect,
                path: "0".parse().expect("path"),
            })
            .expect_err("unsupported cond conversion");
            assert!(
                cond_error
                    .to_string()
                    .contains("currently supports only Common Lisp and Emacs Lisp"),
                "{dialect:?}: {cond_error:#}"
            );
        }
    }
}
