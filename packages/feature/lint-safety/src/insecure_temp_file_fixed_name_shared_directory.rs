//! `insecure-temp-file-fixed-name-shared-directory`: a predictable name written
//! in a directory everyone can write.
//!
//! `(with-open-file (s "/tmp/report.out" :direction :output) …)` names a file in
//! a directory every user on the host can write to, with a name every user on
//! the host can predict. Anyone can create `/tmp/report.out` first — as a
//! symlink to something the program's user owns — and the program then writes
//! through it. The window is not narrow: the attacker plants the link before the
//! program ever starts.
//!
//! The remedy is a name nobody can predict, created with `O_EXCL` semantics —
//! `mkstemp`, `uiop:with-temporary-file`, or at minimum a `gensym`/UUID
//! component. Which of those is right depends on what the file is for, so this
//! is report-only.
//!
//! What is reported is exactly two shapes, both of which put a name the *source*
//! spells into a world-writable directory:
//!
//! - a string literal designator beginning with a shared temporary directory —
//!   `/tmp/`, `/var/tmp/`, `/private/tmp/`, `/usr/tmp/`, `/dev/shm/`;
//! - `(merge-pathnames "fixed-name" *temporary-directory*)`, with the same set
//!   of directories plus the conventional `*…temp…*` / `*…tmp…*` special
//!   variables and `(temporary-directory)`-shaped calls.
//!
//! and only when the call *writes*: `:direction :output`/`:io`, any `:if-exists`,
//! or `:if-does-not-exist :create`. A plain read of a fixed temporary path is a
//! weaker problem with far more benign instances, so it is left alone.
//!
//! Limits, by design:
//!
//! - A name that is not a literal is never reported, which is what keeps the
//!   rule silent on every correct randomization: `(format nil "/tmp/~a"
//!   (gensym))`, `(uiop:tmpize-pathname …)`, `(open (make-temp-name))`. There is
//!   no literal there to be predictable.
//! - `uiop:with-temporary-file` and `mkstemp` are not heads here at all; they
//!   are the fix, not the finding.
//! - A fixed `/tmp` path also earns a `unportable-pathname` finding from
//!   `paredit-feature-lint-portability`, which reads the same literal for an
//!   unrelated reason (a hardcoded POSIX absolute path is not portable). The two
//!   are different complaints in different categories with different remedies;
//!   neither subsumes the other.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_in, symbol_is, unqualified};

use crate::support::{is_unevaluated_at, string_literal};

pub const META: RuleMeta = RuleMeta::new(
    "insecure-temp-file-fixed-name-shared-directory",
    RuleCategory::Security,
    Severity::Warning,
    "a fixed, predictable filename opened for writing under a world-writable temporary directory",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "A name the source spells, in a directory every user can write, can be created first by \
         anyone — as a symlink to a file the program's own user owns. The program then writes \
         through that link. Randomizing the name and creating it with O_EXCL semantics closes it.",
    )
    .with_example(
        "(with-open-file (s \"/tmp/report.out\" :direction :output) ...)",
        "(uiop:with-temporary-file (:stream s) ...)",
    )
    .with_caveat(
        "Only a literal name is reported. A name built at runtime — from `gensym`, a UUID, or \
         `uiop:tmpize-pathname` — has nothing predictable in it and is never flagged. Reads of a \
         fixed temporary path are not reported either; only writes are.",
    ),
);

const HEADS: [NormalizedHead; 2] = [
    NormalizedHead::new("open"),
    NormalizedHead::new("with-open-file"),
];

/// Directories a host shares between every user.
const SHARED_TEMP_DIRECTORIES: [&str; 5] = [
    "/tmp/",
    "/var/tmp/",
    "/private/tmp/",
    "/usr/tmp/",
    "/dev/shm/",
];

/// Calls that answer "where do temporary files go".
const TEMP_DIRECTORY_CALLS: [&str; 4] = [
    "temporary-directory",
    "temp-directory",
    "default-temporary-directory",
    "temporary-directory-pathname",
];

/// One predictable temporary file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PredictableTempFile {
    pub span: ByteSpan,
    pub name: String,
}

