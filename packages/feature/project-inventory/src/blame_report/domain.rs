//! Who last touched each definition, and when.
//!
//! The routing layer for every other report. A finding that says "this
//! docstring is stale" is useful; one that also says who wrote it and when is
//! actionable, and the difference is whether anyone picks it up.
//!
//! Attribution is per definition rather than per line, because a definition is
//! the unit someone owns. `git blame` answers per line; folding those answers
//! up to the definition means choosing one, and the choice here is the *most
//! recent* line in the span — the person who touched it last is the one with it
//! still in mind.
//!
//! Degrades the same way `hotspots` does. When git cannot answer — not a
//! repository, no `git` on `PATH`, a file that is not tracked — the report says
//! so rather than emitting an empty author, which would read like a definition
//! nobody wrote.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use paredit_core_cli::report::{FileFindings, Finding, line_of};
use paredit_core_syntax::definition::definition_shape;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

/// One definition's attribution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribution {
    pub name: String,
    /// The author of the most recent line in the definition.
    pub author: Option<String>,
    /// That line's author date, as `git` formatted it.
    pub date: Option<String>,
    /// The abbreviated commit that touched it last.
    pub commit: Option<String>,
    pub span: ByteSpan,
    pub line: usize,
}

impl Finding for Attribution {
    fn kind(&self) -> &'static str {
        if self.author.is_some() {
            "attributed"
        } else {
            "unattributed"
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
            self.author.clone().unwrap_or_else(|| "-".to_owned()),
            self.date.clone().unwrap_or_else(|| "-".to_owned()),
            self.commit.clone().unwrap_or_else(|| "-".to_owned()),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", json!(self.name)),
            ("author", json!(self.author)),
            ("date", json!(self.date)),
            ("commit", json!(self.commit)),
        ]
    }
}

/// What `git blame` said about one line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineBlame {
    pub author: String,
    pub date: String,
    pub commit: String,
}

/// Per-line attribution, or the reason there is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blame {
    /// Keyed by 1-based line number.
    Lines(BTreeMap<usize, LineBlame>),
    Unavailable(String),
}

impl Blame {
    #[must_use]
    pub const fn reason(&self) -> Option<&String> {
        match self {
            Self::Lines(_) => None,
            Self::Unavailable(reason) => Some(reason),
        }
    }

    /// The most recent attribution within a line range.
    ///
    /// "Most recent" is by date string, which sorts correctly because the date
    /// is requested in ISO-8601 — the one format where lexical and chronological
    /// order agree.
    fn newest_in(&self, from: usize, to: usize) -> Option<&LineBlame> {
        match self {
            Self::Unavailable(_) => None,
            Self::Lines(lines) => lines
                .range(from..=to)
                .map(|(_, blame)| blame)
                .max_by(|left, right| left.date.cmp(&right.date)),
        }
    }
}

#[must_use]
pub fn build_blame_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
    blame: &Blame,
) -> FileFindings<Attribution> {
    let source = tree.source();

    let findings = tree
        .root_view()
        .children
        .iter()
        .filter_map(|form| {
            let head = list_head(form)?;
            let shape = definition_shape(dialect, form, head)?;
            let name = shape.name(form)?.to_owned();
            let start = line_of(source, form.span.start().get());
            let end = line_of(source, form.span.end().get().saturating_sub(1));
            let attribution = blame.newest_in(start, end);
            Some(Attribution {
                name,
                author: attribution.map(|found| found.author.clone()),
                date: attribution.map(|found| found.date.clone()),
                commit: attribution.map(|found| found.commit.clone()),
                span: form.span,
                line: start,
            })
        })
        .collect::<Vec<_>>();

    let unattributed = findings
        .iter()
        .filter(|attribution: &&Attribution| attribution.author.is_none())
        .count();

    let mut summary = vec![("unattributed_count", json!(unattributed))];
    if let Some(reason) = blame.reason() {
        summary.push(("blame_unavailable", json!(reason)));
    }

    FileFindings::new(path.to_path_buf(), dialect, true, findings, summary)
}

/// Runs `git blame` and reads its porcelain output.
///
/// The porcelain format is requested rather than the human one because the
/// human one is localized and reflows; porcelain is a stable key/value stream
/// that has not changed since it was introduced.
#[must_use]
pub fn measure_blame(path: &Path) -> Blame {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let Some(name) = path.file_name() else {
        return Blame::Unavailable("path has no file name".to_owned());
    };

    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(["blame", "--line-porcelain", "--"])
        .arg(name)
        .output();

    match output {
        Err(error) => Blame::Unavailable(format!("git could not be run: {error}")),
        Ok(output) if !output.status.success() => Blame::Unavailable(
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .next()
                .unwrap_or("git blame failed")
                .to_owned(),
        ),
        Ok(output) => Blame::Lines(parse_porcelain(&String::from_utf8_lossy(&output.stdout))),
    }
}

