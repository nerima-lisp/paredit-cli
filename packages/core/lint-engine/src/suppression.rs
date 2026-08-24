//! Inline lint-suppression directives: source comments that silence
//! [`crate::domain::lint_report`] findings, the way `eslint-disable-next-line`
//! or `# noqa` do elsewhere.
//!
//! A directive is any Lisp comment containing one of three tokens:
//!
//! ```lisp
//! ;; paredit:ignore single-arg-comparison
//! (< x)                                    ; suppressed (directive on the line above)
//! (= y)  ; paredit:ignore                  ; suppressed (trailing directive, all rules)
//!
//! ;; paredit:ignore-next-form redundant-quote
//! (defun f ()                              ; suppressed for the whole form,
//!   (list '5))                             ; however many lines it spans
//!
//! ;; paredit:ignore-file redundant-quote   ; suppressed everywhere in the file
//! ```
//!
//! With no rule names a directive suppresses every rule in its scope; with
//! names (comma- or space-separated) only those. A `paredit:ignore` on its own
//! line protects the *next* source line; a trailing one (code before the `;`)
//! protects its *own* line.
//!
//! ## Why three scopes
//!
//! Line scope alone is what a *finding* has — a span, whose start line is the
//! only thing a suppression can match. It is also the wrong granularity for
//! most of what people want to silence: a rule that reports a whole `defun`
//! reports it at the `defun`'s first line, but a reader who wants to silence
//! "this function" has to know that, and a rule that reports somewhere in the
//! middle of the function needs a directive buried in the middle of the
//! function. `ignore-next-form` says the thing the reader means, and resolving
//! it needs the parse tree — which is why [`LintSuppressions::parse_in_tree`]
//! exists alongside the text-only [`LintSuppressions::parse`].
//!
//! ## Reasons
//!
//! Anything after `--` is free-text reason. `--require-suppression-reason`
//! turns a missing one into a reported problem, so a project can insist that
//! every silenced finding says why it was silenced.
//!
//! ## Expiry
//!
//! Any of the three tokens above may carry `-until YYYY-MM-DD` immediately
//! after the token (`paredit:ignore-until 2026-12-31`,
//! `paredit:ignore-next-form-until 2026-12-31`,
//! `paredit:ignore-file-until 2026-12-31`), each still followed by the same
//! optional rule names and `-- reason`. A directive past its date still
//! suppresses — expiry is a prompt to renew or delete it, not a silent
//! deadline — but [`LintSuppressions::expired_directives`] finds it. A
//! malformed or missing date makes the whole comment not a directive at all
//! (it suppresses nothing), rather than one that silently never expires:
//! a typo should show up as a finding reappearing, not as a suppression that
//! outlives it forever.
//!
//! Detection scans the whole file tracking string and `#\` character-literal
//! state, so a `;` inside `"a ; b"` or `#\;` is never mistaken for a comment.

use std::collections::{HashMap, HashSet};

use paredit_core_syntax::sexpr::SyntaxTree;

/// Which rules a directive silences.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Scope {
    /// Every rule (a bare directive).
    All,
    /// Only the named rules.
    Rules(Vec<String>),
}

impl Scope {
    fn covers(&self, rule: &str) -> bool {
        match self {
            Self::All => true,
            Self::Rules(rules) => rules.iter().any(|name| name == rule),
        }
    }
}

/// How far a directive reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuppressionScope {
    /// One line: the directive's own if code precedes it, else the next.
    Line,
    /// Every line of the next form, resolved against the parse tree.
    NextForm,
    /// Every line of the file.
    File,
}

impl SuppressionScope {
    /// The token that introduces each scope, longest first so `ignore-file` is
    /// not read as a bare `ignore` followed by junk.
    const TOKENS: [(&'static str, Self); 3] = [
        ("paredit:ignore-next-form", Self::NextForm),
        ("paredit:ignore-file", Self::File),
        ("paredit:ignore", Self::Line),
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::NextForm => "next-form",
            Self::File => "file",
        }
    }
}

/// Why a directive is worth acting on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuppressionProblem {
    /// It silenced no finding — a stale ignore or a typo'd rule name.
    Unused,
    /// It silenced something, but gave no `-- reason` and the run required
    /// one. Reported separately because the remedy is the opposite: add text
    /// rather than delete the line.
    MissingReason,
    /// Its `-until` date has passed. Reported independent of use — an expired
    /// directive that is still silencing something is exactly the one that
    /// needs a human to decide whether to renew or delete it.
    Expired,
}

impl SuppressionProblem {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unused => "unused",
            Self::MissingReason => "missing-reason",
            Self::Expired => "expired",
        }
    }
}

/// A calendar date, used only to compare a suppression's `-until` against
/// today.
///
/// Not a general-purpose calendar type: no timezone, no calendar arithmetic
/// beyond construction and ordering, because expiry needs nothing else. UTC
/// throughout — a suppression that expires a day early or late at a timezone
/// boundary is a far smaller problem than one whose expiry depends on where
/// the check happens to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
    year: i64,
    month: i64,
    day: i64,
}

