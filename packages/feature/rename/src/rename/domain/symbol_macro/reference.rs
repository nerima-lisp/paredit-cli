use crate::error::RenameResult;

use paredit_core_syntax::common_lisp::{
    CommonLispBindingRefactorForm, CommonLispOperator, common_lisp_binding_refactor_form_for_head,
    common_lisp_symbol_reference_eq, is_common_lisp_declaration_form,
};
use paredit_core_syntax::definition::definition_shape;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, Delimiter, ExpressionKind, ExpressionView, NonEmptyExpressionPath, Path,
    ReaderPrefix, SymbolName, SyntaxTree,
};

use super::super::RenameFunctionOccurrence;
use super::super::binding::collect_shadow_aware_special_form;
use super::super::binding::collect_symbol_atom_spans_unshadowed_ignoring_declared_specials;
use super::super::binding::parameter_form_binds;
use super::super::reader::atom_symbol_span;
use super::super::reader::{
    apply_reader_prefix_context, explicit_reader_form_kind,
    explicit_reader_function_lambda_body_children,
};
use super::super::selection::atom_text;
use super::shared::is_target_define_symbol_macro;

#[derive(Debug, Clone)]
struct SymbolReferenceSite {
    path: ReferencePath,
    span: ByteSpan,
    is_head_position: bool,
}

#[derive(Debug, Clone, Copy)]
struct ReferencePath(usize);

struct ReferencePathNode {
    parent: Option<usize>,
    index: usize,
}

struct ReferencePathArena {
    nodes: Vec<ReferencePathNode>,
    edge_count: usize,
    materialized_index_count: usize,
}

impl ReferencePathArena {
    fn from_path(path: NonEmptyExpressionPath<'_>) -> (Self, ReferencePath) {
        let mut nodes = Vec::with_capacity(path.indexes().len());
        let mut parent = None;
        for index in path.indexes() {
            let node = nodes.len();
            nodes.push(ReferencePathNode { parent, index });
            parent = Some(node);
        }
        let current = parent.unwrap_or_else(|| unreachable!("non-empty path has a root node"));
        (
            Self {
                nodes,
                edge_count: 0,
                materialized_index_count: 0,
            },
            ReferencePath(current),
        )
    }

    fn child(&mut self, path: ReferencePath, index: usize) -> ReferencePath {
        let node = self.nodes.len();
        self.nodes.push(ReferencePathNode {
            parent: Some(path.0),
            index,
        });
        self.edge_count += 1;
        ReferencePath(node)
    }

    fn materialize(&mut self, path: ReferencePath) -> Path {
        let mut indexes = Vec::new();
        let mut cursor = Some(path.0);
        while let Some(node) = cursor {
            indexes.push(self.nodes[node].index);
            cursor = self.nodes[node].parent;
        }
        self.materialized_index_count += indexes.len();
        indexes.reverse();
        Path::from_indexes(indexes)
    }
}

pub fn collect_define_symbol_macro_reference_renames(
    tree: &SyntaxTree,
    dialect: Dialect,
    from: &SymbolName,
    to: &SymbolName,
) -> RenameResult<Vec<RenameFunctionOccurrence>> {
    let mut renames = Vec::new();

    for (top_index, _) in tree.root_children().iter().enumerate() {
        let form_path = Path::root_child(top_index);
        let view = tree.select_path(&form_path)?.view();

        if is_target_define_symbol_macro(&view, dialect, from) {
            continue;
        }

        collect_reference_renames_from_view(&view, form_path, dialect, from, to, &mut renames);
    }

    renames.sort_by_key(|rename| rename.span.start());
    renames.dedup_by(|left, right| left.span == right.span);
    Ok(renames)
}

fn collect_reference_renames_from_view(
    view: &ExpressionView,
    path: Path,
    dialect: Dialect,
    from: &SymbolName,
    to: &SymbolName,
    renames: &mut Vec<RenameFunctionOccurrence>,
) {
    let mut reference_spans = Vec::new();
    let mut shadowed_scope_count = 0usize;
    collect_symbol_atom_spans_unshadowed_ignoring_declared_specials(
        view,
        from,
        &mut reference_spans,
        &mut shadowed_scope_count,
        "",
    );
    collect_symbol_atom_spans_unshadowed_in_reader_function_lambdas(
        view,
        from,
        &mut reference_spans,
        &mut shadowed_scope_count,
        "",
    );
    reference_spans.sort_by_key(|span| (span.start(), span.end()));
    reference_spans.dedup();
    if reference_spans.is_empty() {
        return;
    }

    let mut sites = Vec::new();
    let Some(mut paths) =
        collect_symbol_reference_sites(view, path, false, dialect, from, &mut sites)
    else {
        return;
    };

    let (matching_site_indexes, _) = match_reference_spans_to_sites(&reference_spans, &mut sites);
    for site_index in matching_site_indexes {
        let site = &sites[site_index];
        renames.push(RenameFunctionOccurrence {
            path: paths.materialize(site.path).to_string(),
            span: site.span,
            text: from.as_str().to_owned(),
            replacement: to.as_str().to_owned(),
        });
    }
}

