//! Schema-free structural sanity checks for one S-expression data file.
//!
//! Nothing here evaluates the file, and nothing here knows what any format
//! means. Every check reads only the shape a list already commits to by
//! repeating it: a plist alternates key and value, an alist is a run of pair
//! entries, a table of tuples has one arity most of its rows share. Reporting
//! the entry that breaks the pattern needs no schema — the file's own
//! repetition *is* the schema, for exactly the part these checks look at.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteOffset, ByteSpan, Delimiter, ExpressionKind, ExpressionView, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

use paredit_core_cli::report::{FileFindings, Finding};

/// Which family of checks `data-check` ran, on top of the baseline ones every
/// run performs.
///
/// Every non-[`DataFormat::Baseline`] variant runs the baseline checks too —
/// a format detector only ever *adds* checks, so choosing the wrong format
/// for a file never turns off the schema-free checks that apply everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataFormat {
    /// No format assumed: duplicate alist/plist keys, odd-length plists, and
    /// mismatched top-level tuple arity. Always runs, for every variant below.
    Baseline,
    /// An Emacs `(custom-set-variables ...)` form: each entry should be
    /// `(symbol value)` or `(symbol value now request comment)`.
    EmacsCustomize,
    /// EDN (extensible data notation): Clojure's reader minus the
    /// code-only reader macros EDN forbids.
    Edn,
    /// A `.paredit/rules/*.lisp` or `.paredit/migrations/*.lisp` file this
    /// tool itself reads as Common Lisp.
    PareditConfig,
    /// Emacs's `.dir-locals.el`: an alist of `(mode-or-nil . variable-alist)`.
    DirLocals,
    /// A Racket data file: `.rktd` by extension. `#lang` alone cannot signal
    /// this — every named language is still executable Racket — so `.rktd`
    /// is the only unambiguous marker. Phase 1 gives it no rule of its own
    /// beyond baseline — the value is routing these files into the report at
    /// all.
    RacketData,
}

/// What kind of structural inconsistency one finding reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataIssueKind {
    /// The same key appears twice in what looks like an alist or a plist; the
    /// later value silently overrides the earlier one.
    DuplicateKey,
    /// A plist-shaped list ends on a keyword with no value after it.
    OddLengthPlist,
    /// A top-level list of same-shaped tuples has an entry whose arity
    /// disagrees with the rest.
    NestingMismatch,
    /// A `custom-set-variables` entry is not `(symbol value)` or `(symbol
    /// value now request comment)`.
    EmacsCustomizeMalformedEntry,
    /// A code-only Clojure reader macro (`#(`, `#'`, `#?`/`#?@`) appears in a
    /// file being checked as EDN, which forbids all three.
    EdnInvalidReaderMacro,
    /// A `.paredit/rules`/`.paredit/migrations` file does not parse as
    /// Common Lisp, the dialect this tool itself reads it with.
    PareditConfigParseError,
    /// A `.dir-locals.el` entry is not shaped like `(mode-or-nil .
    /// variable-alist)`, or one of its variable-alist entries is not shaped
    /// like `(variable . value)`.
    DirLocalsMalformedEntry,
    /// A `.dir-locals.el` variable-alist has an `eval` key. Presence only —
    /// judging how dangerous the form under it is belongs to a later phase.
    DirLocalsEvalPresent,
}

impl DataIssueKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::DuplicateKey => "duplicate-key",
            Self::OddLengthPlist => "odd-length-plist",
            Self::NestingMismatch => "nesting-mismatch",
            Self::EmacsCustomizeMalformedEntry => "emacs-customize-malformed-entry",
            Self::EdnInvalidReaderMacro => "edn-invalid-reader-macro",
            Self::PareditConfigParseError => "paredit-config-parse-error",
            Self::DirLocalsMalformedEntry => "dir-locals-malformed-entry",
            Self::DirLocalsEvalPresent => "dir-locals-eval-present",
        }
    }
}

/// One structural inconsistency `data-check` found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataIssue {
    pub kind: DataIssueKind,
    /// One line of prose naming what triggered the finding: the repeated
    /// key, the dangling keyword, or the arity mismatch.
    pub detail: String,
    pub span: ByteSpan,
}

impl Finding for DataIssue {
    fn kind(&self) -> &'static str {
        self.kind.label()
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.detail.clone()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("detail", json!(self.detail))]
    }
}

