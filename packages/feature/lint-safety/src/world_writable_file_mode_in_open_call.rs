//! `world-writable-file-mode-in-open-call`: a permission argument that lets
//! anyone write the file.
//!
//! `(sb-posix:chmod path #o666)` and `(sb-posix:open path flags #o777)` grant
//! write permission to every user on the host. Whatever the file is — a cache, a
//! lock, a log, a script the program later loads — anyone can now replace its
//! contents, and the program will read what they wrote. `#o644` and `#o600`
//! express the same intent without the last part.
//!
//! The rule reads octal literals only, and reports one whose *other* bit is set:
//! `mode & #o002`. That makes `#o755` and `#o644` — world-*readable*, which is
//! ordinary and usually correct — silent, and `#o666`, `#o777`, `#o622` loud.
//!
//! The sticky bit is an explicit exception. `#o1777` is the mode `/tmp` itself
//! has: world-writable *plus* the restriction that only a file's owner may
//! delete or rename it, which is precisely the mitigation for a shared
//! directory. A program that spells `#o1777` has thought about this, so a mode
//! with `#o1000` set is not reported.
//!
//! Limits, by design — both of them false negatives, taken on purpose:
//!
//! - A mode that is not an octal literal is not read. `(chmod path mode)` and
//!   `(chmod path 438)` are invisible; a decimal permission is rare enough, and
//!   guessing at which integers are modes would report array indices.
//! - The rule does not check *where* in the call the literal sits. There is no
//!   other plausible meaning for `#o666` inside an `open` or `chmod` call, and
//!   modelling each library's argument order (`sb-posix`, `osicat`, `iolib`,
//!   `uiop`) would be four models that each go stale.
//!
//! Report-only. What the mode should be depends on who is supposed to read the
//! file.
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

use crate::support::is_unevaluated_at;

pub const META: RuleMeta = RuleMeta::new(
    "world-writable-file-mode-in-open-call",
    RuleCategory::Security,
    Severity::Error,
    "an explicit octal permission argument that grants write access to every user on the host",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "A world-writable file can be replaced by any user on the host, so anything the program \
         later reads back from it is attacker-controlled. Granting write to the owner, or to the \
         owner's group, expresses the same intent without that.",
    )
    .with_example("(sb-posix:chmod path #o666)", "(sb-posix:chmod path #o644)")
    .with_caveat(
        "Only octal literals are read, and only the world-write bit: `#o755` and `#o644` are not \
         reported. `#o1777` is not either — the sticky bit is the deliberate mitigation for a \
         shared directory, so a mode that sets it has already been thought about.",
    ),
);

const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("open"),
    NormalizedHead::new("with-open-file"),
    NormalizedHead::new("chmod"),
];

/// The permission bit for "any user".
const WORLD_WRITE: u32 = 0o002;

/// Only the owner may unlink or rename, however writable the directory is.
const STICKY: u32 = 0o1000;

/// One world-writable mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldWritableMode {
    pub span: ByteSpan,
    pub mode: String,
}

/// The value of an `#o…` literal.
///
/// `#O777` as well as `#o777`: the reader is case-insensitive about the radix
/// character, and a rule that was not would silently miss every file written in
/// the other style.
#[must_use]
pub fn octal_value(text: &str) -> Option<u32> {
    let digits = text
        .strip_prefix("#o")
        .or_else(|| text.strip_prefix("#O"))?;
    if digits.is_empty() {
        return None;
    }
    u32::from_str_radix(digits, 8).ok()
}

/// Whether `mode` grants write to every user without the sticky-bit
/// restriction.
#[must_use]
pub const fn is_world_writable(mode: u32) -> bool {
    mode & WORLD_WRITE != 0 && mode & STICKY == 0
}

/// Every node of `view` that could carry a mode: its own arguments, plus a
/// `with-open-file` binding list's options.
///
/// Deliberately one level deep and no further. A `#o666` nested inside some
/// computed expression is not "the permission argument of this call", and
/// descending would let an unrelated constant in a nested form be read as one.
fn mode_candidates(view: &ExpressionView) -> impl Iterator<Item = &ExpressionView> {
    view.children.iter().skip(1).flat_map(|child| {
        let nested: &[ExpressionView] =
            if child.kind == paredit_core_syntax::sexpr::ExpressionKind::List {
                &child.children
            } else {
                &[]
            };
        std::iter::once(child).chain(nested.iter())
    })
}

