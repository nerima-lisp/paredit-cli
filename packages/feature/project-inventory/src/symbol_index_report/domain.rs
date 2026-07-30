//! Every symbol to its definition site, in one pass.
//!
//! Built for a consumer that will ask "where is this defined" thousands of
//! times — an editor, an agent, a language server — and must not re-parse the
//! tree for each question. `inspect definitions` already lists definitions;
//! what this adds is the *reference* side, so a lookup on any occurrence lands
//! on the definition rather than only a lookup on the definition itself.
//!
//! Resolution is by name and is deliberately syntactic. A symbol defined in the
//! analyzed files resolves to it; one that is not is recorded as *external*
//! with its reference count, which is what makes the index also an answer to
//! "what does this file depend on".
//!
//! Entries come in source order, like every other report here, rather than in
//! name order. A consumer building a lookup table sorts once on ingest; making
//! this report the odd one out would cost every *other* consumer the ability to
//! merge its output with another report's by position.

use std::collections::BTreeMap;
use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::definition::definition_shape;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, list_head};
use serde_json::{Value, json};

/// One symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    pub name: String,
    /// The head that defined it, when the analyzed files do.
    pub category: Option<String>,
    /// How many times it occurs outside its own definition.
    pub reference_count: usize,
    /// Byte offsets of every occurrence, so a consumer can jump without
    /// searching.
    pub occurrences: Vec<usize>,
    pub span: ByteSpan,
}

impl Finding for IndexEntry {
    fn kind(&self) -> &'static str {
        if self.category.is_some() {
            "defined"
        } else {
            "external"
        }
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            self.name.clone(),
            self.category.clone().unwrap_or_else(|| "-".to_owned()),
            format!("references={}", self.reference_count),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", json!(self.name)),
            ("category", json!(self.category)),
            ("reference_count", json!(self.reference_count)),
            ("occurrences", json!(self.occurrences)),
        ]
    }
}

#[must_use]
pub fn build_symbol_index_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<IndexEntry> {
    let root = tree.root_view();

    // Definitions first: an occurrence is a reference only relative to a
    // definition that may appear later in the file.
    let mut defined: BTreeMap<String, (String, ByteSpan)> = BTreeMap::new();
    for form in &root.children {
        let Some(head) = list_head(form) else {
            continue;
        };
        let Some(shape) = definition_shape(dialect, form, head) else {
            continue;
        };
        let Some(name) = shape.name(form) else {
            continue;
        };
        // The name atom itself, so a lookup lands on the name rather than on
        // the whole `defun`.
        let site = shape
            .name_child_index_of(form)
            .and_then(|index| form.children.get(index))
            .map_or(form.span, |name| name.span);
        defined
            .entry(fold(name))
            .or_insert((head.to_ascii_lowercase(), site));
    }

    let mut occurrences: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for_each_subview(&root, |view| {
        if view.kind != ExpressionKind::Atom {
            return;
        }
        let Some(text) = atom_symbol_text(view) else {
            return;
        };
        if !is_symbol(text) {
            return;
        }
        occurrences
            .entry(fold(text))
            .or_default()
            .push(view.span.start().get());
    });

    let findings = occurrences
        .into_iter()
        .map(|(name, mut sites)| {
            sites.sort_unstable();
            let definition = defined.get(&name);
            let span = definition.map_or_else(
                || {
                    let start = sites.first().copied().unwrap_or(0);
                    ByteSpan::new(
                        paredit_core_syntax::sexpr::ByteOffset::new(start),
                        paredit_core_syntax::sexpr::ByteOffset::new(start + name.len()),
                    )
                },
                |(_, span)| *span,
            );
            // The defining occurrence is not a reference to itself.
            let self_reference = usize::from(
                definition.is_some_and(|(_, site)| sites.contains(&site.start().get())),
            );
            IndexEntry {
                category: definition.map(|(category, _)| category.clone()),
                reference_count: sites.len().saturating_sub(self_reference),
                occurrences: sites,
                span,
                name,
            }
        })
        .collect::<Vec<_>>();
    let external = findings
        .iter()
        .filter(|entry| entry.category.is_none())
        .count();

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        // Symbols and definition shapes exist in every dialect this parses.
        true,
        tree.source(),
        findings,
        vec![
            ("symbol_count", json!(defined.len() + external)),
            ("defined_count", json!(defined.len())),
            ("external_count", json!(external)),
        ],
    )
}

