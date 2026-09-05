//! `char-op-string` seen through the shipped registry.
//!
//! This test drives `collect_lint_findings`, which runs every registered rule
//! and then filters for this one, so it cannot live in the rule's own package:
//! that package deliberately has no registry (section 4.2). Seven other rules
//! test themselves the same way and their tests land here too.

#[cfg(test)]
mod tests {
    use crate::lint::report::collect_lint_findings;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;
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
    fn flags_a_declared_non_character() {
        assert_eq!(findings("(char-code (the integer x))").len(), 1);
    }

    #[test]
    fn flags_a_nonliteral_string_expression() {
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

#[cfg(test)]
mod eq_char_comparison_tests {
    use crate::lint::report::collect_lint_findings;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;
    use paredit_feature_lint_numeric::eq_char_comparison::rule::META;
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
            findings("(eq c #\\a)"),
            ["eq compares against character literal #\\a; use eql or char="]
        );
    }

    #[test]
    fn a_literal_still_wins_over_the_inferred_type() {
        assert_eq!(
            findings("(eq (char s 0) #\\a)"),
            ["eq compares against character literal #\\a; use eql or char="]
        );
    }

    #[test]
    fn now_flags_a_standard_function_with_a_character_return_type() {
        assert_eq!(
            findings("(eq (char s 0) c)"),
            ["eq compares against an argument of inferred type character; use eql or char="]
        );
        assert_eq!(findings("(eq (code-char n) c)").len(), 1);
    }

    #[test]
    fn now_flags_a_character_reached_through_a_binding() {
        assert_eq!(findings("(let ((c #\\a)) (eq c d))").len(), 1);
    }

    #[test]
    fn now_flags_a_declared_character() {
        assert_eq!(findings("(eq (the character x) y)").len(), 1);
    }

    #[test]
    fn an_argument_of_unknown_type_is_not_flagged() {
        assert!(findings("(eq x y)").is_empty());
        assert!(findings("(eq (compute x) y)").is_empty());
        assert!(findings("(eq (aref s 0) c)").is_empty());
    }

    #[test]
    fn an_argument_of_a_settled_non_character_type_is_not_flagged() {
        assert!(findings("(eq (length xs) n)").is_empty());
        assert!(findings("(eq (string-upcase s) x)").is_empty());
        assert!(findings("(eq (consp x) y)").is_empty());
    }

    #[test]
    fn a_non_common_lisp_dialect_is_not_flagged() {
        // The type layer is CLHS-only, and so is the report's dialect gate.
        assert!(findings_for("(eq (char s 0) c)", Dialect::Clojure).is_empty());
        assert!(findings_for("(eq (char s 0) c)", Dialect::EmacsLisp).is_empty());
    }

    #[test]
    fn a_quoted_form_the_type_context_cannot_settle_is_not_flagged() {
        // `'(char s 0)` denotes a list, not the call's result; the layer is
        // opaque to quoted forms and stays silent rather than guessing.
        assert!(findings("(eq '(char s 0) c)").is_empty());
    }
}

#[cfg(test)]
mod parse_integer_default_radix_tests {
    use crate::lint::report::collect_lint_findings;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;
    use std::path::Path;

    fn hits(input: &str) -> usize {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        collect_lint_findings(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect")
            .into_iter()
            .filter(|finding| finding.rule == "parse-integer-default-radix")
            .count()
    }

    #[test]
    fn still_flags_the_literal_spelling() {
        assert_eq!(hits("(parse-integer s :radix 10)"), 1);
    }

    #[test]
    fn now_flags_ten_written_in_another_radix() {
        assert_eq!(hits("(parse-integer s :radix #xA)"), 1);
    }

    #[test]
    fn now_flags_ten_reached_through_a_binding() {
        assert_eq!(hits("(let ((r 10)) (parse-integer s :radix r))"), 1);
    }

    #[test]
    fn a_meaningful_radix_is_still_left_alone() {
        assert_eq!(hits("(parse-integer s :radix 16)"), 0);
        assert_eq!(hits("(parse-integer s :radix #x10)"), 0);
    }

    #[test]
    fn a_radix_computed_at_run_time_is_left_alone() {
        assert_eq!(hits("(parse-integer s :radix (current-radix))"), 0);
        assert_eq!(
            hits("(let ((r 10)) (setq r 16) (parse-integer s :radix r))"),
            0
        );
    }
}

#[cfg(test)]
mod constant_if_test_tests {
    use crate::lint::report::collect_lint_findings;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;
    use std::path::Path;

    fn messages(input: &str) -> Vec<String> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        collect_lint_findings(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect")
            .into_iter()
            .filter(|finding| finding.rule == "constant-if-test")
            .map(|finding| finding.message)
            .collect()
    }