/// Builds one file's `data-check` report.
#[must_use]
pub fn build_data_check_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
    format: DataFormat,
) -> FileFindings<DataIssue> {
    let root = tree.root_view();
    let mut findings = Vec::new();

    // Format-agnostic and runs for every variant: a duplicate key or a
    // mismatched tuple is the same problem whether or not the file also
    // matches one of the named conventions below.
    for_each_subview(&root, |view| {
        findings.extend(key_shape_issues(view));
    });
    for top_level in &root.children {
        findings.extend(nesting_mismatch_issues(top_level));
    }

    match format {
        DataFormat::Baseline | DataFormat::RacketData => {
            // Racket data files get no rule of their own yet (see
            // `DataFormat::RacketData`'s doc comment) — routing them here at
            // all, ahead of baseline, is the whole point of the variant.
        }
        DataFormat::EmacsCustomize => findings.extend(emacs_customize_issues(&root)),
        DataFormat::Edn => findings.extend(edn_issues(&root, tree.source())),
        DataFormat::PareditConfig => findings.extend(paredit_config_issues(tree.source())),
        DataFormat::DirLocals => findings.extend(dir_locals_issues(&root)),
    }

    // Every check above reads only the balanced-parens shape, with no
    // operator vocabulary consulted, so it means the same thing for every
    // dialect this tool parses — there is no dialect this report has nothing
    // to say about.
    let modelled = true;

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        tree.source(),
        findings,
        Vec::new(),
    )
}

/// Chooses the format `data-check` should apply to `path` when the caller did
/// not pass `--format`.
///
/// Checked most-specific first, so a `.dir-locals.el` inside a
/// `.paredit/migrations/` tree (unlikely, but not impossible) still reads as
/// `.dir-locals.el` rather than as a config file, and none of these can
/// mutually misfire: each format's own condition is disjoint from the
/// others' by construction (a distinct filename, a distinct path shape, or a
/// distinct dialect).
#[must_use]
pub fn detect_data_format(path: &Path, dialect: Dialect, tree: &SyntaxTree) -> DataFormat {
    if is_dir_locals_path(path) {
        return DataFormat::DirLocals;
    }
    if is_paredit_config_path(path) {
        return DataFormat::PareditConfig;
    }
    if is_racket_data(path, dialect, tree.source()) {
        return DataFormat::RacketData;
    }
    if is_edn_path(path) {
        return DataFormat::Edn;
    }
    if is_emacs_customize(dialect, &tree.root_view()) {
        return DataFormat::EmacsCustomize;
    }
    DataFormat::Baseline
}

/// Whether `path`'s file name is exactly `.dir-locals.el`, the one name Emacs
/// recognizes for this file — there is no extension-only heuristic for it.
fn is_dir_locals_path(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some(".dir-locals.el")
}

/// Whether `path` sits under a `.paredit/rules/` or `.paredit/migrations/`
/// directory, wherever in the path that pair of components appears.
///
/// A component-pair scan rather than a suffix match on the whole path: a
/// caller may pass a relative or an absolute path, and either way the
/// directory pair `.paredit`, `rules` (or `migrations`) is what marks the
/// file, not any particular number of segments before or after it.
fn is_paredit_config_path(path: &Path) -> bool {
    let components: Vec<&str> = path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect();
    components
        .windows(2)
        .any(|pair| pair[0] == ".paredit" && matches!(pair[1], "rules" | "migrations"))
}

/// Whether `path` is a Racket data file: `.rktd` by extension.
///
/// `#lang` alone cannot tell code from data: `typed/racket`, `racket/gui`,
/// and every other named language are still executable Racket, so branching
/// on "anything other than `racket`/`racket/base`" would misclassify code as
/// data. `.rktd` is the one unambiguous signal this format currently has.
fn is_racket_data(path: &Path, dialect: Dialect, _source: &str) -> bool {
    dialect == Dialect::Racket
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("rktd"))
}

/// Whether `path` has the `.edn` extension.
fn is_edn_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("edn"))
}

/// Whether `root` is an Emacs Lisp file with a top-level `(custom-set-variables
/// ...)` form.
///
/// Phase 1 requires the whole file to be recognised this way rather than
/// extracting a sub-region: `custom-set-variables` is very often one form
/// among many others in a larger `init.el`, and narrowing the check to just
/// that form (rather than the file) is future work, not a Phase 1 promise.
fn is_emacs_customize(dialect: Dialect, root: &ExpressionView) -> bool {
    dialect == Dialect::EmacsLisp
        && root
            .children
            .iter()
            .any(|form| list_head(form) == Some("custom-set-variables"))
}

