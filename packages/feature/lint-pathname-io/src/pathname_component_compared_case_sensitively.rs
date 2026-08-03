//! `pathname-component-compared-case-sensitively`: testing a pathname
//! component with `string=` against a literal.
//!
//! `(string= (pathname-type p) "lisp")` is false for `foo.LISP`. A pathname
//! component keeps the case the namestring was written in, and `string=` is
//! case-sensitive, so the test answers a question about spelling when it was
//! meant to answer one about file type.
//!
//! Verified rather than assumed. On SBCL 2.6.0:
//!
//! ```text
//! (pathname-type #p"foo.LISP")                        => "LISP"
//! (pathname-type #p"foo.lisp")                        => "lisp"
//! (string= (pathname-type #p"foo.LISP") "lisp")       => NIL     <- the defect
//! (string-equal (pathname-type #p"foo.LISP") "lisp")  => T       <- the fix
//! ```
//!
//! CLHS 19.2.2.1.2 is why this is not merely an SBCL quirk: a component is
//! read and printed in the host's *customary case*, and 19.2.2.1.2.2 makes
//! `:case :common` invert it — so the same run gives
//! `(pathname-type #p"foo.lisp" :case :common)` => `"LISP"`. Neither spelling
//! is the one a lowercase literal can be relied on to match, in either
//! direction, which is why the rule fires whatever `:case` was asked for.
//!
//! What this rule is *not* about: a type error. `(pathname-type #p"foo")`
//! returns `NIL`, and `(string= nil "lisp")` returns `NIL` rather than
//! signalling — `NIL` is a valid string designator naming `"NIL"`. That was
//! checked; the defect is case and only case.
//!
//! Report-only. `string=` → `string-equal` looks mechanical, and this project
//! has already shipped one autofix that a local `flet` shadowing the operator
//! name turned into silent code deletion. A fix here would need that same
//! shadowing analysis to be safe, and a report costs the reader one edit.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::list_head;

use crate::support::{head_among, is_unevaluated_at, string_literal};

pub const META: RuleMeta = RuleMeta::new(
    "pathname-component-compared-case-sensitively",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a pathname component compared with string=/equal against a literal, which the host's case defeats",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "A pathname component keeps the case its namestring was written in, and CLHS 19.2.2.1.2 \
         leaves the host's customary case to the host. `string=` and `equal` are case-sensitive, \
         so a test against a lowercase literal silently fails for `FOO.LISP` — and `:case :common` \
         inverts the case rather than normalizing it, so asking for it does not help. \
         `string-equal` is the case-insensitive comparison.",
    )
    .with_example(
        "(string= (pathname-type p) \"lisp\")",
        "(string-equal (pathname-type p) \"lisp\")",
    )
    .with_caveat(
        "Only a comparison against a string *literal* is reported. Comparing two components, or a \
         component against a computed string, may well be deliberate.",
    ),
);

const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("string="), NormalizedHead::new("equal")];

/// The pathname readers that return a string whose case is the host's.
///
/// `pathname-directory` returns a *list* and `pathname-version` an integer or
/// keyword, so neither is compared with `string=` in the first place.
/// `pathname-host` is an implementation-defined object, not reliably a string.
const CASE_BEARING_READERS: [&str; 3] = ["pathname-type", "pathname-name", "pathname-device"];

/// One case-sensitive comparison of a pathname component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseSensitiveComparison {
    pub span: ByteSpan,
    /// The comparison operator used.
    pub operator: String,
    /// The pathname reader whose result is being compared.
    pub reader: String,
    /// The literal it is compared against.
    pub literal: String,
}

/// The pathname reader a form calls, if it calls one.
fn pathname_reader(view: &ExpressionView) -> Option<&'static str> {
    head_among(list_head(view)?, &CASE_BEARING_READERS)
}

/// The comparison operators this rule reads, matching [`HEADS`].
const COMPARISONS: [&str; 2] = ["string=", "equal"];

