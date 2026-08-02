//! `path-traversal-via-concatenated-filename`: a value pasted onto a directory
//! the source names, and opened.
//!
//! `(open (concatenate 'string "data/" name))` reads `data/<name>` — unless
//! `name` is `../../etc/passwd`, in which case it reads whatever that resolves
//! to. String concatenation has no notion of a path boundary, so the base
//! directory is a suggestion rather than a confinement.
//!
//! The fix is not "escape the value": it is to resolve the result and check that
//! it is still under the base — `truename` (or `uiop:truenamize`) plus a prefix
//! test — or to take only the file component with `pathname-name` /
//! `file-namestring` before joining.
//!
//! # What is required to fire, and why each part is there
//!
//! All three, on the designator argument of a filesystem operator:
//!
//! 1. **It was assembled** by `concatenate` / `format nil` / `strcat`. A
//!    designator that is a literal, a variable, or a `merge-pathnames` call is
//!    not this rule's shape.
//! 2. **A literal fragment names a base directory** — a `/` with a non-empty
//!    segment in front of it, in one fragment. `"data/"` and `"/var/lib/app/~a"`
//!    qualify; `"/"` and `"~a/~a"` do not. This is the difference between
//!    "the source confined this to a directory" and "the source joined two
//!    values", and it is what keeps the rule off `(format nil "~a/~a" dir name)`,
//!    which is how an enormous amount of correct code joins a path.
//! 3. **Some spliced part is non-literal and not already narrowed.** A part
//!    wrapped in `truename`, `probe-file`, `pathname-name`, `file-namestring`,
//!    `enough-namestring` or `merge-pathnames` has been resolved or reduced to a
//!    name, and cannot carry a `..` through.
//! 4. **No spliced part is a freshly generated unique value.** `(format nil
//!    "/tmp/~a-~a" prefix (gensym))` is the *correct* way to name a scratch
//!    file — the one `insecure-temp-file-fixed-name-shared-directory` asks for
//!    — and a `gensym`, `random`, UUID or clock reading anywhere in the name
//!    says the name is being made unique rather than resolved from input.
//!    Reporting the idiom this package elsewhere recommends is the fastest way
//!    to get a rule switched off.
//!
//! `(open (truename (concatenate 'string "data/" name)))` never reaches the rule
//! at all: the designator's operator is `truename`, not a string builder.
//!
//! # Not a duplicate of `subprocess-string-building`
//!
//! That rule and this one share a technique — "a value was spliced into a string
//! that something then interprets" — and nothing else. Its heads are
//! `run-program`/`run-shell-command`/`shell-command`/`launch-program`; these are
//! filesystem operators. The head sets are disjoint, so no form can draw both,
//! and the two claims are different: there the interpreter is a shell and the
//! dangerous character is `;`, here it is the pathname resolver and the
//! dangerous sequence is `..`. Neither rule's conditions imply the other's, and
//! this rule adds two (the literal base directory, the sanitizer check) that it
//! does not have.
//!
//! Limits, by design: a value narrowed by a project-local helper
//! (`(safe-name x)`) is not recognised as narrowed and will be reported; a
//! designator built with `merge-pathnames` is never reported even when its base
//! is unchecked, because that is a pathname operation with its own semantics and
//! flagging it would fire on most correct path code in the language.
//!
//! Report-only.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{list_head, symbol_in, symbol_is};

use crate::support::{is_unevaluated_at, names_a_base_directory, string_build};

pub const META: RuleMeta = RuleMeta::new(
    "path-traversal-via-concatenated-filename",
    RuleCategory::Security,
    Severity::Error,
    "a filename concatenated from a literal base directory and an unchecked value, opened without \
     resolving it",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "String concatenation has no notion of a path boundary, so a base directory pasted in \
         front of a value confines nothing: a value containing `..` walks straight out of it. \
         Resolving the result with `truename` and checking it is still under the base, or taking \
         only `pathname-name` of the value, is what actually confines it.",
    )
    .with_example(
        "(open (concatenate 'string \"data/\" name))",
        "(open (concatenate 'string \"data/\" (pathname-name name)))",
    )
    .with_caveat(
        "Only an assembled designator with a literal base directory is reported. `(format nil \
         \"~a/~a\" dir name)` joins two values and names no base in the source, so it is not \
         reported; neither is anything built with `merge-pathnames`.",
    ),
);

