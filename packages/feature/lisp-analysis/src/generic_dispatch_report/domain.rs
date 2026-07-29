//! `defgeneric` declarations against the `defmethod` forms that implement them.
//!
//! A generic function's behaviour lives in no single form. `defgeneric` fixes
//! the lambda list and the method combination; each `defmethod` supplies one
//! branch of the dispatch; and nothing checks that the two agree until a call
//! reaches a specializer nobody wrote a method for.
//!
//! Four disagreements are worth naming, and each is invisible from either form
//! alone:
//!
//! - A **method with no `defgeneric`.** Legal — the first `defmethod` creates
//!   the generic implicitly — but the lambda list then is whatever the
//!   first-compiled method happened to say, which is an accident rather than a
//!   decision.
//! - A **`defgeneric` with no method.** Calling it signals
//!   `no-applicable-method`. Usually a rename that missed one side.
//! - An **arity disagreement.** CLHS requires every method's lambda list to be
//!   congruent with the generic's. A method with the wrong number of required
//!   parameters is a compile-time error in a conforming implementation.
//! - A **duplicate method**: same name, qualifier, and specializers. The second
//!   silently replaces the first.
//!
//! Two methods on *different* classes are ordinary CLOS, not a defect, and are
//! reported as part of the dispatch table rather than flagged.

use std::collections::BTreeMap;
use std::path::Path;

use paredit_core_syntax::common_lisp::common_lisp_operator_head_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

use paredit_core_cli::report::{FileFindings, Finding};

/// What the analysis concluded about one generic function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchVerdict {
    /// A `defgeneric` with at least one congruent method.
    Declared,
    /// Methods with no `defgeneric`, so the lambda list is implicit.
    Undeclared,
    /// A `defgeneric` no method implements.
    Unimplemented,
    /// A method's required-parameter count differs from the generic's.
    ArityMismatch,
    /// Two methods share a name, qualifier, and specializer list.
    DuplicateMethod,
}

impl DispatchVerdict {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Declared => "declared",
            Self::Undeclared => "undeclared",
            Self::Unimplemented => "unimplemented",
            Self::ArityMismatch => "arity-mismatch",
            Self::DuplicateMethod => "duplicate-method",
        }
    }

    #[must_use]
    pub const fn is_defect(self) -> bool {
        !matches!(self, Self::Declared)
    }
}

/// One generic function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericFinding {
    pub verdict: DispatchVerdict,
    pub name: String,
    /// Required-parameter count from the `defgeneric`, when one exists.
    pub declared_arity: Option<usize>,
    /// How many `defmethod` forms implement it in this file.
    pub method_count: usize,
    /// Each method's qualifier and specializer list: the dispatch table.
    pub specializers: Vec<String>,
    /// The `defgeneric` when there is one, otherwise the first method — so a
    /// finding always points at a form that exists.
    pub span: ByteSpan,
    pub line: usize,
}

impl Finding for GenericFinding {
    fn kind(&self) -> &'static str {
        self.verdict.label()
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
            format!(
                "declared_arity={}",
                self.declared_arity
                    .map_or_else(|| "-".to_owned(), |arity| arity.to_string())
            ),
            format!("methods={}", self.method_count),
            format!("on=[{}]", self.specializers.join("; ")),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("verdict", json!(self.verdict.label())),
            ("name", json!(self.name)),
            ("declared_arity", json!(self.declared_arity)),
            ("method_count", json!(self.method_count)),
            ("specializers", json!(self.specializers)),
        ]
    }
}

#[derive(Default)]
struct Generic {
    declared_arity: Option<usize>,
    declaration_span: Option<ByteSpan>,
    methods: Vec<Method>,
}

struct Method {
    arity: usize,
    qualifier: String,
    specializers: String,
    span: ByteSpan,
}

