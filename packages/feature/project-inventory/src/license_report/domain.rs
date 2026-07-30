//! The `:license` each system declares, and whether they can ship together.
//!
//! A project that is MIT in one `defsystem` and GPL-3.0 in another has already
//! made a decision nobody made deliberately: the combined work is GPL, and the
//! MIT system's own terms no longer describe what a user receives. That is a
//! legal fact with no technical symptom, which is exactly the kind of thing a
//! tool should be the one to notice.
//!
//! Classification is by *copyleft strength*, which is the only axis that
//! decides combination:
//!
//! - **Permissive** — MIT, BSD, Apache. Combines with anything.
//! - **Weak copyleft** — LGPL, MPL. Combines, with conditions on modification.
//! - **Strong copyleft** — GPL, AGPL. Anything it combines with ships under it.
//! - **Unknown** — anything not on the list. Reported as unknown, never assumed
//!   permissive: an unrecognised licence is the case where guessing is worst.
//!
//! This is not legal advice and does not try to be. It reports what the systems
//! declare and where two declarations pull in different directions; a lawyer
//! decides what that means.

use std::collections::BTreeSet;
use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::common_lisp::common_lisp_operator_head_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

/// How strongly a licence propagates to what it is combined with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Copyleft {
    Permissive,
    Weak,
    Strong,
    /// Not on the list. Never assumed permissive.
    Unknown,
    /// No `:license` declared at all.
    Undeclared,
}

impl Copyleft {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Permissive => "permissive",
            Self::Weak => "weak-copyleft",
            Self::Strong => "strong-copyleft",
            Self::Unknown => "unknown",
            Self::Undeclared => "undeclared",
        }
    }

    /// Whether this needs a human to look at it.
    #[must_use]
    pub const fn needs_review(self) -> bool {
        matches!(self, Self::Unknown | Self::Undeclared)
    }
}

/// Licence identifiers this report recognises, most specific first.
///
/// Ordered so `AGPL` is tested before `GPL` and `LGPL` before `GPL`: a
/// substring match in the other order classifies both as strong copyleft, and
/// LGPL is not.
const LICENSES: [(&str, Copyleft); 14] = [
    ("AGPL", Copyleft::Strong),
    ("LGPL", Copyleft::Weak),
    ("GPL", Copyleft::Strong),
    ("MPL", Copyleft::Weak),
    ("EPL", Copyleft::Weak),
    ("CDDL", Copyleft::Weak),
    ("MIT", Copyleft::Permissive),
    ("BSD", Copyleft::Permissive),
    ("APACHE", Copyleft::Permissive),
    ("ISC", Copyleft::Permissive),
    ("ZLIB", Copyleft::Permissive),
    ("UNLICENSE", Copyleft::Permissive),
    ("CC0", Copyleft::Permissive),
    ("PUBLIC DOMAIN", Copyleft::Permissive),
];

/// One system's licence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemLicense {
    pub system: String,
    /// The `:license` string as written.
    pub license: Option<String>,
    pub copyleft: Copyleft,
    /// Whether this system's licence is weaker than another system's in the
    /// same file, so the combined work ships under the stronger one.
    pub superseded_by: Option<String>,
    pub span: ByteSpan,
}

impl Finding for SystemLicense {
    fn kind(&self) -> &'static str {
        if self.superseded_by.is_some() {
            "superseded"
        } else {
            self.copyleft.label()
        }
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            self.system.clone(),
            self.license.clone().unwrap_or_else(|| "-".to_owned()),
            self.copyleft.label().to_owned(),
            self.superseded_by.clone().unwrap_or_else(|| "-".to_owned()),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("system", json!(self.system)),
            ("license", json!(self.license)),
            ("copyleft", json!(self.copyleft.label())),
            ("superseded_by", json!(self.superseded_by)),
        ]
    }
}

#[must_use]
pub fn build_license_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<SystemLicense> {
    let modelled = dialect == Dialect::CommonLisp;

    let mut systems = Vec::new();
    if modelled {
        for form in &tree.root_view().children {
            let Some(head) = list_head(form) else {
                continue;
            };
            if !common_lisp_operator_head_eq(head, "defsystem") {
                continue;
            }
            let Some(name) = form.children.get(1).and_then(atom_symbol_text) else {
                continue;
            };
            let license = option_value(form, ":license").or_else(|| option_value(form, ":licence"));
            systems.push(SystemLicense {
                system: designator(name),
                copyleft: license.as_deref().map_or(Copyleft::Undeclared, classify),
                license,
                superseded_by: None,
                span: form.span,
            });
        }
    }

    // The strongest declared licence in the file. Anything weaker than it is
    // superseded, because a combined work ships under the strongest term any
    // of its parts carries.
    let strongest = systems
        .iter()
        .filter(|system| matches!(system.copyleft, Copyleft::Strong | Copyleft::Weak))
        .max_by_key(|system| system.copyleft)
        .map(|system| (system.copyleft, system.system.clone()));

    if let Some((strength, holder)) = strongest {
        for system in &mut systems {
            if system.system != holder && system.copyleft < strength {
                system.superseded_by = Some(holder.clone());
            }
        }
    }

    let review = systems
        .iter()
        .filter(|system| system.copyleft.needs_review())
        .count();
    let distinct = systems
        .iter()
        .filter_map(|system| system.license.clone())
        .collect::<BTreeSet<_>>()
        .len();

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        tree.source(),
        systems,
        vec![
            ("distinct_license_count", json!(distinct)),
            ("needs_review_count", json!(review)),
        ],
    )
}

