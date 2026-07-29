//! Every `#n=` and `#n#` in a file, and which labels do not pair up.
//!
//! Reader labels preserve *object identity*. `#1=(a b)` names the cons it
//! reads, and every later `#1#` is that same cons — not a copy of it. Two
//! consequences make this worth reporting:
//!
//! - A label can build a genuinely circular object. `#1=(a . #1#)` is an
//!   infinite list held in finite memory. Printing it without
//!   `*print-circle*` does not terminate, and `equal` on it does not
//!   terminate either.
//! - Every structural refactor in this tool is a byte-span rewrite that assumes
//!   two identical subtrees are interchangeable. Under a label they are not:
//!   duplicating a `#1#` duplicates a *reference*, and extracting the datum a
//!   `#1=` labels detaches every reference to it. A file with labels is one
//!   where `duplicates`, `similarity`, and `extract-function` are all reasoning
//!   about a structure the reader does not build.
//!
//! An unpaired label is a hard error, not a style question: `#1#` with no `#1=`
//! is a read error, and `#1=` never referenced is a label that costs nothing
//! but signals an incomplete edit.

use std::collections::BTreeMap;
use std::path::Path;

use paredit_core_syntax::common_lisp::{CommonLispReaderLabelKind, common_lisp_reader_label_forms};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, SyntaxTree};
use serde_json::{Value, json};

use paredit_core_cli::report::{FileFindings, Finding};

const DATUM_LIMIT: usize = 48;

/// What a label dispatch is, and whether it found its counterpart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelRole {
    /// `#n=`, which names the datum that follows it.
    Definition,
    /// `#n#`, which is the datum `#n=` named.
    Reference,
    /// A `#n=` that nothing refers to. Harmless at run time, and almost always
    /// the residue of a deleted reference.
    UnreferencedDefinition,
    /// A `#n#` with no `#n=` anywhere in the file. A read error.
    DanglingReference,
}

impl LabelRole {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Definition => "definition",
            Self::Reference => "reference",
            Self::UnreferencedDefinition => "unreferenced-definition",
            Self::DanglingReference => "dangling-reference",
        }
    }

    /// Whether this role is a defect rather than an observation.
    #[must_use]
    pub const fn is_broken(self) -> bool {
        matches!(self, Self::UnreferencedDefinition | Self::DanglingReference)
    }
}

/// One `#n=` or `#n#`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CircularLiteral {
    pub role: LabelRole,
    /// The label number as written. A string rather than an integer because
    /// `#01=` and `#1=` are the same label to the reader but not to a reader
    /// that re-prints what it found.
    pub number: String,
    /// The labelled datum, elided. Empty for a reference, which is the datum.
    pub datum: String,
    pub span: ByteSpan,
    pub line: usize,
}

impl Finding for CircularLiteral {
    fn kind(&self) -> &'static str {
        self.role.label()
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("#{}", self.number), self.datum.clone()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("role", json!(self.role.label())),
            ("number", json!(self.number)),
            ("datum", json!(self.datum)),
        ]
    }
}

#[must_use]
pub fn build_circular_literal_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<CircularLiteral> {
    let modelled = dialect == Dialect::CommonLisp;
    let source = tree.source();

    if !modelled {
        return FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("broken_count", json!(0))],
        );
    }

    let forms = common_lisp_reader_label_forms(tree);

    // Both sides are counted before any finding is built: a `#n=` cannot be
    // called unreferenced until the whole file has been read, and a `#n#`
    // cannot be called dangling either.
    let mut definitions: BTreeMap<String, usize> = BTreeMap::new();
    let mut references: BTreeMap<String, usize> = BTreeMap::new();
    for form in &forms {
        let number = label_number(source, form.dispatch_span);
        let side = match form.kind {
            CommonLispReaderLabelKind::Definition => &mut definitions,
            CommonLispReaderLabelKind::Reference => &mut references,
        };
        *side.entry(number).or_default() += 1;
    }

    let findings = forms
        .into_iter()
        .map(|form| {
            let number = label_number(source, form.dispatch_span);
            let role = match form.kind {
                CommonLispReaderLabelKind::Definition => {
                    if references.contains_key(&number) {
                        LabelRole::Definition
                    } else {
                        LabelRole::UnreferencedDefinition
                    }
                }
                CommonLispReaderLabelKind::Reference => {
                    if definitions.contains_key(&number) {
                        LabelRole::Reference
                    } else {
                        LabelRole::DanglingReference
                    }
                }
            };
            CircularLiteral {
                role,
                number,
                datum: datum_text(source, form.dispatch_span, form.span),
                span: form.span,
                line: line_of(source, form.span.start().get()),
            }
        })
        .collect::<Vec<_>>();

    let broken = findings
        .iter()
        .filter(|finding: &&CircularLiteral| finding.role.is_broken())
        .count();

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        findings,
        vec![
            ("label_count", json!(definitions.len())),
            ("broken_count", json!(broken)),
        ],
    )
}

