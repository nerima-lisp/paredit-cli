use super::*;
use crate::domain::dialect::Dialect;

#[test]
fn formats_short_atom_lists_inline() {
    let input = "(defun add (x y) (+ x y))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(defun add (x y)\n  (+ x y))\n"
    );
}

#[test]
fn formats_binding_forms_with_aligned_bindings() {
    let input = "(let ((x 1) (y (+ x 2))) (list x y))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(let ((x 1)\n      (y (+ x 2)))\n  (list x y))\n"
    );
}

#[test]
fn formats_qualified_common_lisp_binding_heads() {
    let input = "(cl:let ((x 1) (y (+ x 2))) (list x y))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(cl:let ((x 1)\n         (y (+ x 2)))\n  (list x y))\n"
    );
}

#[test]
fn formats_bracket_binding_forms_as_name_value_pairs() {
    let input = "(let [x 1 y (+ x 2)] (list x y))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(let [x 1\n      y (+ x 2)]\n  (list x y))\n"
    );
}

#[test]
fn formats_handler_bind_like_a_binding_form() {
    let input =
        "(handler-bind ((error #'handle-error) (warning #'muffle-warning)) (risky) (recover))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(handler-bind ((error #'handle-error)\n               (warning #'muffle-warning))\n  (risky)\n  (recover))\n"
    );
}

#[test]
fn preserves_common_lisp_reader_prefixes() {
    let input = "'(alpha beta)\n`(list ,item ,@rest)\n#'(lambda (value) value)";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "'(alpha beta)\n\n`(list ,item ,@rest)\n\n#'(lambda (value)\n  value)\n"
    );
}

#[test]
fn preserves_dialect_reader_prefix_spellings() {
    let cases = [
        (Dialect::Janet, ";(value)", ";(value)\n"),
        (Dialect::Fennel, "#(value)", "#(value)\n"),
    ];

    for (dialect, input, expected) in cases {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("valid reader form");
        assert_eq!(
            Formatter::new(2).format(&tree),
            expected,
            "{}",
            dialect.label()
        );
    }
}

#[test]
fn preserves_multi_datum_reader_forms_verbatim() {
    let cases = [
        (Dialect::CommonLisp, "#+feature (guarded value)"),
        (Dialect::Clojure, "^:private target"),
        (Dialect::Clojure, r#"^{:doc "x"} target"#),
        (Dialect::Scheme, "#u8(1 2 3)"),
        (Dialect::Clojure, r##"#"foo.*""##),
        (Dialect::Clojure, r#"#:person{:first "Ada"}"#),
        (Dialect::Clojure, r#"#inst "2020-01-01""#),
    ];

    for (dialect, input) in cases {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("valid reader form");
        assert_eq!(
            Formatter::new(2).format(&tree),
            format!("{input}\n"),
            "{}",
            dialect.label()
        );
    }
}

#[test]
fn preserves_common_lisp_reader_eval_forms_verbatim() {
    let input = "#.(foo (bar baz))\n#.(list 1 2 3)";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "#.(foo (bar baz))\n\n#.(list 1 2 3)\n"
    );
}

#[test]
fn formats_restart_bind_like_a_binding_form() {
    let input = "(restart-bind ((retry #'retry :report report-retry) (skip #'skip)) (work))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(restart-bind ((retry #'retry :report report-retry)\n               (skip #'skip))\n  (work))\n"
    );
}

#[test]
fn formats_macro_and_cond_body_forms() {
    let input = "(defmacro when-let ((name value)) (list 'when value (list 'let (list (list name value)) name)))\n(cond ((null x) nil) (t (car x)))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(defmacro when-let ((name value))\n  (list 'when value (list 'let (list (list name value)) name)))\n\n(cond\n  ((null x) nil)\n  (t (car x)))\n"
    );
}

#[test]
fn formats_multi_body_cond_clauses_on_separate_lines() {
    let input = "(cond ((ready-p value) (prepare value) (run value)) (t (fallback)))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(cond\n  ((ready-p value)\n    (prepare value)\n    (run value))\n  (t (fallback)))\n"
    );
}

#[test]
fn formats_case_keyform_and_multi_body_clauses() {
    let input = "(case kind (:ready (prepare value) (run value)) ((:skip :noop) value) (otherwise (fallback)))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(case kind\n  (:ready\n    (prepare value)\n    (run value))\n  ((:skip :noop) value)\n  (otherwise (fallback)))\n"
    );
}

