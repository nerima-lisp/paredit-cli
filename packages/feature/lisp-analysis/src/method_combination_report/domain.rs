//! `defmethod` qualifiers, and the generic functions whose auxiliary methods
//! have no primary to wrap.
//!
//! Standard method combination runs `:before` methods, then the most specific
//! *primary*, then `:after` methods, with `:around` methods wrapped outside all
//! of it. Every one of those is optional except the primary: a generic function
//! with only `:before` and `:after` methods signals `no-applicable-method` when
//! called, because there is nothing for them to run around.
//!
//! That failure is invisible to every form-level analysis. Each `defmethod` is
//! individually well-formed; what is missing is a *different* form that was
//! never written, possibly in a different file. This report is the only place
//! the absence shows up.
//!
//! Specializers are part of the identity, not decoration. `(:before ((x fish)))`
//! and `(:before ((x bird)))` are different methods on different classes, so a
//! primary for `fish` does not cover `bird`. The pairing therefore keys on
//! (name, specializers) rather than on name alone.

use std::collections::BTreeMap;
use std::path::Path;

use paredit_core_syntax::common_lisp::common_lisp_operator_head_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

use paredit_core_cli::report::{FileFindings, Finding, line_of};

/// A method's role in standard method combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Qualifier {
    Primary,
    Before,
    After,
    Around,
    /// A qualifier standard combination does not define. Valid under a
    /// `define-method-combination`, so it is reported rather than judged.
    Other,
}

impl Qualifier {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Before => "before",
            Self::After => "after",
            Self::Around => "around",
            Self::Other => "other",
        }
    }

    fn parse(text: &str) -> Self {
        for (name, qualifier) in [
            (":before", Self::Before),
            (":after", Self::After),
            (":around", Self::Around),
        ] {
            if text.eq_ignore_ascii_case(name) {
                return qualifier;
            }
        }
        Self::Other
    }

    /// Whether this method needs a primary to be reachable.
    #[must_use]
    pub const fn needs_primary(self) -> bool {
        matches!(self, Self::Before | Self::After)
    }
}

/// One `defmethod`, or one generic function missing a primary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodFinding {
    /// `method` for an ordinary observation, `orphaned-auxiliary` for a
    /// `:before`/`:after` with no primary on the same specializers.
    pub orphaned: bool,
    pub name: String,
    pub qualifier: Qualifier,
    /// The specializer of each required parameter, `t` where unspecialized.
    pub specializers: Vec<String>,
    pub span: ByteSpan,
    pub line: usize,
}

impl Finding for MethodFinding {
    fn kind(&self) -> &'static str {
        if self.orphaned {
            "orphaned-auxiliary"
        } else {
            "method"
        }
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            self.name.clone(),
            self.qualifier.label().to_owned(),
            format!("on=({})", self.specializers.join(" ")),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", json!(self.name)),
            ("qualifier", json!(self.qualifier.label())),
            ("specializers", json!(self.specializers)),
            ("orphaned", json!(self.orphaned)),
        ]
    }
}

#[must_use]
pub fn build_method_combination_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<MethodFinding> {
    let modelled = dialect == Dialect::CommonLisp;
    let source = tree.source();

    let mut methods = Vec::new();
    if modelled {
        for form in &tree.root_view().children {
            if let Some(method) = read_method(form, source) {
                methods.push(method);
            }
        }
    }

    // Which (name, specializers) pairs have a primary. Collected over the whole
    // file before any method is judged: a primary written after its `:before`
    // still covers it, and CLHS imposes no order.
    let mut primaries: BTreeMap<(String, Vec<String>), bool> = BTreeMap::new();
    for method in &methods {
        if method.qualifier == Qualifier::Primary {
            primaries.insert((fold(&method.name), method.specializers.clone()), true);
        }
    }

    for method in &mut methods {
        method.orphaned = method.qualifier.needs_primary()
            && !primaries.contains_key(&(fold(&method.name), method.specializers.clone()));
    }

    let orphaned = methods.iter().filter(|method| method.orphaned).count();
    let generics = methods
        .iter()
        .map(|method| fold(&method.name))
        .collect::<std::collections::BTreeSet<_>>()
        .len();

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        methods,
        vec![
            ("generic_function_count", json!(generics)),
            ("orphaned_count", json!(orphaned)),
        ],
    )
}

