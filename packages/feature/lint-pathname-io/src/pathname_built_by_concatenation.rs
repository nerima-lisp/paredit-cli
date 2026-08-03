//! `pathname-built-by-concatenation`: a file designator spelled by gluing
//! strings together around a separator.
//!
//! `(open (concatenate 'string dir "/" name))` and
//! `(load (format nil "~a/~a" dir name))` both hand a filesystem operator a
//! namestring the program assembled from a separator it chose. Two things go
//! wrong with that, and neither is hypothetical:
//!
//! - **The separator is the host's, not the program's.** CLHS 19.1.1 makes a
//!   namestring's syntax implementation- and host-defined; `merge-pathnames`
//!   and `make-pathname` compose the structured components instead, and the
//!   host's own syntax is applied once, by the host.
//! - **The pieces are re-read as pathname syntax.** A component containing a
//!   `*`, a `?`, or a `[` becomes a *wild* pathname when the namestring is
//!   parsed (CLHS 19.2.2.2), and `open` on a wild pathname is required to
//!   signal an error (CLHS `open`, `file-error` on a wild designator). Under
//!   `make-pathname :name` the same string is a literal name.
//!
//! Only a call whose designator argument is a `concatenate`/`format` *with a
//! separator in a literal* is reported. A `(concatenate 'string base ".lisp")`
//! adds a type, not a directory, and reporting it would make the rule
//! unusable.
//!
//! Report-only: splitting a namestring into directory, name, and type means
//! deciding which piece is which, which the concatenation does not say and the
//! rule must not guess.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{list_head, symbol_in, symbol_is};

use crate::support::{FILE_OPERATOR_HEADS, file_designator, is_unevaluated_at, string_literal};

pub const META: RuleMeta = RuleMeta::new(
    "pathname-built-by-concatenation",
    RuleCategory::Portability,
    Severity::Warning,
    "a filesystem call whose designator is a string glued together around a path separator",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "A namestring assembled with `concatenate` or `format` picks a separator the program has \
         no business choosing, and the result is then re-parsed as the host's pathname syntax — so \
         a component containing `*` or `?` silently becomes a wild pathname and `open` signals a \
         file-error. `merge-pathnames` and `make-pathname` compose the components directly, and \
         the host applies its own syntax once.",
    )
    .with_example(
        "(open (concatenate 'string dir \"/\" name))",
        "(open (merge-pathnames (make-pathname :name name) dir))",
    )
    .with_caveat(
        "Only a concatenation that contributes a separator is reported. \
         `(concatenate 'string base \".lisp\")` adds a type, not a directory, and is never \
         reported.",
    ),
);

/// The `result-type` arguments to `concatenate` that produce a namestring.
///
/// `(concatenate 'list …)` fed to `open` is a different defect entirely, and
/// naming the string types keeps this rule about the one it is named for.
const STRING_TYPES: [&str; 4] = [
    "string",
    "simple-string",
    "base-string",
    "simple-base-string",
];

/// How the namestring was assembled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Construction {
    /// `(concatenate 'string … "/" …)`.
    Concatenate,
    /// `(format nil "~a/~a" …)`.
    Format,
}

impl Construction {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Concatenate => "concatenate",
            Self::Format => "format",
        }
    }
}

/// One hand-assembled file designator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConcatenatedPathname {
    pub span: ByteSpan,
    /// The filesystem operator that was given it.
    pub operator: String,
    pub construction: Construction,
}

/// Whether a string literal contributes a directory separator.
///
/// Both spellings, because a program that glues `"\\"` on a Windows host has
/// made the same mistake in the other direction.
#[must_use]
pub fn contributes_a_separator(literal: &str) -> bool {
    literal.contains('/') || literal.contains('\\')
}

/// The `result-type` of a `concatenate` call, as a bare symbol name.
///
/// `'string` reaches this as an atom carrying a `Quote` reader prefix, so the
/// symbol text is read past the prefix rather than off the raw atom text.
fn concatenate_result_type(call: &ExpressionView) -> Option<&str> {
    atom_symbol_text(call.children.get(1)?)
}