#[test]
fn formats_do_iteration_specs_and_end_clause() {
    let input = "(do ((i 0 (1+ i)) (sum 0 (+ sum i))) ((>= i limit) sum total) (incf total sum) (collect i))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(do ((i 0 (1+ i))\n     (sum 0 (+ sum i)))\n  ((>= i limit)\n    sum\n    total)\n  (incf total sum)\n  (collect i))\n"
    );
}

#[test]
fn formats_do_star_like_do() {
    let input = "(do* ((x 0 (1+ x)) (y x (+ x y))) ((> y 10) y) (print y))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(do* ((x 0 (1+ x))\n      (y x (+ x y)))\n  ((> y 10) y)\n  (print y))\n"
    );
}

#[test]
fn formats_prog_bindings_and_tagbody_forms() {
    let input = "(prog ((i 0) (sum 0)) start (incf sum i) (when (> sum limit) (return sum)) (incf i) (go start))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(prog ((i 0)\n       (sum 0))\n  start\n  (incf sum i)\n  (when (> sum limit)\n    (return sum))\n  (incf i)\n  (go start))\n"
    );
}

#[test]
fn formats_prog_star_like_prog() {
    let input = "(prog* ((x 1) (y x)) done (return y))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(prog* ((x 1)\n        (y x))\n  done\n  (return y))\n"
    );
}

#[test]
fn formats_common_lisp_prefix_body_forms() {
    let input =
        "(block done (catch 'retry (unwind-protect (run job) (cleanup job) (release job))))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(block done\n  (catch 'retry\n    (unwind-protect (run job)\n      (cleanup job)\n      (release job))))\n"
    );
}

#[test]
fn formats_common_lisp_with_body_macros() {
    let input = "(with-input-from-string (stream text) (read stream) (finish stream))\n(with-output-to-string (stream) (write value :stream stream) (finish-output stream))\n(with-hash-table-iterator (next table) (multiple-value-bind (more key value) (next) (when more (collect key value))))\n(with-package-iterator (next package :internal :external) (multiple-value-bind (more symbol status package) (next) (when more (collect symbol status package))))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(with-input-from-string (stream text)\n  (read stream)\n  (finish stream))\n\n(with-output-to-string (stream)\n  (write value :stream stream)\n  (finish-output stream))\n\n(with-hash-table-iterator (next table)\n  (multiple-value-bind (more key value) (next)\n    (when more\n      (collect key value))))\n\n(with-package-iterator (next package :internal :external)\n  (multiple-value-bind (more symbol status package) (next)\n    (when more\n      (collect symbol status package))))\n"
    );
}

#[test]
fn formats_eval_when_body_after_situation_list() {
    let input = "(eval-when (:compile-toplevel :load-toplevel :execute) (declaim (optimize speed)) (defun boot () (start)))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(eval-when (:compile-toplevel :load-toplevel :execute)\n  (declaim (optimize speed))\n  (defun boot ()\n    (start)))\n"
    );
}

#[test]
fn formats_lambda_body_after_lambda_list() {
    let input = "(lambda (value) (validate value) (render value))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(lambda (value)\n  (validate value)\n  (render value))\n"
    );
}

#[test]
fn formats_when_body_after_condition() {
    let input = "(when ready-p (prepare) (run))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(when ready-p\n  (prepare)\n  (run))\n"
    );
}

#[test]
fn formats_destructuring_bind_with_two_prefix_forms() {
    let input = "(destructuring-bind (value other) (parse value) (list value other) (finish))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(destructuring-bind (value other) (parse value)\n  (list value other)\n  (finish))\n"
    );
}

#[test]
fn formats_multiple_value_bind_with_two_prefix_forms() {
    let input = "(multiple-value-bind (value foundp) (gethash key table) (list value foundp))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(multiple-value-bind (value foundp) (gethash key table)\n  (list value foundp))\n"
    );
}

#[test]
fn formats_handler_case_clauses_after_protected_form() {
    let input = "(handler-case (risky) (error (condition) (recover condition) (log condition)) (:no-error (value) value))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(handler-case (risky)\n  (error (condition)\n    (recover condition)\n    (log condition))\n  (:no-error (value)\n    value))\n"
    );
}

#[test]
fn formats_restart_case_clauses_after_protected_form() {
    let input = "(restart-case (risky) (retry () (prepare) (risky)) (skip () nil))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(restart-case (risky)\n  (retry ()\n    (prepare)\n    (risky))\n  (skip ()\n    nil))\n"
    );
}

