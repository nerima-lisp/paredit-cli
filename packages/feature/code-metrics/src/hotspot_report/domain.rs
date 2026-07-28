//! Change frequency multiplied by complexity: where a refactor actually pays.
//!
//! Complexity alone ranks the code that is hard to understand, and most of it
//! is hard for a reason and nobody touches it. Change frequency alone ranks the
//! code that moves, and most of that is a configuration table. The *product* is
//! the code that is both — the code a team keeps having to understand and keeps
//! getting wrong — and that ranking is the one worth acting on.
//!
//! Complexity here is the same nesting-depth-and-size measure `inspect
//! complexity` uses, recomputed per definition rather than depended on across a
//! package boundary.
//!
//! Churn comes from `git log`, which makes this the only report in the tool
//! that reads something outside the files it was handed. When git is
//! unavailable — not a repository, no `git` on `PATH`, a shallow clone that
//! truncated the history — the report says so and falls back to complexity
//! alone. That distinction matters: a churn of zero means "never changed", and
//! "we could not ask" is not that.

use std::path::{Path, PathBuf};
use std::process::Command;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::definition::definition_shape;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

/// One definition, ranked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotspot {
    pub name: String,
    /// Commits touching the file, over the window asked for.
    pub churn: usize,
    /// Maximum nesting depth plus form count, the same shape `complexity` uses.
    pub complexity: usize,
    /// `churn * complexity`. The ranking key.
    pub score: usize,
    pub span: ByteSpan,
    pub line: usize,
}

impl Finding for Hotspot {
    fn kind(&self) -> &'static str {
        "hotspot"
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
            format!("score={}", self.score),
            format!("churn={}", self.churn),
            format!("complexity={}", self.complexity),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", json!(self.name)),
            ("churn", json!(self.churn)),
            ("complexity", json!(self.complexity)),
            ("score", json!(self.score)),
        ]
    }
}

/// What `git log` answered, or why it did not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Churn {
    /// The number of commits touching the file.
    Commits(usize),
    /// Git could not answer. The reason is carried so the output can say it
    /// rather than reporting a zero that reads like "never changed".
    Unavailable(String),
}

impl Churn {
    #[must_use]
    pub const fn commits(&self) -> usize {
        match self {
            Self::Commits(count) => *count,
            // One, not zero: an unknown churn must not silently zero out every
            // score and rank a whole tree as uniformly cold.
            Self::Unavailable(_) => 1,
        }
    }

    #[must_use]
    pub const fn reason(&self) -> Option<&String> {
        match self {
            Self::Commits(_) => None,
            Self::Unavailable(reason) => Some(reason),
        }
    }
}

#[must_use]
pub fn build_hotspot_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
    churn: &Churn,
) -> FileFindings<Hotspot> {
    let source = tree.source();
    let commits = churn.commits();

    let findings = tree
        .root_view()
        .children
        .iter()
        .filter_map(|form| {
            let head = list_head(form)?;
            let shape = definition_shape(dialect, form, head)?;
            let name = shape.name(form)?.to_owned();
            let complexity = complexity_of(form);
            Some(Hotspot {
                name,
                churn: commits,
                complexity,
                score: commits.saturating_mul(complexity),
                span: form.span,
                line: line_of(source, form.span.start().get()),
            })
        })
        .collect::<Vec<_>>();

    let hottest = findings
        .iter()
        .map(|hotspot| hotspot.score)
        .max()
        .unwrap_or(0);

    let mut summary = vec![
        ("file_churn", json!(commits)),
        ("hottest_score", json!(hottest)),
    ];
    if let Some(reason) = churn.reason() {
        summary.push(("churn_unavailable", json!(reason)));
    }

    FileFindings::new(path.to_path_buf(), dialect, true, findings, summary)
}

/// Nesting depth plus form count.
///
/// Deliberately the same coarse shape `inspect complexity` reports, so the two
/// rank definitions the same way and a reader comparing them is not comparing
/// two different definitions of the word.
fn complexity_of(view: &ExpressionView) -> usize {
    fn walk(view: &ExpressionView, depth: usize) -> (usize, usize) {
        let mut max_depth = depth;
        let mut forms = 1;
        for child in &view.children {
            let (child_depth, child_forms) = walk(child, depth + 1);
            max_depth = max_depth.max(child_depth);
            forms += child_forms;
        }
        (max_depth, forms)
    }
    let (depth, forms) = walk(view, 0);
    depth + forms
}