/// Duplicate-key and odd-length-plist findings for one list node, if `view`
/// itself is shaped like a plist or an alist.
///
/// A node is checked as at most one of the two shapes: a plist's own children
/// alternate key and value directly, while an alist's children are
/// themselves pair entries. Those shapes cannot both hold for the same list
/// at once, so trying the plist reading first and falling back to the alist
/// reading cannot double-report one list.
fn key_shape_issues(view: &ExpressionView) -> Vec<DataIssue> {
    if !is_paren_list(view) || view.children.len() < 2 {
        return Vec::new();
    }

    match plist_issues(view) {
        Some(issues) => issues,
        None => alist_issues(view),
    }
}

/// The keyword text of an atom in key position, or `None` when it is not one.
///
/// Restricted to keyword atoms (`:foo`) rather than any bare symbol: a flat
/// list of alternating symbols and values is common in code that has nothing
/// to do with a plist (`(a 1 b 2)` inside an arbitrary literal), while a
/// keyword in every key position is the one shape ordinary Lisp code does
/// not produce by accident.
fn keyword_text(view: &ExpressionView) -> Option<&str> {
    atom_text(view).filter(|text| text.starts_with(':'))
}

/// Reads `view`'s children as a plist — key, value, key, value, … — when
/// every key position holds a keyword atom. Returns `None` when that does not
/// hold everywhere, so the caller can try the alist shape instead rather than
/// treating a partial match as a plist with holes.
fn plist_issues(view: &ExpressionView) -> Option<Vec<DataIssue>> {
    let children = &view.children;
    let key_slots = children.len().div_ceil(2);
    let every_key_slot_is_a_keyword =
        (0..key_slots).all(|slot| keyword_text(&children[slot * 2]).is_some());
    if !every_key_slot_is_a_keyword {
        return None;
    }

    let mut issues = Vec::new();
    let mut seen = HashSet::new();
    let mut index = 0;
    while index < children.len() {
        let key_view = &children[index];
        let key = keyword_text(key_view).expect("key slot holds a keyword, checked above");

        if children.get(index + 1).is_none() {
            issues.push(DataIssue {
                kind: DataIssueKind::OddLengthPlist,
                detail: format!("{key} has no value; the plist ends here"),
                span: key_view.span,
            });
            break;
        }

        if !seen.insert(key) {
            issues.push(DataIssue {
                kind: DataIssueKind::DuplicateKey,
                detail: format!(
                    "{key} repeats; the later value silently overrides the earlier one"
                ),
                span: key_view.span,
            });
        }
        index += 2;
    }
    Some(issues)
}

/// Reads `view`'s children as an alist — a run of `(key . value)` or
/// `(key value)` entries — and reports a key that repeats across entries.
///
/// Every child must resolve to an entry key or nothing is reported: one
/// child that is not shaped like an entry (a bare atom, a pair whose key is
/// not an atom, a list of some other arity) means this node is not
/// confidently an alist, and guessing which of its children to check anyway
/// would trade a real signal for a noisy one.
fn alist_issues(view: &ExpressionView) -> Vec<DataIssue> {
    let mut keys = Vec::with_capacity(view.children.len());
    for child in &view.children {
        match alist_entry_key(child) {
            Some(key_view) => keys.push(key_view),
            None => return Vec::new(),
        }
    }

    let mut issues = Vec::new();
    let mut seen = HashSet::new();
    for key_view in keys {
        let Some(key) = atom_text(key_view) else {
            continue;
        };
        if !seen.insert(key) {
            issues.push(DataIssue {
                kind: DataIssueKind::DuplicateKey,
                detail: format!(
                    "{key} repeats; the later value silently overrides the earlier one"
                ),
                span: key_view.span,
            });
        }
    }
    issues
}