const HEADS: [NormalizedHead; 5] = [
    NormalizedHead::new("open"),
    NormalizedHead::new("with-open-file"),
    NormalizedHead::new("probe-file"),
    NormalizedHead::new("delete-file"),
    NormalizedHead::new("load"),
];

/// Operators that resolve a path or reduce a value to a bare file component, so
/// a `..` in it cannot survive.
const NARROWING_OPERATORS: [&str; 6] = [
    "truename",
    "probe-file",
    "pathname-name",
    "file-namestring",
    "enough-namestring",
    "merge-pathnames",
];

/// Operators that manufacture a fresh unique value.
///
/// Their presence identifies the name as a scratch name being made unique, not
/// a path resolved from input — see this module's condition 4.
const UNIQUE_VALUE_OPERATORS: [&str; 9] = [
    "gensym",
    "gentemp",
    "random",
    "make-v4-uuid",
    "make-uuid",
    "uuid",
    "get-universal-time",
    "get-internal-real-time",
    "sxhash",
];

/// One traversable filename.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraversableFilename {
    pub span: ByteSpan,
    pub builder: &'static str,
    /// The literal fragment that names the base directory the value escapes.
    pub base: String,
}

/// The designator argument of a filesystem operator.
///
/// `with-open-file` puts it inside its binding list; everything else takes it
/// directly.
fn designator<'v>(view: &'v ExpressionView, head: &str) -> Option<&'v ExpressionView> {
    if symbol_is(head, "with-open-file") {
        return view.children.get(1)?.children.get(1);
    }
    view.children.get(1)
}

/// Whether `view` has already been resolved or reduced to a file component.
fn is_narrowed(view: &ExpressionView) -> bool {
    list_head(view).is_some_and(|head| symbol_in(head, &NARROWING_OPERATORS))
}

/// Whether `view` manufactures a fresh unique value.
fn is_generated_unique(view: &ExpressionView) -> bool {
    list_head(view).is_some_and(|head| symbol_in(head, &UNIQUE_VALUE_OPERATORS))
}

