//! Symbols whose identity depends on `readtable-case`, and the escapes that
//! pin it.
//!
//! The Common Lisp reader upcases unescaped symbol characters — but only under
//! the default `:upcase` readtable case. Under `:preserve` (Allegro's modern
//! mode, and what `named-readtables` is usually reached for) it does not, and
//! `Foo`, `foo`, and `FOO` become three different symbols.
//!
//! That makes mixed-case source a portability hazard with no local symptom.
//! `(defun parseJSON () …)` reads as `PARSEJSON` on SBCL and as `parseJSON`
//! under `:preserve`; a file that defines it one way and calls it the other
//! works on one implementation and fails on the next.
//!
//! Escaped symbols are the other half. `|Foo|` and `\F`oo mean the same symbol
//! under *every* readtable case, which is exactly why they are used — and also
//! why a rename must not touch their interior. They are reported so a caller
//! knows which occurrences are already pinned.
//!
//! An all-lowercase or all-uppercase symbol is not reported: it reads as the
//! same symbol under `:upcase`, `:downcase`, and `:preserve` alike, so there is
//! nothing at stake.

use std::collections::BTreeSet;
use std::path::Path;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::for_each_subview;
use serde_json::{Value, json};

use paredit_core_cli::report::{FileFindings, Finding};

/// Why a symbol's spelling is load-bearing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseSensitivity {
    /// Mixed case with no escape. Reads as three different symbols under three
    /// different readtable cases.
    MixedCase,
    /// `|Foo|` — pinned by a multiple-escape, and immune to readtable case.
    Escaped,
    /// A single `\` escape, which pins one character and leaves the rest to the
    /// readtable. The subtlest of the three.
    PartiallyEscaped,
}

impl CaseSensitivity {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::MixedCase => "mixed-case",
            Self::Escaped => "escaped",
            Self::PartiallyEscaped => "partially-escaped",
        }
    }

    /// Whether the symbol's identity changes with the readtable case.
    #[must_use]
    pub const fn is_fragile(self) -> bool {
        matches!(self, Self::MixedCase | Self::PartiallyEscaped)
    }
}

/// One symbol whose spelling matters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseSensitiveSymbol {
    pub sensitivity: CaseSensitivity,
    /// The symbol as written.
    pub name: String,
    /// What the default `:upcase` reader produces, so the two can be compared.
    pub upcased: String,
    pub span: ByteSpan,
    pub line: usize,
}

impl Finding for CaseSensitiveSymbol {
    fn kind(&self) -> &'static str {
        self.sensitivity.label()
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.name.clone(), format!("upcase={}", self.upcased)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("sensitivity", json!(self.sensitivity.label())),
            ("name", json!(self.name)),
            ("upcased", json!(self.upcased)),
        ]
    }
}

#[must_use]
pub fn build_readtable_case_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<CaseSensitiveSymbol> {
    let modelled = dialect == Dialect::CommonLisp;
    let source = tree.source();
    let mut findings = Vec::new();

    if modelled {
        for_each_subview(&tree.root_view(), |view| {
            if let Some(finding) = case_sensitive(view, source) {
                findings.push(finding);
            }
        });
    }

    let fragile = findings
        .iter()
        .filter(|finding: &&CaseSensitiveSymbol| finding.sensitivity.is_fragile())
        .count();
    // Distinct upcased names among the fragile symbols. Two spellings that
    // upcase to the same name are the collision worth looking at first: under
    // `:upcase` they are one symbol, under `:preserve` they are two.
    let distinct = findings
        .iter()
        .filter(|finding| finding.sensitivity.is_fragile())
        .map(|finding| &finding.upcased)
        .collect::<BTreeSet<_>>()
        .len();

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        findings,
        vec![
            ("fragile_count", json!(fragile)),
            ("distinct_fragile_name_count", json!(distinct)),
        ],
    )
}

fn case_sensitive(view: &ExpressionView, source: &str) -> Option<CaseSensitiveSymbol> {
    if view.kind != ExpressionKind::Atom {
        return None;
    }
    let text = atom_symbol_text(view)?;

    // A string or character literal is not a symbol; its case is data, not
    // identity, and reporting it would bury every real finding.
    if text.starts_with('"') || text.starts_with("#\\") {
        return None;
    }

    let sensitivity = if text.contains('|') {
        CaseSensitivity::Escaped
    } else if text.contains('\\') {
        CaseSensitivity::PartiallyEscaped
    } else if is_mixed_case(text) {
        CaseSensitivity::MixedCase
    } else {
        return None;
    };

    Some(CaseSensitiveSymbol {
        sensitivity,
        upcased: text.to_ascii_uppercase(),
        name: text.to_owned(),
        span: view.span,
        line: line_of(source, view.span.start().get()),
    })
}

/// Whether a symbol holds both an upper- and a lower-case letter.
///
/// Both are required. `FOO` and `foo` read as the same symbol under every
/// readtable case in practical use, so only a symbol that mixes them has an
/// identity that depends on which case is in effect.
fn is_mixed_case(text: &str) -> bool {
    text.chars().any(char::is_uppercase) && text.chars().any(char::is_lowercase)
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

    fn report(source: &str) -> FileFindings<CaseSensitiveSymbol> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_readtable_case_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    #[test]
    fn a_mixed_case_symbol_is_reported_with_what_upcase_would_produce() {
        let report = report("(defun parseJSON () 1)");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].sensitivity, CaseSensitivity::MixedCase);
        assert_eq!(report.findings[0].name, "parseJSON");
        assert_eq!(report.findings[0].upcased, "PARSEJSON");
    }

    #[test]
    fn an_all_lowercase_symbol_is_not_reported() {
        let report = report("(defun parse-json () 1)");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn an_all_uppercase_symbol_is_not_reported() {
        let report = report("(defun PARSE-JSON () 1)");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn an_escaped_symbol_is_reported_as_pinned_rather_than_fragile() {
        let report = report("(defun |Foo| () 1)");
        assert_eq!(report.findings[0].sensitivity, CaseSensitivity::Escaped);
        assert!(!report.findings[0].sensitivity.is_fragile());
        assert_eq!(report.summary[0], ("fragile_count", json!(0)));
    }

    #[test]
    fn a_string_literal_is_data_rather_than_a_symbol() {
        let report = report("(defun f () \"MixedCase\")");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn two_spellings_that_upcase_alike_count_as_one_name() {
        let report = report("(defun parseJSON () 1)\n(defun parseJson () 2)");
        assert_eq!(report.summary[0], ("fragile_count", json!(2)));
        assert_eq!(report.summary[1], ("distinct_fragile_name_count", json!(1)));
    }

    #[test]
    fn findings_are_in_source_order() {
        let report = report("(list aB cD eF)");
        let starts = report
            .findings
            .iter()
            .map(|finding| finding.span.start().get())
            .collect::<Vec<_>>();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
        assert_eq!(starts.len(), 3);
    }

    #[test]
    fn an_unmodelled_dialect_says_so_rather_than_reporting_nothing() {
        // Clojure is case-sensitive by design, so this report says nothing
        // about it — which is exactly what it must say, rather than "clean".
        let tree = SyntaxTree::parse_with_dialect("(defn parseJSON [] 1)", Dialect::Clojure)
            .expect("parse");
        let report = build_readtable_case_report(Path::new("t.clj"), Dialect::Clojure, &tree);
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }
}