/// The key atom of one alist entry — `(key . value)` or `(key value)` — or
/// `None` when `child` is not shaped like an entry.
fn alist_entry_key(child: &ExpressionView) -> Option<&ExpressionView> {
    if !is_paren_list(child) {
        return None;
    }
    let key = child.children.first()?;
    atom_text(key)?;
    match child.children.len() {
        2 => Some(key),
        3 if is_dot(&child.children[1]) => Some(key),
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

/// Reports the entries of `top_level` whose arity disagrees with the shape
/// most of its sibling entries share.
///
/// Deliberately conservative: this fires only when `top_level` is itself a
/// paren list of three or more paren-list children, and only when a strict
/// majority of them share one arity. A file mixing genuinely different
/// per-entry shapes — the common case for anything that is not literally a
/// table of same-shaped rows — has no majority and is left alone entirely,
/// which is the point: a false "your data is malformed" is worse than
/// staying quiet on a real one.
fn nesting_mismatch_issues(top_level: &ExpressionView) -> Vec<DataIssue> {
    if !is_paren_list(top_level) || top_level.children.len() < 3 {
        return Vec::new();
    }
    if !top_level.children.iter().all(is_paren_list) {
        return Vec::new();
    }

    let mut arity_counts: HashMap<usize, usize> = HashMap::new();
    for entry in &top_level.children {
        *arity_counts.entry(entry.children.len()).or_insert(0) += 1;
    }

    let Some((&majority_arity, &majority_count)) =
        arity_counts.iter().max_by_key(|(_, count)| **count)
    else {
        return Vec::new();
    };
    // A strict majority is unique when it exists, so which candidate
    // `max_by_key` happened to visit last cannot change this outcome.
    if majority_count * 2 <= top_level.children.len() {
        return Vec::new();
    }

    top_level
        .children
        .iter()
        .filter(|entry| entry.children.len() != majority_arity)
        .map(|entry| DataIssue {
            kind: DataIssueKind::NestingMismatch,
            detail: format!(
                "has {} element(s); {majority_count} of {} sibling entries have {majority_arity}",
                entry.children.len(),
                top_level.children.len()
            ),
            span: entry.span,
        })
        .collect()
}

/// `emacs-customize`: reports every `custom-set-variables` entry whose shape
/// is not `(symbol value)` or `(symbol value now request comment)`.
fn emacs_customize_issues(root: &ExpressionView) -> Vec<DataIssue> {
    let mut issues = Vec::new();
    for form in &root.children {
        if list_head(form) != Some("custom-set-variables") {
            continue;
        }
        for entry in form.children.iter().skip(1) {
            issues.extend(emacs_customize_entry_issues(entry));
        }
    }
    issues
}

fn emacs_customize_entry_issues(entry: &ExpressionView) -> Vec<DataIssue> {
    if !is_paren_list(entry) {
        return vec![DataIssue {
            kind: DataIssueKind::EmacsCustomizeMalformedEntry,
            detail: "entry is not a list".to_owned(),
            span: entry.span,
        }];
    }

    let arity = entry.children.len();
    if !matches!(arity, 2 | 5) {
        return vec![DataIssue {
            kind: DataIssueKind::EmacsCustomizeMalformedEntry,
            detail: format!(
                "has {arity} element(s); expected 2 (symbol value) or 5 (symbol value now request comment)"
            ),
            span: entry.span,
        }];
    }

    let Some(symbol) = entry.children.first() else {
        return Vec::new();
    };
    if looks_like_symbol(symbol) {
        return Vec::new();
    }
    vec![DataIssue {
        kind: DataIssueKind::EmacsCustomizeMalformedEntry,
        detail: "first element is not a symbol".to_owned(),
        span: symbol.span,
    }]
}

/// A conservative "is this a symbol, not a string or a keyword" check.
///
/// Not a full number-literal grammar: distinguishing a symbol from a number
/// precisely needs the same digit/sign/exponent state machine
/// `selector::pattern`'s `Number` constraint already has, and duplicating it
/// here for one Phase 1 heuristic is not worth the second copy to keep in
/// sync. A bare leading digit is rejected instead, which covers the entries
/// this check exists for — `custom-set-variables` never binds a literal
/// number as if it were a variable name.
fn looks_like_symbol(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| {
        !text.starts_with('"')
            && !text.starts_with(':')
            && !text.starts_with(|c: char| c.is_ascii_digit())
    })
}

/// `edn`: reports every occurrence of a code-only Clojure reader macro EDN
/// forbids — `#(` anonymous-function shorthand, `#'` var-quote, and
/// `#?`/`#?@` reader conditionals — while leaving `#{...}` set literals,
/// `#_` discards (which never reach the tree at all), and ordinary tagged
/// literals (`#inst`, `#uuid`, …) alone.
///
/// `#(` and `#{` both classify to the same `ReaderPrefix::HashLiteral` in the
/// reader policy, so the two are told apart here by the node's own
/// `delimiter` — `Paren` for `#(...)`, `Brace` for `#{...}` — which the
/// parser already records independently of that shared reader-macro
/// classification. `#'` (var-quote, forbidden) and `@` (deref, not
/// mentioned by EDN and left alone) collide the same way, as
/// `ReaderPrefix::Function`; nothing in the parsed tree tells those two
/// apart, so that one *is* resolved by inspecting the source byte at the
/// prefix's own start position.
fn edn_issues(root: &ExpressionView, source: &str) -> Vec<DataIssue> {
    let mut issues = Vec::new();
    for_each_subview(root, |view| {
        for &prefix in &view.reader_prefixes {
            let detail = match prefix {
                ReaderPrefix::HashLiteral if view.delimiter == Some(Delimiter::Paren) => {
                    "#(...) anonymous function shorthand is not valid EDN"
                }
                ReaderPrefix::Function
                    if source.as_bytes().get(view.span.start().get()) == Some(&b'#') =>
                {
                    "#'var-quote is not valid EDN"
                }
                ReaderPrefix::ReaderConditional | ReaderPrefix::ReaderConditionalSplicing => {
                    "#?/#?@ reader conditionals are not valid EDN"
                }
                _ => continue,
            };
            issues.push(DataIssue {
                kind: DataIssueKind::EdnInvalidReaderMacro,
                detail: detail.to_owned(),
                span: view.span,
            });
        }
    });
    issues
}

/// `paredit-config`: reports a parse failure under `Dialect::CommonLisp`, the
/// dialect `lint-custom`'s `parse_ruleset` and `migrate`'s recipe reader both
/// use for `.paredit/rules`/`.paredit/migrations` files.
///
/// Re-parses `source` from scratch rather than trusting the tree this report
/// was already handed: that tree parsed successfully under whichever dialect
/// `data-check` detected for the file, which is not necessarily
/// `Dialect::CommonLisp` — an unrecognised extension falls back to the
/// permissive legacy reader, which accepts source the strict Common Lisp
/// reader these two features actually use would refuse. A finding, not a
/// propagated error: a malformed rule file is a fact about that file, worth
/// reporting alongside every other file in the run rather than aborting it.
fn paredit_config_issues(source: &str) -> Vec<DataIssue> {
    let Err(error) = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp) else {
        return Vec::new();
    };
    let position = error.position();
    let start = ByteOffset::new(position);
    let end = ByteOffset::new((position + 1).min(source.len()).max(position));
    vec![DataIssue {
        kind: DataIssueKind::PareditConfigParseError,
        detail: format!("does not parse as Common Lisp: {error}"),
        span: ByteSpan::new(start, end),
    }]
}