fn match_reference_spans_to_sites(
    reference_spans: &[ByteSpan],
    sites: &mut [SymbolReferenceSite],
) -> (Vec<usize>, usize) {
    sites.sort_by_key(|site| (site.span.start(), site.span.end(), site.is_head_position));

    let mut matches = Vec::with_capacity(reference_spans.len().min(sites.len()));
    let mut reference_index = 0usize;
    let mut site_index = 0usize;
    let mut probes = 0usize;

    while reference_index < reference_spans.len() && site_index < sites.len() {
        let reference_key = span_key(reference_spans[reference_index]);
        let site_key = span_key(sites[site_index].span);

        match site_key.cmp(&reference_key) {
            std::cmp::Ordering::Less => {
                probes += 1;
                site_index += 1;
            }
            std::cmp::Ordering::Greater => {
                probes += 1;
                reference_index += 1;
            }
            std::cmp::Ordering::Equal => {
                let mut matching_site = None;
                while site_index < sites.len() && span_key(sites[site_index].span) == reference_key
                {
                    probes += 1;
                    if matching_site.is_none() && !sites[site_index].is_head_position {
                        matching_site = Some(site_index);
                    }
                    site_index += 1;
                }
                if let Some(matching_site) = matching_site {
                    matches.push(matching_site);
                }
                reference_index += 1;
            }
        }
    }

    (matches, probes)
}

const fn span_key(
    span: ByteSpan,
) -> (
    paredit_core_syntax::sexpr::ByteOffset,
    paredit_core_syntax::sexpr::ByteOffset,
) {
    (span.start(), span.end())
}

fn collect_symbol_atom_spans_unshadowed_in_reader_function_lambdas(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    input: &str,
) {
    collect_symbol_spans_in_context(view, symbol, output, shadowed_scope_count, input, 0);
}

#[allow(clippy::too_many_arguments)]
fn collect_symbol_spans_in_context(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    input: &str,
    quasiquote_depth: usize,
) {
    let Some(quasiquote_depth) = apply_reader_prefix_context(view, quasiquote_depth) else {
        return;
    };

    if view.kind == ExpressionKind::Atom {
        if view.reader_prefixes.contains(&ReaderPrefix::Function) {
            return;
        }

        if quasiquote_depth == 0
            && atom_text(view)
                .is_some_and(|text| common_lisp_symbol_reference_eq(text, symbol.as_str()))
        {
            if let Some(span) = atom_symbol_span(view) {
                output.push(span);
            }
        }
        return;
    }

    if is_common_lisp_declaration_view(view) {
        return;
    }

    if collect_explicit_reader_form_symbol_spans(
        view,
        symbol,
        output,
        shadowed_scope_count,
        input,
        quasiquote_depth,
    ) {
        return;
    }

    if quasiquote_depth > 0 {
        for child in &view.children {
            collect_symbol_spans_in_context(
                child,
                symbol,
                output,
                shadowed_scope_count,
                input,
                quasiquote_depth,
            );
        }
        return;
    }

    if collect_common_lisp_function_definition_symbol_spans(
        view,
        symbol,
        output,
        shadowed_scope_count,
        input,
    ) {
        return;
    }

    if collect_shadow_aware_special_form(view, symbol, output, shadowed_scope_count, input) {
        return;
    }

    for child in &view.children {
        collect_symbol_spans_in_context(child, symbol, output, shadowed_scope_count, input, 0);
    }
}

fn is_common_lisp_declaration_view(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::List
        && view
            .children
            .first()
            .and_then(atom_text)
            .is_some_and(is_common_lisp_declaration_form)
}

fn collect_common_lisp_function_definition_symbol_spans(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    input: &str,
) -> bool {
    let Some(head) = view.children.first().and_then(atom_text) else {
        return false;
    };

    if common_lisp_binding_refactor_form_for_head(head)
        != Some(CommonLispBindingRefactorForm::FunctionDefinition)
    {
        return false;
    }

    let Some(shape) = definition_shape(Dialect::CommonLisp, view, head) else {
        return false;
    };

    if shape
        .lambda_list(view)
        .is_some_and(|parameter_form| parameter_form_binds(parameter_form, symbol, input))
    {
        *shadowed_scope_count += 1;
        return true;
    }

    if matches!(
        CommonLispOperator::from_head(head),
        Some(CommonLispOperator::DefineSetfExpander | CommonLispOperator::DefineCompilerMacro)
    ) {
        return true;
    }

    for body in shape.body_forms(view) {
        collect_symbol_spans_in_context(body, symbol, output, shadowed_scope_count, input, 0);
    }

    true
}