#[test]
fn keeps_short_defsystem_forms_on_one_line() {
    let input = "(defsystem \"foo\"\n  :description \"short\"\n  :version \"0.1.0\"\n  :depends-on (:asdf))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(defsystem \"foo\" :description \"short\" :version \"0.1.0\" :depends-on (:asdf))\n"
    );
}

#[test]
fn preserves_reader_prefix_on_short_defsystem_idempotently() {
    let tree = SyntaxTree::parse("'(defsystem x)").expect("valid");
    let formatted = Formatter::new(2).format(&tree);
    assert_eq!(formatted, "'(defsystem x)\n");

    let reparsed = SyntaxTree::parse(&formatted).expect("formatted output is valid");
    assert_eq!(Formatter::new(2).format(&reparsed), formatted);
}

#[test]
fn breaks_long_defsystem_forms_keeping_option_pairs_together() {
    let input = "(defsystem \"my-really-quite-long-system-name\" :description \"a considerably longer description string here\" :version \"0.1.0\" :depends-on (:alexandria :bordeaux-threads))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(defsystem \"my-really-quite-long-system-name\"\n  :description \"a considerably longer description string here\"\n  :version \"0.1.0\"\n  :depends-on (:alexandria :bordeaux-threads))\n"
    );
}

#[test]
fn formats_define_compiler_macro_like_a_definition() {
    let input = "(define-compiler-macro fast-add (x y) (list '+ x y))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(define-compiler-macro fast-add (x y)\n  (list '+ x y))\n"
    );
}

#[test]
fn formats_setf_definition_forms_like_definitions() {
    let input = "(define-setf-expander place (env) (values) (list place env))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(define-setf-expander place (env)\n  (values)\n  (list place env))\n"
    );
}

#[test]
fn formats_common_lisp_assignment_pairs() {
    let input = "(setq x 1 y (+ x 2) total (compute-total x y))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(setq x 1\n      y (+ x 2)\n      total (compute-total x y))\n"
    );
}

#[test]
fn formats_setf_place_value_pairs() {
    let input = "(setf (slot-value user 'name) (compute-name user) (slot-value user 'age) (compute-age user))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(setf (slot-value user 'name) (compute-name user)\n      (slot-value user 'age) (compute-age user))\n"
    );
}

#[test]
fn formats_incomplete_assignment_pair_without_dropping_operands() {
    let input = "(psetq ready-p)";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(Formatter::new(2).format(&tree), "(psetq ready-p)\n");
}

#[test]
fn formats_define_symbol_macro_like_a_definition() {
    let input = "(define-symbol-macro current-user (slot-value *session* 'user))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(define-symbol-macro current-user\n  (slot-value *session* 'user))\n"
    );
}

#[test]
fn formats_symbol_macrolet_like_binding_form() {
    let input = "(symbol-macrolet ((value (compute value)) (used other)) (list value used))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(symbol-macrolet ((value (compute value))\n                  (used other))\n  (list value used))\n"
    );
}

#[test]
fn formats_macrolet_like_local_functions() {
    let input = "(macrolet ((with-x (x) (list x outer))) (with-x 1) (with-x 2))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(macrolet ((with-x (x)\n             (list x outer)))\n  (with-x 1)\n  (with-x 2))\n"
    );
}

#[test]
fn formats_compiler_macrolet_like_local_functions() {
    let input = "(compiler-macrolet ((with-x (x) (list x outer))) (with-x 1) (with-x 2))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(compiler-macrolet ((with-x (x)\n                      (list x outer)))\n  (with-x 1)\n  (with-x 2))\n"
    );
}

#[test]
fn formats_multiple_local_callable_bindings_with_aligned_bindings() {
    let input = "(macrolet ((with-a (x) (list x outer)) (with-b (y) (list y outer))) (with-a 1) (with-b 2))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(macrolet ((with-a (x)\n             (list x outer))\n           (with-b (y)\n             (list y outer)))\n  (with-a 1)\n  (with-b 2))\n"
    );
}

#[test]
fn formats_local_callable_bodies_on_dedicated_lines() {
    let input = "(labels ((parse (x) (validate x) (build x)) (emit (y) (write y) (finish))) (parse input) (emit output))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(labels ((parse (x)\n           (validate x)\n           (build x))\n         (emit (y)\n           (write y)\n           (finish)))\n  (parse input)\n  (emit output))\n"
    );
}

#[test]
fn formats_declarations_with_inline_specs() {
    let input =
        "(locally (declare (optimize speed)) (declaim (inline f)) (proclaim (special x)) (f))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(locally\n  (declare (optimize speed))\n  (declaim (inline f))\n  (proclaim (special x))\n  (f))\n"
    );
}