impl Date {
    /// Parses a strict `YYYY-MM-DD`. Rejects a month or day outside its
    /// broad numeric range; it does not check the day against the month's
    /// actual length, which only matters for a date nobody would type.
    fn parse(text: &str) -> Option<Self> {
        let mut parts = text.splitn(3, '-');
        let year: i64 = parts.next()?.parse().ok()?;
        let month: i64 = parts.next()?.parse().ok()?;
        let day: i64 = parts.next()?.parse().ok()?;
        if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
            return None;
        }
        Some(Self { year, month, day })
    }

    /// Today, from the wall clock.
    #[must_use]
    pub fn today() -> Self {
        let days = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs() / 86_400);
        Self::from_days_since_epoch(i64::try_from(days).unwrap_or(0))
    }

    /// Howard Hinnant's `civil_from_days`: days-since-1970-01-01 to a
    /// proleptic-Gregorian (year, month, day). <https://howardhinnant.github.io/date_algorithms.html>
    const fn from_days_since_epoch(days: i64) -> Self {
        let z = days + 719_468;
        let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
        let doe = z - era * 146_097; // [0, 146096]
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
        let mp = (5 * doy + 2) / 153; // [0, 11]
        let day = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
        let month = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
        Self {
            year: if month <= 2 { y + 1 } else { y },
            month,
            day,
        }
    }
}

impl std::fmt::Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// One parsed directive, retained for unused-suppression reporting and for
/// in-place removal.
#[derive(Debug, Clone)]
struct Directive {
    /// The line the directive comment sits on.
    comment_line: usize,
    /// The first line it protects. For [`SuppressionScope::File`] this is 1.
    target_line: usize,
    /// The last line it protects, inclusive.
    target_end_line: usize,
    scope: SuppressionScope,
    rules: Scope,
    reason: Option<String>,
    /// The `-until` date, if the directive carried one.
    expires: Option<Date>,
    /// Whether non-whitespace code preceded the `;` on the comment's line.
    trailing: bool,
    /// Byte range of the whole comment, from its `;` to the end of the line
    /// (newline excluded).
    comment_span: (usize, usize),
    /// Byte range of the whole line including its newline, so a directive on
    /// its own line can be deleted outright.
    line_span: (usize, usize),
    /// Byte range of the rule-name list inside the file, so a directive that is
    /// only partly stale can be rewritten without disturbing the rest of the
    /// comment.
    rules_span: (usize, usize),
}

/// A directive (or some of its named rules) that a run flagged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnusedSuppression {
    pub comment_line: usize,
    pub target_line: usize,
    /// `None` for a bare directive that matched nothing; `Some(rules)` for the
    /// named rules that matched nothing.
    pub unused_rules: Option<Vec<String>>,
    pub scope: SuppressionScope,
    pub problem: SuppressionProblem,
}

/// One directive as recorded for `--report-suppressions`, independent of
/// whether it currently silences anything — the full inventory, one step past
/// [`UnusedSuppression`], which reports only the stale ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuppressionInventoryEntry {
    pub comment_line: usize,
    pub target_line: usize,
    pub target_end_line: usize,
    pub scope: SuppressionScope,
    /// `None` for a bare directive (every rule); `Some` for a named one.
    pub rules: Option<Vec<String>>,
    pub reason: Option<String>,
    /// `YYYY-MM-DD`, for a directive carrying an `-until` expiry.
    pub expires: Option<String>,
    /// Whether the directive currently silences at least one finding.
    pub used: bool,
}

/// One in-place edit that removes or narrows a stale directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuppressionEdit {
    /// The byte region to replace.
    pub start: usize,
    pub end: usize,
    /// The replacement, empty for an outright deletion.
    pub text: String,
    /// The line the directive was on, for the report.
    pub comment_line: usize,
    /// The rule names dropped, or `None` for a bare directive.
    pub removed_rules: Option<Vec<String>>,
}

/// The set of lint suppressions parsed from one source file.
#[derive(Debug, Default)]
pub struct LintSuppressions {
    directives: Vec<Directive>,
}

impl LintSuppressions {
    /// Parses every directive in `text`, resolving `ignore-next-form` against
    /// the parse tree.
    ///
    /// This is the reading every scan uses. The tree is needed for exactly one
    /// thing — how far the next form reaches — and every caller has already
    /// parsed the file.
    #[must_use]
    pub fn parse_in_tree(text: &str, tree: &SyntaxTree) -> Self {
        let mut parsed = Self::parse(text);
        parsed.resolve_form_scopes(text, tree);
        parsed
    }