/// The value of a `defsystem` keyword option.
fn option_value(form: &ExpressionView, keyword: &str) -> Option<String> {
    let mut children = form.children.iter().skip(2);
    while let Some(child) = children.next() {
        if atom_symbol_text(child).is_some_and(|text| text.eq_ignore_ascii_case(keyword)) {
            return children
                .next()
                .and_then(atom_symbol_text)
                .map(|text| text.trim_matches('"').to_owned());
        }
    }
    None
}

/// Which copyleft class a licence string names.
///
/// Substring matching against an ordered table, because `:license` is free
/// text: "GPL-3.0-or-later", "GNU GPL v3", and "gpl3" are all written in
/// practice and none of them is an SPDX identifier.
fn classify(license: &str) -> Copyleft {
    let upper = license.to_ascii_uppercase();
    LICENSES
        .iter()
        .find(|(name, _)| upper.contains(name))
        .map_or(Copyleft::Unknown, |(_, class)| *class)
}

fn designator(name: &str) -> String {
    name.trim_start_matches("#:")
        .trim_start_matches(':')
        .trim_matches(['|', '"'])
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(source: &str) -> FileFindings<SystemLicense> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_license_report(Path::new("t.asd"), Dialect::CommonLisp, &tree)
    }

    #[test]
    fn a_permissive_license_is_classified_as_such() {
        let report = report("(defsystem \"app\" :license \"MIT\")");
        assert_eq!(report.findings[0].copyleft, Copyleft::Permissive);
    }

    #[test]
    fn lgpl_is_weak_copyleft_rather_than_strong() {
        // Substring order matters: matching `GPL` first would call this strong.
        let report = report("(defsystem \"app\" :license \"LGPL-2.1\")");
        assert_eq!(report.findings[0].copyleft, Copyleft::Weak);
    }

    #[test]
    fn agpl_is_strong_copyleft() {
        let report = report("(defsystem \"app\" :license \"AGPL-3.0\")");
        assert_eq!(report.findings[0].copyleft, Copyleft::Strong);
    }

    #[test]
    fn free_text_spellings_are_recognised() {
        for spelling in ["GPL-3.0-or-later", "GNU GPL v3", "gpl3"] {
            let report = report(&format!("(defsystem \"app\" :license \"{spelling}\")"));
            assert_eq!(report.findings[0].copyleft, Copyleft::Strong, "{spelling}");
        }
    }

    #[test]
    fn an_unrecognised_license_is_unknown_rather_than_assumed_permissive() {
        let report = report("(defsystem \"app\" :license \"Bespoke-1.0\")");
        assert_eq!(report.findings[0].copyleft, Copyleft::Unknown);
        assert!(report.findings[0].copyleft.needs_review());
    }

    #[test]
    fn a_system_with_no_license_is_reported_as_undeclared() {
        let report = report("(defsystem \"app\")");
        assert_eq!(report.findings[0].copyleft, Copyleft::Undeclared);
    }

    #[test]
    fn a_permissive_system_beside_a_gpl_one_is_superseded() {
        let report =
            report("(defsystem \"a\" :license \"MIT\")\n(defsystem \"b\" :license \"GPL-3.0\")");
        let a = &report.findings[0];
        assert_eq!(a.superseded_by.as_deref(), Some("b"));
        assert_eq!(a.kind(), "superseded");
    }

    #[test]
    fn two_permissive_systems_do_not_supersede_each_other() {
        let report = report(
            "(defsystem \"a\" :license \"MIT\")\n(defsystem \"b\" :license \"BSD-3-Clause\")",
        );
        assert!(
            report
                .findings
                .iter()
                .all(|system| system.superseded_by.is_none())
        );
    }

    #[test]
    fn the_british_spelling_of_the_option_is_read_too() {
        let report = report("(defsystem \"app\" :licence \"MIT\")");
        assert_eq!(report.findings[0].license.as_deref(), Some("MIT"));
    }

    #[test]
    fn an_unmodelled_dialect_says_so_rather_than_reporting_nothing() {
        let tree =
            SyntaxTree::parse_with_dialect("(defproject app)", Dialect::Clojure).expect("parse");
        let report = build_license_report(Path::new("t.clj"), Dialect::Clojure, &tree);
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }
}
