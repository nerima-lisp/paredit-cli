//! Dialect-aware conversion between `if` and `cond` forms.

use crate::error::{
    ConservativeRefusal, DialectRefusal, DocumentRefusal, EditResult, ShapeRefusal,
};

use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, Path, SyntaxTree};

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

pub fn plan_convert_if_to_cond(
    request: ConvertIfToCondRequest<'_>,
) -> EditResult<ConvertIfToCondPlan> {
    require_supported_dialect(request.dialect, "convert-if-to-cond")?;
    let tree =
        SyntaxTree::parse_with_dialect(request.input, request.dialect).map_err(|source| {
            DocumentRefusal::InputNotAnSexprDocument {
                operation: "convert-if-to-cond",
                source,
            }
        })?;
    let form = tree.select_path(&request.path)?.view();
    if tree.has_comment_in(form.span) {
        return Err(ConservativeRefusal::Comments {
            operation: "convert-if-to-cond",
        }
        .into());
    }
    require_named_form(&form, request.dialect, "if", "convert-if-to-cond")?;
    if !(3..=4).contains(&form.children.len()) {
        return Err(ShapeRefusal::NotIfForm {
            operation: "convert-if-to-cond",
        }
        .into());
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

pub fn plan_convert_cond_to_if(
    request: ConvertCondToIfRequest<'_>,
) -> EditResult<ConvertCondToIfPlan> {
    require_supported_dialect(request.dialect, "convert-cond-to-if")?;
    let tree =
        SyntaxTree::parse_with_dialect(request.input, request.dialect).map_err(|source| {
            DocumentRefusal::InputNotAnSexprDocument {
                operation: "convert-cond-to-if",
                source,
            }
        })?;
    let form = tree.select_path(&request.path)?.view();
    if tree.has_comment_in(form.span) {
        return Err(ConservativeRefusal::Comments {
            operation: "convert-cond-to-if",
        }
        .into());
    }
    require_named_form(&form, request.dialect, "cond", "convert-cond-to-if")?;
    let clauses = &form.children[1..];
    if clauses.is_empty() {
        return Err(ShapeRefusal::NoClauses {
            operation: "convert-cond-to-if",
        }
        .into());
    }
    for clause in clauses {
        if clause.kind != ExpressionKind::List
            || !clause.reader_prefixes.is_empty()
            || clause.children.len() != 2
        {
            return Err(ShapeRefusal::ClauseNotTestAndConsequent {
                operation: "convert-cond-to-if",
            }
            .into());
        }
    }

    let mut replacement = None;
    for (index, clause) in clauses.iter().enumerate().rev() {
        let test = clause.children[0].span.slice(request.input);
        let consequent = clause.children[1].span.slice(request.input);

        // A trailing catch-all becomes the `else` branch rather than a nested
        // `(if t ...)`. Nesting it is correct and does two unhelpful things:
        // it emits a constant test, which this tool's own `constant-if-test`
        // rule reports, and it makes the conversion pair non-terminating —
        // `if → cond` writes a catch-all clause, so a round trip added a level
        // of nesting every time it ran.
        let is_final_catch_all =
            index + 1 == clauses.len() && is_catch_all_test(&clause.children[0], request.input);
        replacement = Some(match (replacement, is_final_catch_all) {
            (None, true) => consequent.to_owned(),
            (None, false) => format!("(if {test} {consequent})"),
            (Some(else_form), _) => format!("(if {test} {consequent} {else_form})"),
        });
    }
    let replacement = replacement.ok_or(ShapeRefusal::ClausesEmpty {
        operation: "convert-cond-to-if",
    })?;
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

/// Whether a `cond` clause's test always succeeds.
///
/// Recognises the three spellings this tool and the surrounding ecosystem
/// actually produce: bare `t`, the reader-prefixed `'t`, and the fully written
/// `(quote t)` that `convert-if-to-cond` emits. Recognising *only* what could
/// be proved constant in general is not the goal — an unrecognised catch-all
/// simply keeps the old nested-`if` output, which stays correct.
///
/// Deliberately not extended to arbitrary non-nil literals. `(cond (42 x))` is
/// a catch-all too, and reading it as one would rewrite code whose author may
/// have meant something else by it.
fn is_catch_all_test(test: &ExpressionView, input: &str) -> bool {
    let is_t = |text: &str| common_lisp_symbol_reference_eq(text, "t");

    match test.kind {
        // `t` and `'t`: the same atom, distinguished by its reader prefix,
        // which `atom_symbol_text` looks past.
        ExpressionKind::Atom => atom_symbol_text(test).is_some_and(is_t),
        // `(quote t)`, written out.
        ExpressionKind::List => {
            test.reader_prefixes.is_empty()
                && test.children.len() == 2
                && atom_symbol_text(&test.children[0])
                    .is_some_and(|head| common_lisp_symbol_reference_eq(head, "quote"))
                && atom_symbol_text(&test.children[1]).is_some_and(is_t)
        }
        _ => {
            let _ = input;
            false
        }
    }
}

pub fn require_supported_dialect(dialect: Dialect, operation: &'static str) -> EditResult<()> {
    if !matches!(dialect, Dialect::CommonLisp | Dialect::EmacsLisp) {
        return Err(DialectRefusal::CurrentlyCommonLispAndEmacsLisp { operation }.into());
    }
    Ok(())
}

fn require_named_form(
    form: &ExpressionView,
    dialect: Dialect,
    name: &str,
    operation: &'static str,
) -> EditResult<()> {
    if form.kind != ExpressionKind::List || !form.reader_prefixes.is_empty() {
        return Err(ShapeRefusal::NotPlainExpectedForm {
            operation,
            expected: name.to_owned(),
        }
        .into());
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
        return Err(ShapeRefusal::NotExpectedForm {
            operation,
            expected: name.to_owned(),
        }
        .into());
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

fn parse_output(rewritten: &str, dialect: Dialect, operation: &'static str) -> EditResult<()> {
    SyntaxTree::parse_with_dialect(rewritten, dialect)
        .map_err(|source| DocumentRefusal::OutputNotAnSexprDocument { operation, source })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `if → cond → if` is now the identity, not merely parseable.
    ///
    /// It used to produce `(if ready yes (if (quote t) no))`: correct, and one
    /// level deeper than it started. Since `if → cond` writes the catch-all
    /// clause that `cond → if` now recognises, the old behaviour meant a caller
    /// alternating the two conversions grew the form without bound.
    #[test]
    fn if_cond_round_trip_is_the_identity_for_both_dialects() {
        for dialect in [Dialect::CommonLisp, Dialect::EmacsLisp] {
            for input in ["(if ready yes no)", "(if ready yes)"] {
                let if_plan = plan_convert_if_to_cond(ConvertIfToCondRequest {
                    input,
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
                assert_eq!(cond_plan.rewritten, input, "round-tripping {input}");
            }
        }
    }

    /// Every spelling of the catch-all a `cond` is likely to carry.
    #[test]
    fn a_trailing_catch_all_clause_becomes_the_else_branch() {
        for (input, expected) in [
            ("(cond (ready yes) (t no))", "(if ready yes no)"),
            ("(cond (ready yes) ('t no))", "(if ready yes no)"),
            ("(cond (ready yes) ((quote t) no))", "(if ready yes no)"),
            // Upper case: Common Lisp reads `T` and `t` as the same symbol.
            ("(cond (ready yes) (T no))", "(if ready yes no)"),
        ] {
            let plan = plan_convert_cond_to_if(ConvertCondToIfRequest {
                input,
                dialect: Dialect::CommonLisp,
                path: "0".parse().expect("path"),
            })
            .expect("cond plan");
            assert_eq!(plan.rewritten, expected, "converting {input}");
        }
    }

    /// A catch-all that is not last is a real test position: the clauses after
    /// it are unreachable, and rewriting it as an `else` would silently delete
    /// them.
    #[test]
    fn a_catch_all_before_the_last_clause_stays_a_test() {
        let plan = plan_convert_cond_to_if(ConvertCondToIfRequest {
            input: "(cond (t first) (ready second))",
            dialect: Dialect::CommonLisp,
            path: "0".parse().expect("path"),
        })
        .expect("cond plan");

        assert_eq!(plan.rewritten, "(if t first (if ready second))");
    }

    /// A single catch-all clause is the whole form: `(cond (t x))` is `x`.
    #[test]
    fn a_lone_catch_all_clause_reduces_to_its_consequent() {
        let plan = plan_convert_cond_to_if(ConvertCondToIfRequest {
            input: "(cond (t (run)))",
            dialect: Dialect::CommonLisp,
            path: "0".parse().expect("path"),
        })
        .expect("cond plan");

        assert_eq!(plan.rewritten, "(run)");
    }

    /// Only `t` counts. A non-nil literal is a catch-all in the language and
    /// not necessarily one in the author's intent, so it is left alone.
    #[test]
    fn a_non_t_constant_test_is_not_treated_as_a_catch_all() {
        let plan = plan_convert_cond_to_if(ConvertCondToIfRequest {
            input: "(cond (ready yes) (42 no))",
            dialect: Dialect::CommonLisp,
            path: "0".parse().expect("path"),
        })
        .expect("cond plan");

        assert_eq!(plan.rewritten, "(if ready yes (if 42 no))");
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