    /// Parses every directive in `text` without a tree.
    ///
    /// `ignore-next-form` degrades to covering only the line after the comment,
    /// which is what a line-scoped directive would have covered. Used by the
    /// unit tests below and by any caller that has text but no parse.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut directives = Vec::new();
        let mut in_string = false;
        let mut line_start = 0;
        let total_lines = text.lines().count();
        for (index, raw) in text.split_inclusive('\n').enumerate() {
            let line = raw.strip_suffix('\n').unwrap_or(raw);
            let line_number = index + 1;
            let line_span = (line_start, line_start + raw.len());
            line_start += raw.len();

            let scan = scan_line(line, in_string);
            in_string = scan.in_string;
            let had_code = scan.had_code;
            let Some(start) = scan.comment_start else {
                continue;
            };
            let comment = &line[start..];
            let Some(parsed) = parse_directive(comment) else {
                continue;
            };
            let comment_offset = line_span.0 + start;
            let (target_line, target_end_line) = match parsed.scope {
                SuppressionScope::File => (1, total_lines.max(1)),
                SuppressionScope::Line | SuppressionScope::NextForm => {
                    // A trailing directive protects its own line; one alone on
                    // its line protects the next.
                    let target = if had_code {
                        line_number
                    } else {
                        line_number + 1
                    };
                    (target, target)
                }
            };
            directives.push(Directive {
                comment_line: line_number,
                target_line,
                target_end_line,
                scope: parsed.scope,
                rules: parsed.rules,
                reason: parsed.reason,
                expires: parsed.expires,
                trailing: had_code,
                comment_span: (comment_offset, line_span.0 + line.len()),
                line_span,
                rules_span: (
                    comment_offset + parsed.rules_span.0,
                    comment_offset + parsed.rules_span.1,
                ),
            });
        }
        Self { directives }
    }

    /// Widens each `ignore-next-form` directive to the full line span of the
    /// first top-level form that starts after it.
    ///
    /// Top-level only, deliberately. A directive nested inside a definition
    /// would otherwise have to name which of several sibling forms it meant,
    /// and the whole point of the scope is not having to say.
    fn resolve_form_scopes(&mut self, text: &str, tree: &SyntaxTree) {
        if !self
            .directives
            .iter()
            .any(|directive| directive.scope == SuppressionScope::NextForm)
        {
            return;
        }
        let starts = line_starts(text);
        let forms: Vec<(usize, usize)> = tree
            .root_view()
            .children
            .iter()
            .map(|child| {
                (
                    line_of(&starts, child.span.start().get()),
                    line_of(&starts, child.span.end().get().saturating_sub(1)),
                )
            })
            .collect();
        for directive in &mut self.directives {
            if directive.scope != SuppressionScope::NextForm {
                continue;
            }
            let anchor = directive.comment_line;
            if let Some((start, end)) = forms.iter().find(|(start, _)| *start > anchor) {
                directive.target_line = *start;
                directive.target_end_line = *end;
            }
        }
    }

    /// Whether `rule` is suppressed on 1-based `line`.
    #[must_use]
    pub fn is_suppressed(&self, rule: &str, line: usize) -> bool {
        self.directives.iter().any(|directive| {
            (directive.target_line..=directive.target_end_line).contains(&line)
                && directive.rules.covers(rule)
        })
    }

    /// Whether any directive was parsed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.directives.is_empty()
    }

    /// How many directives were parsed, for reporting.
    #[must_use]
    pub fn len(&self) -> usize {
        self.directives.len()
    }

    /// The directives (or named rules within them) worth acting on, given
    /// `present`: for each line, the rule names that actually reported a
    /// finding there.
    ///
    /// A bare directive is unused when nothing in its range reported; a named
    /// directive is unused for each listed rule absent from its range. With
    /// `require_reason`, a directive that *is* used but carries no `-- reason`
    /// is additionally reported — a separate problem with a separate remedy.
    #[must_use]
    pub fn unused_directives(
        &self,
        present: &HashMap<usize, HashSet<&'static str>>,
        require_reason: bool,
    ) -> Vec<UnusedSuppression> {
        let mut unused = Vec::new();
        for directive in &self.directives {
            let in_range = findings_in_range(directive, present);
            let stale = match &directive.rules {
                Scope::All => in_range.is_empty().then(Vec::new),
                Scope::Rules(rules) => {
                    let stale: Vec<String> = rules
                        .iter()
                        .filter(|rule| !in_range.contains(rule.as_str()))
                        .cloned()
                        .collect();
                    (!stale.is_empty()).then_some(stale)
                }
            };
            match stale {
                Some(rules) => unused.push(UnusedSuppression {
                    comment_line: directive.comment_line,
                    target_line: directive.target_line,
                    unused_rules: match &directive.rules {
                        Scope::All => None,
                        Scope::Rules(_) => Some(rules),
                    },
                    scope: directive.scope,
                    problem: SuppressionProblem::Unused,
                }),
                // Only a directive that is pulling its weight is asked to
                // justify itself; a stale one is reported as stale and removed,
                // not annotated.
                None if require_reason && directive.reason.is_none() => {
                    unused.push(UnusedSuppression {
                        comment_line: directive.comment_line,
                        target_line: directive.target_line,
                        unused_rules: None,
                        scope: directive.scope,
                        problem: SuppressionProblem::MissingReason,
                    });
                }
                None => {}
            }
        }
        unused
    }

    /// The directives whose `-until` date has passed as of `today`, whether or
    /// not they currently silence anything.
    ///
    /// Deliberately independent of `unused_directives`: an expired directive
    /// that is still actively silencing a finding is exactly the one worth
    /// flagging — its author is the only one who can say whether the finding
    /// underneath it is now fine to see again, or whether the date just needs
    /// pushing out.
    #[must_use]
    pub fn expired_directives(&self, today: Date) -> Vec<UnusedSuppression> {
        self.directives
            .iter()
            .filter_map(|directive| {
                let expires = directive.expires?;
                (expires < today).then(|| UnusedSuppression {
                    comment_line: directive.comment_line,
                    target_line: directive.target_line,
                    unused_rules: match &directive.rules {
                        Scope::All => None,
                        Scope::Rules(rules) => Some(rules.clone()),
                    },
                    scope: directive.scope,
                    problem: SuppressionProblem::Expired,
                })
            })
            .collect()
    }

    /// Every parsed directive, whether or not it currently silences anything —
    /// the full surface `--report-suppressions` audits, one step past
    /// [`Self::unused_directives`], which reports only the stale ones.
    #[must_use]
    pub fn inventory(
        &self,
        present: &HashMap<usize, HashSet<&'static str>>,
    ) -> Vec<SuppressionInventoryEntry> {
        self.directives
            .iter()
            .map(|directive| {
                let in_range = findings_in_range(directive, present);
                let (rules, used) = match &directive.rules {
                    Scope::All => (None, !in_range.is_empty()),
                    Scope::Rules(rules) => (
                        Some(rules.clone()),
                        rules.iter().any(|rule| in_range.contains(rule.as_str())),
                    ),
                };
                SuppressionInventoryEntry {
                    comment_line: directive.comment_line,
                    target_line: directive.target_line,
                    target_end_line: directive.target_end_line,
                    scope: directive.scope,
                    rules,
                    reason: directive.reason.clone(),
                    expires: directive.expires.map(|date| date.to_string()),
                    used,
                }
            })
            .collect()
    }

    /// The edits that delete every stale directive and narrow every partly
    /// stale one, sorted by position so a caller can apply them in one pass.
    ///
    /// A directive alone on its line is deleted line and all; a trailing one
    /// loses only its comment. A directive of which *some* rules are stale
    /// keeps the rest, rewritten in place, so removing a typo cannot silently
    /// un-suppress a finding that was genuinely being ignored.
    ///
    /// Only [`SuppressionProblem::Unused`] entries produce an edit: a missing
    /// reason is fixed by writing one, which is not a machine's call.
    #[must_use]
    pub fn removal_edits(
        &self,
        present: &HashMap<usize, HashSet<&'static str>>,
    ) -> Vec<SuppressionEdit> {
        let mut edits = Vec::new();
        for directive in &self.directives {
            let in_range = findings_in_range(directive, present);
            let (removed, kept) = match &directive.rules {
                Scope::All => {
                    if !in_range.is_empty() {
                        continue;
                    }
                    (None, Vec::new())
                }
                Scope::Rules(rules) => {
                    let (kept, removed): (Vec<String>, Vec<String>) = rules
                        .iter()
                        .cloned()
                        .partition(|rule| in_range.contains(rule.as_str()));
                    if removed.is_empty() {
                        continue;
                    }
                    (Some(removed), kept)
                }
            };

            if kept.is_empty() {
                edits.push(delete_directive(directive, removed));
            } else {
                edits.push(SuppressionEdit {
                    start: directive.rules_span.0,
                    end: directive.rules_span.1,
                    text: kept.join(" "),
                    comment_line: directive.comment_line,
                    removed_rules: removed,
                });
            }
        }
        edits.sort_by_key(|edit| (edit.start, edit.end));
        edits
    }
}

