//! The two ways a `defmacro` template betrays its caller.
//!
//! A macro is a program that writes a program, and the program it writes is
//! spliced into a scope it cannot see. Two failures follow from that, both
//! invisible at the definition site and both reported here:
//!
//! - **Variable capture.** A template that binds a literal name — `` `(let
//!   ((result ,form)) …) `` — silently shadows the caller's `result`. Any
//!   argument form referring to the caller's `result` now sees the macro's.
//!   Common Lisp has no hygienic macros, so the only defence is a `gensym`, and
//!   the only way to check that it was used is to look.
//! - **Multiple evaluation.** A template that unquotes one parameter twice —
//!   `` `(if (> ,x 0) ,x 0) `` — evaluates the caller's argument form twice.
//!   `(m (pop stack))` then pops twice. This is why the `once-only` idiom
//!   exists.
//!
//! Both checks are syntactic and conservative in the same direction: they read
//! the template as written and report what is *visible* there. A template that
//! computes a binding name, or splices in a form built elsewhere, is beyond
//! this analysis and produces no finding rather than a guess.

use std::collections::BTreeMap;
use std::path::Path;

use paredit_core_syntax::common_lisp::common_lisp_operator_head_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

use paredit_core_cli::report::{FileFindings, Finding, line_of};

/// What a template does wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HygieneRisk {
    /// The template binds a literal name that is not obviously a gensym.
    VariableCapture,
    /// The template unquotes one parameter more than once.
    MultipleEvaluation,
}

impl HygieneRisk {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::VariableCapture => "variable-capture",
            Self::MultipleEvaluation => "multiple-evaluation",
        }
    }
}

/// One hygiene risk in one macro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HygieneFinding {
    pub risk: HygieneRisk,
    /// The macro the template belongs to.
    pub macro_name: String,
    /// The captured binding name, or the multiply-evaluated parameter.
    pub subject: String,
    /// How many times the parameter is unquoted, for
    /// [`HygieneRisk::MultipleEvaluation`]. `0` for a capture.
    pub occurrences: usize,
    /// The remedy, named rather than left to the reader.
    pub remedy: &'static str,
    pub span: ByteSpan,
    pub line: usize,
}

impl Finding for HygieneFinding {
    fn kind(&self) -> &'static str {
        self.risk.label()
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            self.macro_name.clone(),
            self.subject.clone(),
            format!("occurrences={}", self.occurrences),
            self.remedy.to_owned(),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("risk", json!(self.risk.label())),
            ("macro_name", json!(self.macro_name)),
            ("subject", json!(self.subject)),
            ("occurrences", json!(self.occurrences)),
            ("remedy", json!(self.remedy)),
        ]
    }
}

/// Binding forms whose second child is a binding list.
const BINDING_FORMS: [&str; 4] = ["let", "let*", "flet", "labels"];

#[must_use]
pub fn build_macro_hygiene_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<HygieneFinding> {
    let modelled = dialect == Dialect::CommonLisp;
    let source = tree.source();
    let mut findings = Vec::new();
    let mut macro_count = 0;

    if modelled {
        for form in &tree.root_view().children {
            let Some(head) = list_head(form) else {
                continue;
            };
            if !common_lisp_operator_head_eq(head, "defmacro") {
                continue;
            }
            let Some(name) = form.children.get(1).and_then(atom_symbol_text) else {
                continue;
            };
            macro_count += 1;
            analyze(form, name, source, &mut findings);
        }
    }

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        findings,
        vec![("macro_count", json!(macro_count))],
    )
}

fn analyze(form: &ExpressionView, name: &str, source: &str, findings: &mut Vec<HygieneFinding>) {
    let parameters = form
        .children
        .get(2)
        .map(parameter_names)
        .unwrap_or_default();

    // Names bound by the macro's own body outside the template — the usual
    // `(let ((var (gensym))) ...)` prelude — are safe by construction, because
    // whatever they hold at expansion time is a fresh symbol.
    let gensym_bound = gensym_bindings(form);

    for body in form.children.iter().skip(3) {
        find_captures(body, name, &gensym_bound, source, findings);
        find_multiple_evaluation(body, name, &parameters, source, findings);
    }
}

/// The required and `&body`/`&rest` parameter names of a macro lambda list.
fn parameter_names(lambda_list: &ExpressionView) -> Vec<String> {
    lambda_list
        .children
        .iter()
        .filter_map(atom_symbol_text)
        .filter(|name| !name.starts_with('&'))
        .map(str::to_ascii_uppercase)
        .collect()
}

