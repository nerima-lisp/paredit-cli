//! `output-stream-without-if-exists`: opening a file for output and leaving
//! what happens to an existing one to the default.
//!
//! `(with-open-file (s path :direction :output) …)` reads as "write this
//! file". It is not. CLHS `open` gives `:if-exists` the default `:error`
//! whenever the pathname's version is not `:newest` — which is every pathname
//! on a host without file versions, so every pathname on Unix and Windows. The
//! form works on a machine where the file is absent and signals `file-error`
//! on the machine where it is not, which is the worst possible distribution of
//! outcomes across development and production.
//!
//! Verified rather than assumed. On SBCL 2.6.0:
//!
//! ```text
//! (with-open-file (s "exists.txt" :direction :output) (write-string "x" s))
//!   => The file #P"…/exists.txt" already exists: File exists
//!      condition type: SB-EXT:FILE-EXISTS
//! ```
//!
//! Only a *literally* `:output` or `:io` direction is reported. A direction
//! read from a variable cannot be judged, and `:input` — the default, and the
//! overwhelmingly common case — never can be.
//!
//! Report-only, and deliberately so: `:supersede`, `:overwrite`, `:append`,
//! `:rename`, and an explicit `:error` are five different programs, and the
//! form does not say which one was meant. A rule that guessed `:supersede`
//! would silently turn "refuse to clobber" into "clobber".
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

use crate::support::{has_keyword, is_keyword, is_unevaluated_at, keyword_value};

pub const META: RuleMeta = RuleMeta::new(
    "output-stream-without-if-exists",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a file opened for output with no :if-exists, so an existing file signals file-error",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "`:if-exists` defaults to `:error` for any pathname whose version is not `:newest`, which \
         is every pathname on a host without file versions. So an output form that omits it works \
         while the file is absent and signals `file-error` once it is not — a failure that first \
         appears on the second run, or in production.",
    )
    .with_example(
        "(with-open-file (s path :direction :output) (write-string text s))",
        "(with-open-file (s path :direction :output :if-exists :supersede) (write-string text s))",
    )
    .with_caveat(
        "Only a literal `:output` or `:io` direction is reported; a direction held in a variable \
         cannot be judged. An explicit `:if-exists :error` is accepted — refusing to clobber is a \
         decision, and this rule is about the ones nobody made.",
    ),
);

const HEADS: [NormalizedHead; 2] = [
    NormalizedHead::new("with-open-file"),
    NormalizedHead::new("open"),
];

/// The directions that can find a file already there.
///
/// `:probe` opens nothing and `:input` never creates or truncates, so neither
/// has an `:if-exists` question to answer.
const WRITING_DIRECTIONS: [&str; 2] = [":output", ":io"];

/// One output stream opened without an existing-file policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingIfExists {
    pub span: ByteSpan,
    /// `with-open-file` or `open`, for the message.
    pub operator: String,
    /// The literal direction that made this a writing open.
    pub direction: String,
}

/// The list carrying a form's keyword options, and the index they start at.
///
/// `open` takes them directly; `with-open-file` puts them in the binding form
/// after the variable and the designator. Both then step in pairs from index 2.
fn option_list<'a>(view: &'a ExpressionView, head: &str) -> Option<&'a ExpressionView> {
    if symbol_is(head, "with-open-file") {
        view.children.get(1)
    } else {
        Some(view)
    }
}

/// Reads one open-a-file form and reports the missing `:if-exists`.
#[must_use]
pub fn examine(view: &ExpressionView) -> Option<MissingIfExists> {
    let head = list_head(view)?;
    if !symbol_is(head, "with-open-file") && !symbol_is(head, "open") {
        return None;
    }
    let options = option_list(view, head)?;

    let direction = keyword_value(options, 2, ":direction")?;
    let writing = WRITING_DIRECTIONS
        .iter()
        .copied()
        .find(|candidate| is_keyword(direction, candidate))?;

    if has_keyword(options, 2, ":if-exists") {
        return None;
    }

    Some(MissingIfExists {
        span: view.span,
        operator: head.to_ascii_lowercase(),
        direction: writing.to_owned(),
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
                "this {} opens the file with :direction {} and no :if-exists, so it signals \
                 file-error the moment the file already exists; say :supersede, :append, \
                 :overwrite, or :error explicitly",
                found.operator, found.direction
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

    fn found(input: &str) -> Option<(String, String)> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view).map(|item| (item.operator, item.direction))
    }

    #[test]
    fn flags_a_with_open_file_output_with_no_policy() {
        assert_eq!(
            found("(with-open-file (s path :direction :output) (write-string x s))"),
            Some(("with-open-file".to_owned(), ":output".to_owned()))
        );
    }

    #[test]
    fn flags_a_bare_open_for_output() {
        assert_eq!(
            found("(open path :direction :output)"),
            Some(("open".to_owned(), ":output".to_owned()))
        );
    }

    #[test]
    fn flags_an_io_direction_too() {
        assert_eq!(
            found("(with-open-file (s path :direction :io) (use s))"),
            Some(("with-open-file".to_owned(), ":io".to_owned()))
        );
    }

    #[test]
    fn flags_it_past_other_options() {
        assert_eq!(
            found("(with-open-file (s path :element-type 'character :direction :output) (use s))"),
            Some(("with-open-file".to_owned(), ":output".to_owned()))
        );
    }

    #[test]
    fn accepts_every_explicit_policy() {
        for policy in [":supersede", ":append", ":overwrite", ":rename", ":error"] {
            let input =
                format!("(with-open-file (s path :direction :output :if-exists {policy}) (use s))");
            assert_eq!(
                found(&input),
                None,
                "{policy} is a decision, not an omission"
            );
        }
    }

    #[test]
    fn accepts_an_if_exists_written_before_the_direction() {
        assert_eq!(
            found("(open path :if-exists :supersede :direction :output)"),
            None
        );
    }

    #[test]
    fn does_not_flag_an_input_stream() {
        assert_eq!(found("(with-open-file (s path) (read s))"), None);
        assert_eq!(
            found("(with-open-file (s path :direction :input) (read s))"),
            None
        );
        assert_eq!(found("(open path :direction :probe)"), None);
    }

    #[test]
    fn does_not_flag_a_computed_direction() {
        assert_eq!(found("(open path :direction mode)"), None);
        assert_eq!(
            found("(open path :direction (if writing :output :input))"),
            None
        );
    }

    /// `:direction` appearing as a *value* must not arm the rule.
    ///
    /// The trailing `:output` is load-bearing: without it, an implementation
    /// that scanned position by position instead of in pairs would also answer
    /// `None`, and the fixture would not tell the two apart.
    #[test]
    fn does_not_read_a_value_as_a_direction() {
        assert_eq!(
            found("(open path :external-format :direction :output)"),
            None
        );
    }

    #[test]
    fn a_valueless_if_exists_is_present_not_missing() {
        // Malformed, but the omission this rule is named for is not what is
        // wrong with it; reporting it here would be a second complaint about
        // the same form.
        assert_eq!(found("(open path :direction :output :if-exists)"), None);
    }

    #[test]
    fn reads_the_head_and_keywords_case_insensitively() {
        assert_eq!(
            found("(WITH-OPEN-FILE (S PATH :DIRECTION :OUTPUT) (USE S))"),
            Some(("with-open-file".to_owned(), ":output".to_owned()))
        );
    }

    #[test]
    fn does_not_flag_a_malformed_form_with_no_binding() {
        assert_eq!(found("(with-open-file)"), None);
        assert_eq!(found("(open)"), None);
    }
}