/// The rule names that reported a finding anywhere in a directive's range.
fn findings_in_range(
    directive: &Directive,
    present: &HashMap<usize, HashSet<&'static str>>,
) -> HashSet<&'static str> {
    let mut found = HashSet::new();
    for line in directive.target_line..=directive.target_end_line {
        if let Some(rules) = present.get(&line) {
            found.extend(rules.iter().copied());
        }
    }
    found
}

/// The whole-directive deletion: the line for a standalone directive, just the
/// comment for a trailing one.
const fn delete_directive(directive: &Directive, removed: Option<Vec<String>>) -> SuppressionEdit {
    let (start, end) = if directive.trailing {
        (directive.comment_span.0, directive.comment_span.1)
    } else {
        (directive.line_span.0, directive.line_span.1)
    };
    SuppressionEdit {
        start,
        end,
        text: String::new(),
        comment_line: directive.comment_line,
        removed_rules: removed,
    }
}

/// The byte offset each line starts at, for mapping a span back to a line.
fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
}

/// The 1-based line containing `offset`.
fn line_of(starts: &[usize], offset: usize) -> usize {
    match starts.binary_search(&offset) {
        Ok(index) => index + 1,
        Err(index) => index,
    }
}

/// One directive as read out of a comment, with offsets relative to the `;`.
struct ParsedDirective {
    scope: SuppressionScope,
    rules: Scope,
    reason: Option<String>,
    expires: Option<Date>,
    rules_span: (usize, usize),
}