#[must_use]
pub fn build_generic_dispatch_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<GenericFinding> {
    let modelled = dialect == Dialect::CommonLisp;
    let source = tree.source();
    let mut generics: BTreeMap<String, Generic> = BTreeMap::new();

    if modelled {
        for form in &tree.root_view().children {
            let Some(head) = list_head(form) else {
                continue;
            };
            if common_lisp_operator_head_eq(head, "defgeneric") {
                let Some(name) = form.children.get(1).and_then(atom_symbol_text) else {
                    continue;
                };
                let entry = generics.entry(fold(name)).or_default();
                entry.declared_arity = form.children.get(2).map(required_arity);
                entry.declaration_span = Some(form.span);
            } else if common_lisp_operator_head_eq(head, "defmethod") {
                if let Some((name, method)) = read_method(form) {
                    generics.entry(name).or_default().methods.push(method);
                }
            }
        }
    }

    let findings = generics
        .into_iter()
        .filter_map(|(name, generic)| {
            let span = generic
                .declaration_span
                .or_else(|| generic.methods.first().map(|method| method.span))?;
            let verdict = verdict_for(&generic);
            Some(GenericFinding {
                verdict,
                name,
                declared_arity: generic.declared_arity,
                method_count: generic.methods.len(),
                specializers: generic
                    .methods
                    .iter()
                    .map(|method| {
                        if method.qualifier.is_empty() {
                            method.specializers.clone()
                        } else {
                            format!("{} {}", method.qualifier, method.specializers)
                        }
                    })
                    .collect(),
                span,
                line: line_of(source, span.start().get()),
            })
        })
        .collect::<Vec<_>>();

    let defects = findings
        .iter()
        .filter(|generic| generic.verdict.is_defect())
        .count();

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        findings,
        vec![("defect_count", json!(defects))],
    )
}

/// The first disagreement, in severity order.
///
/// Ordered rather than arbitrary: an arity mismatch is a conformance error and
/// a missing `defgeneric` is a style question, so a generic with both reports
/// the one worth fixing first.
fn verdict_for(generic: &Generic) -> DispatchVerdict {
    if generic.methods.is_empty() {
        return DispatchVerdict::Unimplemented;
    }
    if let Some(declared) = generic.declared_arity {
        if generic
            .methods
            .iter()
            .any(|method| method.arity != declared)
        {
            return DispatchVerdict::ArityMismatch;
        }
    }

    let mut seen = generic
        .methods
        .iter()
        .map(|method| (method.qualifier.as_str(), method.specializers.as_str()))
        .collect::<Vec<_>>();
    seen.sort_unstable();
    let count = seen.len();
    seen.dedup();
    if seen.len() != count {
        return DispatchVerdict::DuplicateMethod;
    }

    if generic.declared_arity.is_none() {
        return DispatchVerdict::Undeclared;
    }
    DispatchVerdict::Declared
}

fn read_method(form: &ExpressionView) -> Option<(String, Method)> {
    let name = fold(atom_symbol_text(form.children.get(1)?)?);
    // The qualifier is optional and sits between the name and the lambda list,
    // so the lambda list is found by shape rather than by a fixed index.
    let lambda_index = form
        .children
        .iter()
        .enumerate()
        .skip(2)
        .find(|(_, child)| child.kind == ExpressionKind::List)
        .map(|(index, _)| index)?;
    let qualifier = if lambda_index > 2 {
        atom_symbol_text(form.children.get(2)?)
            .unwrap_or_default()
            .to_owned()
    } else {
        String::new()
    };
    let lambda_list = &form.children[lambda_index];

    Some((
        name,
        Method {
            arity: required_arity(lambda_list),
            qualifier,
            specializers: specializer_key(lambda_list),
            span: form.span,
        },
    ))
}

/// How many required parameters a lambda list has.
///
/// The count stops at the first lambda-list keyword: `&optional` and everything
/// after it takes no part in dispatch, and congruence is defined on the
/// required parameters.
fn required_arity(lambda_list: &ExpressionView) -> usize {
    required_parameters(lambda_list).count()
}

