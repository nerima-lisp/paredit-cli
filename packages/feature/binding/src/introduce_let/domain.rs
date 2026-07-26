//! Use case for introducing a local binding around a selected expression.

mod occurrences;
mod rewrite;
mod syntax;
#[cfg(test)]
mod tests;
mod types;

use anyhow::{Context, Result, bail};

use super::mutation_safety::reject_common_lisp_reader_conditionals;
use crate::domain::dialect::{IntroduceLetOperation, VerifiedSemanticPolicy};
use crate::domain::sexpr::{ByteOffset, ByteSpan, Path, SyntaxTree};

use occurrences::{
    EquivalentExpressionSpans, collect_equivalent_expression_spans, is_path_shadowed_by_binding,
    is_span_shadowed_by_binding, rebase_spans,
};
use rewrite::{introduced_let, replace_span, replace_spans_within_span};
pub use types::{IntroduceLetPlan, IntroduceLetRequest};

pub fn plan_introduce_let(request: IntroduceLetRequest<'_>) -> Result<IntroduceLetPlan> {
    let semantic = request
        .dialect
        .verify_introduce_let()
        .context("introduce-let is not supported for this dialect")?;
    let input_tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("introduce-let input is not a valid S-expression document")?;
    reject_common_lisp_reader_conditionals(&input_tree, request.dialect)?;

    let selected_span = request.target.span;
    let binding_value = selected_span.slice(request.input).to_owned();
    let enclosing = request.enclosing_span.slice(request.input);
    let enclosing_tree = SyntaxTree::parse_with_dialect(enclosing, request.dialect)
        .context("failed to parse enclosing list for introduce-let")?;
    let enclosing_view = enclosing_tree.select_path(&Path::root_child(0))?.view();

    let selected_relative_span = ByteSpan::new(
        ByteOffset::new(selected_span.start().get() - request.enclosing_span.start().get()),
        ByteOffset::new(selected_span.end().get() - request.enclosing_span.start().get()),
    );

    if selected_path_shadowed_by_binding(semantic, &request)? {
        bail!(
            "introduce-let target is inside an existing binding for '{}'; choose a different --name",
            request.name.as_str()
        );
    }

    let (occurrence_spans, skipped_shadowed_occurrence_spans) = if request.all_occurrences {
        let mut collection = EquivalentExpressionSpans::default();
        collect_equivalent_expression_spans(
            semantic,
            &enclosing_view,
            &request.target,
            request.name.as_str(),
            false,
            &mut collection,
        );
        (
            rebase_spans(collection.replacement_spans, request.enclosing_span.start()),
            rebase_spans(
                collection.skipped_shadowed_spans,
                request.enclosing_span.start(),
            ),
        )
    } else {
        if is_span_shadowed_by_binding(
            semantic,
            &enclosing_view,
            selected_relative_span,
            request.name.as_str(),
            false,
        ) {
            bail!(
                "introduce-let target is inside an existing binding for '{}'; choose a different --name",
                request.name.as_str()
            );
        }
        (vec![selected_span], Vec::new())
    };

    if !occurrence_spans.contains(&selected_span) {
        bail!(
            "introduce-let target is inside an existing binding for '{}'; choose a different --name",
            request.name.as_str()
        );
    }

    let enclosed_replacement = replace_spans_within_span(
        request.input,
        request.enclosing_span,
        &occurrence_spans,
        request.name.as_str(),
    );
    let replacement = introduced_let(
        request.dialect,
        &request.name,
        &binding_value,
        &enclosed_replacement,
    );
    let rewritten = replace_span(request.input, request.enclosing_span, &replacement);

    SyntaxTree::parse_with_dialect(&rewritten, request.dialect)
        .context("introduced-let output is not a valid S-expression document")?;

    let changed = rewritten != request.input;

    Ok(IntroduceLetPlan {
        dialect: request.dialect,
        path: request.path,
        selected_span,
        enclosing_span: request.enclosing_span,
        name: request.name,
        binding_value,
        occurrence_spans,
        skipped_shadowed_occurrence_spans,
        replacement,
        rewritten,
        changed,
    })
}

fn selected_path_shadowed_by_binding(
    semantic: VerifiedSemanticPolicy<IntroduceLetOperation>,
    request: &IntroduceLetRequest<'_>,
) -> Result<bool> {
    let Some(path) = &request.path else {
        return Ok(false);
    };
    let Some((top_level_index, relative_path)) = path.indexes().split_first() else {
        return Ok(false);
    };

    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("failed to parse document for introduce-let")?;
    let top_level_view = tree
        .select_path(&Path::root_child(top_level_index.get()))?
        .view();

    Ok(is_path_shadowed_by_binding(
        semantic,
        &top_level_view,
        relative_path,
        request.name.as_str(),
        false,
    ))
}
