use std::collections::BTreeMap;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, Delimiter, ExpressionKind, ExpressionView, Path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormReportRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Option<Path>,
    pub target: ExpressionView,
    pub include_source: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormReport {
    pub dialect: Dialect,
    pub path: Option<Path>,
    pub span: ByteSpan,
    pub kind: FormKind,
    pub delimiter: Option<Delimiter>,
    pub head: Option<String>,
    pub definition_like: bool,
    pub child_count: usize,
    pub atom_count: usize,
    pub list_count: usize,
    pub max_depth: usize,
    pub symbols: Vec<FormSymbolReport>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormKind {
    Atom,
    List,
}

impl FormKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Atom => "atom",
            Self::List => "list",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormSymbolReport {
    pub symbol: String,
    pub count: usize,
    pub first_span: ByteSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SymbolAccumulator {
    count: usize,
    first_span: ByteSpan,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct FormStats {
    atom_count: usize,
    list_count: usize,
    max_depth: usize,
    symbols: BTreeMap<String, SymbolAccumulator>,
}

impl FormStats {
    fn collect(view: &ExpressionView) -> Self {
        let mut stats = Self::default();
        collect_stats(view, &mut stats);
        stats
    }

    fn into_symbols(self) -> Vec<FormSymbolReport> {
        self.symbols
            .into_iter()
            .map(|(symbol, accumulator)| FormSymbolReport {
                symbol,
                count: accumulator.count,
                first_span: accumulator.first_span,
            })
            .collect()
    }
}

/// Returns `(atom_count, list_count, max_depth)` for `view` without
/// materializing per-symbol occurrence tracking. Callers that only need
/// structural size and nesting depth (e.g. complexity reports) should use
/// this instead of [`build_form_report`], which also collects symbols.
#[must_use]
pub fn collect_structural_stats(view: &ExpressionView) -> (usize, usize, usize) {
    let stats = FormStats::collect(view);
    (stats.atom_count, stats.list_count, stats.max_depth)
}

pub fn build_form_report(request: FormReportRequest<'_>) -> FormReport {
    let stats = FormStats::collect(&request.target);
    let head = expression_head(&request.target).map(ToOwned::to_owned);
    let definition_like = head
        .as_deref()
        .is_some_and(|head| request.dialect.is_definition_head(head));
    let source = request
        .include_source
        .then(|| request.target.span.slice(request.input).to_owned());

    FormReport {
        dialect: request.dialect,
        path: request.path,
        span: request.target.span,
        kind: form_kind(&request.target),
        delimiter: request.target.delimiter,
        head,
        definition_like,
        child_count: request.target.children.len(),
        atom_count: stats.atom_count,
        list_count: stats.list_count,
        max_depth: stats.max_depth,
        symbols: stats.into_symbols(),
        source,
    }
}

fn collect_stats(view: &ExpressionView, stats: &mut FormStats) {
    let mut pending = vec![(view, 0_usize)];

    while let Some((view, depth)) = pending.pop() {
        stats.max_depth = stats.max_depth.max(depth);

        match view.kind {
            ExpressionKind::Root | ExpressionKind::List => {
                stats.list_count += 1;
                pending.extend(view.children.iter().rev().map(|child| (child, depth + 1)));
            }
            ExpressionKind::Atom => {
                stats.atom_count += 1;
                if let Some(text) = view
                    .text
                    .as_deref()
                    .filter(|text| !text.is_empty() && !text.starts_with('"'))
                {
                    let entry = stats
                        .symbols
                        .entry(text.to_owned())
                        .or_insert(SymbolAccumulator {
                            count: 0,
                            first_span: view.span,
                        });
                    entry.count += 1;
                }
            }
        }
    }
}

fn form_kind(view: &ExpressionView) -> FormKind {
    match view.kind {
        ExpressionKind::Atom => FormKind::Atom,
        ExpressionKind::List | ExpressionKind::Root => FormKind::List,
    }
}

fn expression_head(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::List)
        .then(|| {
            view.children
                .first()
                .and_then(|child| child.text.as_deref())
        })
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::sexpr::{ByteOffset, Path, SyntaxTree};

    fn report(input: &str, path: Path, include_source: bool) -> FormReport {
        let tree = SyntaxTree::parse(input).expect("parse input");
        let target = tree.select_path(&path).expect("select target").view();
        build_form_report(FormReportRequest {
            input,
            dialect: Dialect::CommonLisp,
            path: Some(path),
            target,
            include_source,
        })
    }

    #[test]
    fn reports_nested_list_statistics_and_first_symbol_spans() {
        let result = report("(foo bar (foo baz))", Path::root_child(0), true);

        assert_eq!(result.kind, FormKind::List);
        assert_eq!(result.delimiter, Some(Delimiter::Paren));
        assert_eq!(result.head.as_deref(), Some("foo"));
        assert!(!result.definition_like);
        assert_eq!(result.child_count, 3);
        assert_eq!(result.atom_count, 4);
        assert_eq!(result.list_count, 2);
        assert_eq!(result.max_depth, 2);
        assert_eq!(result.source.as_deref(), Some("(foo bar (foo baz))"));
        assert_eq!(
            result.symbols,
            vec![
                FormSymbolReport {
                    symbol: "bar".to_owned(),
                    count: 1,
                    first_span: ByteSpan::new(ByteOffset::new(5), ByteOffset::new(8)),
                },
                FormSymbolReport {
                    symbol: "baz".to_owned(),
                    count: 1,
                    first_span: ByteSpan::new(ByteOffset::new(14), ByteOffset::new(17)),
                },
                FormSymbolReport {
                    symbol: "foo".to_owned(),
                    count: 2,
                    first_span: ByteSpan::new(ByteOffset::new(1), ByteOffset::new(4)),
                },
            ]
        );
    }

    #[test]
    fn excludes_source_when_not_requested_and_handles_atom_targets() {
        let result = report("(foo)", Path::root_child(0).child(0), false);

        assert_eq!(result.kind, FormKind::Atom);
        assert_eq!(result.delimiter, None);
        assert_eq!(result.head, None);
        assert_eq!(result.child_count, 0);
        assert_eq!(result.atom_count, 1);
        assert_eq!(result.list_count, 0);
        assert_eq!(result.max_depth, 0);
        assert!(result.source.is_none());
        assert_eq!(result.symbols.len(), 1);
        assert_eq!(result.symbols[0].symbol, "foo");
    }

    #[test]
    fn string_atoms_are_not_counted_as_symbols() {
        let result = report("(print \"foo\" foo)", Path::root_child(0), true);

        assert_eq!(result.atom_count, 3);
        assert_eq!(
            result
                .symbols
                .iter()
                .map(|symbol| symbol.symbol.as_str())
                .collect::<Vec<_>>(),
            ["foo", "print"]
        );
        assert_eq!(result.symbols[0].count, 1);
        assert_eq!(result.symbols[1].count, 1);
    }

    #[test]
    fn reports_statistics_for_deeply_nested_forms_without_recursion() {
        const DEPTH: usize = 30_000;
        let mut input = "(".repeat(DEPTH);
        input.push('x');
        input.push_str(&")".repeat(DEPTH));

        let result = report(&input, Path::root_child(0), false);

        assert_eq!(result.atom_count, 1);
        assert_eq!(result.list_count, DEPTH);
        assert_eq!(result.max_depth, DEPTH);
        assert_eq!(result.symbols.len(), 1);
        assert_eq!(result.symbols[0].symbol, "x");
    }
}