/// How, if at all, `designator` was assembled from parts around a separator.
#[must_use]
pub fn construction_of(designator: &ExpressionView) -> Option<Construction> {
    let head = list_head(designator)?;

    if symbol_is(head, "concatenate") {
        let result_type = concatenate_result_type(designator)?;
        if !symbol_in(result_type, &STRING_TYPES) {
            return None;
        }
        // The pieces are everything after the operator and the result type.
        let pieces = designator.children.get(2..)?;
        if !pieces
            .iter()
            .filter_map(string_literal)
            .any(contributes_a_separator)
        {
            return None;
        }
        if is_fully_determined_and_tame(pieces) {
            return None;
        }
        return Some(Construction::Concatenate);
    }

    if symbol_is(head, "format") {
        // Only `(format nil …)` returns the string; `(format stream …)` and
        // `(format t …)` return NIL, and a filesystem call given NIL is a
        // different complaint.
        if !atom_symbol_text(designator.children.get(1)?).is_some_and(|to| symbol_is(to, "nil")) {
            return None;
        }
        let control = string_literal(designator.children.get(2)?)?;
        return contributes_a_separator(control).then_some(Construction::Format);
    }

    None
}

/// Characters that make a namestring component *wild* when it is re-read.
///
/// CLHS 19.2.2.2. This is what turns a hand-assembled path into a defect rather
/// than merely a portability smell: `(pathname "/tmp/a*b.txt")` has a `:name` of
/// `#<PATTERN "a" :MULTI-CHAR-WILD "b">`, and `open` on it signals.
const WILD_CHARACTERS: [char; 3] = ['*', '?', '['];

/// Whether every piece is a string literal that cannot introduce a wildcard.
///
/// When that holds, the assembled namestring is known in full at read time and
/// contains no wild character, so the failure this rule exists to predict —
/// a component silently becoming a `:wild` pattern — cannot occur.
///
/// This narrowing came from an audit over SBCL's sources and 38 Quicklisp
/// systems, where the un-narrowed rule produced 9 findings and every one was of
/// this shape. Reporting them would be worse than useless, because the obvious
/// remedy is wrong: `(merge-pathnames "bar.lisp" "/tmp/foo")` silently *drops*
/// `foo`, since `merge-pathnames` reads the last segment as a name. Concatenation
/// is genuinely the simpler correct spelling here.
fn is_fully_determined_and_tame(pieces: &[ExpressionView]) -> bool {
    pieces
        .iter()
        .all(|piece| string_literal(piece).is_some_and(|text| !text.contains(WILD_CHARACTERS)))
}

