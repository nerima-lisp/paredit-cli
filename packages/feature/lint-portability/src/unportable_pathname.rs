//! `unportable-pathname`: a host's path syntax written into a string.
//!
//! `(open "C:\\data\\in.txt")` and `(load "../lib/util.lisp")` both hand a
//! filesystem operator a string whose meaning is the host's, not the
//! program's. Common Lisp has pathnames precisely so that a program can name a
//! file without naming a filesystem, and `merge-pathnames` over a
//! `make-pathname` says the same thing on every host.
//!
//! Only the *unambiguous* markers are reported: a backslash separator, a
//! Windows drive letter, and a leading `/` or `~`. A bare `"config.lisp"` is
//! portable and common, and reporting it would make the rule unusable.
//!
//! Report-only: rewriting a path string into a pathname designator means
//! deciding what is directory, what is name, and what is type, which the string
//! does not say and the rule must not guess.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, list_head};

pub const META: RuleMeta = RuleMeta::new(
    "unportable-pathname",
    RuleCategory::Portability,
    Severity::Warning,
    "a filesystem call given a string that spells one host's path syntax",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "A namestring is parsed by the host's pathname syntax, so a backslash separator, a drive \
         letter, or an absolute root means something on one platform and something else — or \
         nothing — on another. `make-pathname` and `merge-pathnames` name a file without naming a \
         filesystem.",
    )
    .with_example(
        "(with-open-file (s \"data/in.txt\") …)",
        "(with-open-file (s (merge-pathnames #p\"in.txt\" *data-directory*)) …)",
    )
    .with_caveat(
        "A plain relative filename with no separator — `\"config.lisp\"` — is portable and is \
         never reported.",
    ),
);

/// Filesystem operators, paired with the argument holding the file designator.
const FILE_OPERATORS: [(&str, usize); 11] = [
    ("open", 1),
    ("load", 1),
    ("probe-file", 1),
    ("truename", 1),
    ("delete-file", 1),
    ("directory", 1),
    ("compile-file", 1),
    ("ensure-directories-exist", 1),
    ("rename-file", 1),
    ("file-write-date", 1),
    ("with-open-file", 1),
];

const HEADS: [NormalizedHead; 11] = [
    NormalizedHead::new("open"),
    NormalizedHead::new("load"),
    NormalizedHead::new("probe-file"),
    NormalizedHead::new("truename"),
    NormalizedHead::new("delete-file"),
    NormalizedHead::new("directory"),
    NormalizedHead::new("compile-file"),
    NormalizedHead::new("ensure-directories-exist"),
    NormalizedHead::new("rename-file"),
    NormalizedHead::new("file-write-date"),
    NormalizedHead::new("with-open-file"),
];

/// What makes a namestring host-specific.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathAssumption {
    /// A `\` separator: Windows only, and doubly confusing inside a Lisp
    /// string where it is also the escape character.
    Backslash,
    /// A `C:` drive prefix.
    DriveLetter,
    /// A leading `/` or `~`, which names a specific machine's layout.
    AbsoluteRoot,
    /// A `/` separator anywhere else.
    UnixSeparator,
}

impl PathAssumption {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Backslash => "a backslash separator",
            Self::DriveLetter => "a drive letter",
            Self::AbsoluteRoot => "an absolute root",
            Self::UnixSeparator => "a directory separator",
        }
    }
}

/// One host-specific namestring passed to a filesystem call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnportablePath {
    pub span: ByteSpan,
    pub assumption: PathAssumption,
}

/// What, if anything, makes `namestring` host-specific.
///
/// Checked in order of how loudly each marker says "one platform": a drive
/// letter and a backslash are Windows and nothing else, an absolute root names
/// a machine's layout, and a separator alone merely assumes a syntax.
#[must_use]
pub fn path_assumption(namestring: &str) -> Option<PathAssumption> {
    if namestring.contains('\\') {
        return Some(PathAssumption::Backslash);
    }
    let mut characters = namestring.chars();
    if let (Some(first), Some(':')) = (characters.next(), characters.next()) {
        if first.is_ascii_alphabetic() {
            return Some(PathAssumption::DriveLetter);
        }
    }
    if namestring.starts_with('/') || namestring.starts_with('~') {
        return Some(PathAssumption::AbsoluteRoot);
    }
    namestring
        .contains('/')
        .then_some(PathAssumption::UnixSeparator)
}