fn required_parameters(lambda_list: &ExpressionView) -> impl Iterator<Item = &ExpressionView> {
    lambda_list.children.iter().take_while(|parameter| {
        atom_symbol_text(parameter).is_none_or(|text| !text.starts_with('&'))
    })
}

fn specializer_key(lambda_list: &ExpressionView) -> String {
    let names = required_parameters(lambda_list)
        .map(|parameter| match parameter.kind {
            ExpressionKind::List => parameter
                .children
                .get(1)
                .and_then(atom_symbol_text)
                .map_or_else(|| "t".to_owned(), fold),
            _ => "t".to_owned(),
        })
        .collect::<Vec<_>>();
    format!("({})", names.join(" "))
}

fn fold(name: &str) -> String {
    name.to_ascii_uppercase()
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

    fn report(source: &str) -> FileFindings<GenericFinding> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_generic_dispatch_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    fn verdict(source: &str) -> DispatchVerdict {
        let report = report(source);
        assert_eq!(report.findings.len(), 1, "{report:?}");
        report.findings[0].verdict
    }

    #[test]
    fn a_declared_generic_with_a_congruent_method_is_sound() {
        assert_eq!(
            verdict("(defgeneric speak (x))\n(defmethod speak ((x fish)) 1)"),
            DispatchVerdict::Declared
        );
    }

    #[test]
    fn methods_with_no_defgeneric_are_reported_as_undeclared() {
        assert_eq!(
            verdict("(defmethod speak ((x fish)) 1)"),
            DispatchVerdict::Undeclared
        );
    }

    #[test]
    fn a_defgeneric_with_no_method_is_reported_as_unimplemented() {
        assert_eq!(
            verdict("(defgeneric speak (x))"),
            DispatchVerdict::Unimplemented
        );
    }

    #[test]
    fn a_method_with_the_wrong_required_arity_is_reported() {
        assert_eq!(
            verdict("(defgeneric speak (x))\n(defmethod speak ((x fish) y) 1)"),
            DispatchVerdict::ArityMismatch
        );
    }

    #[test]
    fn two_methods_on_the_same_specializers_are_a_duplicate() {
        assert_eq!(
            verdict(
                "(defgeneric speak (x))\n\
                 (defmethod speak ((x fish)) 1)\n\
                 (defmethod speak ((x fish)) 2)"
            ),
            DispatchVerdict::DuplicateMethod
        );
    }

    #[test]
    fn two_methods_on_different_classes_are_ordinary_dispatch() {
        assert_eq!(
            verdict(
                "(defgeneric speak (x))\n\
                 (defmethod speak ((x fish)) 1)\n\
                 (defmethod speak ((x bird)) 2)"
            ),
            DispatchVerdict::Declared
        );
    }

    #[test]
    fn a_qualifier_distinguishes_two_methods_on_the_same_specializers() {
        assert_eq!(
            verdict(
                "(defgeneric speak (x))\n\
                 (defmethod speak ((x fish)) 1)\n\
                 (defmethod speak :before ((x fish)) 2)"
            ),
            DispatchVerdict::Declared
        );
    }

    #[test]
    fn optional_parameters_do_not_count_toward_congruence() {
        assert_eq!(
            verdict("(defgeneric speak (x))\n(defmethod speak ((x fish) &optional y) 1)"),
            DispatchVerdict::Declared
        );
    }

    #[test]
    fn the_dispatch_table_is_reported_beside_the_verdict() {
        let report = report("(defgeneric speak (x))\n(defmethod speak ((x fish)) 1)");
        assert_eq!(report.findings[0].specializers, vec!["(FISH)".to_owned()]);
    }

    #[test]
    fn an_unmodelled_dialect_says_so_rather_than_reporting_nothing() {
        let tree =
            SyntaxTree::parse_with_dialect("(defmulti f :k)", Dialect::Clojure).expect("parse");
        let report = build_generic_dispatch_report(Path::new("t.clj"), Dialect::Clojure, &tree);
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }
}