/// Reads `git blame --line-porcelain` into per-line attributions.
///
/// Each record opens with `<sha> <orig-line> <final-line> [<count>]`, carries
/// `author` and `author-time` headers, and closes with a tab-prefixed copy of
/// the source line.
pub(crate) fn parse_porcelain(output: &str) -> BTreeMap<usize, LineBlame> {
    let mut lines = BTreeMap::new();
    let mut commit = String::new();
    let mut final_line = 0usize;
    let mut author = String::new();
    let mut time = 0i64;

    for record in output.lines() {
        if let Some(rest) = record.strip_prefix("author ") {
            author = rest.to_owned();
        } else if let Some(rest) = record.strip_prefix("author-time ") {
            time = rest.trim().parse().unwrap_or(0);
        } else if record.starts_with('\t') {
            if final_line > 0 {
                lines.insert(
                    final_line,
                    LineBlame {
                        author: author.clone(),
                        date: iso_date(time),
                        commit: commit.chars().take(8).collect(),
                    },
                );
            }
        } else {
            let mut fields = record.split_whitespace();
            let Some(sha) = fields.next() else { continue };
            // A header line opens a record only if it is a full hex object
            // name; every other key/value header falls through to here too.
            if sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
                commit = sha.to_owned();
                final_line = fields.nth(1).and_then(|n| n.parse().ok()).unwrap_or(0);
            }
        }
    }

    lines
}

/// A Unix timestamp as an ISO-8601 date.
///
/// Written by hand rather than pulled from a date crate: the only property
/// needed is that lexical order matches chronological order, and adding a
/// dependency to 26 crates for `YYYY-MM-DD` is not worth it.
fn iso_date(timestamp: i64) -> String {
    let days = timestamp.div_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Howard Hinnant's `civil_from_days`, which is the standard branch-free
/// conversion and is exact for every date this will ever see.
const fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blame_of(entries: &[(usize, &str, &str)]) -> Blame {
        Blame::Lines(
            entries
                .iter()
                .map(|(line, author, date)| {
                    (
                        *line,
                        LineBlame {
                            author: (*author).to_owned(),
                            date: (*date).to_owned(),
                            commit: "abcdef12".to_owned(),
                        },
                    )
                })
                .collect(),
        )
    }

    fn report(source: &str, blame: &Blame) -> FileFindings<Attribution> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_blame_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree, blame)
    }

    #[test]
    fn a_definition_takes_the_author_of_its_most_recent_line() {
        let blame = blame_of(&[(1, "ada", "2020-01-01"), (2, "grace", "2024-06-01")]);
        let report = report("(defun f (x)\n  x)", &blame);
        assert_eq!(report.findings[0].author.as_deref(), Some("grace"));
        assert_eq!(report.findings[0].date.as_deref(), Some("2024-06-01"));
    }

    #[test]
    fn a_line_outside_the_definition_does_not_attribute_it() {
        let blame = blame_of(&[(1, "ada", "2020-01-01"), (9, "grace", "2099-01-01")]);
        let report = report("(defun f (x) x)", &blame);
        assert_eq!(report.findings[0].author.as_deref(), Some("ada"));
    }

    #[test]
    fn a_definition_with_no_blame_line_is_unattributed() {
        let report = report("(defun f (x) x)", &blame_of(&[(9, "ada", "2020-01-01")]));
        assert_eq!(report.findings[0].kind(), "unattributed");
        assert_eq!(report.summary[0], ("unattributed_count", json!(1)));
    }

    #[test]
    fn an_unavailable_blame_is_said_rather_than_implied() {
        let report = report(
            "(defun f (x) x)",
            &Blame::Unavailable("not a repository".to_owned()),
        );
        assert!(
            report
                .summary
                .iter()
                .any(|(name, _)| *name == "blame_unavailable")
        );
        assert!(report.findings[0].author.is_none());
    }

    #[test]
    fn the_commit_is_abbreviated_for_reading() {
        let report = report("(defun f (x) x)", &blame_of(&[(1, "ada", "2020-01-01")]));
        assert_eq!(report.findings[0].commit.as_deref(), Some("abcdef12"));
    }

    #[test]
    fn porcelain_output_is_read_into_per_line_attributions() {
        let output = "0123456789abcdef0123456789abcdef01234567 1 1 1\n\
             author Ada Lovelace\n\
             author-time 1577836800\n\
             author-tz +0000\n\
             \t(defun f (x) x)\n";
        let lines = parse_porcelain(output);
        let first = lines.get(&1).expect("line 1 is attributed");
        assert_eq!(first.author, "Ada Lovelace");
        assert_eq!(first.date, "2020-01-01");
        assert_eq!(first.commit, "01234567");
    }

    #[test]
    fn an_iso_date_sorts_lexically_the_way_it_sorts_chronologically() {
        assert!(iso_date(0) < iso_date(1_577_836_800));
        assert_eq!(iso_date(0), "1970-01-01");
    }

    #[test]
    fn a_leap_day_converts_exactly() {
        // 2020-02-29 is 18321 days after the epoch.
        assert_eq!(iso_date(18_321 * 86_400), "2020-02-29");
    }

    #[test]
    fn measuring_blame_outside_a_repository_reports_the_reason() {
        let blame = measure_blame(Path::new("/definitely-not-a-repo.lisp"));
        assert!(matches!(blame, Blame::Unavailable(_)), "{blame:?}");
    }

    #[test]
    fn findings_are_in_source_order() {
        let report = report(
            "(defun a () 1)\n(defun b () 2)\n(defun c () 3)",
            &blame_of(&[(1, "ada", "2020-01-01")]),
        );
        let starts = report
            .findings
            .iter()
            .map(|attribution| attribution.span.start().get())
            .collect::<Vec<_>>();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
    }
}
