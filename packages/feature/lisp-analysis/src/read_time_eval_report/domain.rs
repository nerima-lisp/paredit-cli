//! Every `#.` in a file: code that runs while the file is being *read*.
//!
//! `#.(form)` evaluates `form` at read time and splices the result in as a
//! literal. That has three consequences worth a report of its own:
//!
//! - **Build reproducibility.** `#.(get-universal-time)` bakes the build clock
//!   into the fasl. Two builds of identical source produce different output,
//!   and nothing downstream can tell why.
//! - **Trust.** Reading a file executes it. Anything that reads untrusted
//!   source with `*read-eval*` left at its default `t` is running that source,
//!   not parsing it.
//! - **Analysis.** Every other report in this tool sees `#.(…)` as an opaque
//!   atom, because what it denotes is not knowable without evaluating it. A
//!   file with `#.` has regions where the rest of the tool is guessing, and
//!   this is the report that says where.
//!
//! Detection is on the reader dispatch, not on a form's head, because that is
//! what `#.` is: `#.` and `#'` differ in the reader, and by the time there is a
//! tree the difference is already baked into what the tree contains.

use std::path::Path;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::for_each_subview;
use serde_json::{Value, json};

use paredit_core_cli::report::{FileFindings, Finding};

/// How much of the evaluated form the finding quotes.
const FORM_LIMIT: usize = 64;

/// Read-time evaluation whose result is knowable without running it.
///
/// `#.(quote x)` and `#.'x` are read-time evaluation that a reader could have
/// spelled without `#.`; flagging them at the same weight as
/// `#.(run-program …)` would drown the findings that matter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Risk {
    /// The evaluated form is a self-quoting literal or a `quote`, so reading
    /// the file cannot do anything but produce that value.
    Inert,
    /// Anything else. The form is a call, and this layer does not evaluate.
    Live,
}

impl Risk {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Inert => "inert",
            Self::Live => "live",
        }
    }
}

/// One `#.` dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadTimeEval {
    pub risk: Risk,
    /// The evaluated form as written, elided.
    pub form: String,
    /// The operator in head position, when the form is a call. The single most
    /// useful field for triage: `+` and `run-program` are not the same finding.
    pub head: Option<String>,
    pub span: ByteSpan,
    pub line: usize,
}

impl Finding for ReadTimeEval {
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
            self.head.clone().unwrap_or_else(|| "-".to_owned()),
            self.form.clone(),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("risk", json!(self.risk.label())),
            ("head", json!(self.head)),
            ("form", json!(self.form)),
        ]
    }
}

#[must_use]
pub fn build_read_time_eval_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<ReadTimeEval> {
    let modelled = dialect == Dialect::CommonLisp;
    let source = tree.source();
    let mut findings = Vec::new();

    if modelled {
        for_each_subview(&tree.root_view(), |view| {
            if let Some(finding) = read_time_eval(view, source) {
                findings.push(finding);
            }
        });
    }

    let live = findings
        .iter()
        .filter(|finding: &&ReadTimeEval| finding.risk == Risk::Live)
        .count();

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        findings,
        vec![("live_count", json!(live))],
    )
}

/// Reads one node as a `#.` dispatch, or `None`.
///
/// A dialect-aware parse keeps `#.(…)` as one opaque atom, so the test is on
/// the atom's own text rather than on a reader-prefix list: there is no child
/// node to inspect.
fn read_time_eval(view: &ExpressionView, source: &str) -> Option<ReadTimeEval> {
    let text = source.get(view.span.start().get()..view.span.end().get())?;
    let form = text.strip_prefix("#.")?;
    if form.is_empty() {
        return None;
    }

    Some(ReadTimeEval {
        risk: risk_of(form),
        head: head_of(form),
        form: elide(form),
        span: view.span,
        line: line_of(source, view.span.start().get()),
    })
}

/// Whether the evaluated form can do anything beyond denoting a value.
fn risk_of(form: &str) -> Risk {
    let trimmed = form.trim();
    if trimmed.starts_with('\'') || trimmed.starts_with('"') {
        return Risk::Inert;
    }
    if !trimmed.starts_with('(') {
        // A bare symbol or number. Reading it looks up a variable at most,
        // which cannot run arbitrary code.
        return Risk::Inert;
    }
    match head_of(form).as_deref() {
        Some(head) if head.eq_ignore_ascii_case("quote") => Risk::Inert,
        _ => Risk::Live,
    }
}

/// The operator in head position, when the form is a call.
fn head_of(form: &str) -> Option<String> {
    let inner = form.trim().strip_prefix('(')?;
    let head = inner
        .split(|character: char| character.is_whitespace() || character == '(' || character == ')')
        .find(|token| !token.is_empty())?;
    Some(head.to_owned())
}

fn elide(form: &str) -> String {
    let collapsed = form.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= FORM_LIMIT {
        return collapsed;
    }
    let head = collapsed
        .chars()
        .take(FORM_LIMIT.saturating_sub(1))
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

    fn report(source: &str) -> FileFindings<ReadTimeEval> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_read_time_eval_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    #[test]
    fn a_call_at_read_time_is_live() {
        let report = report("(defvar *built* #.(get-universal-time))");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].risk, Risk::Live);
        assert_eq!(
            report.findings[0].head.as_deref(),
            Some("get-universal-time")
        );
    }

    #[test]
    fn a_quoted_datum_at_read_time_is_inert() {
        let report = report("(defvar *x* #.(quote foo))");
        assert_eq!(report.findings[0].risk, Risk::Inert);
    }

    #[test]
    fn the_live_count_excludes_inert_dispatches() {
        let report = report("(list #.(quote a) #.(f))");
        assert_eq!(report.summary, vec![("live_count", json!(1))]);
    }

    #[test]
    fn a_sharp_quote_is_not_a_read_time_eval() {
        let report = report("(mapcar #'car xs)");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_file_with_no_dispatch_reports_nothing() {
        let report = report("(defun f () 1)");
        assert!(report.findings.is_empty());
    }

    #[test]
    fn findings_are_in_source_order() {
        let report = report("(list #.(a) #.(b) #.(c))");
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
        let source = "(def x (f))";
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Clojure).expect("parse");
        let report = build_read_time_eval_report(Path::new("t.clj"), Dialect::Clojure, &tree);
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn a_head_is_read_only_from_a_call() {
        assert_eq!(head_of("(f x)").as_deref(), Some("f"));
        assert_eq!(head_of("x"), None);
    }
}
