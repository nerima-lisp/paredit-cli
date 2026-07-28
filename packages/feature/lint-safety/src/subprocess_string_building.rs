//! `subprocess-string-building`: a command line assembled from strings.
//!
//! `(run-program (format nil "grep ~a file" pattern))` puts whatever `pattern`
//! holds into a shell command. A pattern containing `; rm -rf ~` is then a
//! second command. Passing the program and its arguments as a *list* — which
//! every `run-program` variant accepts — removes the shell from the path
//! entirely, and with it the question.
//!
//! The rule fires on the string-building operators specifically (`format`,
//! `concatenate`, `+`-style appends) rather than on any computed argument,
//! because that is what distinguishes "a command line was assembled here" from
//! "a command name was looked up in a variable".
//!
//! Report-only. The rewrite is a different calling convention, not a
//! substitution.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_in};

pub const META: RuleMeta = RuleMeta::new(
    "subprocess-string-building",
    RuleCategory::Security,
    Severity::Error,
    "a subprocess launched with a command line built by format or concatenate, which a value can escape",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "A command line assembled from strings is parsed by a shell, so any interpolated value \
         that contains a shell metacharacter becomes syntax rather than data. Passing the program \
         and its arguments as separate list elements removes the shell from the path.",
    )
    .with_example(
        "(run-program (format nil \"grep ~a f\" pattern))",
        "(run-program \"grep\" (list pattern \"f\"))",
    )
    .with_caveat(
        "Only the string-building operators are reported. `(run-program *tool* args)` passes a \
         value through without assembling anything and is left alone.",
    ),
);

const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("run-program"),
    NormalizedHead::new("run-shell-command"),
    NormalizedHead::new("shell-command"),
    NormalizedHead::new("launch-program"),
];

/// Operators that assemble a string from parts.
const STRING_BUILDERS: [&str; 5] = [
    "format",
    "concatenate",
    "with-output-to-string",
    "string+",
    "uiop:strcat",
];

/// One assembled command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssembledCommand {
    pub span: ByteSpan,
    pub builder: String,
}

/// Reads one subprocess call and reports the assembled command line it is
/// given.
///
/// Only the first argument is read: every variant of `run-program` in the wild
/// takes the program (or the whole command line) there, and the later arguments
/// are keyword options whose strings are not commands.
#[must_use]
pub fn examine(view: &ExpressionView) -> Option<AssembledCommand> {
    let head = list_head(view)?;
    if !symbol_in(
        head,
        &[
            "run-program",
            "run-shell-command",
            "shell-command",
            "launch-program",
        ],
    ) {
        return None;
    }
    let command = view.children.get(1)?;
    let builder = list_head(command)?;
    if !symbol_in(builder, &STRING_BUILDERS) {
        return None;
    }
    // `(format nil …)` builds a string; `(format t …)` prints and returns nil,
    // which is not a command line and not this rule's finding.
    if symbol_in(builder, &["format"])
        && !command
            .children
            .get(1)
            .and_then(atom_text)
            .is_some_and(|destination| destination.eq_ignore_ascii_case("nil"))
    {
        return None;
    }
    Some(AssembledCommand {
        span: view.span,
        builder: paredit_core_syntax::view_query::unqualified(builder).to_ascii_lowercase(),
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
                    "this command line is assembled by {}, so an interpolated value containing a \
                     shell metacharacter becomes syntax; pass the program and its arguments as a \
                     list instead",
                    found.builder
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

    fn builder(input: &str) -> Option<String> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view).map(|found| found.builder)
    }

    #[test]
    fn flags_a_format_built_command_line() {
        assert_eq!(
            builder(r#"(run-program (format nil "grep ~a f" pattern))"#),
            Some("format".to_owned())
        );
    }

    #[test]
    fn flags_a_concatenated_command_line() {
        assert_eq!(
            builder(r#"(run-program (concatenate 'string "grep " pattern))"#),
            Some("concatenate".to_owned())
        );
    }

    #[test]
    fn flags_every_launcher_spelling() {
        assert_eq!(
            builder(r#"(shell-command (format nil "~a" cmd))"#),
            Some("format".to_owned())
        );
        assert_eq!(
            builder(r#"(launch-program (format nil "~a" cmd))"#),
            Some("format".to_owned())
        );
    }

    #[test]
    fn does_not_flag_a_literal_command() {
        assert_eq!(builder(r#"(run-program "ls" (list "-l"))"#), None);
    }

    #[test]
    fn does_not_flag_a_command_passed_through_a_variable() {
        assert_eq!(builder("(run-program *tool* args)"), None);
    }

    #[test]
    fn does_not_flag_a_format_that_prints_rather_than_builds() {
        // `(format t …)` returns nil; it is not a command line.
        assert_eq!(builder(r#"(run-program (format t "~a" cmd))"#), None);
    }

    #[test]
    fn does_not_flag_a_call_with_no_command() {
        assert_eq!(builder("(run-program)"), None);
    }

    #[test]
    fn reads_the_package_qualified_launcher() {
        assert_eq!(
            builder(r#"(uiop:run-program (format nil "~a" cmd))"#),
            Some("format".to_owned())
        );
    }
}