/// Reads a `defmethod`, or `None` for any other form.
///
/// A method's qualifier sits between the name and the lambda list and may be
/// absent, so the lambda list is found by *shape* — the first list-valued child
/// — rather than by a fixed index.
fn read_method(form: &ExpressionView, source: &str) -> Option<MethodFinding> {
    let head = list_head(form)?;
    if !common_lisp_operator_head_eq(head, "defmethod") {
        return None;
    }
    let name = atom_symbol_text(form.children.get(1)?)?.to_owned();

    let lambda_index = form
        .children
        .iter()
        .enumerate()
        .skip(2)
        .find(|(_, child)| child.kind == ExpressionKind::List)
        .map(|(index, _)| index)?;

    let qualifier = if lambda_index > 2 {
        Qualifier::parse(atom_symbol_text(form.children.get(2)?).unwrap_or_default())
    } else {
        Qualifier::Primary
    };

    Some(MethodFinding {
        orphaned: false,
        name,
        qualifier,
        specializers: specializers(&form.children[lambda_index]),
        span: form.span,
        line: line_of(source, form.span.start().get()),
    })
}

/// The specializer of each required parameter.
///
/// An unspecialized parameter is recorded as `t` rather than skipped, so that
/// `(a b)` and `((a fish) b)` produce different keys — they are different
/// methods, and collapsing them would let one stand in for the other.
fn specializers(lambda_list: &ExpressionView) -> Vec<String> {
    let mut found = Vec::new();
    for parameter in &lambda_list.children {
        match parameter.kind {
            // `&optional` and everything after it is not part of a method's
            // specializer list, so the scan stops there.
            ExpressionKind::Atom => {
                let text = atom_symbol_text(parameter).unwrap_or_default();
                if text.starts_with('&') {
                    break;
                }
                found.push("t".to_owned());
            }
            ExpressionKind::List => {
                let specializer = parameter
                    .children
                    .get(1)
                    .and_then(atom_symbol_text)
                    .unwrap_or("t");
                found.push(fold(specializer));
            }
            ExpressionKind::Root => {}
        }
    }
    found
}

fn fold(name: &str) -> String {
    name.to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(source: &str) -> FileFindings<MethodFinding> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_method_combination_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    #[test]
    fn a_method_with_no_qualifier_is_primary() {
        let report = report("(defmethod speak ((x fish)) 1)");
        assert_eq!(report.findings[0].qualifier, Qualifier::Primary);
        assert_eq!(report.findings[0].specializers, vec!["FISH".to_owned()]);
    }

    #[test]
    fn each_standard_qualifier_is_recognised() {
        let report = report(
            "(defmethod s :before ((x f)) 1)\n\
             (defmethod s :after ((x f)) 1)\n\
             (defmethod s :around ((x f)) 1)",
        );
        assert_eq!(
            report
                .findings
                .iter()
                .map(|method| method.qualifier)
                .collect::<Vec<_>>(),
            vec![Qualifier::Before, Qualifier::After, Qualifier::Around]
        );
    }

    #[test]
    fn an_auxiliary_method_with_no_primary_is_orphaned() {
        let report = report("(defmethod speak :before ((x fish)) 1)");
        assert!(report.findings[0].orphaned);
        assert_eq!(report.findings[0].kind(), "orphaned-auxiliary");
    }

    #[test]
    fn a_primary_written_after_its_before_method_still_covers_it() {
        let report =
            report("(defmethod speak :before ((x fish)) 1)\n(defmethod speak ((x fish)) 2)");
        assert!(report.findings.iter().all(|method| !method.orphaned));
    }

    #[test]
    fn a_primary_on_another_class_does_not_cover_this_one() {
        let report =
            report("(defmethod speak ((x fish)) 1)\n(defmethod speak :before ((x bird)) 2)");
        assert_eq!(report.summary[1], ("orphaned_count", json!(1)));
    }

    #[test]
    fn an_around_method_does_not_need_a_primary_of_its_own_to_be_reported_sound() {
        // `:around` calls `call-next-method`, which may reach a primary on a
        // superclass; claiming otherwise would be a false positive.
        let report = report("(defmethod speak :around ((x fish)) 1)");
        assert!(!report.findings[0].orphaned);
    }

    #[test]
    fn an_unspecialized_parameter_is_recorded_rather_than_skipped() {
        let report = report("(defmethod speak ((x fish) y) 1)");
        assert_eq!(
            report.findings[0].specializers,
            vec!["FISH".to_owned(), "t".to_owned()]
        );
    }

    #[test]
    fn a_lambda_list_keyword_ends_the_specializer_list() {
        let report = report("(defmethod speak ((x fish) &optional y) 1)");
        assert_eq!(report.findings[0].specializers, vec!["FISH".to_owned()]);
    }

    #[test]
    fn a_non_method_definition_is_not_reported() {
        let report = report("(defun speak (x) x)");
        assert!(report.findings.is_empty());
    }

    #[test]
    fn an_unmodelled_dialect_says_so_rather_than_reporting_nothing() {
        let tree =
            SyntaxTree::parse_with_dialect("(defn f [x] x)", Dialect::Clojure).expect("parse");
        let report = build_method_combination_report(Path::new("t.clj"), Dialect::Clojure, &tree);
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }
}