/// How many commits in the last `window` touched `path`.
///
/// Runs `git log` in the file's own directory rather than the process's, so a
/// report over several repositories answers for each of them.
#[must_use]
pub fn measure_churn(path: &Path, window: &str) -> Churn {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let Some(name) = path.file_name() else {
        return Churn::Unavailable("path has no file name".to_owned());
    };

    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(["log", "--format=%H", "--since"])
        .arg(window)
        .arg("--")
        .arg(name)
        .output();

    match output {
        Err(error) => Churn::Unavailable(format!("git could not be run: {error}")),
        Ok(output) if !output.status.success() => Churn::Unavailable(
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .next()
                .unwrap_or("git log failed")
                .to_owned(),
        ),
        Ok(output) => Churn::Commits(
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count(),
        ),
    }
}

/// The path a churn measurement should be taken against.
///
/// Split out so the workflow can canonicalize once and the domain stays
/// testable without a repository.
#[must_use]
pub fn churn_target(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
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

    fn report(source: &str, churn: &Churn) -> FileFindings<Hotspot> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_hotspot_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree, churn)
    }

    #[test]
    fn the_score_is_churn_times_complexity() {
        let report = report("(defun f (x) (list x))", &Churn::Commits(3));
        let hotspot = &report.findings[0];
        assert_eq!(hotspot.churn, 3);
        assert_eq!(hotspot.score, 3 * hotspot.complexity);
    }

    #[test]
    fn a_deeper_definition_scores_higher_at_the_same_churn() {
        let flat = report("(defun f (x) x)", &Churn::Commits(2));
        let nested = report("(defun f (x) (a (b (c (d x)))))", &Churn::Commits(2));
        assert!(nested.findings[0].score > flat.findings[0].score);
    }

    #[test]
    fn a_file_nobody_changed_scores_zero() {
        let report = report("(defun f (x) (list x))", &Churn::Commits(0));
        assert_eq!(report.findings[0].score, 0);
    }

    #[test]
    fn an_unavailable_churn_falls_back_to_complexity_rather_than_zero() {
        let report = report(
            "(defun f (x) (list x))",
            &Churn::Unavailable("not a repository".to_owned()),
        );
        let hotspot = &report.findings[0];
        assert_eq!(hotspot.churn, 1);
        assert_eq!(hotspot.score, hotspot.complexity);
    }

    #[test]
    fn an_unavailable_churn_is_said_rather_than_implied() {
        let report = report(
            "(defun f (x) x)",
            &Churn::Unavailable("not a repository".to_owned()),
        );
        assert!(
            report
                .summary
                .iter()
                .any(|(name, _)| *name == "churn_unavailable"),
            "{report:?}"
        );
    }

    #[test]
    fn an_available_churn_carries_no_unavailability_notice() {
        let report = report("(defun f (x) x)", &Churn::Commits(1));
        assert!(
            report
                .summary
                .iter()
                .all(|(name, _)| *name != "churn_unavailable")
        );
    }

    #[test]
    fn a_file_with_no_definition_reports_a_zero_hottest_score() {
        let report = report("(print 1)", &Churn::Commits(9));
        assert_eq!(report.summary[1], ("hottest_score", json!(0)));
    }

    #[test]
    fn measuring_churn_outside_a_repository_reports_the_reason() {
        // `/` is not a git repository on any platform this runs on.
        let churn = measure_churn(Path::new("/definitely-not-a-repo.lisp"), "1 year ago");
        assert!(matches!(churn, Churn::Unavailable(_)), "{churn:?}");
    }

    #[test]
    fn findings_are_in_source_order() {
        let report = report(
            "(defun a () 1)\n(defun b () 2)\n(defun c () 3)",
            &Churn::Commits(1),
        );
        let starts = report
            .findings
            .iter()
            .map(|hotspot| hotspot.span.start().get())
            .collect::<Vec<_>>();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
    }
}