/// The digits between `#` and its `=`/`#` terminator.
fn label_number(source: &str, dispatch: ByteSpan) -> String {
    source
        .get(dispatch.start().get()..dispatch.end().get())
        .unwrap_or_default()
        .trim_start_matches('#')
        .trim_end_matches(['=', '#'])
        .to_owned()
}

/// The text a `#n=` labels, or the empty string for a reference.
fn datum_text(source: &str, dispatch: ByteSpan, whole: ByteSpan) -> String {
    let text = source
        .get(dispatch.end().get()..whole.end().get())
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if text.chars().count() <= DATUM_LIMIT {
        return text;
    }
    let head = text
        .chars()
        .take(DATUM_LIMIT.saturating_sub(1))
        .collect::<String>();
    format!("{head}…")
}

fn line_of(source: &str, offset: usize) -> usize {
    1 + source
        .get(..offset.min(source.len()))
        .unwrap_or(source)
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(source: &str) -> FileFindings<CircularLiteral> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_circular_literal_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    fn roles(report: &FileFindings<CircularLiteral>) -> Vec<LabelRole> {
        report.findings.iter().map(|finding| finding.role).collect()
    }

    #[test]
    fn a_paired_label_reports_both_sides_as_sound() {
        let report = report("(defvar *x* '(#1=(a b) #1#))");
        assert_eq!(
            roles(&report),
            vec![LabelRole::Definition, LabelRole::Reference]
        );
        assert_eq!(report.summary[1], ("broken_count", json!(0)));
    }

    #[test]
    fn a_definition_nothing_refers_to_is_reported() {
        let report = report("(defvar *x* '#1=(a b))");
        assert_eq!(roles(&report), vec![LabelRole::UnreferencedDefinition]);
        assert_eq!(report.summary[1], ("broken_count", json!(1)));
    }

    #[test]
    fn a_reference_with_no_definition_is_reported() {
        let report = report("(defvar *x* '(a #2#))");
        assert_eq!(roles(&report), vec![LabelRole::DanglingReference]);
    }

    #[test]
    fn the_label_number_is_read_off_the_dispatch() {
        let report = report("(defvar *x* '(#7=(a) #7#))");
        assert_eq!(report.findings[0].number, "7");
        assert_eq!(report.findings[1].number, "7");
    }

    #[test]
    fn a_definition_reports_the_datum_it_labels() {
        let report = report("(defvar *x* '(#1=(a b) #1#))");
        assert_eq!(report.findings[0].datum, "(a b)");
    }

    #[test]
    fn a_file_with_no_label_reports_nothing() {
        let report = report("(defun f () 1)");
        assert!(report.findings.is_empty());
    }

    #[test]
    fn findings_are_in_source_order() {
        let report = report("(list #1=(a) #1# #2=(b) #2#)");
        let starts = report
            .findings
            .iter()
            .map(|finding| finding.span.start().get())
            .collect::<Vec<_>>();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
        assert_eq!(starts.len(), 4);
    }

    #[test]
    fn an_unmodelled_dialect_says_so_rather_than_reporting_nothing() {
        let source = "(defn f [] 1)";
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Clojure).expect("parse");
        let report = build_circular_literal_report(Path::new("t.clj"), Dialect::Clojure, &tree);
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn only_the_unpaired_roles_count_as_broken() {
        assert!(LabelRole::UnreferencedDefinition.is_broken());
        assert!(LabelRole::DanglingReference.is_broken());
        assert!(!LabelRole::Definition.is_broken());
        assert!(!LabelRole::Reference.is_broken());
    }
}