/// Reads one filesystem call and reports the hand-assembled designator it was
/// given.
#[must_use]
pub fn examine(view: &ExpressionView) -> Option<ConcatenatedPathname> {
    let (operator, designator) = file_designator(view)?;
    construction_of(designator).map(|construction| ConcatenatedPathname {
        span: designator.span,
        operator: operator.to_owned(),
        construction,
    })
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&FILE_OPERATOR_HEADS)
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
                "this builds the designator for {} with {} around a path separator; \
                 the separator is the host's to choose, and a `*` or `?` in a part \
                 makes the result a wild pathname — use merge-pathnames/make-pathname",
                found.operator,
                found.construction.as_str()
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

    fn found(input: &str) -> Option<(String, Construction)> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view).map(|item| (item.operator, item.construction))
    }

    #[test]
    fn flags_a_concatenated_designator() {
        assert_eq!(
            found(r#"(open (concatenate 'string dir "/" name))"#),
            Some(("open".to_owned(), Construction::Concatenate))
        );
    }

    #[test]
    fn flags_a_formatted_designator() {
        assert_eq!(
            found(r#"(load (format nil "~a/~a" dir name))"#),
            Some(("load".to_owned(), Construction::Format))
        );
    }

    #[test]
    fn flags_a_separator_embedded_in_a_longer_literal() {
        assert_eq!(
            found(r#"(probe-file (concatenate 'string root "/etc/" name))"#),
            Some(("probe-file".to_owned(), Construction::Concatenate))
        );
    }

    #[test]
    fn flags_a_backslash_separator() {
        assert_eq!(
            found(r#"(open (format nil "~a\\~a" dir name))"#),
            Some(("open".to_owned(), Construction::Format))
        );
    }

    #[test]
    fn reads_the_designator_inside_a_with_open_file_binding() {
        assert_eq!(
            found(r#"(with-open-file (s (concatenate 'string d "/" n)) (read s))"#),
            Some(("with-open-file".to_owned(), Construction::Concatenate))
        );
    }

    #[test]
    fn flags_a_merge_pathnames_given_a_hand_built_namestring() {
        assert_eq!(
            found(r#"(merge-pathnames (format nil "~a/~a" a b) base)"#),
            Some(("merge-pathnames".to_owned(), Construction::Format))
        );
    }

    #[test]
    fn does_not_flag_a_concatenation_that_only_adds_a_type() {
        assert_eq!(found(r#"(load (concatenate 'string base ".lisp"))"#), None);
        assert_eq!(found(r#"(load (format nil "~a.fasl" base))"#), None);
    }

    #[test]
    fn does_not_flag_a_concatenation_with_no_literal_at_all() {
        assert_eq!(found("(load (concatenate 'string a b))"), None);
    }

    #[test]
    fn does_not_flag_an_all_literal_concatenation_with_no_wild_character() {
        // Every piece is a literal and none holds `*`, `?` or `[`, so the
        // assembled namestring is known in full at read time and cannot become
        // a `:wild` pattern — the failure this rule exists to predict.
        //
        // This narrowing came from an audit over SBCL's sources and 38
        // Quicklisp systems: the un-narrowed rule produced 9 findings and all 9
        // were this shape. Reporting them would be actively harmful, because
        // the remedy that looks obvious is wrong —
        // `(merge-pathnames "bar.lisp" "/tmp/foo")` is `#P"/tmp/bar.lisp"`,
        // silently dropping `foo`, since the last segment reads as a name.
        assert_eq!(
            found(r#"(load (concatenate 'string "/tmp/d" "/" "name.txt"))"#),
            None
        );
    }

    #[test]
    fn still_flags_an_all_literal_concatenation_that_holds_a_wild_character() {
        // Verified in SBCL 2.6.6: `(wild-pathname-p "/tmp/a*b.txt")` is T, and
        // `open` on it signals a `file-error` — `NO-NATIVE-NAMESTRING-ERROR`,
        // naming the `:NAME` component `#<PATTERN "a" :MULTI-CHAR-WILD "b">`.
        assert_eq!(
            found(r#"(load (concatenate 'string "/tmp/d" "/" "a*b.txt"))"#),
            Some(("load".to_owned(), Construction::Concatenate))
        );
    }

    #[test]
    fn still_flags_a_concatenation_whose_pieces_are_not_all_known() {
        // A variable can hold anything, wild characters included, so the
        // narrowing above must not extend to it.
        assert_eq!(
            found(r#"(load (concatenate 'string "/tmp/d" "/" name))"#),
            Some(("load".to_owned(), Construction::Concatenate))
        );
    }

    #[test]
    fn does_not_flag_a_non_string_concatenate() {
        assert_eq!(found(r#"(load (concatenate 'list a "/" b))"#), None);
    }

    #[test]
    fn does_not_flag_a_format_that_writes_to_a_stream() {
        // `(format t …)` returns NIL, so this is not a designator being built.
        assert_eq!(found(r#"(load (format t "~a/~a" a b))"#), None);
        assert_eq!(found(r#"(load (format s "~a/~a" a b))"#), None);
    }

    #[test]
    fn does_not_flag_a_plain_string_literal_designator() {
        // That is `unportable-pathname`'s finding, not this rule's.
        assert_eq!(found(r#"(load "data/in.txt")"#), None);
    }

    #[test]
    fn does_not_flag_a_non_filesystem_call() {
        assert_eq!(found(r#"(princ (concatenate 'string a "/" b))"#), None);
    }

    #[test]
    fn does_not_flag_a_computed_designator() {
        assert_eq!(found("(load (config-path))"), None);
        assert_eq!(found("(load *init-file*)"), None);
    }

    #[test]
    fn reads_the_head_case_insensitively() {
        assert_eq!(
            found(r#"(OPEN (CONCATENATE 'STRING dir "/" name))"#),
            Some(("open".to_owned(), Construction::Concatenate))
        );
    }

    #[test]
    fn separator_test_reads_both_spellings() {
        assert!(contributes_a_separator("/"));
        assert!(contributes_a_separator("a/b"));
        assert!(contributes_a_separator("\\"));
        assert!(!contributes_a_separator(".lisp"));
        assert!(!contributes_a_separator(""));
    }
}
