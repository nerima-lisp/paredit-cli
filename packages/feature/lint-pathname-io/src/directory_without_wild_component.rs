//! `directory-without-wild-component`: asking for a directory's contents with
//! a pathname that matches only the directory.
//!
//! `(directory "/var/log/")` looks like "list that directory". It is not.
//! `directory` returns the truenames of the files the pathspec *matches*, and a
//! pathspec with no wild component matches exactly one thing — the directory
//! itself.
//!
//! Verified rather than assumed. On SBCL 2.6.0, over a directory `d/`
//! containing `one.lisp`, `two.txt`, and `sub/`:
//!
//! ```text
//! (directory "…/d/")       => (#P"…/d/")                    <- the directory
//! (directory "…/d")        => (#P"…/d/")                    <- the same
//! (directory "…/d/*.*")    => (#P"…/d/one.lisp" #P"…/d/sub/" #P"…/d/two.txt")
//! (directory "…/d/**/*.*") => (… also …/d/sub/three.lisp)
//! ```
//!
//! **The original framing of this rule was wrong and is not what is reported.**
//! It was proposed as "CLHS leaves the no-wildcard result
//! implementation-dependent". It does not: the `directory` dictionary entry
//! defines the result as the truenames of the matching files, and a wildcard-
//! free pathname matches itself. SBCL is behaving correctly. The defect is not
//! that the answer is unspecified — it is that the answer is a one-element list
//! nobody wanted, and it is silent.
//!
//! Only a *literal* designator ending in a directory separator is reported.
//! That is the shape whose intent is unambiguous: a trailing `/` says
//! "directory", and no wildcard says "and match only it". `(directory "a.txt")`
//! is a legitimate existence test and is never reported, and a computed
//! designator cannot be judged at all.
//!
//! Report-only. `*.*` lists one level and `**/*.*` recurses; which was wanted
//! is the whole question.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{list_head, symbol_is};

use crate::support::{is_unevaluated_at, string_literal};

pub const META: RuleMeta = RuleMeta::new(
    "directory-without-wild-component",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a (directory \"…/\") whose pathspec has no wildcard, so it matches only the directory itself",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "`directory` returns the truenames of the files its pathspec matches. A pathspec naming a \
         directory with no wild component matches exactly one file — that directory — so the call \
         returns a one-element list containing the argument rather than its contents, and does so \
         without any error.",
    )
    .with_example("(directory \"/var/log/\")", "(directory \"/var/log/*.*\")")
    .with_caveat(
        "Only a literal pathspec ending in a separator is reported. `(directory \"a.txt\")` is a \
         legitimate existence test, and a computed pathspec cannot be judged.",
    ),
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("directory")];

/// The characters CLHS 19.2.2.2 gives wildcard meaning to in a namestring.
///
/// `*` is the multi-character wildcard, `?` the single-character one, and `[`
/// opens a character set. Any of them makes the pathspec match more than
/// itself, which is all this rule needs to know.
const WILDCARD_CHARACTERS: [char; 3] = ['*', '?', '['];

/// One `directory` call that will match only its own argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WildlessDirectory {
    pub span: ByteSpan,
    /// The pathspec as written, for the message.
    pub pathspec: String,
}

/// Whether a namestring names a directory: it ends in a separator.
#[must_use]
pub fn names_a_directory(namestring: &str) -> bool {
    namestring.ends_with('/') || namestring.ends_with('\\')
}

/// Whether a namestring carries any wild component.
#[must_use]
pub fn has_wildcard(namestring: &str) -> bool {
    namestring.contains(WILDCARD_CHARACTERS)
}

/// Reads one `directory` call and reports a pathspec that matches only itself.
#[must_use]
pub fn examine(view: &ExpressionView) -> Option<WildlessDirectory> {
    let head = list_head(view)?;
    if !symbol_is(head, "directory") {
        return None;
    }
    let pathspec = string_literal(view.children.get(1)?)?;

    if !names_a_directory(pathspec) || has_wildcard(pathspec) {
        return None;
    }
    Some(WildlessDirectory {
        span: view.span,
        pathspec: pathspec.to_owned(),
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
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        let Some(found) = examine(view) else {
            return Ok(());
        };
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        sink.report(
            found.span,
            format!(
                "\"{}\" has no wild component, so this matches only the directory itself and \
                 returns a one-element list; add *.* for one level or **/*.* to recurse",
                found.pathspec
            ),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn found(input: &str) -> Option<String> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view).map(|item| item.pathspec)
    }

    #[test]
    fn flags_a_wildcard_free_directory_pathspec() {
        assert_eq!(
            found(r#"(directory "/var/log/")"#),
            Some("/var/log/".to_owned())
        );
        assert_eq!(found(r#"(directory "src/")"#), Some("src/".to_owned()));
    }

    #[test]
    fn flags_a_windows_separator_too() {
        assert_eq!(
            found(r#"(directory "C:\\logs\\")"#),
            Some("C:\\\\logs\\\\".to_owned())
        );
    }

    #[test]
    fn does_not_flag_a_pathspec_that_already_has_a_wildcard() {
        assert_eq!(found(r#"(directory "/var/log/*.*")"#), None);
        assert_eq!(found(r#"(directory "/var/log/**/*.*")"#), None);
        assert_eq!(found(r#"(directory "/var/log/*.lisp")"#), None);
        assert_eq!(found(r#"(directory "/var/log/?.txt")"#), None);
        assert_eq!(found(r#"(directory "/var/log/[ab].txt")"#), None);
    }

    /// A wild *directory* component: ends in a separator, so it looks like this
    /// rule's shape, and is wild, so it is not this rule's finding.
    ///
    /// The cases above name a file rather than a directory, so the
    /// `names_a_directory` test already declines them and they cannot tell the
    /// two guards apart. Mutation testing is how that was found.
    #[test]
    fn does_not_flag_a_wild_directory_component() {
        assert_eq!(found(r#"(directory "/var/*/log/")"#), None);
        assert_eq!(found(r#"(directory "/var/log/**/")"#), None);
    }

    /// A pathspec naming a file is an existence test, which is a real use of
    /// `directory` and not this rule's business.
    #[test]
    fn does_not_flag_a_pathspec_naming_a_file() {
        assert_eq!(found(r#"(directory "a.txt")"#), None);
        assert_eq!(found(r#"(directory "/var/log/syslog")"#), None);
    }

    #[test]
    fn does_not_flag_a_computed_pathspec() {
        assert_eq!(found("(directory root)"), None);
        assert_eq!(found("(directory (log-root))"), None);
        assert_eq!(found(r#"(directory #p"/var/log/")"#), None);
    }

    #[test]
    fn does_not_flag_a_call_with_no_argument() {
        assert_eq!(found("(directory)"), None);
    }

    #[test]
    fn reads_the_head_case_insensitively() {
        assert_eq!(
            found(r#"(DIRECTORY "/var/log/")"#),
            Some("/var/log/".to_owned())
        );
    }

    #[test]
    fn the_shape_tests_read_what_they_say() {
        assert!(names_a_directory("a/"));
        assert!(names_a_directory("a\\"));
        assert!(!names_a_directory("a"));
        assert!(!names_a_directory(""));
        assert!(has_wildcard("*.lisp"));
        assert!(has_wildcard("a?b"));
        assert!(has_wildcard("[ab]"));
        assert!(!has_wildcard("plain/"));
    }
}
