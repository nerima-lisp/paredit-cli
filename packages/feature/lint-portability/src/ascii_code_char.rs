//! `ascii-code-char`: naming a character by its numeric code.
//!
//! `(code-char 65)` is `#\A` only where the character set agrees with ASCII.
//! The standard does not require that — `char-code-limit` and the mapping are
//! implementation-defined — so the literal both assumes an encoding and hides
//! which character it means from anyone reading the code.
//!
//! Only the printable ASCII range is rewritten. Below 32 the character has no
//! printable spelling except a name (`#\Newline`, `#\Tab`) and getting those
//! names right is implementation-specific in its own way; at and above 127 the
//! encoding assumption is exactly what the rule is objecting to, so
//! substituting a literal would bake it in rather than remove it.
//!
//! Fixable within that range: `#\A` is the same character wherever `(code-char
//! 65)` was, and is one wherever it was not.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleFix, RuleMeta,
    Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, list_head};

pub const META: RuleMeta = RuleMeta::new(
    "ascii-code-char",
    RuleCategory::Portability,
    Severity::Warning,
    "a (code-char N) call with a literal code, which assumes ASCII and hides which character it means",
    Fixability::Fixable,
)
.with_explanation(
    RuleExplanation::new(
        "The standard does not fix the mapping from character codes to characters, so a numeric \
         literal names a different character on an implementation that does not use ASCII — and \
         names nothing recognisable to a reader on any implementation.",
    )
    .with_example("(code-char 65)", "#\\A")
    .with_caveat(
        "Only codes 32-126 are reported. Below that the character needs a name whose spelling is \
         itself implementation-specific; at 127 and above, substituting a literal would bake in \
         the very encoding assumption the rule objects to.",
    ),
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("code-char")];

/// The inclusive printable-ASCII range the rule will rewrite.
const PRINTABLE: std::ops::RangeInclusive<u32> = 32..=126;

/// One rewritable `(code-char N)` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsciiCodeChar {
    pub span: ByteSpan,
    pub code: u32,
    /// The character literal to write instead, `#\A` spelling included.
    pub literal: String,
}

/// The character literal for a printable code, with the two characters that
/// need their name rather than themselves.
///
/// `#\ ` (a literal space) and `#\\` are legal but read badly and, in the space
/// case, are easy to mistake for a truncated form. Their names are unambiguous
/// and universally supported.
fn literal_for(code: u32) -> Option<String> {
    if !PRINTABLE.contains(&code) {
        return None;
    }
    let character = char::from_u32(code)?;
    Some(match character {
        ' ' => "#\\Space".to_owned(),
        '\\' => "#\\\\".to_owned(),
        other => format!("#\\{other}"),
    })
}

/// Reads one `code-char` call and reports the literal it should be.
#[must_use]
pub fn examine(view: &ExpressionView) -> Option<AsciiCodeChar> {
    if !list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("code-char")) {
        return None;
    }
    if view.children.len() != 2 {
        return None;
    }
    let argument = &view.children[1];
    if !argument.reader_prefixes.is_empty() {
        return None;
    }
    // Only a plain decimal literal. `#x41` and `(+ 64 1)` are both `65`, and
    // neither is a spelling this rule claimed to read.
    let code: u32 = atom_text(argument)?.parse().ok()?;
    let literal = literal_for(code)?;
    Some(AsciiCodeChar {
        span: view.span,
        code,
        literal,
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
        let Some(found) = examine(view) else {
            return Ok(());
        };
        sink.report_fixed(
            found.span,
            format!(
                "(code-char {}) assumes ASCII and hides which character it means; {} is the same \
                 character everywhere",
                found.code, found.literal
            ),
            RuleFix::single(
                found.span,
                found.literal.clone(),
                format!("Write the character literal {}", found.literal),
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

    fn literal(input: &str) -> Option<String> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view).map(|found| found.literal)
    }

    #[test]
    fn rewrites_a_printable_ascii_code() {
        assert_eq!(literal("(code-char 65)"), Some("#\\A".to_owned()));
        assert_eq!(literal("(code-char 122)"), Some("#\\z".to_owned()));
        assert_eq!(literal("(code-char 48)"), Some("#\\0".to_owned()));
    }

    #[test]
    fn names_the_two_characters_that_read_badly_as_themselves() {
        assert_eq!(literal("(code-char 32)"), Some("#\\Space".to_owned()));
        assert_eq!(literal("(code-char 92)"), Some("#\\\\".to_owned()));
    }

    #[test]
    fn leaves_control_characters_alone() {
        assert_eq!(literal("(code-char 10)"), None);
        assert_eq!(literal("(code-char 0)"), None);
        assert_eq!(literal("(code-char 9)"), None);
    }

    #[test]
    fn leaves_everything_at_or_above_delete_alone() {
        assert_eq!(literal("(code-char 127)"), None);
        assert_eq!(literal("(code-char 233)"), None);
        assert_eq!(literal("(code-char 12354)"), None);
    }

    #[test]
    fn leaves_a_computed_code_alone() {
        assert_eq!(literal("(code-char n)"), None);
        assert_eq!(literal("(code-char (+ 64 1))"), None);
    }

    #[test]
    fn leaves_a_non_decimal_literal_alone() {
        // `#x41` is 65, but reading radix prefixes is not a claim this rule
        // made, and a rule must not rewrite a spelling it did not parse.
        assert_eq!(literal("(code-char #x41)"), None);
    }

    #[test]
    fn leaves_a_wrong_arity_call_alone() {
        assert_eq!(literal("(code-char)"), None);
        assert_eq!(literal("(code-char 65 66)"), None);
    }

    #[test]
    fn reads_the_head_case_insensitively() {
        assert_eq!(literal("(CODE-CHAR 65)"), Some("#\\A".to_owned()));
    }
}