/// The `(designator, options)` of a file-opening call, whichever spelling it
/// uses.
///
/// `with-open-file` puts both inside its binding list — `(var designator
/// options…)` — and `open` puts them directly in the call. Reading them through
/// one function is what keeps the two spellings from drifting apart.
fn opened_file<'v>(
    view: &'v ExpressionView,
    head: &str,
) -> Option<(&'v ExpressionView, &'v [ExpressionView])> {
    if symbol_is(head, "with-open-file") {
        let binding = view.children.get(1)?;
        return Some((binding.children.get(1)?, binding.children.get(2..)?));
    }
    Some((view.children.get(1)?, view.children.get(2..)?))
}

/// Whether the options say this call writes.
fn writes(options: &[ExpressionView]) -> bool {
    let mut index = 0;
    while index < options.len() {
        let Some(key) = atom_text(&options[index]) else {
            index += 1;
            continue;
        };
        let value = options.get(index + 1).and_then(atom_text);
        let writing = match key.to_ascii_lowercase().as_str() {
            // Any `:if-exists` at all is a statement about creating or
            // truncating, which a read never makes.
            ":if-exists" => true,
            ":direction" => value.is_some_and(|v| {
                v.eq_ignore_ascii_case(":output") || v.eq_ignore_ascii_case(":io")
            }),
            ":if-does-not-exist" => value.is_some_and(|v| v.eq_ignore_ascii_case(":create")),
            _ => false,
        };
        if writing {
            return true;
        }
        index += 1;
    }
    false
}

/// The fixed name a designator spells inside a shared temporary directory.
fn fixed_temp_name(designator: &ExpressionView) -> Option<String> {
    if let Some(text) = string_literal(designator) {
        return shared_temp_path(text).map(str::to_owned);
    }
    merge_pathnames_temp_name(designator)
}

/// `text` if it names a file — not just the directory itself — under a shared
/// temporary directory.
fn shared_temp_path(text: &str) -> Option<&str> {
    SHARED_TEMP_DIRECTORIES
        .iter()
        .find_map(|directory| text.strip_prefix(directory))
        .filter(|rest| !rest.is_empty())
        .map(|_| text)
}

/// `(merge-pathnames "fixed" *temporary-directory*)`.
fn merge_pathnames_temp_name(designator: &ExpressionView) -> Option<String> {
    let head = list_head(designator)?;
    if !symbol_is(head, "merge-pathnames") {
        return None;
    }
    let name = string_literal(designator.children.get(1)?)?;
    if name.is_empty() {
        return None;
    }
    let base = designator.children.get(2)?;
    names_a_temp_directory(base).then(|| name.to_owned())
}

/// Whether `view` denotes the host's temporary directory.
fn names_a_temp_directory(view: &ExpressionView) -> bool {
    if let Some(text) = string_literal(view) {
        return SHARED_TEMP_DIRECTORIES
            .iter()
            .any(|directory| text.starts_with(directory) || directory.starts_with(text));
    }
    if let Some(head) = list_head(view) {
        return symbol_in(head, &TEMP_DIRECTORY_CALLS);
    }
    let Some(name) = atom_text(view) else {
        return false;
    };
    let stripped = unqualified(name);
    // The `*earmuffed*` special every implementation and portability layer
    // spells slightly differently.
    stripped.starts_with('*') && stripped.ends_with('*') && {
        let lowered = stripped.to_ascii_lowercase();
        lowered.contains("temporary-director")
            || lowered.contains("temp-director")
            || lowered.contains("tmp-director")
            || lowered.contains("temp-dir")
            || lowered.contains("tmp-dir")
    }
}

