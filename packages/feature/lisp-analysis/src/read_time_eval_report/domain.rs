//! Code that runs while a file is being *read*, across every dialect that has
//! a construct for it.
//!
//! Common Lisp's `#.(form)` and Clojure/EDN's `#=(form)` both evaluate `form`
//! at read time and splice the result in as a literal; Emacs's
//! `.dir-locals.el` runs whatever sits under an `eval:` key the moment the
//! file is visited. All three share the same three consequences:
//!
//! - **Build reproducibility.** `#.(get-universal-time)` bakes the build clock
//!   into the fasl. Two builds of identical source produce different output,
//!   and nothing downstream can tell why.
//! - **Trust.** Reading a file executes it. Anything that reads untrusted
//!   source with `*read-eval*` left at its default `t` (or opens a directory
//!   with a stray `.dir-locals.el`) is running that source, not parsing it.
//! - **Analysis.** Every other report in this tool sees `#.(…)`/`#=(…)` as an
//!   opaque atom, because what it denotes is not knowable without evaluating
//!   it. A file with one has regions where the rest of the tool is guessing,
//!   and this is the report that says where.
//!
//! `#.` and `#=` detection is on the reader dispatch, not on a form's head,
//! because that is what those prefixes are: `#.` and `#'` differ in the
//! reader, and by the time there is a tree the difference is already baked
//! into what the tree contains. `.dir-locals.el` detection is different in
//! kind — `eval` is a known alist key, not a reader dispatch — so it walks
//! the file's alist shape instead of scanning for a prefix.
//!
//! Every dialect without one of these three constructs (Scheme, Racket,
//! Emacs Lisp outside `.dir-locals.el`, Hy, Carp, Janet, Fennel, LFE) reports
//! `unmodelled` rather than a clean empty result, matching every other report
//! in this module.

use std::path::Path;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list};
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
}

impl Finding for ReadTimeEval {
    fn kind(&self) -> &'static str {
        self.risk.label()
    }

    fn span(&self) -> ByteSpan {
        self.span
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
    let source = tree.source();
    let root = tree.root_view();

    let (modelled, findings) = match dialect {
        Dialect::CommonLisp => (true, dispatch_findings(&root, source, "#.")),
        Dialect::Clojure => (true, dispatch_findings(&root, source, "#=")),
        Dialect::EmacsLisp if is_dir_locals_path(path) => {
            (true, dir_locals_eval_findings(&root, source))
        }
        _ => (false, Vec::new()),
    };

    let live = findings
        .iter()
        .filter(|finding: &&ReadTimeEval| finding.risk == Risk::Live)
        .count();

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        source,
        findings,
        vec![("live_count", json!(live))],
    )
}

/// Every read-time-eval dispatch (`#.` for Common Lisp, `#=` for Clojure/EDN)
/// under `root`.
fn dispatch_findings(root: &ExpressionView, source: &str, prefix: &str) -> Vec<ReadTimeEval> {
    let mut findings = Vec::new();
    for_each_subview(root, |view| {
        if let Some(finding) = read_time_eval(view, source, prefix) {
            findings.push(finding);
        }
    });
    findings
}

/// Reads one node as a `prefix` dispatch (`#.` in Common Lisp, `#=` in
/// Clojure/EDN), or `None`.
///
/// A dialect-aware parse keeps the dispatch and its form together as one
/// node, so the test is on that node's own text rather than on a
/// reader-prefix list: there is no child node to inspect. `risk_of`,
/// `head_of`, and `elide` know nothing about which dialect's prefix found the
/// form, so this one function serves both.
fn read_time_eval(view: &ExpressionView, source: &str, prefix: &str) -> Option<ReadTimeEval> {
    let text = source.get(view.span.start().get()..view.span.end().get())?;
    let form = text.strip_prefix(prefix)?;
    if form.is_empty() {
        return None;
    }

    Some(ReadTimeEval {
        risk: risk_of(form),
        head: head_of(form),
        form: elide(form),
        span: view.span,
    })
}

/// Whether `path`'s file name is exactly `.dir-locals.el`, the one name
/// Emacs recognizes for this file.
///
/// `data-report`'s `data-check --format dir-locals` already answers this
/// same question (`is_dir_locals_path`), but `lisp-analysis` and
/// `data-report` are independent leaf reports with no dependency between
/// them today. Adding one for a six-line filename check would trade a
/// one-line duplication for a standing coupling neither report otherwise
/// needs, so this repeats the check instead.
fn is_dir_locals_path(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some(".dir-locals.el")
}

/// Every `eval` key's value in a `.dir-locals.el` variable-alist,
/// risk-classified the same way as a `#.`/`#=` dispatch.
///
/// `data-report`'s `dir-locals` format already finds every `eval` key's
/// *presence* (`DirLocalsEvalPresent`) without judging it. This is a
/// different question — is what runs when Emacs visits this directory inert
/// or live — and answering it is what this report exists to do, so it walks
/// the same alist shape again rather than consuming a finding kind that
/// carries no risk information.
fn dir_locals_eval_findings(root: &ExpressionView, source: &str) -> Vec<ReadTimeEval> {
    let mut findings = Vec::new();
    let Some(top) = root.children.first().filter(|view| is_paren_list(view)) else {
        return findings;
    };

    for entry in &top.children {
        let Some(variable_alist) = dir_locals_alist_value(entry).filter(|view| is_paren_list(view))
        else {
            continue;
        };
        for variable_entry in &variable_alist.children {
            let Some((key_view, value_view)) = dir_locals_pair(variable_entry) else {
                continue;
            };
            if atom_text(key_view) != Some("eval") {
                continue;
            }
            let Some(text) = source.get(value_view.span.start().get()..value_view.span.end().get())
            else {
                continue;
            };
            findings.push(ReadTimeEval {
                risk: risk_of(text),
                head: head_of(text),
                form: elide(text),
                span: value_view.span,
            });
        }
    }
    findings
}