/// Parses a comment (`;`-prefixed) into a directive, or `None` if it carries
/// none.
fn parse_directive(comment: &str) -> Option<ParsedDirective> {
    let (scope, position, token) = SuppressionScope::TOKENS
        .iter()
        .find_map(|(token, scope)| comment.find(token).map(|at| (*scope, at, *token)))?;
    let mut names_start = position + token.len();

    // `-until` sits glued to the token (`ignore-until`, `ignore-file-until`,
    // `ignore-next-form-until`), so it is read here rather than as a fourth
    // token: the alternative is three more entries in `TOKENS`, one per
    // existing scope, for a modifier that applies to all of them alike.
    let expires = match comment[names_start..].strip_prefix("-until") {
        None => None,
        Some(after_until) => {
            let leading_space = after_until.len() - after_until.trim_start().len();
            let trimmed = after_until.trim_start();
            let date_text = trimmed
                .split(|c: char| c.is_whitespace())
                .next()
                .unwrap_or("");
            // A `-until` with no parseable date makes the whole comment not a
            // directive, rather than one that silently never expires.
            let date = Date::parse(date_text)?;
            names_start += "-until".len() + leading_space + date_text.len();
            Some(date)
        }
    };
    let rest = &comment[names_start..];

    // Everything after `--` is prose, not a rule name.
    let (names, reason) = match rest.find("--") {
        Some(marker) => {
            let reason = rest[marker + 2..].trim();
            (
                &rest[..marker],
                (!reason.is_empty()).then(|| reason.to_owned()),
            )
        }
        None => (rest, None),
    };

    let rules: Vec<String> = names
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .collect();

    // The span of just the rule names, so a partial removal can rewrite them
    // without touching the token before or the reason after.
    let leading = names.len() - names.trim_start().len();
    let trailing = names.len() - names.trim_end().len();
    let rules_span = (names_start + leading, names_start + names.len() - trailing);

    Some(ParsedDirective {
        scope,
        rules: if rules.is_empty() {
            Scope::All
        } else {
            Scope::Rules(rules)
        },
        expires,
        reason,
        rules_span,
    })
}

/// The byte offset where a line's comment begins (if any), whether
/// non-whitespace code preceded that comment, and the in-string state after
/// the line.
struct LineScan {
    comment_start: Option<usize>,
    had_code: bool,
    in_string: bool,
}