/// Reads one file-opening or permission call.
#[must_use]
pub fn examine(view: &ExpressionView, context: &RuleContext<'_>) -> Option<WorldWritableMode> {
    let head = list_head(view)?;
    if !symbol_in(head, &["open", "with-open-file", "chmod"]) {
        return None;
    }
    let found = mode_candidates(view).find_map(|candidate| {
        let text = atom_text(candidate)?;
        let mode = octal_value(text)?;
        is_world_writable(mode).then(|| (candidate.span, text.to_owned()))
    })?;

    // Asked last, and only once there is something to report.
    if is_unevaluated_at(context.tree(), view.span) {
        return None;
    }
    Some(WorldWritableMode {
        span: found.0,
        mode: found.1,
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
                    "{} grants write permission to every user on the host, so anyone can replace \
                     this file's contents; drop the world-write bit (#o644, #o664, #o600)",
                    found.mode
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

    fn modes(input: &str) -> Vec<String> {
        findings_for_heads(
            input,
            &["open", "with-open-file", "chmod"],
            |view, context| {
                examine(view, context)
                    .map(|found| found.mode)
                    .into_iter()
                    .collect::<Vec<_>>()
            },
        )
    }

    #[test]
    fn flags_a_world_writable_chmod() {
        assert_eq!(modes(r#"(chmod "/var/lib/app/db" #o666)"#), vec!["#o666"]);
        assert_eq!(modes("(sb-posix:chmod path #o777)"), vec!["#o777"]);
    }

    #[test]
    fn flags_a_mode_passed_to_open() {
        assert_eq!(modes("(open path :mode #o666)"), vec!["#o666"]);
    }

    #[test]
    fn flags_a_mode_in_a_with_open_file_binding() {
        assert_eq!(
            modes(r#"(with-open-file (s "out" :direction :output :mode #o622) s)"#),
            vec!["#o622"]
        );
    }

    #[test]
    fn reads_the_upper_case_radix_marker() {
        assert_eq!(modes("(chmod path #O666)"), vec!["#O666"]);
    }

    // --- near misses ------------------------------------------------------

    #[test]
    fn does_not_flag_a_mode_without_the_world_write_bit() {
        assert!(modes("(chmod path #o644)").is_empty());
        assert!(modes("(chmod path #o755)").is_empty());
        assert!(modes("(chmod path #o600)").is_empty());
        assert!(modes("(chmod path #o664)").is_empty());
    }

    #[test]
    fn does_not_flag_a_sticky_shared_directory() {
        // The mode `/tmp` itself has: world-writable, but only the owner may
        // unlink. That is the mitigation, not the defect.
        assert!(modes("(chmod path #o1777)").is_empty());
    }

    #[test]
    fn does_not_flag_a_computed_mode() {
        assert!(modes("(chmod path mode)").is_empty());
        assert!(modes("(chmod path (logior #o600 extra))").is_empty());
    }

    #[test]
    fn does_not_flag_a_decimal_argument() {
        // `666` must use digits that are *valid octal*, or the test passes for
        // the wrong reason: `438` contains an `8`, so a mutation that dropped
        // the `#o` requirement entirely would still fail to parse it and the
        // test would stay green. Decimal `666` is `#o1232`, and reading it as
        // octal would make it world-writable.
        assert!(modes("(chmod path 666)").is_empty());
        assert!(modes("(chmod path 438)").is_empty());
    }

    #[test]
    fn does_not_flag_an_ordinary_open() {
        assert!(modes(r#"(open "data.txt" :direction :output)"#).is_empty());
        assert!(modes(r#"(with-open-file (s "data.txt") (read-line s))"#).is_empty());
    }

    // --- quote and string contexts ---------------------------------------

    #[test]
    fn does_not_flag_a_quoted_form() {
        assert!(modes("'(chmod path #o666)").is_empty());
        assert!(modes("'(progn (chmod path #o666))").is_empty());
        assert!(modes("(quote (chmod path #o666))").is_empty());
        assert!(modes("`(chmod path #o666)").is_empty());
        assert!(modes("'(a ,(chmod path #o666))").is_empty());
    }

    #[test]
    fn flags_an_unquoted_form_inside_a_backquote() {
        assert_eq!(modes("`(a ,(chmod path #o666))"), vec!["#o666"]);
    }

    #[test]
    fn does_not_flag_text_inside_a_string_literal() {
        assert!(modes(r#"(log-it "(chmod path #o666)")"#).is_empty());
    }
}