/// `dir-locals`: reports a malformed `(mode-or-nil . variable-alist)` entry
/// or a malformed `(variable . value)` pair inside one, and separately flags
/// every `eval` key found in a variable-alist.
fn dir_locals_issues(root: &ExpressionView) -> Vec<DataIssue> {
    let mut issues = Vec::new();
    let Some(top) = root.children.first().filter(|view| is_paren_list(view)) else {
        return issues;
    };

    for entry in &top.children {
        let Some(alist_value) = dir_locals_alist_value(entry) else {
            issues.push(DataIssue {
                kind: DataIssueKind::DirLocalsMalformedEntry,
                detail: "expected (mode-or-nil . variable-alist)".to_owned(),
                span: entry.span,
            });
            continue;
        };
        if !is_paren_list(alist_value) {
            issues.push(DataIssue {
                kind: DataIssueKind::DirLocalsMalformedEntry,
                detail: "variable-alist is not a list".to_owned(),
                span: alist_value.span,
            });
            continue;
        }

        for variable_entry in &alist_value.children {
            match alist_entry_key(variable_entry) {
                Some(key_view) => {
                    if atom_text(key_view) == Some("eval") {
                        issues.push(DataIssue {
                            kind: DataIssueKind::DirLocalsEvalPresent,
                            detail: "an eval key is present in this variable-alist".to_owned(),
                            span: variable_entry.span,
                        });
                    }
                }
                None => issues.push(DataIssue {
                    kind: DataIssueKind::DirLocalsMalformedEntry,
                    detail: "expected (variable . value)".to_owned(),
                    span: variable_entry.span,
                }),
            }
        }
    }
    issues
}

