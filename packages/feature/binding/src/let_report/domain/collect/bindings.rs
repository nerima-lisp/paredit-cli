use anyhow::{Context, Result};

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, Delimiter, ExpressionKind, ExpressionView};

use super::super::syntax::atom_text;

#[derive(Debug, Clone)]
pub struct LetBindingCandidate {
    pub index: usize,
    pub name: String,
    pub value_span: ByteSpan,
}

pub fn let_binding_candidates(
    dialect: Dialect,
    binding_form: &ExpressionView,
) -> Result<(&'static str, Vec<LetBindingCandidate>)> {
    match dialect {
        Dialect::Clojure | Dialect::Hy | Dialect::Carp | Dialect::Janet | Dialect::Fennel => {
            vector_let_binding_candidates(binding_form)
        }
        Dialect::CommonLisp
        | Dialect::EmacsLisp
        | Dialect::Lfe
        | Dialect::Scheme
        | Dialect::Racket
        | Dialect::Unknown => list_pair_let_binding_candidates(binding_form),
    }
}

fn vector_let_binding_candidates(
    binding_form: &ExpressionView,
) -> Result<(&'static str, Vec<LetBindingCandidate>)> {
    if binding_form.kind != ExpressionKind::List
        || binding_form.delimiter != Some(Delimiter::Bracket)
    {
        anyhow::bail!("dialect expects vector let bindings: [name value ...]");
    }
    if binding_form.children.len() % 2 != 0 {
        anyhow::bail!("vector let binding form must contain name/value pairs");
    }

    let candidates = binding_form
        .children
        .chunks_exact(2)
        .enumerate()
        .map(|(index, pair)| {
            let name = atom_text(&pair[0])
                .context("let binding name must be an atom")?
                .to_owned();
            Ok(LetBindingCandidate {
                index,
                name,
                value_span: pair[1].span,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(("vector", candidates))
}

fn list_pair_let_binding_candidates(
    binding_form: &ExpressionView,
) -> Result<(&'static str, Vec<LetBindingCandidate>)> {
    if binding_form.kind != ExpressionKind::List || binding_form.delimiter != Some(Delimiter::Paren)
    {
        anyhow::bail!("dialect expects list-pair let bindings: ((name value) ...)");
    }

    let candidates = binding_form
        .children
        .iter()
        .enumerate()
        .map(|(index, pair)| {
            // A bare symbol, or a parenthesized `(name)` with no value form,
            // binds NAME to an implicit nil per the `let`/`let*` binding-list
            // grammar. Use a zero-width span right after NAME as a sentinel
            // for "no explicit value form": `view_at_span` never matches a
            // zero-width span against a real node, so downstream lookups
            // correctly see no value expression to inspect.
            if pair.kind == ExpressionKind::Atom {
                let name = atom_text(pair)
                    .context("let binding name must be an atom")?
                    .to_owned();
                let end = pair.span.end();
                return Ok(LetBindingCandidate {
                    index,
                    name,
                    value_span: ByteSpan::new(end, end),
                });
            }
            if pair.kind != ExpressionKind::List || pair.delimiter != Some(Delimiter::Paren) {
                anyhow::bail!("let binding must be a symbol or a (name value) pair");
            }
            if pair.children.len() == 1 {
                let name = atom_text(&pair.children[0])
                    .context("let binding name must be an atom")?
                    .to_owned();
                let end = pair.span.end();
                return Ok(LetBindingCandidate {
                    index,
                    name,
                    value_span: ByteSpan::new(end, end),
                });
            }
            if pair.children.len() != 2 {
                anyhow::bail!("let binding pair must contain a name and value");
            }
            let name = atom_text(&pair.children[0])
                .context("let binding name must be an atom")?
                .to_owned();
            Ok(LetBindingCandidate {
                index,
                name,
                value_span: pair.children[1].span,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(("list-pair", candidates))
}