/// The `variable-alist` half of a `(mode-or-nil . variable-alist)` entry, or
/// `None` when `entry` is not shaped like one.
fn dir_locals_alist_value(entry: &ExpressionView) -> Option<&ExpressionView> {
    dir_locals_pair(entry).map(|(_, value)| value)
}

/// Reads `entry` as a `(key . value)` or `(key value)` pair, or `None` when
/// it is not shaped like one.
fn dir_locals_pair(entry: &ExpressionView) -> Option<(&ExpressionView, &ExpressionView)> {
    if !is_paren_list(entry) {
        return None;
    }
    let key = entry.children.first()?;
    atom_text(key)?;
    match entry.children.len() {
        2 => Some((key, &entry.children[1])),
        3 if is_dot(&entry.children[1]) => Some((key, &entry.children[2])),
        _ => None,
    }
}

/// Whether `view` is the `.` of a dotted pair — punctuation, not a value.
///
/// This tool's structural parser has no notion of a cons cell: `(a . b)`
/// parses as a three-child list whose middle child is a plain atom spelled
/// `.`. Recognising that atom is the only way to read a dotted-pair alist
/// entry at all.
fn is_dot(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::Atom
        && view.reader_prefixes.is_empty()
        && atom_text(view) == Some(".")
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
    fn a_head_is_read_only_from_a_call() {
        assert_eq!(head_of("(f x)").as_deref(), Some("f"));
        assert_eq!(head_of("x"), None);
    }

    fn report_with(source: &str, dialect: Dialect, path: &str) -> FileFindings<ReadTimeEval> {
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
        build_read_time_eval_report(Path::new(path), dialect, &tree)
    }

    #[test]
    fn a_clojure_read_eval_call_is_live() {
        let report = report_with(
            "(def x #=(System/currentTimeMillis))",
            Dialect::Clojure,
            "t.clj",
        );
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].risk, Risk::Live);
        assert_eq!(
            report.findings[0].head.as_deref(),
            Some("System/currentTimeMillis")
        );
    }

    #[test]
    fn a_clojure_read_eval_quoted_datum_is_inert() {
        let report = report_with("(def x #=(quote foo))", Dialect::Clojure, "t.clj");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].risk, Risk::Inert);
    }

    #[test]
    fn a_clojure_read_eval_with_no_space_before_the_payload_is_still_found() {
        // `=` is not a Clojure symbol constituent, so `#=foo` unambiguously
        // dispatches `#=` on the bare symbol `foo` with no space required.
        // The reader-policy fix that gives `#=` its own two-byte dispatch
        // (rather than falling into the generic tagged-literal scanner,
        // which would otherwise fold `=foo` into one tag name) is what makes
        // this a read-time-eval finding at all.
        let report = report_with("(def x #=foo)", Dialect::Clojure, "t.clj");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].risk, Risk::Inert);
    }

    #[test]
    fn clojure_files_without_read_eval_are_modelled_with_no_findings() {
        let report = report_with("(def x 1)", Dialect::Clojure, "t.clj");
        assert!(report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn a_dir_locals_eval_call_is_live() {
        let source = r#"((nil . ((eval . (delete-file "x")))))"#;
        let report = report_with(source, Dialect::EmacsLisp, ".dir-locals.el");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].risk, Risk::Live);
        assert_eq!(report.findings[0].head.as_deref(), Some("delete-file"));
    }

    #[test]
    fn a_dir_locals_eval_quoted_datum_is_inert() {
        let source = "((nil . ((eval . 'x))))";
        let report = report_with(source, Dialect::EmacsLisp, ".dir-locals.el");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].risk, Risk::Inert);
    }

    #[test]
    fn a_dir_locals_file_with_no_eval_key_is_modelled_with_no_findings() {
        let source = "((nil . ((indent-tabs-mode . nil))))";
        let report = report_with(source, Dialect::EmacsLisp, ".dir-locals.el");
        assert!(report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn an_emacs_lisp_file_that_is_not_dir_locals_is_unmodelled() {
        let report = report_with("(setq x 1)", Dialect::EmacsLisp, "init.el");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn scheme_reports_unmodelled_rather_than_reporting_nothing() {
        let report = report_with("(define (f x) (f x))", Dialect::Scheme, "t.scm");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn racket_reports_unmodelled_rather_than_reporting_nothing() {
        let report = report_with("(define (f x) (f x))", Dialect::Racket, "t.rkt");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn hy_reports_unmodelled_rather_than_reporting_nothing() {
        let report = report_with("(defn f [x] (f x))", Dialect::Hy, "t.hy");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }
}
