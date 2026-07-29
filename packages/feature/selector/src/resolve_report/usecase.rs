//! Turning resolved selections into a printable report.
//!
//! Everything here is derivation from what `selector::resolve` already
//! returned: coordinates, a stable id, a head symbol, a bounded preview. No
//! matching happens in this crate.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::selector::{
    Capture, LineIndex, LinePosition, SelectorTarget, stable_selector_ids,
};
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionPath, SyntaxTree};
use paredit_core_syntax::view_query::list_head;

/// How much source text a match carries in its preview.
///
/// Enough to recognise the form, short enough that a hundred matches still
/// fit an agent's context. A caller who wants the whole form has the path and
/// `edit select`.
pub const DEFAULT_PREVIEW_BYTES: usize = 80;

/// One resolved match, ready to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMatch {
    pub path: ExpressionPath,
    pub span: ByteSpan,
    pub start: LinePosition,
    pub end: LinePosition,
    /// `list`, `atom`, or `range` when the match covers several siblings.
    pub kind: &'static str,
    /// The head symbol, for a paren list whose head is a bare symbol.
    pub head: Option<String>,
    /// The stable id, absent only for a multi-form range, which has no single
    /// node to hash.
    pub id: Option<String>,
    /// How many sibling forms the match covers; 1 for everything but a range.
    pub form_count: usize,
    pub preview: String,
    pub captures: Vec<Capture>,
}

/// The whole report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveReport {
    pub dialect: Dialect,
    pub selector: String,
    pub matches: Vec<ResolvedMatch>,
}

impl ResolveReport {
    /// Whether the selector named nothing, for the `--fail-on-empty` gate.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.matches.is_empty()
    }
}

/// Builds the report from targets `selector::resolve` produced.
#[must_use]
pub fn build_resolve_report(
    tree: &SyntaxTree,
    dialect: Dialect,
    selector: String,
    targets: &[SelectorTarget],
    preview_bytes: usize,
) -> ResolveReport {
    let source = tree.source();
    let index = LineIndex::new(source);
    // One pass for the whole file: an id cannot be computed for one form
    // alone, since its ordinal depends on the forms around it.
    let ids = stable_selector_ids(tree, dialect);

    let matches = targets
        .iter()
        .map(|target| {
            let view = tree
                .select_path(&target.path)
                .ok()
                .map(|found| found.view());
            let kind = match (target.form_count(), view.as_ref().map(|view| view.kind)) {
                (count, _) if count > 1 => "range",
                (_, Some(ExpressionKind::Atom)) => "atom",
                (_, Some(ExpressionKind::List)) => "list",
                _ => "unknown",
            };
            ResolvedMatch {
                path: target.path.clone(),
                span: target.span,
                start: index.position_of(source, target.span.start().get()),
                end: index.position_of(source, target.span.end().get()),
                kind,
                head: (target.form_count() == 1)
                    .then(|| view.as_ref().and_then(list_head).map(ToOwned::to_owned))
                    .flatten(),
                id: (target.form_count() == 1)
                    .then(|| {
                        ids.iter()
                            .find(|(path, _)| *path == target.path)
                            .map(|(_, id)| id.to_string())
                    })
                    .flatten(),
                form_count: target.form_count(),
                preview: preview(source, target.span, preview_bytes),
                captures: target.captures.clone(),
            }
        })
        .collect();

    ResolveReport {
        dialect,
        selector,
        matches,
    }
}

/// A single-line, length-bounded rendering of the matched source.
///
/// Newlines and runs of whitespace collapse so one match stays one line,
/// which is what makes the text output greppable.
fn preview(source: &str, span: ByteSpan, max_bytes: usize) -> String {
    let text = source.get(span.as_range()).unwrap_or_default();
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.len() <= max_bytes {
        return collapsed;
    }
    let mut end = max_bytes.min(collapsed.len());
    while !collapsed.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &collapsed[..end])
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use paredit_core_syntax::selector::{SelectorRequest, SelectorTerm, resolve};

    fn report(source: &str, term: SelectorTerm, all: bool) -> ResolveReport {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).unwrap();
        let mut request = SelectorRequest::new(term);
        request.all = all;
        let targets = resolve(&tree, Dialect::CommonLisp, &request).unwrap();
        build_resolve_report(
            &tree,
            Dialect::CommonLisp,
            request.describe(),
            &targets,
            DEFAULT_PREVIEW_BYTES,
        )
    }

    #[test]
    fn a_match_carries_coordinates_a_head_and_an_id() {
        let report = report(
            "(defun f (x)\n  x)\n",
            SelectorTerm::Name("f".to_owned()),
            false,
        );
        let found = &report.matches[0];
        assert_eq!(found.path.to_string(), "0");
        assert_eq!(found.kind, "list");
        assert_eq!(found.head.as_deref(), Some("defun"));
        assert_eq!(found.start.to_string(), "1:1");
        assert_eq!(found.end.to_string(), "2:5");
        assert_eq!(found.id.as_ref().unwrap().len(), 16);
        assert_eq!(found.form_count, 1);
    }

    #[test]
    fn a_preview_collapses_a_multi_line_form_onto_one_line() {
        let report = report(
            "(defun f (x)\n  (g x)\n  x)",
            SelectorTerm::Name("f".to_owned()),
            false,
        );
        assert_eq!(report.matches[0].preview, "(defun f (x) (g x) x)");
    }

    #[test]
    fn a_long_preview_is_truncated_on_a_character_boundary() {
        let long = format!("(f {})", "λ ".repeat(80));
        let report = report(&long, SelectorTerm::Path("0".parse().unwrap()), false);
        let preview = &report.matches[0].preview;
        assert!(preview.ends_with("..."));
        assert!(preview.len() <= DEFAULT_PREVIEW_BYTES + 3);
    }

    #[test]
    fn a_range_reports_its_form_count_and_no_id() {
        let tree =
            SyntaxTree::parse_with_dialect("(progn (a) (b) (c))", Dialect::CommonLisp).unwrap();
        let mut request = SelectorRequest::new(SelectorTerm::Path("0.1".parse().unwrap()));
        request.range_end = Some(SelectorTerm::Path("0.3".parse().unwrap()));
        let targets = resolve(&tree, Dialect::CommonLisp, &request).unwrap();
        let report = build_resolve_report(
            &tree,
            Dialect::CommonLisp,
            request.describe(),
            &targets,
            DEFAULT_PREVIEW_BYTES,
        );
        assert_eq!(report.matches[0].kind, "range");
        assert_eq!(report.matches[0].form_count, 3);
        assert_eq!(report.matches[0].id, None);
    }

    #[test]
    fn an_empty_report_is_reported_as_empty() {
        let tree = SyntaxTree::parse_with_dialect("(a)", Dialect::CommonLisp).unwrap();
        let report = build_resolve_report(
            &tree,
            Dialect::CommonLisp,
            "--name missing".to_owned(),
            &[],
            DEFAULT_PREVIEW_BYTES,
        );
        assert!(report.is_empty());
    }
}
