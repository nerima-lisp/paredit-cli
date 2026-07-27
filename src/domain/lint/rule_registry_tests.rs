//! `char-op-string` seen through the shipped registry.
//!
//! This test drives `collect_lint_findings`, which runs every registered rule
//! and then filters for this one, so it cannot live in the rule's own package:
//! that package deliberately has no registry (section 4.2). Seven other rules
//! test themselves the same way and their tests land here too.

#[cfg(test)]
mod tests {
    use crate::domain::dialect::Dialect;
    use crate::domain::lint_report::collect_lint_findings;
    use crate::domain::sexpr::SyntaxTree;
    use paredit_feature_lint_string_char::char_op_string::rule::META;
    use std::path::Path;

    fn findings_for(input: &str, dialect: Dialect) -> Vec<String> {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse");
        collect_lint_findings(Path::new("test.lisp"), dialect, &tree)
            .expect("collect")
            .into_iter()
            .filter(|finding| finding.rule == META.name().as_str())
            .map(|finding| finding.message)
            .collect()
    }

    fn findings(input: &str) -> Vec<String> {
        findings_for(input, Dialect::CommonLisp)
    }

    #[test]
    fn still_flags_the_literal_spelling_with_its_own_message() {
        assert_eq!(
            findings(r#"(char= "a" c)"#),
            [r#"char= is given string literal "a"; it requires a character (type error)"#]
        );
    }

    #[test]
    fn a_literal_still_wins_over_the_inferred_type() {
        // `(length xs)` is not a character either, but the reader can see the
        // `"a"`, so the message keeps naming it.
        assert_eq!(
            findings(r#"(char= (length xs) "a")"#),
            [r#"char= is given string literal "a"; it requires a character (type error)"#]
        );
    }

    #[test]
    fn now_flags_a_standard_function_whose_return_type_is_not_a_character() {
        assert_eq!(
            findings("(char= (length xs) c)"),
            [
                "char= is given an argument of an inferred non-character type; it requires a character (type error)"
            ]
        );
    }

    #[test]
    fn now_flags_a_non_character_reached_through_a_binding() {
        assert_eq!(findings("(let ((n 5)) (char= n c))").len(), 1);
    }

    #[test]
    fn now_flags_a_declared_non_character() {
        assert_eq!(findings("(char-code (the integer x))").len(), 1);
    }

    #[test]
    fn now_flags_a_string_that_is_not_written_as_a_literal() {
        // The case the spelling test was always reaching for, finally stated
        // as the type it is.
        assert_eq!(findings("(char-upcase (symbol-name s))").len(), 1);
    }

    #[test]
    fn an_argument_of_unknown_type_is_not_flagged() {
        // `Unknown` must map to silence: this is the whole discipline the
        // type layer is worth having.
        assert!(findings("(char= a b)").is_empty());
        assert!(findings("(char= (compute x) c)").is_empty());
        assert!(findings("(char-code (car xs))").is_empty());
    }

    #[test]
    fn an_argument_that_really_is_a_character_is_not_flagged() {
        // The type context answers here; the answer is `character`.
        assert!(findings("(char= (char-upcase c) d)").is_empty());
        assert!(findings("(char= (code-char n) d)").is_empty());
        assert!(findings(r"(char= #\a c)").is_empty());
    }

    #[test]
    fn a_non_common_lisp_dialect_is_not_flagged() {
        // The type layer is CLHS-only, and so is the report's dialect gate.
        assert!(findings_for("(char= (length xs) c)", Dialect::Clojure).is_empty());
        assert!(findings_for("(char= (length xs) c)", Dialect::EmacsLisp).is_empty());
    }
}