/// Scans one line (given the incoming in-string state) and returns the byte
/// offset where its comment begins (if any), whether non-whitespace code
/// preceded that comment, and the in-string state after the line.
fn scan_line(line: &str, mut in_string: bool) -> LineScan {
    let mut had_code = false;
    let mut chars = line.char_indices().peekable();
    while let Some((offset, ch)) = chars.next() {
        if in_string {
            match ch {
                '\\' => {
                    chars.next(); // skip the escaped character
                }
                '"' => {
                    in_string = false;
                }
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => {
                in_string = true;
                had_code = true;
            }
            // A `#\c` character literal: the char after `#\` is data, not code
            // structure, so skip all three and never read its `;` as a comment.
            '#' if chars.peek().is_some_and(|(_, c)| *c == '\\') => {
                had_code = true;
                chars.next();
                chars.next();
            }
            ';' => {
                return LineScan {
                    comment_start: Some(offset),
                    had_code,
                    in_string,
                };
            }
            _ => {
                if !ch.is_whitespace() {
                    had_code = true;
                }
            }
        }
    }
    LineScan {
        comment_start: None,
        had_code,
        in_string,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_line_directive_protects_the_next_line() {
        let sup = LintSuppressions::parse(";; paredit:ignore single-arg-comparison\n(< x)\n");
        assert!(sup.is_suppressed("single-arg-comparison", 2));
        assert!(!sup.is_suppressed("single-arg-comparison", 1));
    }

    #[test]
    fn trailing_directive_protects_its_own_line() {
        let sup = LintSuppressions::parse("(< x)  ; paredit:ignore single-arg-comparison\n");
        assert!(sup.is_suppressed("single-arg-comparison", 1));
    }

    #[test]
    fn bare_directive_suppresses_every_rule() {
        let sup = LintSuppressions::parse(";; paredit:ignore\n(< x)\n");
        assert!(sup.is_suppressed("single-arg-comparison", 2));
        assert!(sup.is_suppressed("redundant-quote", 2));
    }

    #[test]
    fn named_directive_only_suppresses_that_rule() {
        let sup = LintSuppressions::parse(";; paredit:ignore redundant-quote\n(< x)\n");
        assert!(sup.is_suppressed("redundant-quote", 2));
        assert!(!sup.is_suppressed("single-arg-comparison", 2));
    }

    #[test]
    fn multiple_rules_comma_or_space_separated() {
        let sup = LintSuppressions::parse(
            ";; paredit:ignore redundant-quote, single-arg-comparison\n(x)\n",
        );
        assert!(sup.is_suppressed("redundant-quote", 2));
        assert!(sup.is_suppressed("single-arg-comparison", 2));
    }

    #[test]
    fn a_semicolon_inside_a_string_is_not_a_directive() {
        let sup = LintSuppressions::parse("(format t \"; paredit:ignore\")\n(< x)\n");
        assert!(sup.is_empty());
        assert!(!sup.is_suppressed("single-arg-comparison", 2));
    }

    #[test]
    fn a_semicolon_char_literal_is_not_a_directive() {
        let sup = LintSuppressions::parse("(princ #\\; ) ; paredit:ignore redundant-quote\n");
        // The `#\;` is data; the real comment after it is the directive, which
        // (being trailing) protects its own line.
        assert!(sup.is_suppressed("redundant-quote", 1));
    }

    #[test]
    fn no_directive_means_no_suppression() {
        let sup = LintSuppressions::parse("(< x) ; just a normal comment\n");
        assert!(sup.is_empty());
    }

    #[test]
    fn multiline_string_does_not_leak_a_comment() {
        // The `;` sits inside a string that opened on the previous line.
        let sup = LintSuppressions::parse(
            "(defparameter *s* \"line one\n ; paredit:ignore\n end\")\n(< x)\n",
        );
        assert!(sup.is_empty());
    }

    #[test]
    fn a_file_directive_reaches_every_line() {
        let sup =
            LintSuppressions::parse(";; paredit:ignore-file redundant-quote\n(a)\n(b)\n(c)\n");
        for line in 1..=4 {
            assert!(sup.is_suppressed("redundant-quote", line), "line {line}");
        }
        assert!(!sup.is_suppressed("single-arg-comparison", 3));
    }

    #[test]
    fn a_next_form_directive_covers_the_whole_form() {
        let source = ";; paredit:ignore-next-form redundant-quote\n\
                      (defun f ()\n  (list '5)\n  (list '6))\n\
                      (defun g () (list '7))\n";
        let tree = SyntaxTree::parse(source).expect("parse");
        let sup = LintSuppressions::parse_in_tree(source, &tree);
        // Lines 2-4 are the first defun.
        for line in 2..=4 {
            assert!(sup.is_suppressed("redundant-quote", line), "line {line}");
        }
        // Line 5 is the next definition and is not covered.
        assert!(!sup.is_suppressed("redundant-quote", 5));
    }

    #[test]
    fn a_next_form_directive_without_a_tree_falls_back_to_one_line() {
        let source = ";; paredit:ignore-next-form redundant-quote\n(defun f ()\n  (list '5))\n";
        let sup = LintSuppressions::parse(source);
        assert!(sup.is_suppressed("redundant-quote", 2));
        assert!(!sup.is_suppressed("redundant-quote", 3));
    }

    #[test]
    fn a_next_form_directive_with_no_following_form_suppresses_nothing_new() {
        let source = "(a)\n;; paredit:ignore-next-form redundant-quote\n";
        let tree = SyntaxTree::parse(source).expect("parse");
        let sup = LintSuppressions::parse_in_tree(source, &tree);
        assert!(!sup.is_suppressed("redundant-quote", 1));
    }

    #[test]
    fn the_longest_token_wins_so_ignore_file_is_not_read_as_ignore() {
        let sup = LintSuppressions::parse(";; paredit:ignore-file\n(a)\n(b)\n");
        assert!(sup.is_suppressed("any-rule", 3));
    }

    fn present(entries: &[(usize, &'static str)]) -> HashMap<usize, HashSet<&'static str>> {
        let mut map: HashMap<usize, HashSet<&'static str>> = HashMap::new();
        for (line, rule) in entries {
            map.entry(*line).or_default().insert(rule);
        }
        map
    }

    #[test]
    fn a_named_directive_matching_a_finding_is_used() {
        let sup = LintSuppressions::parse(";; paredit:ignore single-arg-comparison\n(< x)\n");
        let unused = sup.unused_directives(&present(&[(2, "single-arg-comparison")]), false);
        assert!(unused.is_empty());
    }

    #[test]
    fn a_named_directive_matching_nothing_is_unused() {
        let sup = LintSuppressions::parse(";; paredit:ignore single-arg-comparison\n(foo)\n");
        let unused = sup.unused_directives(&present(&[]), false);
        assert_eq!(unused.len(), 1);
        assert_eq!(
            unused[0].unused_rules.as_deref(),
            Some(&["single-arg-comparison".to_owned()][..])
        );
        assert_eq!(unused[0].comment_line, 1);
        assert_eq!(unused[0].target_line, 2);
        assert_eq!(unused[0].problem, SuppressionProblem::Unused);
    }

    #[test]
    fn a_bare_directive_over_a_finding_free_line_is_unused() {
        let sup = LintSuppressions::parse(";; paredit:ignore\n(clean)\n");
        let unused = sup.unused_directives(&present(&[]), false);
        assert_eq!(unused.len(), 1);
        assert_eq!(unused[0].unused_rules, None);
    }

    #[test]
    fn a_bare_directive_over_any_finding_is_used() {
        let sup = LintSuppressions::parse(";; paredit:ignore\n(< x)\n");
        let unused = sup.unused_directives(&present(&[(2, "single-arg-comparison")]), false);
        assert!(unused.is_empty());
    }

    #[test]
    fn only_the_stale_rule_of_a_multi_rule_directive_is_reported() {
        let sup = LintSuppressions::parse(
            ";; paredit:ignore redundant-quote single-arg-comparison\n(< x)\n",
        );
        // single-arg-comparison fired here; redundant-quote did not.
        let unused = sup.unused_directives(&present(&[(2, "single-arg-comparison")]), false);
        assert_eq!(unused.len(), 1);
        assert_eq!(
            unused[0].unused_rules.as_deref(),
            Some(&["redundant-quote".to_owned()][..])
        );
    }

    #[test]
    fn two_directives_on_one_target_line_union() {
        let sup = LintSuppressions::parse(
            ";; paredit:ignore redundant-quote\n(< x)  ; paredit:ignore single-arg-comparison\n",
        );
        // Line 2 gets `redundant-quote` (from the line above) and
        // `single-arg-comparison` (trailing on itself).
        assert!(sup.is_suppressed("redundant-quote", 2));
        assert!(sup.is_suppressed("single-arg-comparison", 2));
    }

    #[test]
    fn a_finding_anywhere_in_a_form_range_uses_the_directive() {
        let source = ";; paredit:ignore-next-form redundant-quote\n\
                      (defun f ()\n  (list '5))\n";
        let tree = SyntaxTree::parse(source).expect("parse");
        let sup = LintSuppressions::parse_in_tree(source, &tree);
        // The finding is on line 3, not on the directive's anchor line.
        let unused = sup.unused_directives(&present(&[(3, "redundant-quote")]), false);
        assert!(unused.is_empty());
    }

    #[test]
    fn a_reason_is_parsed_and_not_read_as_a_rule_name() {
        let sup = LintSuppressions::parse(
            ";; paredit:ignore redundant-quote -- keeps the macro readable\n(list '5)\n",
        );
        assert!(sup.is_suppressed("redundant-quote", 2));
        // "keeps", "the", … must not have become rule names, or the directive
        // would report four stale rules.
        let unused = sup.unused_directives(&present(&[(2, "redundant-quote")]), true);
        assert!(unused.is_empty());
    }

    #[test]
    fn requiring_a_reason_reports_a_used_directive_that_has_none() {
        let sup = LintSuppressions::parse(";; paredit:ignore redundant-quote\n(list '5)\n");
        assert!(
            sup.unused_directives(&present(&[(2, "redundant-quote")]), false)
                .is_empty()
        );
        let flagged = sup.unused_directives(&present(&[(2, "redundant-quote")]), true);
        assert_eq!(flagged.len(), 1);
        assert_eq!(flagged[0].problem, SuppressionProblem::MissingReason);
    }

    #[test]
    fn requiring_a_reason_does_not_double_report_a_stale_directive() {
        // A stale directive is reported once, as stale: telling someone to
        // justify a line they are about to delete is noise.
        let sup = LintSuppressions::parse(";; paredit:ignore redundant-quote\n(clean)\n");
        let flagged = sup.unused_directives(&present(&[]), true);
        assert_eq!(flagged.len(), 1);
        assert_eq!(flagged[0].problem, SuppressionProblem::Unused);
    }

    /// Applies edits back-to-front so earlier offsets stay valid.
    fn apply(text: &str, edits: &[SuppressionEdit]) -> String {
        let mut out = text.to_owned();
        for edit in edits.iter().rev() {
            out.replace_range(edit.start..edit.end, &edit.text);
        }
        out
    }

    #[test]
    fn removing_a_standalone_stale_directive_takes_the_whole_line() {
        let source = ";; paredit:ignore redundant-quote\n(clean)\n";
        let sup = LintSuppressions::parse(source);
        let edits = sup.removal_edits(&present(&[]));
        assert_eq!(edits.len(), 1);
        assert_eq!(apply(source, &edits), "(clean)\n");
    }

    #[test]
    fn removing_a_trailing_stale_directive_keeps_the_code() {
        let source = "(clean)  ; paredit:ignore redundant-quote\n";
        let sup = LintSuppressions::parse(source);
        let edits = sup.removal_edits(&present(&[]));
        assert_eq!(apply(source, &edits), "(clean)  \n");
    }

    #[test]
    fn a_partly_stale_directive_keeps_the_rules_that_fired() {
        let source = ";; paredit:ignore redundant-quote single-arg-comparison\n(< x)\n";
        let sup = LintSuppressions::parse(source);
        let edits = sup.removal_edits(&present(&[(2, "single-arg-comparison")]));
        assert_eq!(edits.len(), 1);
        assert_eq!(
            edits[0].removed_rules.as_deref(),
            Some(&["redundant-quote".to_owned()][..])
        );
        assert_eq!(
            apply(source, &edits),
            ";; paredit:ignore single-arg-comparison\n(< x)\n"
        );
    }

    #[test]
    fn narrowing_a_directive_preserves_its_reason() {
        let source = ";; paredit:ignore redundant-quote, single-arg-comparison -- why\n(< x)\n";
        let sup = LintSuppressions::parse(source);
        let edits = sup.removal_edits(&present(&[(2, "single-arg-comparison")]));
        assert_eq!(
            apply(source, &edits),
            ";; paredit:ignore single-arg-comparison -- why\n(< x)\n"
        );
    }

    #[test]
    fn a_used_directive_produces_no_edit() {
        let source = ";; paredit:ignore redundant-quote\n(list '5)\n";
        let sup = LintSuppressions::parse(source);
        assert!(
            sup.removal_edits(&present(&[(2, "redundant-quote")]))
                .is_empty()
        );
    }

    #[test]
    fn several_stale_directives_come_back_in_file_order() {
        let source = ";; paredit:ignore a-rule\n(x)\n;; paredit:ignore b-rule\n(y)\n";
        let sup = LintSuppressions::parse(source);
        let edits = sup.removal_edits(&present(&[]));
        assert_eq!(edits.len(), 2);
        assert!(edits[0].start < edits[1].start);
        assert_eq!(apply(source, &edits), "(x)\n(y)\n");
    }

    // --- expiry (`-until`) ---

    #[test]
    fn a_date_round_trips_through_display() {
        let date = Date::parse("2026-07-29").expect("parse");
        assert_eq!(date.to_string(), "2026-07-29");
    }

    #[test]
    fn known_epoch_days_convert_to_the_right_date() {
        assert_eq!(Date::from_days_since_epoch(0).to_string(), "1970-01-01");
        // 2024 is a leap year; day 366 of it should land on 2024-12-31.
        assert_eq!(
            Date::from_days_since_epoch(19_723).to_string(),
            "2024-01-01"
        );
        assert_eq!(
            Date::from_days_since_epoch(20_088).to_string(),
            "2024-12-31"
        );
    }

    #[test]
    fn a_directive_still_suppresses_after_it_expires() {
        let sup = LintSuppressions::parse(
            ";; paredit:ignore-until 2020-01-01 redundant-quote\n(list '5)\n",
        );
        assert!(sup.is_suppressed("redundant-quote", 2));
    }

    #[test]
    fn expired_directives_reports_a_past_date() {
        let sup = LintSuppressions::parse(
            ";; paredit:ignore-until 2020-01-01 redundant-quote\n(list '5)\n",
        );
        let today = Date::parse("2026-01-01").expect("parse");
        let expired = sup.expired_directives(today);
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].problem, SuppressionProblem::Expired);
        assert_eq!(
            expired[0].unused_rules.as_deref(),
            Some(&["redundant-quote".to_owned()][..])
        );
    }

    #[test]
    fn a_future_expiry_is_not_reported() {
        let sup = LintSuppressions::parse(";; paredit:ignore-until 2099-01-01\n(x)\n");
        let today = Date::parse("2026-01-01").expect("parse");
        assert!(sup.expired_directives(today).is_empty());
    }

    #[test]
    fn a_directive_without_until_never_expires() {
        let sup = LintSuppressions::parse(";; paredit:ignore redundant-quote\n(list '5)\n");
        let today = Date::parse("2099-01-01").expect("parse");
        assert!(sup.expired_directives(today).is_empty());
    }

    #[test]
    fn a_malformed_until_date_suppresses_nothing() {
        let sup = LintSuppressions::parse(";; paredit:ignore-until not-a-date\n(< x)\n");
        assert!(sup.is_empty());
        assert!(!sup.is_suppressed("single-arg-comparison", 2));
    }

    #[test]
    fn an_until_with_no_date_at_all_suppresses_nothing() {
        let sup = LintSuppressions::parse(";; paredit:ignore-until\n(< x)\n");
        assert!(sup.is_empty());
    }

    #[test]
    fn until_applies_to_file_and_next_form_scopes_too() {
        let file_sup =
            LintSuppressions::parse(";; paredit:ignore-file-until 2020-01-01\n(a)\n(b)\n");
        assert!(file_sup.is_suppressed("any-rule", 2));
        let today = Date::parse("2026-01-01").expect("parse");
        assert_eq!(file_sup.expired_directives(today).len(), 1);

        let source = ";; paredit:ignore-next-form-until 2020-01-01 redundant-quote\n(defun f ()\n  (list '5))\n";
        let tree = SyntaxTree::parse(source).expect("parse");
        let form_sup = LintSuppressions::parse_in_tree(source, &tree);
        assert!(form_sup.is_suppressed("redundant-quote", 3));
        assert_eq!(form_sup.expired_directives(today).len(), 1);
    }

    #[test]
    fn until_keeps_a_trailing_reason_and_rule_list() {
        let sup = LintSuppressions::parse(
            ";; paredit:ignore-until 2020-01-01 redundant-quote -- migrating off it\n(list '5)\n",
        );
        assert!(sup.is_suppressed("redundant-quote", 2));
        let unused = sup.unused_directives(&present(&[(2, "redundant-quote")]), true);
        assert!(unused.is_empty(), "the reason must not be read as a rule");
    }

    // --- inventory (`--report-suppressions`) ---

    #[test]
    fn inventory_lists_every_directive_used_or_not() {
        let source = ";; paredit:ignore redundant-quote -- why\n(list '5)\n\
                      ;; paredit:ignore single-arg-comparison\n(clean)\n";
        let sup = LintSuppressions::parse(source);
        let entries = sup.inventory(&present(&[(2, "redundant-quote")]));
        assert_eq!(entries.len(), 2);

        let used = entries.iter().find(|e| e.comment_line == 1).expect("entry");
        assert!(used.used);
        assert_eq!(used.reason.as_deref(), Some("why"));
        assert_eq!(
            used.rules.as_deref(),
            Some(&["redundant-quote".to_owned()][..])
        );

        let unused = entries.iter().find(|e| e.comment_line == 3).expect("entry");
        assert!(!unused.used);
        assert_eq!(unused.reason, None);
    }

    #[test]
    fn inventory_reports_a_bare_directive_as_no_rules() {
        let sup = LintSuppressions::parse(";; paredit:ignore\n(< x)\n");
        let entries = sup.inventory(&present(&[(2, "single-arg-comparison")]));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].rules, None);
        assert!(entries[0].used);
    }

    #[test]
    fn inventory_carries_the_expiry_as_a_date_string() {
        let sup = LintSuppressions::parse(
            ";; paredit:ignore-until 2026-12-31 redundant-quote\n(list '5)\n",
        );
        let entries = sup.inventory(&present(&[]));
        assert_eq!(entries[0].expires.as_deref(), Some("2026-12-31"));
    }

    #[test]
    fn inventory_is_empty_for_a_file_with_no_directives() {
        let sup = LintSuppressions::parse("(a)\n(b)\n");
        assert!(sup.inventory(&present(&[])).is_empty());
    }
}