/// Reads one file-opening call.
#[must_use]
pub fn examine(view: &ExpressionView, context: &RuleContext<'_>) -> Option<PredictableTempFile> {
    let head = list_head(view)?;
    if !symbol_in(head, &["open", "with-open-file"]) {
        return None;
    }
    let (designator, options) = opened_file(view, head)?;
    if !writes(options) {
        return None;
    }
    let name = fixed_temp_name(designator)?;

    // Asked last, and only once there is something to report.
    if is_unevaluated_at(context.tree(), view.span) {
        return None;
    }
    Some(PredictableTempFile {
        span: designator.span,
        name,
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
                    "{} is a predictable name in a world-writable directory, so anyone can plant \
                     a symlink there before this runs; create the file with a randomized name and \
                     O_EXCL semantics (uiop:with-temporary-file, mkstemp)",
                    found.name
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

    fn names(input: &str) -> Vec<String> {
        findings_for_heads(input, &["open", "with-open-file"], |view, context| {
            examine(view, context)
                .map(|found| found.name)
                .into_iter()
                .collect::<Vec<_>>()
        })
    }

    #[test]
    fn flags_a_fixed_tmp_name_opened_for_writing() {
        assert_eq!(
            names(
                r#"(with-open-file (s "/tmp/report.out" :direction :output) (write-line "x" s))"#
            ),
            vec!["/tmp/report.out"]
        );
    }

    #[test]
    fn flags_the_open_spelling() {
        assert_eq!(
            names(r#"(open "/var/tmp/cache.dat" :if-exists :supersede)"#),
            vec!["/var/tmp/cache.dat"]
        );
    }

    #[test]
    fn flags_a_merge_pathnames_into_the_temp_directory() {
        assert_eq!(
            names(
                r#"(with-open-file (s (merge-pathnames "session.dat" *temporary-directory*) :direction :output) s)"#
            ),
            vec!["session.dat"]
        );
    }

    #[test]
    fn flags_every_shared_directory_spelling() {
        assert_eq!(names(r#"(open "/dev/shm/lock" :direction :io)"#).len(), 1);
        assert_eq!(
            names(r#"(open "/private/tmp/x" :if-does-not-exist :create)"#).len(),
            1
        );
    }

    // --- near misses ------------------------------------------------------

    #[test]
    fn does_not_flag_a_randomized_name() {
        assert!(
            names(
                r#"(with-open-file (s (format nil "/tmp/work-~a" (gensym)) :direction :output) s)"#
            )
            .is_empty()
        );
        assert!(names("(open (uiop:tmpize-pathname base) :direction :output)").is_empty());
        assert!(names("(open (make-temp-name) :direction :output)").is_empty());
    }

    #[test]
    fn does_not_flag_a_read_of_a_fixed_temporary_path() {
        assert!(names(r#"(with-open-file (s "/tmp/report.out") (read-line s))"#).is_empty());
        assert!(names(r#"(open "/tmp/report.out" :direction :input)"#).is_empty());
    }

    #[test]
    fn does_not_flag_a_write_outside_a_shared_directory() {
        assert!(names(r#"(open "/var/log/app.log" :direction :output)"#).is_empty());
        assert!(names(r#"(open "reports/out.txt" :direction :output)"#).is_empty());
    }

    #[test]
    fn does_not_flag_the_bare_temporary_directory() {
        // No file name after the directory: nothing predictable is opened.
        assert!(names(r#"(open "/tmp/" :direction :output)"#).is_empty());
    }

    #[test]
    fn does_not_flag_merge_pathnames_against_an_ordinary_base() {
        assert!(
            names(r#"(open (merge-pathnames "session.dat" *data-directory*) :direction :output)"#)
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_call_with_no_designator() {
        assert!(names("(open)").is_empty());
        assert!(names("(with-open-file (s))").is_empty());
    }

    // --- quote and string contexts ---------------------------------------

    #[test]
    fn does_not_flag_a_quoted_form() {
        assert!(names(r#"'(open "/tmp/x" :if-exists :supersede)"#).is_empty());
        assert!(names(r#"'(progn (open "/tmp/x" :if-exists :supersede))"#).is_empty());
        assert!(names(r#"(quote (open "/tmp/x" :if-exists :supersede))"#).is_empty());
        assert!(names(r#"`(open "/tmp/x" :if-exists :supersede)"#).is_empty());
        assert!(names(r#"'(a ,(open "/tmp/x" :if-exists :supersede))"#).is_empty());
    }

    #[test]
    fn flags_an_unquoted_form_inside_a_backquote() {
        assert_eq!(
            names(r#"`(a ,(open "/tmp/x" :if-exists :supersede))"#),
            vec!["/tmp/x"]
        );
    }

    #[test]
    fn does_not_flag_text_inside_a_string_literal() {
        assert!(names(r#"(log-it "(open \"/tmp/x\" :if-exists :supersede)")"#).is_empty());
    }
}