    #[test]
    fn still_flags_the_literal_spellings() {
        assert_eq!(messages("(if t 1 2)").len(), 1);
        assert_eq!(messages("(if nil 1 2)").len(), 1);
    }

    #[test]
    fn now_flags_a_test_the_value_layer_settles() {
        assert_eq!(messages("(if (= 1 1) 1 2)").len(), 1);
        assert_eq!(messages("(if (zerop 0) 1 2)").len(), 1);
        assert_eq!(messages("(if (< 2 1) 1 2)").len(), 1);
    }

    #[test]
    fn now_flags_a_test_reached_through_a_binding() {
        assert_eq!(messages("(let ((flag t)) (if flag 1 2))").len(), 1);
    }

    #[test]
    fn the_message_still_names_the_settled_value() {
        // The wording frame is unchanged; a folded test reports the constant
        // it settles to rather than the source text it was written as.
        let message = messages("(if (= 1 1) 1 2)").pop().expect("one finding");
        assert!(message.contains('t'), "{message}");
    }

    #[test]
    fn a_genuine_run_time_condition_is_left_alone() {
        assert!(messages("(if x 1 2)").is_empty());
        assert!(messages("(if (ready-p) 1 2)").is_empty());
        assert!(messages("(let ((flag t)) (setq flag nil) (if flag 1 2))").is_empty());
    }

    #[test]
    fn a_truthy_non_boolean_constant_still_settles_the_branch() {
        // Every value but `nil` is true in Common Lisp, so `5` decides the
        // branch just as `t` does.
        assert_eq!(messages("(if 5 1 2)").len(), 1);
    }
}

#[cfg(test)]
mod redundant_the_tests {
    use crate::lint::report::collect_lint_findings;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;
    use paredit_feature_lint_form_shape::redundant_the::rule::META;
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
    fn still_flags_the_vacuous_t_with_its_own_message() {
        assert_eq!(
            findings("(the t x)"),
            ["(the t form) is a vacuous type declaration; it is just form"]
        );
    }

    #[test]
    fn now_flags_a_standard_function_that_already_returns_the_asserted_type() {
        assert_eq!(
            findings("(the integer (length xs))"),
            ["(the integer form) asserts a type the form already has; it is just form"]
        );
    }

    #[test]
    fn now_flags_an_assertion_a_supertype_of_what_the_form_returns() {
        // `length` returns an integer, and every integer is a number, so
        // asserting `number` adds nothing either. The lattice is what makes
        // the weaker claim redundant too.
        assert_eq!(findings("(the number (length xs))").len(), 1);
    }

    #[test]
    fn now_flags_a_type_reached_through_a_binding() {
        assert_eq!(findings("(let ((n 5)) (the integer n))").len(), 1);
    }

    #[test]
    fn the_reasoning_does_not_run_through_the_assertion_being_judged() {
        // The trap this rule could fall into: if the type layer recorded a
        // `the` form's assertion against the *inner* form, every `the` would
        // look redundant. It records it against the `the` form itself, so an
        // inner form the layer cannot settle stays unsettled.
        assert!(findings("(the integer x)").is_empty());
        assert!(findings("(the integer (compute x))").is_empty());
    }

    #[test]
    fn a_form_of_unknown_type_is_not_flagged() {
        // `Unknown` maps to silence.
        assert!(findings("(the string (car xs))").is_empty());
        assert!(findings("(the list (gethash k h))").is_empty());
    }

    #[test]
    fn an_assertion_that_narrows_is_not_flagged() {
        // `length` returns an integer; `float` is a *different* type and the
        // assertion is a real (if wrong) claim, not a redundant one.
        assert!(findings("(the float (length xs))").is_empty());
        // `character` is narrower than nothing the form proves.
        assert!(findings("(the character (car xs))").is_empty());
    }

    #[test]
    fn an_unmodelled_or_compound_specifier_is_left_alone() {
        // A `the` this layer cannot read is not one it may delete.
        assert!(findings("(the fixnum (length xs))").is_empty());
        assert!(findings("(the (integer 0 9) (length xs))").is_empty());
        assert!(findings("(the my-struct (length xs))").is_empty());
    }