/// Reads one comparison and reports the pathname component it tests
/// case-sensitively.
///
/// Either operand may be the reader — `(string= "lisp" (pathname-type p))` is
/// the same defect written the other way round.
#[must_use]
pub fn examine(view: &ExpressionView) -> Option<CaseSensitiveComparison> {
    let head = list_head(view)?;
    head_among(head, &COMPARISONS)?;
    let left = view.children.get(1)?;
    let right = view.children.get(2)?;

    let (reader_form, literal_form) = match pathname_reader(left) {
        Some(_) => (left, right),
        None => (right, left),
    };
    let reader = pathname_reader(reader_form)?;
    let literal = string_literal(literal_form)?;

    Some(CaseSensitiveComparison {
        span: view.span,
        operator: head.to_ascii_lowercase(),
        reader: reader.to_owned(),
        literal: literal.to_owned(),
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
                "{} compares {} against \"{}\" case-sensitively, so a component the host spelled \
                 in another case does not match; use string-equal",
                found.operator, found.reader, found.literal
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

    fn found(input: &str) -> Option<(String, String, String)> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view).map(|item| (item.operator, item.reader, item.literal))
    }

    #[test]
    fn flags_the_canonical_extension_test() {
        assert_eq!(
            found(r#"(string= (pathname-type p) "lisp")"#),
            Some((
                "string=".to_owned(),
                "pathname-type".to_owned(),
                "lisp".to_owned()
            ))
        );
    }

    #[test]
    fn flags_the_operands_in_either_order() {
        assert_eq!(
            found(r#"(string= "lisp" (pathname-type p))"#),
            Some((
                "string=".to_owned(),
                "pathname-type".to_owned(),
                "lisp".to_owned()
            ))
        );
    }

    #[test]
    fn flags_equal_as_well() {
        assert_eq!(
            found(r#"(equal (pathname-name p) "readme")"#),
            Some((
                "equal".to_owned(),
                "pathname-name".to_owned(),
                "readme".to_owned()
            ))
        );
    }

    #[test]
    fn flags_every_case_bearing_reader() {
        assert!(found(r#"(string= (pathname-type p) "a")"#).is_some());
        assert!(found(r#"(string= (pathname-name p) "a")"#).is_some());
        assert!(found(r#"(string= (pathname-device p) "a")"#).is_some());
    }

    /// `:case :common` inverts the case rather than normalizing it, so it does
    /// not make the comparison safe and must not silence the rule.
    #[test]
    fn flags_it_even_with_an_explicit_case_argument() {
        assert!(found(r#"(string= (pathname-type p :case :common) "lisp")"#).is_some());
    }

    #[test]
    fn does_not_flag_the_case_insensitive_comparison() {
        assert_eq!(found(r#"(string-equal (pathname-type p) "lisp")"#), None);
        assert_eq!(found(r#"(equalp (pathname-type p) "lisp")"#), None);
    }

    #[test]
    fn does_not_flag_a_comparison_against_a_computed_string() {
        assert_eq!(found("(string= (pathname-type p) wanted)"), None);
        assert_eq!(found("(string= (pathname-type a) (pathname-type b))"), None);
    }

    #[test]
    fn does_not_flag_a_reader_that_returns_no_string() {
        // A list and an integer are not compared with `string=` by mistake in
        // the way a type is.
        assert_eq!(found(r#"(equal (pathname-directory p) "/tmp/")"#), None);
        assert_eq!(found(r#"(equal (pathname-version p) "1")"#), None);
    }

    #[test]
    fn does_not_flag_an_ordinary_string_comparison() {
        assert_eq!(found(r#"(string= (name-of x) "lisp")"#), None);
        assert_eq!(found(r#"(string= a "lisp")"#), None);
    }

    /// Two literals and no reader at all.
    ///
    /// This is the fixture that makes the "one operand must be a pathname
    /// reader" test live. The cases above have a non-literal operand, so
    /// dropping the reader requirement still leaves them declined by the
    /// *literal* requirement, and they cannot tell the two guards apart.
    /// Mutation testing is how that was found.
    #[test]
    fn does_not_flag_a_comparison_of_two_literals() {
        assert_eq!(found(r#"(string= "a" "lisp")"#), None);
        assert_eq!(found(r#"(equal "a" "b")"#), None);
    }

    #[test]
    fn does_not_flag_a_comparison_with_a_missing_operand() {
        assert_eq!(found("(string= (pathname-type p))"), None);
        assert_eq!(found("(string=)"), None);
    }

    #[test]
    fn reads_the_head_case_insensitively() {
        assert_eq!(
            found(r#"(STRING= (PATHNAME-TYPE p) "lisp")"#),
            Some((
                "string=".to_owned(),
                "pathname-type".to_owned(),
                "lisp".to_owned()
            ))
        );
    }
}