/// Names the macro body binds to a freshly generated symbol.
///
/// Recognised by the initial form's head rather than by the name's spelling:
/// `g!foo` is a convention and `(gensym)` is a proof, and only one of those is
/// something to build a report on.
fn gensym_bindings(form: &ExpressionView) -> BTreeMap<String, ()> {
    let mut bound = BTreeMap::new();
    walk(form, &mut |view| {
        let Some(head) = list_head(view) else { return };
        if !BINDING_FORMS
            .iter()
            .any(|name| common_lisp_operator_head_eq(head, name))
        {
            return;
        }
        for binding in view
            .children
            .get(1)
            .map(|list| list.children.as_slice())
            .unwrap_or_default()
        {
            let Some(name) = binding.children.first().and_then(atom_symbol_text) else {
                continue;
            };
            let generated = binding
                .children
                .get(1)
                .and_then(list_head)
                .is_some_and(|head| {
                    ["gensym", "gentemp", "make-symbol", "copy-symbol"]
                        .iter()
                        .any(|generator| common_lisp_operator_head_eq(head, generator))
                });
            if generated {
                bound.insert(name.to_ascii_uppercase(), ());
            }
        }
    });
    bound
}

/// Reports every literal binding inside a quasiquoted template.
fn find_captures(
    view: &ExpressionView,
    macro_name: &str,
    gensym_bound: &BTreeMap<String, ()>,
    source: &str,
    findings: &mut Vec<HygieneFinding>,
) {
    if !is_quasiquoted(view) {
        for child in &view.children {
            find_captures(child, macro_name, gensym_bound, source, findings);
        }
        return;
    }

    walk(view, &mut |inner| {
        let Some(head) = list_head(inner) else { return };
        if !BINDING_FORMS
            .iter()
            .any(|name| common_lisp_operator_head_eq(head, name))
        {
            return;
        }
        for binding in inner
            .children
            .get(1)
            .map(|list| list.children.as_slice())
            .unwrap_or_default()
        {
            let Some(bound) = binding_target(binding) else {
                continue;
            };
            // An unquoted binding name — `(let ((,var ...)) …)` — is whatever
            // the macro computed, which is the `gensym` idiom spelled at the
            // use site rather than the binding site. The unquote is read off
            // the node's reader prefixes, not off its text: `atom_symbol_text`
            // deliberately steps past the prefix, so `,var` and `var` are the
            // same string by the time a name is in hand.
            if is_unquoted(bound) {
                continue;
            }
            let Some(folded) = atom_symbol_text(bound).map(str::to_ascii_uppercase) else {
                continue;
            };
            if gensym_bound.contains_key(&folded) {
                continue;
            }
            findings.push(HygieneFinding {
                risk: HygieneRisk::VariableCapture,
                macro_name: macro_name.to_ascii_uppercase(),
                subject: folded,
                occurrences: 0,
                remedy: "bind the name to (gensym) outside the template and unquote it",
                span: binding.span,
                line: line_of(source, binding.span.start().get()),
            });
        }
    });
}

/// Reports every parameter a template unquotes more than once.
fn find_multiple_evaluation(
    view: &ExpressionView,
    macro_name: &str,
    parameters: &[String],
    source: &str,
    findings: &mut Vec<HygieneFinding>,
) {
    if !is_quasiquoted(view) {
        for child in &view.children {
            find_multiple_evaluation(child, macro_name, parameters, source, findings);
        }
        return;
    }

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    walk(view, &mut |inner| {
        if inner.kind != ExpressionKind::Atom {
            return;
        }
        // `,@x` splices a list and evaluates it once, however many forms come
        // out, so only the plain `,x` unquote is counted. Both are read off the
        // reader prefixes rather than the text, which no longer carries them.
        if !inner.reader_prefixes.contains(&ReaderPrefix::Unquote) {
            return;
        }
        let Some(name) = atom_symbol_text(inner) else {
            return;
        };
        let folded = name.to_ascii_uppercase();
        if parameters.contains(&folded) {
            *counts.entry(folded).or_default() += 1;
        }
    });

    for (name, occurrences) in counts {
        if occurrences < 2 {
            continue;
        }
        findings.push(HygieneFinding {
            risk: HygieneRisk::MultipleEvaluation,
            macro_name: macro_name.to_ascii_uppercase(),
            subject: name,
            occurrences,
            remedy: "bind the argument once in the expansion (the once-only idiom)",
            span: view.span,
            line: line_of(source, view.span.start().get()),
        });
    }
}