/// The string literal a form is, if it is one.
///
/// The syntax layer keeps a string's quotes in its text, and only a token that
/// opens and closes with one is a literal — which is also what rules out a
/// symbol whose name happens to contain a slash.
fn string_literal(view: &ExpressionView) -> Option<&str> {
    let text = atom_text(view)?;
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    Some(inner)
}

/// Reads one filesystem call and reports the namestring it was given.
#[must_use]
pub fn examine(view: &ExpressionView) -> Option<UnportablePath> {
    let head = list_head(view)?;
    let (name, position) = FILE_OPERATORS
        .iter()
        .find(|(name, _)| head.eq_ignore_ascii_case(name))
        .copied()?;

    // `with-open-file` puts its designator inside the binding form:
    // `(with-open-file (stream <designator> …) …)`.
    let designator = if name.eq_ignore_ascii_case("with-open-file") {
        view.children.get(1)?.children.get(1)?
    } else {
        view.children.get(position)?
    };

    let namestring = string_literal(designator)?;
    path_assumption(namestring).map(|assumption| UnportablePath {
        span: designator.span,
        assumption,
    })
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        if let Some(found) = examine(view) {
            sink.report(
                found.span,
                format!(
                    "this namestring spells {}, which is one host's syntax; \
                     build the pathname with make-pathname/merge-pathnames instead",
                    found.assumption.as_str()
                ),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn assumption(input: &str) -> Option<PathAssumption> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view).map(|found| found.assumption)
    }

    #[test]
    fn reads_each_marker() {
        assert_eq!(
            path_assumption("C:\\tmp\\x"),
            Some(PathAssumption::Backslash)
        );
        assert_eq!(
            path_assumption("C:/tmp/x"),
            Some(PathAssumption::DriveLetter)
        );
        assert_eq!(
            path_assumption("/etc/hosts"),
            Some(PathAssumption::AbsoluteRoot)
        );
        assert_eq!(
            path_assumption("~/config"),
            Some(PathAssumption::AbsoluteRoot)
        );
        assert_eq!(
            path_assumption("data/in.txt"),
            Some(PathAssumption::UnixSeparator)
        );
    }

    #[test]
    fn a_plain_relative_filename_is_portable() {
        assert_eq!(path_assumption("config.lisp"), None);
        assert_eq!(path_assumption(""), None);
    }

    #[test]
    fn flags_a_filesystem_call_given_a_host_path() {
        assert_eq!(
            assumption(r#"(load "/etc/init.lisp")"#),
            Some(PathAssumption::AbsoluteRoot)
        );
        assert_eq!(
            assumption(r#"(probe-file "data/in.txt")"#),
            Some(PathAssumption::UnixSeparator)
        );
    }

    #[test]
    fn reads_the_designator_inside_a_with_open_file_binding() {
        assert_eq!(
            assumption(r#"(with-open-file (s "data/in.txt") (read s))"#),
            Some(PathAssumption::UnixSeparator)
        );
    }

    #[test]
    fn does_not_flag_a_portable_filename() {
        assert_eq!(assumption(r#"(load "init.lisp")"#), None);
    }

    #[test]
    fn does_not_flag_a_computed_designator() {
        assert_eq!(assumption("(load (config-path))"), None);
        assert_eq!(assumption("(load *init-file*)"), None);
    }

    #[test]
    fn does_not_flag_a_pathname_literal() {
        // `#p"…"` is already a pathname; its syntax is the reader's problem.
        assert_eq!(assumption(r#"(load #p"data/in.txt")"#), None);
    }

    #[test]
    fn does_not_flag_a_slash_bearing_symbol() {
        assert_eq!(assumption("(load and/or)"), None);
    }

    #[test]
    fn reads_the_head_case_insensitively() {
        assert_eq!(
            assumption(r#"(LOAD "/etc/init.lisp")"#),
            Some(PathAssumption::AbsoluteRoot)
        );
    }
}