/// Whether an atom is a symbol rather than a literal.
///
/// Numbers, strings, characters, and keywords are excluded: a keyword is a
/// symbol, but indexing `:name` alongside the function `name` would give a
/// consumer two things under one entry that are not the same object.
fn is_symbol(text: &str) -> bool {
    let Some(first) = text.chars().next() else {
        return false;
    };
    if matches!(first, '"' | ':' | '#') {
        return false;
    }
    if first.is_ascii_digit() {
        return false;
    }
    // A leading `-` or `.` followed by a digit is a number, not a symbol.
    if matches!(first, '-' | '+' | '.')
        && text
            .chars()
            .nth(1)
            .is_some_and(|second| second.is_ascii_digit())
    {
        return false;
    }
    true
}

fn fold(name: &str) -> String {
    name.to_ascii_uppercase()
}

/// Which child of a definition form holds its name.
///
/// `DefinitionShape::name` returns the text, not the node, and the node is what
/// a span is needed from. Finding it by matching the text back onto the
/// children is exact here because a definition's name appears once in its own
/// header.
trait NameChild {
    fn name_child_index_of(&self, form: &ExpressionView) -> Option<usize>;
}

impl NameChild for paredit_core_syntax::definition::DefinitionShape {
    fn name_child_index_of(&self, form: &ExpressionView) -> Option<usize> {
        let name = self.name(form)?;
        form.children
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, child)| atom_symbol_text(child) == Some(name))
            .map(|(index, _)| index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(source: &str) -> FileFindings<IndexEntry> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_symbol_index_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    fn entry<'a>(report: &'a FileFindings<IndexEntry>, name: &str) -> &'a IndexEntry {
        report
            .findings
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(name))
            .unwrap_or_else(|| panic!("{name} is indexed: {report:?}"))
    }

    #[test]
    fn a_definition_is_indexed_with_its_category() {
        let report = report("(defun render (x) x)");
        assert_eq!(entry(&report, "render").category.as_deref(), Some("defun"));
        assert_eq!(entry(&report, "render").kind(), "defined");
    }

    #[test]
    fn a_symbol_nothing_defines_is_indexed_as_external() {
        let report = report("(defun render (x) (external-call x))");
        assert_eq!(entry(&report, "external-call").category, None);
        assert_eq!(entry(&report, "external-call").kind(), "external");
    }

    #[test]
    fn the_defining_occurrence_is_not_counted_as_a_reference() {
        let report = report("(defun render (x) x)");
        assert_eq!(entry(&report, "render").reference_count, 0);
    }

    #[test]
    fn a_call_site_counts_as_a_reference() {
        let report = report("(defun render (x) x)\n(defun main () (render 1))");
        assert_eq!(entry(&report, "render").reference_count, 1);
    }

    #[test]
    fn every_occurrence_offset_is_recorded_for_jumping() {
        let report = report("(defun f (x) (list x x))");
        assert_eq!(entry(&report, "x").occurrences.len(), 3);
    }

    #[test]
    fn a_literal_is_not_indexed_as_a_symbol() {
        let report = report("(defun f () (list 1 \"s\" :k #\\c))");
        for name in ["1", "S", ":K"] {
            assert!(
                report.findings.iter().all(|entry| entry.name != name),
                "{name} was indexed: {report:?}"
            );
        }
    }

    #[test]
    fn a_negative_number_is_not_mistaken_for_a_symbol() {
        let report = report("(defun f () (- -1 2))");
        assert!(report.findings.iter().all(|entry| entry.name != "-1"));
        // The operator itself is a symbol and must stay indexed.
        assert!(report.findings.iter().any(|entry| entry.name == "-"));
    }

    #[test]
    fn the_index_is_in_source_order_like_every_other_report() {
        let report = report("(defun zeta () (alpha))\n(defun alpha () 1)");
        let starts = report
            .findings
            .iter()
            .map(|entry| entry.span.start().get())
            .collect::<Vec<_>>();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
    }

    #[test]
    fn the_summary_separates_defined_symbols_from_external_ones() {
        let report = report("(defun render (x) (external-call x))");
        assert_eq!(report.summary[1], ("defined_count", json!(1)));
        assert!(
            report
                .summary
                .iter()
                .any(|(name, value)| *name == "external_count" && value.as_u64() > Some(0))
        );
    }
}