/// The node a binding binds: the binding itself when it is a bare name, or its
/// first child when it is a `(name value)` pair.
///
/// Returns the *node* rather than the name so the caller can still ask about
/// its reader prefixes, which is the only place the unquote survives.
fn binding_target(binding: &ExpressionView) -> Option<&ExpressionView> {
    match binding.kind {
        ExpressionKind::Atom => Some(binding),
        ExpressionKind::List => binding.children.first(),
        ExpressionKind::Root => None,
    }
}

fn is_unquoted(view: &ExpressionView) -> bool {
    view.reader_prefixes.contains(&ReaderPrefix::Unquote)
        || view
            .reader_prefixes
            .contains(&ReaderPrefix::UnquoteSplicing)
        || view
            .text
            .as_deref()
            .is_some_and(|text| text.starts_with(','))
}

/// Whether a node is quasiquoted, by prefix node or by folded-in spelling.
///
/// Both spellings occur: a backquote before a list becomes a reader prefix,
/// while a backquote consumed into an atom leaves a leading `` ` `` in the
/// text.
fn is_quasiquoted(view: &ExpressionView) -> bool {
    view.reader_prefixes.contains(&ReaderPrefix::Quasiquote)
        || view
            .text
            .as_deref()
            .is_some_and(|text| text.starts_with('`'))
}

fn walk(view: &ExpressionView, visit: &mut impl FnMut(&ExpressionView)) {
    visit(view);
    for child in &view.children {
        walk(child, visit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(source: &str) -> FileFindings<HygieneFinding> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_macro_hygiene_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    #[test]
    fn a_literal_binding_in_a_template_is_a_capture_risk() {
        let report = report("(defmacro m (form) `(let ((result ,form)) (list result)))");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].risk, HygieneRisk::VariableCapture);
        assert_eq!(report.findings[0].subject, "RESULT");
    }

    #[test]
    fn a_gensym_bound_name_is_not_a_capture_risk() {
        let report = report(
            "(defmacro m (form) (let ((result (gensym))) `(let ((,result ,form)) (list ,result))))",
        );
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn an_unquoted_binding_name_is_not_a_capture_risk() {
        let report = report("(defmacro m (var form) `(let ((,var ,form)) ,var))");
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.risk != HygieneRisk::VariableCapture),
            "{report:?}"
        );
    }

    #[test]
    fn a_parameter_unquoted_twice_is_a_multiple_evaluation_risk() {
        let report = report("(defmacro m (x) `(if (> ,x 0) ,x 0))");
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.risk == HygieneRisk::MultipleEvaluation)
            .expect("multiple evaluation is reported");
        assert_eq!(finding.subject, "X");
        assert_eq!(finding.occurrences, 2);
    }

    #[test]
    fn a_parameter_unquoted_once_is_not_reported() {
        let report = report("(defmacro m (x) `(list ,x))");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_splicing_unquote_evaluates_once_however_many_forms_it_produces() {
        let report = report("(defmacro m (&body body) `(progn ,@body ,@body))");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_name_that_is_not_a_parameter_is_not_counted_for_multiple_evaluation() {
        let report = report("(defmacro m (x) `(list ,x ,*global* ,*global*))");
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.risk != HygieneRisk::MultipleEvaluation),
            "{report:?}"
        );
    }

    #[test]
    fn a_binding_outside_a_template_is_the_macros_own_and_is_not_reported() {
        let report = report("(defmacro m (form) (let ((result form)) `(list ,result)))");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn the_macro_count_is_reported_even_when_nothing_is_wrong() {
        let report = report("(defmacro m (x) `(list ,x))");
        assert_eq!(report.summary, vec![("macro_count", json!(1))]);
    }

    #[test]
    fn a_defun_is_not_a_macro() {
        let report = report("(defun f (form) (let ((result form)) result))");
        assert_eq!(report.summary, vec![("macro_count", json!(0))]);
    }

    #[test]
    fn an_unmodelled_dialect_says_so_rather_than_reporting_nothing() {
        let tree =
            SyntaxTree::parse_with_dialect("(defmacro m [x] x)", Dialect::Clojure).expect("parse");
        let report = build_macro_hygiene_report(Path::new("t.clj"), Dialect::Clojure, &tree);
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }
}