/// The `variable-alist` half of a `(mode-or-nil . variable-alist)` entry, or
/// `None` when `entry` is not shaped like one.
///
/// Reuses [`alist_entry_key`]'s dotted-pair-or-2-list reading: hand-written
/// `.dir-locals.el` files use the explicit dot (`(nil . (...))`) exactly as
/// often as the plain 2-list spelling (`(nil (...))`), and this tool's
/// structural parser tells the two apart the same way an alist entry already
/// has to.
fn dir_locals_alist_value(entry: &ExpressionView) -> Option<&ExpressionView> {
    if !is_paren_list(entry) {
        return None;
    }
    let mode = entry.children.first()?;
    atom_text(mode)?;
    match entry.children.len() {
        2 => Some(&entry.children[1]),
        3 if is_dot(&entry.children[1]) => Some(&entry.children[2]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(source: &str) -> FileFindings<DataIssue> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_data_check_report(
            Path::new("t.lisp"),
            Dialect::CommonLisp,
            &tree,
            DataFormat::Baseline,
        )
    }

    #[test]
    fn a_dotted_alist_with_a_genuine_duplicate_key_is_reported() {
        let report = report("((key1 . v1) (key2 . v2) (key1 . v3))");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].kind, DataIssueKind::DuplicateKey);
        assert!(report.findings[0].detail.contains("key1"), "{report:?}");
    }

    #[test]
    fn a_proper_list_alist_with_a_genuine_duplicate_key_is_reported() {
        let report = report("((key1 v1) (key2 v2) (key1 v3))");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].kind, DataIssueKind::DuplicateKey);
    }

    #[test]
    fn an_alist_with_no_duplicates_reports_nothing() {
        let report = report("((key1 . v1) (key2 . v2) (key3 . v3))");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_well_formed_plist_reports_nothing() {
        let report = report("(:a 1 :b 2 :c 3)");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_plist_with_a_duplicate_key_is_reported() {
        let report = report("(:a 1 :b 2 :a 3)");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].kind, DataIssueKind::DuplicateKey);
        assert!(report.findings[0].detail.contains(":a"), "{report:?}");
    }

    #[test]
    fn a_plist_with_an_odd_trailing_keyword_is_reported() {
        let report = report("(:a 1 :b)");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].kind, DataIssueKind::OddLengthPlist);
        assert!(report.findings[0].detail.contains(":b"), "{report:?}");
    }

    #[test]
    fn a_file_with_no_data_like_structure_reports_nothing_and_does_not_crash() {
        let report = report("(defun add (a b) (+ a b))");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn an_atom_only_file_reports_nothing_and_does_not_crash() {
        let report = report("42");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn an_empty_file_reports_nothing_and_does_not_crash() {
        let report = report("");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_top_level_tuple_table_with_one_mismatched_entry_is_reported() {
        let report = report("((a 1 2) (b 1 2) (c 1 2) (d 1))");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].kind, DataIssueKind::NestingMismatch);
    }

    #[test]
    fn a_top_level_tuple_table_with_no_clear_majority_reports_nothing() {
        // Three entries, three different arities: no strict majority, so this
        // stays quiet rather than guessing which entry is "wrong".
        let report = report("((a 1) (b 1 2) (c 1 2 3))");
        assert!(
            report
                .findings
                .iter()
                .all(|issue| issue.kind != DataIssueKind::NestingMismatch),
            "{report:?}"
        );
    }

    #[test]
    fn dialect_modelled_is_always_true() {
        let tree = SyntaxTree::parse_with_dialect("(:a 1)", Dialect::Fennel).expect("parse");
        let report = build_data_check_report(
            Path::new("t.fnl"),
            Dialect::Fennel,
            &tree,
            DataFormat::Baseline,
        );
        assert!(report.dialect_modelled);
    }

    fn report_with(source: &str, dialect: Dialect, format: DataFormat) -> FileFindings<DataIssue> {
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
        build_data_check_report(Path::new("t"), dialect, &tree, format)
    }

    mod emacs_customize {
        use super::*;

        #[test]
        fn well_formed_entries_report_nothing() {
            let report = report_with(
                "(custom-set-variables\n '(foo-var \"value\")\n '(bar-var t now request \"comment\"))",
                Dialect::EmacsLisp,
                DataFormat::EmacsCustomize,
            );
            assert!(report.findings.is_empty(), "{report:?}");
        }

        #[test]
        fn an_odd_arity_entry_is_reported() {
            let report = report_with(
                "(custom-set-variables '(foo-var))",
                Dialect::EmacsLisp,
                DataFormat::EmacsCustomize,
            );
            assert_eq!(report.findings.len(), 1, "{report:?}");
            assert_eq!(
                report.findings[0].kind,
                DataIssueKind::EmacsCustomizeMalformedEntry
            );
        }

        #[test]
        fn a_non_symbol_first_element_is_reported() {
            let report = report_with(
                "(custom-set-variables '(\"not-a-symbol\" 1))",
                Dialect::EmacsLisp,
                DataFormat::EmacsCustomize,
            );
            assert_eq!(report.findings.len(), 1, "{report:?}");
            assert_eq!(
                report.findings[0].kind,
                DataIssueKind::EmacsCustomizeMalformedEntry
            );
            assert!(report.findings[0].detail.contains("symbol"), "{report:?}");
        }
    }

    mod edn {
        use super::*;

        #[test]
        fn a_set_literal_reports_nothing() {
            let report = report_with("#{1 2 3}", Dialect::Clojure, DataFormat::Edn);
            assert!(report.findings.is_empty(), "{report:?}");
        }

        #[test]
        fn a_tagged_literal_reports_nothing() {
            let report = report_with(r#"#inst "2020-01-01""#, Dialect::Clojure, DataFormat::Edn);
            assert!(report.findings.is_empty(), "{report:?}");
        }

        #[test]
        fn an_anonymous_function_shorthand_is_reported() {
            let report = report_with("#(+ % 1)", Dialect::Clojure, DataFormat::Edn);
            assert_eq!(report.findings.len(), 1, "{report:?}");
            assert_eq!(
                report.findings[0].kind,
                DataIssueKind::EdnInvalidReaderMacro
            );
        }

        #[test]
        fn a_var_quote_is_reported() {
            let report = report_with("#'foo", Dialect::Clojure, DataFormat::Edn);
            assert_eq!(report.findings.len(), 1, "{report:?}");
            assert_eq!(
                report.findings[0].kind,
                DataIssueKind::EdnInvalidReaderMacro
            );
        }

        #[test]
        fn a_deref_is_not_reported() {
            // `@` shares `ReaderPrefix::Function` with `#'`; only the latter is
            // forbidden by EDN.
            let report = report_with("@foo", Dialect::Clojure, DataFormat::Edn);
            assert!(report.findings.is_empty(), "{report:?}");
        }

        #[test]
        fn a_reader_conditional_is_reported() {
            let report = report_with("#?(:clj 1 :cljs 2)", Dialect::Clojure, DataFormat::Edn);
            assert_eq!(report.findings.len(), 1, "{report:?}");
            assert_eq!(
                report.findings[0].kind,
                DataIssueKind::EdnInvalidReaderMacro
            );
        }

        #[test]
        fn a_splicing_reader_conditional_is_reported() {
            let report = report_with(
                "[1 #?@(:clj [2] :cljs [3]) 4]",
                Dialect::Clojure,
                DataFormat::Edn,
            );
            assert_eq!(report.findings.len(), 1, "{report:?}");
            assert_eq!(
                report.findings[0].kind,
                DataIssueKind::EdnInvalidReaderMacro
            );
        }

        #[test]
        fn a_discard_never_reaches_the_tree_so_nothing_is_reported() {
            let report = report_with("[1 #_2 3]", Dialect::Clojure, DataFormat::Edn);
            assert!(report.findings.is_empty(), "{report:?}");
        }
    }

    mod paredit_config {
        use super::*;

        #[test]
        fn a_well_formed_rule_file_reports_nothing() {
            let report = report_with(
                "(defrule :name \"x\" :pattern (foo))",
                Dialect::CommonLisp,
                DataFormat::PareditConfig,
            );
            assert!(report.findings.is_empty(), "{report:?}");
        }

        #[test]
        fn a_file_that_does_not_parse_as_common_lisp_is_reported() {
            // `#{` is accepted by the legacy permissive reader (as a
            // `HashLiteral`-prefixed brace list) but has no meaning under
            // strict Common Lisp, where `#` dispatch on `{` is an
            // unsupported reader dispatch — exactly the gap this check
            // exists to surface.
            let report = report_with("#{1 2}", Dialect::Unknown, DataFormat::PareditConfig);
            assert_eq!(report.findings.len(), 1, "{report:?}");
            assert_eq!(
                report.findings[0].kind,
                DataIssueKind::PareditConfigParseError
            );
        }
    }

    mod dir_locals {
        use super::*;

        #[test]
        fn a_well_formed_dir_locals_file_reports_nothing() {
            let report = report_with(
                "((nil . ((indent-tabs-mode . nil)))\n (c-mode . ((c-file-style . \"gnu\"))))",
                Dialect::EmacsLisp,
                DataFormat::DirLocals,
            );
            assert!(report.findings.is_empty(), "{report:?}");
        }

        #[test]
        fn a_non_list_variable_alist_is_reported() {
            let report = report_with("((nil . 5))", Dialect::EmacsLisp, DataFormat::DirLocals);
            assert_eq!(report.findings.len(), 1, "{report:?}");
            assert_eq!(
                report.findings[0].kind,
                DataIssueKind::DirLocalsMalformedEntry
            );
        }

        #[test]
        fn a_malformed_variable_entry_is_reported() {
            let report = report_with(
                "((nil . ((too many parts here))))",
                Dialect::EmacsLisp,
                DataFormat::DirLocals,
            );
            assert_eq!(report.findings.len(), 1, "{report:?}");
            assert_eq!(
                report.findings[0].kind,
                DataIssueKind::DirLocalsMalformedEntry
            );
        }

        #[test]
        fn an_eval_key_is_reported_as_its_own_distinct_kind() {
            let report = report_with(
                "((nil . ((eval . (progn (message \"hi\"))))))",
                Dialect::EmacsLisp,
                DataFormat::DirLocals,
            );
            assert_eq!(report.findings.len(), 1, "{report:?}");
            assert_eq!(report.findings[0].kind, DataIssueKind::DirLocalsEvalPresent);
        }
    }

    mod racket_data {
        use super::*;

        #[test]
        fn a_clean_file_reports_nothing() {
            let report = report_with("(1 2 3)", Dialect::Racket, DataFormat::RacketData);
            assert!(report.findings.is_empty(), "{report:?}");
        }

        #[test]
        fn baseline_checks_still_apply() {
            let report = report_with(
                "((a . 1) (b . 2) (a . 3))",
                Dialect::Racket,
                DataFormat::RacketData,
            );
            assert_eq!(report.findings.len(), 1, "{report:?}");
            assert_eq!(report.findings[0].kind, DataIssueKind::DuplicateKey);
        }
    }

    mod detect_format {
        use super::*;

        fn detect(path: &str, dialect: Dialect, source: &str) -> DataFormat {
            let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
            detect_data_format(Path::new(path), dialect, &tree)
        }

        #[test]
        fn an_el_file_with_custom_set_variables_is_emacs_customize() {
            let format = detect(
                "init.el",
                Dialect::EmacsLisp,
                "(custom-set-variables '(foo-var 1))",
            );
            assert_eq!(format, DataFormat::EmacsCustomize);
        }

        #[test]
        fn an_el_file_without_custom_set_variables_is_baseline() {
            let format = detect("init.el", Dialect::EmacsLisp, "(setq foo 1)");
            assert_eq!(format, DataFormat::Baseline);
        }

        #[test]
        fn an_edn_file_is_edn() {
            let format = detect("data.edn", Dialect::Clojure, "{:a 1}");
            assert_eq!(format, DataFormat::Edn);
        }

        #[test]
        fn a_path_under_paredit_rules_is_paredit_config() {
            let format = detect(
                "project/.paredit/rules/no-foo.lisp",
                Dialect::CommonLisp,
                "(defrule :name \"x\")",
            );
            assert_eq!(format, DataFormat::PareditConfig);
        }

        #[test]
        fn a_path_under_paredit_migrations_is_paredit_config() {
            let format = detect(
                ".paredit/migrations/001-rename.lisp",
                Dialect::CommonLisp,
                "(defmigration :name \"x\")",
            );
            assert_eq!(format, DataFormat::PareditConfig);
        }

        #[test]
        fn dot_dir_locals_el_is_dir_locals() {
            let format = detect(".dir-locals.el", Dialect::EmacsLisp, "((nil . ()))");
            assert_eq!(format, DataFormat::DirLocals);
        }

        #[test]
        fn an_rktd_file_is_racket_data() {
            let format = detect("board.rktd", Dialect::Racket, "(1 2 3)");
            assert_eq!(format, DataFormat::RacketData);
        }

        #[test]
        fn a_rkt_file_with_a_non_racket_lang_directive_is_still_baseline() {
            // `typed/racket` is executable code, not data, even though its
            // `#lang` name differs from plain `racket`/`racket/base` -- only
            // the `.rktd` extension signals data.
            let format = detect("board.rkt", Dialect::Racket, "#lang typed/racket\n(1 2 3)");
            assert_eq!(format, DataFormat::Baseline);
        }

        #[test]
        fn a_rkt_file_with_the_racket_lang_directive_is_baseline() {
            let format = detect("prog.rkt", Dialect::Racket, "#lang racket\n(+ 1 2)");
            assert_eq!(format, DataFormat::Baseline);
        }

        #[test]
        fn a_rkt_file_with_no_lang_directive_is_baseline() {
            let format = detect("prog.rkt", Dialect::Racket, "(+ 1 2)");
            assert_eq!(format, DataFormat::Baseline);
        }

        #[test]
        fn an_ordinary_lisp_file_is_baseline() {
            let format = detect("data.lisp", Dialect::CommonLisp, "((a . 1) (b . 2))");
            assert_eq!(format, DataFormat::Baseline);
        }
    }
}