/// Reads one filesystem call.
#[must_use]
pub fn examine(view: &ExpressionView, context: &RuleContext<'_>) -> Option<TraversableFilename> {
    let head = list_head(view)?;
    if !symbol_in(
        head,
        &[
            "open",
            "with-open-file",
            "probe-file",
            "delete-file",
            "load",
        ],
    ) {
        return None;
    }
    let build = string_build(designator(view, head)?)?;

    let base = build
        .literal_fragments()
        .into_iter()
        .find(|fragment| names_a_base_directory(fragment))?;
    let base = base.to_owned();

    let spliced = build.interpolated();
    if spliced.is_empty() || spliced.iter().all(|part| is_narrowed(part)) {
        return None;
    }
    if spliced.iter().any(|part| is_generated_unique(part)) {
        return None;
    }

    // Asked last, and only once there is something to report.
    if is_unevaluated_at(context.tree(), view.span) {
        return None;
    }
    Some(TraversableFilename {
        span: view.span,
        builder: build.builder(),
        base,
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
        if let Some(found) = examine(view, context) {
            sink.report(
                found.span,
                format!(
                    "this filename is built by {} from the base {:?} and an unchecked value, and \
                     concatenation does not stop at a path boundary: a value containing .. leaves \
                     the base; resolve it with truename and check the prefix, or splice only \
                     (pathname-name value)",
                    found.builder, found.base
                ),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::testing::findings_for_heads;

    fn bases(input: &str) -> Vec<String> {
        findings_for_heads(
            input,
            &[
                "open",
                "with-open-file",
                "probe-file",
                "delete-file",
                "load",
            ],
            |view, context| {
                examine(view, context)
                    .map(|found| found.base)
                    .into_iter()
                    .collect::<Vec<_>>()
            },
        )
    }

    #[test]
    fn flags_a_concatenated_filename() {
        assert_eq!(
            bases(r#"(open (concatenate 'string "data/" name))"#),
            vec!["data/"]
        );
    }

    #[test]
    fn flags_a_format_built_filename() {
        assert_eq!(
            bases(r#"(with-open-file (s (format nil "/var/lib/app/~a" user-file)) (read-line s))"#),
            vec!["/var/lib/app/~a"]
        );
    }

    #[test]
    fn flags_every_filesystem_spelling() {
        assert_eq!(
            bases(r#"(probe-file (concatenate 'string "uploads/" f))"#).len(),
            1
        );
        assert_eq!(
            bases(r#"(delete-file (concatenate 'string "uploads/" f))"#).len(),
            1
        );
        assert_eq!(
            bases(r#"(load (concatenate 'string "modules/" m))"#).len(),
            1
        );
        assert_eq!(bases(r#"(open (uiop:strcat "data/" name))"#).len(), 1);
    }

    // --- near misses ------------------------------------------------------

    #[test]
    fn does_not_flag_a_join_of_two_values() {
        // No base directory in the source: `"~a/~a"` and `"/"` name none.
        assert!(bases(r#"(open (format nil "~a/~a" *root* name))"#).is_empty());
        assert!(bases(r#"(open (concatenate 'string dir "/" name))"#).is_empty());
    }

    #[test]
    fn does_not_flag_an_all_literal_filename() {
        assert!(bases(r#"(open (concatenate 'string "data/" "index.txt"))"#).is_empty());
        assert!(bases(r#"(open "data/index.txt")"#).is_empty());
    }

    #[test]
    fn does_not_flag_a_canonicalized_path() {
        assert!(bases(r#"(open (truename (concatenate 'string "data/" name)))"#).is_empty());
        assert!(bases(r#"(open (merge-pathnames name *data-directory*))"#).is_empty());
    }

    #[test]
    fn does_not_flag_a_randomized_scratch_name() {
        // The idiom `insecure-temp-file-fixed-name-shared-directory` asks for.
        assert!(
            bases(
                r#"(with-open-file (s (format nil "/tmp/~a-~a" prefix (gensym)) :direction :output) s)"#
            )
            .is_empty()
        );
        assert!(bases(r#"(open (concatenate 'string "cache/" (random 1000000)))"#).is_empty());
    }

    #[test]
    fn does_not_flag_a_value_reduced_to_its_file_component() {
        assert!(bases(r#"(open (concatenate 'string "data/" (pathname-name name)))"#).is_empty());
        assert!(bases(r#"(open (concatenate 'string "data/" (file-namestring name)))"#).is_empty());
    }

    #[test]
    fn does_not_flag_a_printing_format() {
        // `(format t …)` returns nil; it assembled no filename.
        assert!(bases(r#"(open (format t "data/~a" name))"#).is_empty());
    }

    #[test]
    fn does_not_flag_a_call_with_no_designator() {
        assert!(bases("(open)").is_empty());
        assert!(bases("(with-open-file (s))").is_empty());
    }

    // --- quote and string contexts ---------------------------------------

    #[test]
    fn does_not_flag_a_quoted_form() {
        assert!(bases(r#"'(open (concatenate 'string "data/" name))"#).is_empty());
        assert!(bases(r#"'(progn (open (concatenate 'string "data/" name)))"#).is_empty());
        assert!(bases(r#"(quote (open (concatenate 'string "data/" name)))"#).is_empty());
        assert!(bases(r#"`(open (concatenate 'string "data/" name))"#).is_empty());
        assert!(bases(r#"'(a ,(open (concatenate 'string "data/" name)))"#).is_empty());
    }

    #[test]
    fn flags_an_unquoted_form_inside_a_backquote() {
        assert_eq!(
            bases(r#"`(a ,(open (concatenate 'string "data/" name)))"#),
            vec!["data/"]
        );
    }

    #[test]
    fn does_not_flag_text_inside_a_string_literal() {
        assert!(bases(r#"(log-it "(open (concatenate 'string \"data/\" name))")"#).is_empty());
    }
}
