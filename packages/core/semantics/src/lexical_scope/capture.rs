//! Variable-capture detection for let-inlining.
//!
//! Inlining a binding splices the binding value's text into each reference
//! site. That is only sound when every free variable of the value resolves to
//! the same binding at the value position and at each site. If a nested binding
//! in the let body shadows one of those free variables, the splice silently
//! changes meaning. This module answers "which free variables would be
//! captured" by reusing the shared shadow-aware reference collector rather than
//! re-deriving shadowing rules.

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SymbolName, SyntaxTree};

use super::syntax::atom_symbol_text;
use super::traversal::{collect_unshadowed_symbol_references, symbol_name_matches};

/// Returns the free variables of `value_view` that a nested binding in the let
/// body would capture if the value were spliced into each `reference_spans`
/// site. An empty result means the inline is capture-safe.
///
/// `scope_span` must cover the whole let form and `reference_spans` must be the
/// substitution sites within it (both anchored in `input`).
#[must_use]
pub fn value_capture(
    dialect: Dialect,
    input: &str,
    scope_span: ByteSpan,
    binding_name: &SymbolName,
    value_view: &ExpressionView,
    reference_spans: &[ByteSpan],
) -> Vec<SymbolName> {
    if reference_spans.is_empty() {
        return Vec::new();
    }
    let mut free_vars = value_free_variables(dialect, value_view, input);
    // The binding name is itself bound by the let form, so it never resolves to
    // a different binding at the reference sites (any inner rebinding would have
    // shadowed the site out of the reference set), and scanning the whole form
    // for it would misread the let's own binding as a shadow.
    free_vars
        .retain(|symbol| !symbol_name_matches(dialect, symbol.as_str(), binding_name.as_str()));
    if free_vars.is_empty() {
        return Vec::new();
    }
    if dialect == Dialect::Unknown {
        return free_vars;
    }

    let base = scope_span.slice(input);
    let scope_start = scope_span.start().get();
    let mut sites: Vec<(usize, usize)> = reference_spans
        .iter()
        .map(|span| {
            (
                span.start().get() - scope_start,
                span.end().get() - scope_start,
            )
        })
        .collect();
    sites.sort_unstable();

    let Ok(original) = SyntaxTree::parse_with_dialect(base, dialect) else {
        return free_vars;
    };
    let original_root = original.root_view();
    let Some(original_form) = original_root.children.first() else {
        return free_vars;
    };

    let mut captured = Vec::new();
    for symbol in &free_vars {
        let original_free = count_unshadowed(dialect, original_form, symbol, base);
        let probed = splice_all(base, &sites, symbol.as_str());
        let Ok(tree) = SyntaxTree::parse_with_dialect(&probed, dialect) else {
            // A value that cannot be safely re-inserted is treated as unsafe.
            captured.push(symbol.clone());
            continue;
        };
        let root = tree.root_view();
        let Some(form) = root.children.first() else {
            captured.push(symbol.clone());
            continue;
        };
        let probed_free = count_unshadowed(dialect, form, symbol, &probed);
        // Each site whose spliced value stays free adds exactly one reference;
        // a shorter total means a site landed inside a shadowing binding.
        if probed_free < original_free + sites.len() {
            captured.push(symbol.clone());
        }
    }
    captured
}

fn value_free_variables(
    dialect: Dialect,
    value_view: &ExpressionView,
    input: &str,
) -> Vec<SymbolName> {
    let mut candidates = Vec::new();
    collect_candidate_symbols(value_view, &mut candidates);
    candidates.sort_unstable();
    candidates.dedup();
    candidates
        .into_iter()
        .filter_map(|name| {
            let symbol = SymbolName::new(name).ok()?;
            let mut refs = Vec::new();
            collect_unshadowed_symbol_references(dialect, value_view, &symbol, input, &mut refs);
            (!refs.is_empty()).then_some(symbol)
        })
        .collect()
}

fn collect_candidate_symbols(view: &ExpressionView, out: &mut Vec<String>) {
    if view.kind == ExpressionKind::Atom {
        if let Some(text) = atom_symbol_text(view) {
            out.push(text.to_owned());
        }
        return;
    }
    for child in &view.children {
        collect_candidate_symbols(child, out);
    }
}

fn count_unshadowed(
    dialect: Dialect,
    form: &ExpressionView,
    symbol: &SymbolName,
    input: &str,
) -> usize {
    let mut refs = Vec::new();
    collect_unshadowed_symbol_references(dialect, form, symbol, input, &mut refs);
    refs.len()
}

fn splice_all(base: &str, sites: &[(usize, usize)], replacement: &str) -> String {
    let mut output = base.to_owned();
    for &(start, end) in sites.iter().rev() {
        output.replace_range(start..end, replacement);
    }
    output
}