#[allow(clippy::too_many_arguments)]
fn collect_explicit_reader_form_symbol_spans(
    view: &ExpressionView,
    symbol: &SymbolName,
    output: &mut Vec<ByteSpan>,
    shadowed_scope_count: &mut usize,
    input: &str,
    quasiquote_depth: usize,
) -> bool {
    if view.kind != ExpressionKind::List || view.children.len() < 2 {
        return false;
    }

    let Some(kind_name) = explicit_reader_form_kind(view) else {
        return false;
    };

    match kind_name.as_str() {
        "quote" => true,
        "function" if quasiquote_depth == 0 => {
            if let Some(children) = explicit_reader_function_lambda_body_children(view) {
                for (_, child) in children {
                    collect_symbol_spans_in_context(
                        child,
                        symbol,
                        output,
                        shadowed_scope_count,
                        input,
                        quasiquote_depth,
                    );
                }
            }
            true
        }
        "function" => true,
        "quasiquote" => {
            for child in &view.children[1..] {
                collect_symbol_spans_in_context(
                    child,
                    symbol,
                    output,
                    shadowed_scope_count,
                    input,
                    quasiquote_depth + 1,
                );
            }
            true
        }
        "unquote" | "unquote-splicing" if quasiquote_depth > 0 => {
            for child in &view.children[1..] {
                collect_symbol_spans_in_context(
                    child,
                    symbol,
                    output,
                    shadowed_scope_count,
                    input,
                    quasiquote_depth - 1,
                );
            }
            true
        }
        _ => false,
    }
}

fn collect_symbol_reference_sites(
    view: &ExpressionView,
    path: Path,
    is_head_position: bool,
    dialect: Dialect,
    from: &SymbolName,
    sites: &mut Vec<SymbolReferenceSite>,
) -> Option<ReferencePathArena> {
    struct Frame<'a> {
        view: &'a ExpressionView,
        path: ReferencePath,
        is_head_position: bool,
    }

    let path = NonEmptyExpressionPath::try_from(&path).ok()?;
    let (mut paths, root_path) = ReferencePathArena::from_path(path);
    let mut stack = vec![Frame {
        view,
        path: root_path,
        is_head_position,
    }];

    while let Some(frame) = stack.pop() {
        if is_target_define_symbol_macro(frame.view, dialect, from) {
            continue;
        }

        if frame.view.kind == ExpressionKind::Atom {
            if let Some(span) = atom_symbol_span(frame.view) {
                sites.push(SymbolReferenceSite {
                    span,
                    path: frame.path,
                    is_head_position: frame.is_head_position,
                });
            }
        }

        let parent_is_paren_list = frame.view.kind == ExpressionKind::List
            && frame.view.delimiter == Some(Delimiter::Paren);
        for (child_index, child) in frame.view.children.iter().enumerate().rev() {
            let child_path = paths.child(frame.path, child_index);
            stack.push(Frame {
                view: child,
                path: child_path,
                is_head_position: parent_is_paren_list && child_index == 0,
            });
        }
    }

    Some(paths)
}

#[cfg(test)]
mod tests {
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{ByteOffset, ByteSpan, Path, SymbolName, SyntaxTree};

    use super::{
        ReferencePath, SymbolReferenceSite, collect_symbol_reference_sites,
        match_reference_spans_to_sites,
    };

    fn span(index: usize) -> ByteSpan {
        ByteSpan::new(ByteOffset::new(index * 2), ByteOffset::new(index * 2 + 1))
    }

    #[test]
    fn matches_ten_thousand_sites_with_one_probe_per_atom() {
        let atom_count = 10_000usize;
        let reference_spans = (0..atom_count).map(span).collect::<Vec<_>>();
        let mut sites = (0..atom_count)
            .rev()
            .map(|index| SymbolReferenceSite {
                path: ReferencePath(index),
                span: span(index),
                is_head_position: false,
            })
            .collect::<Vec<_>>();

        let (matches, probes) = match_reference_spans_to_sites(&reference_spans, &mut sites);

        assert_eq!(matches.len(), atom_count);
        assert_eq!(probes, atom_count);
    }

    #[test]
    fn duplicate_spans_prefer_a_non_head_reference_site() {
        let duplicate_span = span(1);
        let reference_spans = vec![duplicate_span];
        let mut sites = vec![
            SymbolReferenceSite {
                path: ReferencePath(0),
                span: duplicate_span,
                is_head_position: true,
            },
            SymbolReferenceSite {
                path: ReferencePath(1),
                span: duplicate_span,
                is_head_position: false,
            },
        ];

        let (matches, probes) = match_reference_spans_to_sites(&reference_spans, &mut sites);

        assert_eq!(probes, 2);
        assert_eq!(matches.len(), 1);
        assert!(!sites[matches[0]].is_head_position);
    }

    #[test]
    fn deep_reference_site_walk_allocates_one_path_node_per_edge() {
        let depth = 6_000usize;
        let input = format!("{}target{}", "(".repeat(depth), ")".repeat(depth));
        let tree = SyntaxTree::parse(&input).expect("deep input should parse");
        let root_path = Path::root_child(0);
        let view = tree.select_path(&root_path).expect("root form").view();
        let from = SymbolName::new("target").expect("symbol");
        let mut sites = Vec::new();

        let paths = collect_symbol_reference_sites(
            &view,
            root_path,
            false,
            Dialect::CommonLisp,
            &from,
            &mut sites,
        )
        .expect("root-child path is non-empty");

        assert_eq!(sites.len(), 1);
        assert_eq!(paths.edge_count, depth);
        assert_eq!(paths.materialized_index_count, 0);
    }
}