    #[test]
    fn a_non_common_lisp_dialect_is_not_flagged() {
        assert!(findings_for("(the integer (length xs))", Dialect::Clojure).is_empty());
        assert!(findings_for("(the integer (length xs))", Dialect::EmacsLisp).is_empty());
    }
}

#[cfg(test)]
mod eq_number_comparison_tests {
    use crate::lint::report::collect_lint_findings;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;
    use paredit_feature_lint_numeric::eq_number_comparison::rule::META;
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
            findings("(eq n 5)"),
            ["eq compares against number literal 5"]
        );
    }

    #[test]
    fn a_literal_still_wins_over_the_inferred_type() {
        // `(length xs)` is an integer too, but the reader can see the `0`, so
        // the message keeps naming it.
        assert_eq!(
            findings("(eq (length xs) 0)"),
            ["eq compares against number literal 0"]
        );
    }

    #[test]
    fn now_flags_a_standard_function_with_a_numeric_return_type() {
        assert_eq!(
            findings("(eq (length xs) n)"),
            ["eq compares against an argument of inferred type number"]
        );
    }

    #[test]
    fn now_flags_a_number_reached_through_a_binding() {
        assert_eq!(findings("(let ((n 5)) (eq n m))").len(), 1);
    }

    #[test]
    fn now_flags_a_declared_number() {
        assert_eq!(findings("(eq (the integer x) y)").len(), 1);
    }

    #[test]
    fn an_argument_of_unknown_type_is_not_flagged() {
        assert!(findings("(eq x y)").is_empty());
        assert!(findings("(eq (compute x) y)").is_empty());
        assert!(findings("(eq (car xs) (cdr ys))").is_empty());
    }

    #[test]
    fn an_argument_of_a_settled_non_numeric_type_is_not_flagged() {
        // The type context answers here; the answer just is not `number`.
        assert!(findings("(eq (symbol-name s) x)").is_empty());
        assert!(findings("(eq (char s 0) c)").is_empty());
        assert!(findings("(eq (consp x) y)").is_empty());
    }

    #[test]
    fn a_non_common_lisp_dialect_is_not_flagged() {
        // The type layer is CLHS-only, and so is the report's dialect gate.
        assert!(findings_for("(eq (count xs) n)", Dialect::Clojure).is_empty());
        assert!(findings_for("(eq (length xs) n)", Dialect::EmacsLisp).is_empty());
    }

    #[test]
    fn a_quoted_form_the_type_context_cannot_settle_is_not_flagged() {
        // `'(length xs)` denotes a list, not the call's result; the layer is
        // opaque to quoted forms and stays silent rather than guessing.
        assert!(findings("(eq '(length xs) n)").is_empty());
    }
}

#[cfg(test)]
mod zero_divisor_tests {
    use crate::lint::policy::RuleSelection;
    use crate::lint::report::collect_lint_findings;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;
    use paredit_feature_lint_numeric::zero_divisor::rule::META;
    use std::path::Path;

    fn findings(input: &str) -> Vec<String> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        collect_lint_findings(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect")
            .into_iter()
            .filter(|finding| finding.rule == "zero-divisor")
            .map(|finding| finding.message)
            .collect()
    }

    #[test]
    fn still_flags_the_literal_spelling() {
        assert_eq!(findings("(/ x 0)").len(), 1);
        assert_eq!(findings("(mod x 0)").len(), 1);
    }

    #[test]
    fn now_flags_a_zero_reached_through_a_binding() {
        // The case the value layer exists for: the reader sees `z`, not `0`.
        assert_eq!(findings("(let ((z 0)) (/ x z))").len(), 1);
    }

    #[test]
    fn now_flags_a_zero_reached_through_folded_arithmetic() {
        assert_eq!(findings("(/ x (- 1 1))").len(), 1);
    }

    #[test]
    fn now_flags_a_zero_written_in_another_radix() {
        assert_eq!(findings("(/ x #x0)").len(), 1);
    }

    #[test]
    fn now_flags_a_zero_reached_through_a_file_constant() {
        assert_eq!(findings("(defconstant +none+ 0)(/ x +none+)").len(), 1);
    }

    #[test]
    fn a_binding_that_might_be_reassigned_is_not_flagged() {
        // Zero false positives is the whole contract: the value at the use
        // site is whatever the `setq` wrote.
        assert!(findings("(let ((z 0)) (setq z 1) (/ x z))").is_empty());
    }

    #[test]
    fn a_divisor_that_is_merely_unknown_is_not_flagged() {
        assert!(findings("(/ x y)").is_empty());
        assert!(findings("(/ x (read))").is_empty());
        assert!(findings("(let ((z (read))) (/ x z))").is_empty());
    }

    #[test]
    fn a_float_zero_is_still_left_alone() {
        // `0.0` is a different, implementation-defined story, as before.
        assert!(findings("(/ x 0.0)").is_empty());
    }

    #[test]
    fn a_zero_numerator_is_still_fine() {
        assert!(findings("(let ((z 0)) (/ z x))").is_empty());
    }

    #[test]
    fn the_rule_is_still_selectable_by_name() {
        assert!(RuleSelection::Only(&["zero-divisor"]).includes(META.name()));
        assert!(!RuleSelection::Only(&["if-not"]).includes(META.name()));
    }
}