#[test]
fn formats_multiple_declaration_specs_with_alignment() {
    let input = "(declare (optimize speed) (type fixnum index) (ignorable scratch))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(declare (optimize speed)\n         (type fixnum index)\n         (ignorable scratch))\n"
    );
}

#[test]
fn formats_loop_clauses_with_common_lisp_indentation() {
    let input = "(loop for item in items when (valid-p item) collect (transform item) finally (return result))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(loop for item in items\n      when (valid-p item)\n        collect (transform item)\n      finally (return result))\n"
    );
}

#[test]
fn formats_loop_binding_and_action_clauses() {
    let input =
        "(loop with total = 0 for item in items do (incf total item) finally (return total))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(loop with total = 0\n      for item in items\n      do (incf total item)\n      finally (return total))\n"
    );
}

#[test]
fn preserves_leading_line_comment_above_form() {
    let input = ";; doc\n(defun f (x) (+ x 1))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        ";; doc\n(defun f (x)\n  (+ x 1))\n"
    );
}

#[test]
fn preserves_trailing_line_comment_on_form_line() {
    let input = "(foo)  ; note\n(bar)";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(Formatter::new(2).format(&tree), "(foo) ; note\n\n(bar)\n");
}

#[test]
fn preserves_interior_comment_by_rendering_form_verbatim() {
    let input = "(defun f (x)\n  ;; inner\n  (+ x 1))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(defun f (x)\n  ;; inner\n  (+ x 1))\n"
    );
}

#[test]
fn preserves_comment_only_document() {
    let input = ";; alpha\n;; beta\n";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(Formatter::new(2).format(&tree), ";; alpha\n;; beta\n");
}

#[test]
fn preserves_leading_block_comment() {
    let input = "#| header |#\n(foo)";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(Formatter::new(2).format(&tree), "#| header |#\n(foo)\n");
}

#[test]
fn preserves_datum_reader_comment() {
    let input = "#;(ignored form)\n(kept)";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "#;(ignored form)\n(kept)\n"
    );
}

#[test]
fn preserves_trailing_standalone_comment_at_end_of_file() {
    let input = "(foo)\n;; tail";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(Formatter::new(2).format(&tree), "(foo)\n\n;; tail\n");
}

#[test]
fn preserves_string_that_contains_a_semicolon() {
    let input = "(defvar path \";not-a-comment\")";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(defvar path \";not-a-comment\")\n"
    );
}

#[test]
fn formatting_never_drops_comments_and_is_idempotent() {
    let input = concat!(
        ";;; header -*- lexical-binding: t; -*-\n",
        ";; commentary\n",
        "(defun add (a b)\n",
        "  ;; inner note\n",
        "  (+ a b)) ; trailing\n",
        "#| block |#\n",
        "#;(skipped)\n",
        "(defvar x 1)\n",
        ";; footer\n",
    );
    let formatter = Formatter::new(2);
    let tree = SyntaxTree::parse(input).expect("valid");
    let formatted = formatter.format(&tree);

    for comment in [
        ";;; header -*- lexical-binding: t; -*-",
        ";; commentary",
        ";; inner note",
        "; trailing",
        "#| block |#",
        "#;(skipped)",
        ";; footer",
    ] {
        assert!(
            formatted.contains(comment),
            "formatted output dropped comment: {comment}\n---\n{formatted}"
        );
    }

    let reparsed = SyntaxTree::parse(&formatted).expect("formatted output parses again");
    let reformatted = formatter.format(&reparsed);
    assert_eq!(
        formatted, reformatted,
        "comment-preserving format must be idempotent"
    );
}

#[test]
fn formats_thirty_thousand_nested_lists_without_overflow() {
    const DEPTH: usize = 30_000;

    let input = format!("{}value{}", "(".repeat(DEPTH), ")".repeat(DEPTH));
    let tree = SyntaxTree::parse(&input).expect("valid deeply nested input");
    let formatted = Formatter::new(2).format(&tree);

    SyntaxTree::parse(&formatted).expect("deeply formatted output parses again");
    assert!(formatted.contains("value"));
}

#[test]
fn clamps_extreme_indent_without_overflow_or_unbounded_padding() {
    let input = "(defun render (value) (prepare value) (emit value))";
    let tree = SyntaxTree::parse(input).expect("valid");
    let formatted = Formatter::new(usize::MAX).format(&tree);

    assert!(formatted.len() < 1_024);
    SyntaxTree::parse(&formatted).expect("formatted output parses again");
}
