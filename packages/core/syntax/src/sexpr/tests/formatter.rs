use unicode_width::UnicodeWidthStr;

use super::*;
use crate::dialect::Dialect;
use crate::sexpr::ReaderPrefixStyle;

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
    // `#'` is two columns wide, so the `(lambda` it prefixes opens at column
    // 2 and the lambda body belongs at column 4.
    assert_eq!(
        Formatter::new(2).format(&tree),
        "'(alpha beta)\n\n`(list ,item ,@rest)\n\n#'(lambda (value)\n    value)\n"
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

/// A Hy comma is a symbol, so the formatter must re-emit it as one — in
/// particular it must not drop it, and must not glue it to a neighbour.
///
/// `treefmt` formats this repository's own Lisp with paredit, so a regression
/// here is a build break rather than a cosmetic one. These are the shapes
/// `hy_reads_a_comma_as_a_symbol_constituent_not_as_unquote` pins on the parse
/// side; every one of them comes back byte-identical.
#[test]
fn preserves_hy_commas_verbatim() {
    let cases = [
        "(,)",
        "(, 1 2)",
        "[1 ,]",
        r#"{"a" 1 ,}"#,
        "(a , b)",
        "[1, 2]",
        "(= (first x) ',)",
        "(setv x (,))",
    ];

    for input in cases {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::Hy)
            .unwrap_or_else(|error| panic!("{input}: {error:?}"));
        let once = Formatter::new(2).format(&tree);
        assert_eq!(once, format!("{input}\n"), "{input}");
        let reparsed =
            SyntaxTree::parse_with_dialect(&once, Dialect::Hy).expect("output parses again");
        assert_eq!(Formatter::new(2).format(&reparsed), once, "{input}");
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
fn prefix_body_break_does_not_leave_trailing_whitespace() {
    let input = "(unwind-protect (restart-case (perform-a-very-long-operation) (retry () (perform-a-very-long-operation))) (cleanup))";
    let tree = SyntaxTree::parse(input).expect("valid");
    let formatted = Formatter::new(2).with_max_width(40).format(&tree);

    assert!(formatted.lines().all(|line| line.trim_end() == line));
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

/// Each of these 30 CJK characters is 3 UTF-8 bytes but a single full-width
/// display column pair (East Asian Width `Wide`, so 2 columns). Byte-length
/// accounting would put this string at 96 "columns", past a 70-column
/// budget; measured the way a terminal actually renders it, it is 66.
#[test]
fn measures_inline_width_by_display_columns_not_utf8_bytes() {
    let cjk = "日".repeat(30);
    let input = format!("(f \"{cjk}\")");
    let tree = SyntaxTree::parse(&input).expect("valid");
    assert_eq!(
        Formatter::new(2).with_max_width(70).format(&tree),
        format!("{input}\n"),
        "a CJK string within the display-column budget must stay inline"
    );
}

/// The same shape, but the display width itself exceeds the budget, so this
/// must still wrap — the fix is a different unit of measurement, not "never
/// wrap wide text".
#[test]
fn wraps_when_display_width_itself_exceeds_the_budget() {
    let cjk = "日".repeat(30);
    let input = format!("(f \"{cjk}\")");
    let tree = SyntaxTree::parse(&input).expect("valid");
    let formatted = Formatter::new(2).with_max_width(50).format(&tree);
    assert_ne!(formatted, format!("{input}\n"));
    assert!(formatted.contains('\n'), "{formatted}");
}

/// Two CJK characters sum to a display width of 4, so `(f "日日")` is
/// exactly 10 columns wide. `Bounded::push_str`'s `<=` comparison in
/// `compact_node` (`formatter/core.rs`) must accept a form that lands
/// exactly on the width budget, not just one strictly under it.
#[test]
fn cjk_width_fits_exactly_at_the_max_width_boundary() {
    let input = "(f \"日日\")";
    assert_eq!(
        unicode_width::UnicodeWidthStr::width(input),
        10,
        "sanity: input is exactly 10 display columns wide"
    );
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).with_max_width(10).format(&tree),
        format!("{input}\n"),
        "a form landing exactly on the width budget must still fit inline"
    );
}

/// The same form one column over the budget must not fit inline: the
/// boundary is `<=`, not `<`.
#[test]
fn cjk_width_one_column_over_the_max_width_boundary_wraps() {
    let input = "(f \"日日\")";
    let tree = SyntaxTree::parse(input).expect("valid");
    let formatted = Formatter::new(2).with_max_width(9).format(&tree);
    assert_ne!(
        formatted,
        format!("{input}\n"),
        "one column over the budget must not fit inline"
    );
    assert!(formatted.contains('\n'), "{formatted}");
}

#[test]
fn cjk_width_aware_formatting_is_idempotent() {
    let input =
        "(defun 挨拶する (名前) (すごく長い関数の名前を呼び出す名前 名前 \"こんにちは、世界\"))";
    let tree = SyntaxTree::parse(input).expect("valid");
    let formatter = Formatter::new(2);
    let formatted = formatter.format(&tree);

    let reparsed = SyntaxTree::parse(&formatted).expect("formatted output parses again");
    assert_eq!(
        formatter.format(&reparsed),
        formatted,
        "CJK-aware width accounting must still be idempotent"
    );
}

#[test]
fn block_comment_reindent_is_off_by_default() {
    let input = "   #| Header\n      continuation\n   |#\n(foo)";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "#| Header\n      continuation\n   |#\n(foo)\n",
        "without opting in, a block comment's interior lines are untouched"
    );
}

#[test]
fn block_comment_reindent_realigns_interior_lines_when_enabled() {
    let input = "   #| Header\n      continuation\n   |#\n(foo)";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2)
            .with_reindent_block_comments(true)
            .format(&tree),
        "#| Header\n   continuation\n|#\n(foo)\n",
        "continuation lines keep their indentation relative to each other, \
         realigned to the comment's new (top-level) depth"
    );
}

/// A nested `#|...|#` must reindent without corrupting the inner markers:
/// only each line's leading whitespace ever moves.
#[test]
fn block_comment_reindent_preserves_nested_markers() {
    let input = concat!(
        "     #| outer\n",
        "        #| inner\n",
        "           deep\n",
        "        |#\n",
        "        still outer\n",
        "     |#\n",
        "(after)",
    );
    let tree = SyntaxTree::parse(input).expect("valid");
    let formatted = Formatter::new(2)
        .with_reindent_block_comments(true)
        .format(&tree);
    assert_eq!(
        formatted,
        "#| outer\n   #| inner\n      deep\n   |#\n   still outer\n|#\n(after)\n"
    );

    let reparsed = SyntaxTree::parse(&formatted).expect("reindented output parses again");
    assert_eq!(
        Formatter::new(2)
            .with_reindent_block_comments(true)
            .format(&reparsed),
        formatted,
        "reindenting a block comment must still be idempotent"
    );
}

/// Leading-whitespace amounts are measured in display width, not byte
/// length, so a tab and a run of spaces that occupy the same number of
/// columns are treated as equally indented: a tab-indented continuation
/// line realigns to the same column as an equally-wide space-indented one,
/// and a line indented further keeps exactly that extra offset.
#[test]
fn block_comment_reindent_preserves_relative_indentation_across_tabs_and_spaces() {
    let input = "   #| Header\n   space three\n\t  tab three\n\t\t     tab tab seven\n   |#\n(foo)";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2)
            .with_reindent_block_comments(true)
            .format(&tree),
        "#| Header\nspace three\ntab three\n    tab tab seven\n|#\n(foo)\n",
        "a tab-indented line and an equally-wide space-indented line must \
         land on the same column, and a line indented four columns deeper \
         must keep exactly that four-column offset"
    );
}

/// A blank interior line is left exactly as written rather than padded to
/// the new depth or collapsed away.
#[test]
fn block_comment_reindent_leaves_blank_interior_lines_alone() {
    let input = "   #| Header\n\n      continuation\n   |#\n(foo)";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2)
            .with_reindent_block_comments(true)
            .format(&tree),
        "#| Header\n\n   continuation\n|#\n(foo)\n"
    );
}

/// A single-line block comment has no interior lines, so the flag changes
/// nothing about it.
#[test]
fn block_comment_reindent_is_a_no_op_for_a_single_line_comment() {
    let input = "#| header |#\n(foo)";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2)
            .with_reindent_block_comments(true)
            .format(&tree),
        "#| header |#\n(foo)\n"
    );
}

/// Only `CommonLisp`, `Lfe`, `Scheme`, `Racket`, and `Unknown` read `#|...|#`
/// as a block comment at all (`DialectReaderPolicy::supports_block_comments`);
/// every other comment kind's text never starts with `#|`, so the flag has
/// nothing to act on for dialects that lack the syntax.
#[test]
fn block_comment_reindent_leaves_other_comment_kinds_untouched() {
    let input = "(ns app)\n;; a note\n(defn f [] 1)";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("valid Clojure");
    assert_eq!(
        Formatter::with_dialect(2, Dialect::Clojure)
            .with_reindent_block_comments(true)
            .format(&tree),
        Formatter::with_dialect(2, Dialect::Clojure).format(&tree),
        "a line comment is never reindented, on or off"
    );
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
        // `defvar` uses `DefinitionNameBody` (name on head line, value at
        // body).  The semicolon inside the string is preserved — it is never
        // mistaken for a comment.
        "(defvar path\n  \";not-a-comment\")\n"
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
fn common_lisp_escaped_newline_before_comment_is_idempotent() {
    let input = format!(
        "\u{000f}\0 A co\"\\co\"\\\n {} A-hi ;t #| nd e",
        "\0".repeat(56)
    );
    let formatter = Formatter::new(2);
    let tree = SyntaxTree::parse_with_dialect(&input, Dialect::CommonLisp)
        .expect("input must parse as Common Lisp");
    let once = formatter.format(&tree);
    let reparsed = SyntaxTree::parse_with_dialect(&once, Dialect::CommonLisp)
        .expect("formatted output must parse as Common Lisp");

    assert_eq!(formatter.format(&reparsed), once);
}

// --- FR-005: comment-column alignment ---

#[test]
fn comment_column_off_by_default_uses_a_single_space() {
    // `with_comment_column` never called: must reproduce the formatter's
    // original, pre-FR-005 behavior exactly.
    let input = "(a) ; one\n(bb) ; two";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(Formatter::new(2).format(&tree), "(a) ; one\n\n(bb) ; two\n");
}

#[test]
fn comment_column_auto_never_groups_anything_while_blank_lines_stay_at_their_default() {
    // With `max_blank_lines` unset, `Self::format` always renders exactly one
    // blank line between top-level forms — the original, preserved behavior
    // — so no two forms are ever rendered back-to-back. Auto alignment's
    // runs are defined over back-to-back forms (see
    // `Formatter::trailing_comment_columns`'s doc comment for why: anything
    // looser breaks idempotency), so with blank lines left at their default,
    // every run is exactly one form and every comment keeps its plain
    // one-space default, even for forms that were adjacent in the source.
    let input = "(a) ; one\n(bb) ; two";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).with_comment_column(0).format(&tree),
        "(a) ; one\n\n(bb) ; two\n",
        "comment-column alignment needs max_blank_lines set to ever see two \
         forms rendered back-to-back"
    );
}

#[test]
fn comment_column_auto_aligns_a_run_of_adjacent_commented_forms() {
    // Both forms carry a trailing comment and, with blank lines capped at 1,
    // render back-to-back (the source already has none between them): one
    // run, aligned to one column past the wider of the two, `(bb)` at 4
    // columns.
    let input = "(a) ; one\n(bb) ; two";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2)
            .with_comment_column(0)
            .with_max_blank_lines(1)
            .format(&tree),
        "(a)  ; one\n(bb) ; two\n",
        "both comments must align to column 5, one past `(bb)`'s width"
    );
}

#[test]
fn comment_column_auto_gives_each_run_its_own_independent_alignment() {
    // Two runs of adjacent commented forms, separated by `(mid)`, which
    // carries no trailing comment and so breaks the run. Each run must align
    // to its own widest member, not the document's widest member overall —
    // otherwise the first (narrower) run would be dragged out to match the
    // second (wider) one.
    let input = "(a) ; one\n(bb) ; two\n(mid)\n(ccccccc) ; three\n(d) ; four";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2)
            .with_comment_column(0)
            .with_max_blank_lines(1)
            .format(&tree),
        "(a)  ; one\n(bb) ; two\n(mid)\n(ccccccc) ; three\n(d)       ; four\n",
        "the first run must align to column 5 (its own widest member, `(bb)`), \
         not column 10 (the second run's widest member, `(ccccccc)`)"
    );
}

#[test]
fn comment_column_auto_breaks_a_run_on_a_rendered_blank_line() {
    // Same two commented forms as the basic alignment case, but separated by
    // a blank line the configured maximum still lets through: that renders
    // back-to-back forms is exactly what a run requires, and a blank line
    // between them means they never are, so each keeps its own one-space
    // default rather than aligning to the pair's shared widest column.
    let input = "(a) ; one\n\n(bb) ; two";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2)
            .with_comment_column(0)
            .with_max_blank_lines(1)
            .format(&tree),
        "(a) ; one\n\n(bb) ; two\n",
        "a rendered blank line breaks the run, so neither comment moves"
    );
}

#[test]
fn comment_column_fixed_aligns_every_trailing_comment_uniformly() {
    // A fixed column is an absolute position: it applies to every trailing
    // comment in the document, run or no run, blank line or no blank line.
    let input = "(a) ; one\n\n(bb) ; two";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).with_comment_column(10).format(&tree),
        "(a)       ; one\n\n(bb)      ; two\n",
        "both comments must start at column 10 despite the blank line between them"
    );
}

#[test]
fn comment_column_fixed_falls_back_to_one_space_past_a_form_wider_than_the_column() {
    // `(abcde)` is already 7 columns wide, past the configured column 3: the
    // alignment column can only push a comment right, never delete a space
    // to pull it left onto or past the form's own text.
    let input = "(abcde) ; note";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).with_comment_column(3).format(&tree),
        "(abcde) ; note\n",
        "a form already past the target column still gets exactly one space"
    );
}

#[test]
fn comment_column_auto_is_idempotent() {
    // `with_max_blank_lines` alongside `with_comment_column`, both non-default:
    // exactly the combination `Formatter::trailing_comment_columns`'s doc
    // comment calls out as the one place FR-005 and FR-006 are not
    // independent of each other.
    let input = "(a) ; one\n(bb) ; two\n(mid)\n(ccccccc) ; three\n(d) ; four";
    let tree = SyntaxTree::parse(input).expect("valid");
    let formatter = Formatter::new(2)
        .with_comment_column(0)
        .with_max_blank_lines(1);
    let formatted = formatter.format(&tree);

    let reparsed = SyntaxTree::parse(&formatted).expect("formatted output parses again");
    assert_eq!(
        formatter.format(&reparsed),
        formatted,
        "comment-column alignment must still be idempotent"
    );
}

#[test]
fn comment_column_auto_is_idempotent_with_blank_lines_left_at_their_default() {
    // The degenerate case of the test above: `max_blank_lines` unset means
    // every run is a singleton (see
    // `comment_column_auto_never_groups_anything_while_blank_lines_stay_at_their_default`),
    // which is trivially stable, but it is still worth locking in given how
    // easy it would be to reintroduce the raw-source-gap grouping bug this
    // design specifically avoids.
    let input = "(a) ; one\n(bb) ; two\n(mid)\n(ccccccc) ; three\n(d) ; four";
    let tree = SyntaxTree::parse(input).expect("valid");
    let formatter = Formatter::new(2).with_comment_column(0);
    let formatted = formatter.format(&tree);

    let reparsed = SyntaxTree::parse(&formatted).expect("formatted output parses again");
    assert_eq!(
        formatter.format(&reparsed),
        formatted,
        "comment-column alignment must still be idempotent even when every \
         run degenerates to a single item"
    );
}

// --- FR-006: configurable blank-line normalization ---

#[test]
fn max_blank_lines_unset_collapses_any_source_gap_to_exactly_one_blank_line() {
    // `with_max_blank_lines` never called: must reproduce the formatter's
    // original, pre-FR-006 behavior exactly, even when the source has more
    // than one blank line between forms.
    let input = "(a)\n\n\n\n(b)";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(Formatter::new(2).format(&tree), "(a)\n\n(b)\n");
}

#[test]
fn max_blank_lines_preserves_the_source_up_to_the_configured_maximum() {
    let input = "(a)\n\n\n(b)"; // two blank lines in the source
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).with_max_blank_lines(2).format(&tree),
        "(a)\n\n\n(b)\n",
        "two blank lines are within the configured maximum, so both are kept"
    );
}

#[test]
fn max_blank_lines_collapses_a_source_gap_wider_than_the_maximum() {
    let input = "(a)\n\n\n(b)"; // two blank lines in the source
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).with_max_blank_lines(1).format(&tree),
        "(a)\n\n(b)\n",
        "two blank lines is over the configured maximum of one, so it collapses to one"
    );
}

#[test]
fn max_blank_lines_zero_never_inserts_a_blank_line() {
    let input = "(a)\n\n\n(b)"; // two blank lines in the source
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).with_max_blank_lines(0).format(&tree),
        "(a)\n(b)\n",
        "zero means no blank line ever, regardless of what the source had"
    );
}

#[test]
fn max_blank_lines_never_inserts_a_blank_line_the_source_did_not_have() {
    // The configured maximum is a ceiling, not a target: it must never
    // manufacture blank lines the source did not already contain.
    let input = "(a)\n(b)"; // zero blank lines in the source
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).with_max_blank_lines(5).format(&tree),
        "(a)\n(b)\n",
        "a maximum of five must not inflate a zero-blank-line gap"
    );
}

#[test]
fn max_blank_lines_is_idempotent() {
    let input = "(a)\n\n\n(b)\n\n(c)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let formatter = Formatter::new(2).with_max_blank_lines(1);
    let formatted = formatter.format(&tree);

    let reparsed = SyntaxTree::parse(&formatted).expect("formatted output parses again");
    assert_eq!(
        formatter.format(&reparsed),
        formatted,
        "blank-line normalization must still be idempotent"
    );
}

// --- FR-005/FR-006 interaction: a leading comment must never drift onto the
// wrong neighboring form when blank lines collapse. ---

#[test]
fn blank_line_collapse_never_reattaches_a_leading_comment_to_the_form_above_it() {
    // `; for b` is a leading (own-line) comment of `(b)`, separated from `(a)`
    // above it by one blank line in the source. Preserving that one blank
    // line must keep the comment glued to `(b)` — directly above it, with no
    // blank line of its own — while the configured gap lands *before* the
    // comment, between it and `(a)`.
    let input = "(a)\n\n; for b\n(b)";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).with_max_blank_lines(1).format(&tree),
        "(a)\n\n; for b\n(b)\n"
    );
}

#[test]
fn blank_line_collapse_to_zero_still_keeps_a_leading_comment_with_its_own_form() {
    // Same source as above, but every blank line is suppressed. The comment
    // must still read as belonging to `(b)` — on its own line, immediately
    // above it — rather than silently becoming a same-line trailing comment
    // of `(a)` just because the blank line that used to separate them is
    // gone. Comment attachment is decided once, at parse time, from source
    // position; it must never depend on how blank lines are later rendered.
    let input = "(a)\n\n; for b\n(b)";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).with_max_blank_lines(0).format(&tree),
        "(a)\n; for b\n(b)\n"
    );
}

#[test]
fn comment_column_and_blank_line_collapse_together_do_not_drift_a_leading_comment() {
    // FR-005 and FR-006 enabled together, on a document exercising both at
    // once: `(a)`'s trailing comment must still align only within its own
    // (single-member) run, and `; for d`, a leading comment of `(d)`
    // separated from `(cccccc)` by a blank line, must still land directly
    // above `(d)` even once every blank line is suppressed by
    // `with_max_blank_lines(0)` — never reattached to `(cccccc)` as if it
    // were that form's trailing comment.
    let input = "(a) ; nb\n(cccccc)\n\n; for d\n(d) ; nd";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2)
            .with_comment_column(0)
            .with_max_blank_lines(0)
            .format(&tree),
        "(a) ; nb\n(cccccc)\n; for d\n(d) ; nd\n"
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

/// Parses and formats `input` as Clojure.
///
/// Both halves have to agree on the dialect: parsing decides which reader
/// prefixes exist, and formatting decides which operator table lays lists out.
fn format_clojure(input: &str) -> String {
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("valid Clojure");
    Formatter::with_dialect(2, Dialect::Clojure).format(&tree)
}

#[test]
fn keeps_the_clojure_namespace_name_on_the_ns_head_line() {
    let input = "(ns myapp.core \"Core namespace.\" (:require [clojure.string :as str]))";
    assert_eq!(
        format_clojure(input),
        "(ns myapp.core\n  \"Core namespace.\"\n  (:require [clojure.string :as str]))\n"
    );
}

#[test]
fn keeps_the_clojure_parameter_vector_on_the_defn_head_line() {
    let input = "(defn greet [first-name last-name] (log first-name) (str first-name last-name))";
    assert_eq!(
        format_clojure(input),
        "(defn greet [first-name last-name]\n  (log first-name)\n  (str first-name last-name))\n"
    );
}

#[test]
fn moves_clojure_definition_children_below_a_docstring_or_attribute_map() {
    let input =
        "(defn greet \"Greets a person.\" {:added \"1.0\"} [name & opts] (str \"Hello, \" name))";
    assert_eq!(
        format_clojure(input),
        concat!(
            "(defn greet\n",
            "  \"Greets a person.\"\n",
            "  {:added \"1.0\"}\n",
            "  [name & opts]\n",
            "  (str \"Hello, \" name))\n"
        )
    );
}

#[test]
fn gives_each_clojure_arity_clause_its_own_line() {
    let input = "(defn multi ([x] (multi x 1)) ([x y] (+ x y)) ([x y & more] (apply + x y more)))";
    assert_eq!(
        format_clojure(input),
        concat!(
            "(defn multi\n",
            "  ([x] (multi x 1))\n",
            "  ([x y] (+ x y))\n",
            "  ([x y & more] (apply + x y more)))\n"
        )
    );
}

#[test]
fn formats_clojure_fn_forms_with_and_without_a_name() {
    let cases = [
        ("(fn [x] (inc x))", "(fn [x] (inc x))\n"),
        ("(fn named [x] (inc x))", "(fn named [x] (inc x))\n"),
        (
            "(fn ([x] x) ([x y] (+ x y)))",
            "(fn\n  ([x] x)\n  ([x y] (+ x y)))\n",
        ),
    ];

    for (input, expected) in cases {
        assert_eq!(format_clojure(input), expected, "{input}");
    }
}

#[test]
fn keeps_the_clojure_type_and_field_vector_on_the_defrecord_head_line() {
    let input = "(defrecord Circle [radius] Shape (area [this] (* Math/PI radius radius)))";
    assert_eq!(
        format_clojure(input),
        concat!(
            "(defrecord Circle [radius]\n",
            "  Shape\n",
            "  (area [this] (* Math/PI radius radius)))\n"
        )
    );
}

#[test]
fn keeps_the_clojure_defmethod_dispatch_value_on_the_head_line() {
    let input = "(defmethod encode :json [x] (validate x) (str x))";
    assert_eq!(
        format_clojure(input),
        "(defmethod encode :json [x]\n  (validate x)\n  (str x))\n"
    );
}

#[test]
fn keeps_each_clojure_cond_test_and_result_on_one_line() {
    let input = "(cond (pos? x) :positive (neg? x) :negative :else :zero)";
    assert_eq!(
        format_clojure(input),
        "(cond\n  (pos? x) :positive\n  (neg? x) :negative\n  :else :zero)\n"
    );
}

#[test]
fn keeps_clojure_case_and_condp_clause_pairs_on_one_line() {
    let cases = [
        // `case`'s trailing default has no partner, so it gets its own line.
        ("(case x :a 1 :b 2 3)", "(case x\n  :a 1\n  :b 2\n  3)\n"),
        (
            "(condp = x 1 :one 2 :two :other)",
            "(condp = x\n  1 :one\n  2 :two\n  :other)\n",
        ),
    ];

    for (input, expected) in cases {
        assert_eq!(format_clojure(input), expected, "{input}");
    }
}

#[test]
fn gives_each_clojure_threading_step_its_own_line() {
    let input = "(-> x (assoc :a 1) (update :b inc))";
    assert_eq!(
        format_clojure(input),
        "(-> x\n    (assoc :a 1)\n    (update :b inc))\n"
    );
}

#[test]
fn keeps_the_rebinding_name_of_a_clojure_as_threading_form_on_the_head_line() {
    let input = "(as-> x v (assoc v :a 1) (update v :b inc))";
    assert_eq!(
        format_clojure(input),
        "(as-> x v\n      (assoc v :a 1)\n      (update v :b inc))\n"
    );
}

#[test]
fn formats_clojure_binding_vectors_as_name_value_pairs() {
    let input = "(let [full (str a b) upper (str/upper-case full) n 42] (str \"Hello, \" upper))";
    assert_eq!(
        format_clojure(input),
        concat!(
            "(let [full (str a b)\n",
            "      upper (str/upper-case full)\n",
            "      n 42]\n",
            "  (str \"Hello, \" upper))\n"
        )
    );
}

#[test]
fn keeps_clojure_metadata_attached_to_the_form_it_decorates() {
    // The parser emits `^:private` and `^:const` as siblings of the symbol they
    // decorate, so a layout that counted raw children would give each its own
    // line.
    let cases = [
        (
            "(def ^:private ^:const max-retries 3)",
            "(def ^:private ^:const max-retries 3)\n",
        ),
        (
            "(defn ^:private tagged [x] (log x) (inc x))",
            "(defn ^:private tagged [x]\n  (log x)\n  (inc x))\n",
        ),
    ];

    for (input, expected) in cases {
        assert_eq!(format_clojure(input), expected, "{input}");
    }
}

#[test]
fn keeps_clojure_reader_conditional_feature_pairs_together() {
    let input = "#?(:clj (def platform :jvm) :cljs (def platform :js))";
    assert_eq!(
        format_clojure(input),
        "#?(:clj (def platform :jvm)\n   :cljs (def platform :js))\n"
    );
}

#[test]
fn keeps_short_clojure_forms_with_a_single_body_child_on_one_line() {
    // Breaking these gains nothing: there is no second body form to line the
    // first one up with.
    let cases = [
        "(def data #{1 2 3})",
        "(defmulti encode :type)",
        "(defn- helper [x y] (+ x y))",
        "(deftype Point [x y])",
        "(-> x inc)",
    ];

    for input in cases {
        assert_eq!(format_clojure(input), format!("{input}\n"), "{input}");
    }
}

#[test]
fn keeps_the_head_alone_on_its_line_for_clojure_head_body_forms() {
    // `do`, `try`, and `comment` exist to put the head on a line of its own, so
    // they do not collapse the way the prefix-body styles do.
    let input = "(do (a) (b))";
    assert_eq!(format_clojure(input), "(do\n  (a)\n  (b))\n");
}

#[test]
fn clojure_formatting_is_idempotent() {
    let input = concat!(
        "(ns myapp.core \"Core namespace.\" (:require [clojure.string :as str]))\n",
        "(def ^:private ^:const max-retries 3)\n",
        "(defn greet \"Greets a person.\" {:added \"1.0\"} [name] (str \"Hello, \" name))\n",
        "(defn multi ([x] (multi x 1)) ([x y] (+ x y)))\n",
        "(defrecord Circle [radius] Shape (area [this] (* Math/PI radius radius)))\n",
        "(defn classify [x] (let [y (abs x) z (inc y)] (cond (pos? y) :positive (neg? y) :negative :else :zero)))\n",
        "(defn threaded [x] (-> x (assoc :a 1) (update :b inc) (->> (map inc))))\n",
        "#?(:clj (def platform :jvm) :cljs (def platform :js))\n",
    );

    let formatted = format_clojure(input);
    let reparsed =
        SyntaxTree::parse_with_dialect(&formatted, Dialect::Clojure).expect("output parses again");
    assert_eq!(
        Formatter::with_dialect(2, Dialect::Clojure).format(&reparsed),
        formatted,
        "Clojure formatting must be stable after reparsing"
    );
}

#[test]
fn clojure_layouts_do_not_leak_into_other_dialects() {
    // `Formatter::new` must keep laying every dialect out with the Common Lisp
    // table, so a caller that does not know the dialect sees no change. The
    // input is deliberately too wide to fit on one line: that is the only way
    // the two tables' layouts become visible.
    let input =
        "(defn greet [first-name last-name] (log first-name) (str first-name \" \" last-name))";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("valid Clojure");

    // Outside Clojure `defn` has no layout of its own, so the plain list
    // layout applies: `greet` stays on the head line because it fits, and
    // subsequent elements align under it (Emacs convention for general lists).
    let common_lisp_layout = concat!(
        "(defn greet\n",
        "      [first-name last-name]\n",
        "      (log first-name)\n",
        "      (str first-name \" \" last-name))\n"
    );
    assert_eq!(Formatter::new(2).format(&tree), common_lisp_layout);
    assert_eq!(
        Formatter::with_dialect(2, Dialect::CommonLisp).format(&tree),
        common_lisp_layout
    );
    assert_eq!(
        Formatter::with_dialect(2, Dialect::Clojure).format(&tree),
        concat!(
            "(defn greet [first-name last-name]\n",
            "  (log first-name)\n",
            "  (str first-name \" \" last-name))\n"
        )
    );
}

#[test]
fn with_max_width_narrows_the_inline_fit_threshold() {
    // A plain call (`ListStyle::General`) is the one shape `compact_node`
    // will inline at all — binding/definition-style heads like `defun`/`let`
    // always break onto multiple lines regardless of width.
    let input = "(foo (bar 1 2) (baz 3 4))";
    let tree = SyntaxTree::parse(input).expect("valid");
    let default = Formatter::new(2).format(&tree);
    assert_eq!(
        default, "(foo (bar 1 2) (baz 3 4))\n",
        "fits inline by default"
    );

    let narrowed = Formatter::new(2).with_max_width(10).format(&tree);
    assert_ne!(
        narrowed, default,
        "a budget narrower than the form itself must stop it fitting inline"
    );
    assert!(narrowed.contains('\n'));
}

#[test]
fn with_max_width_widens_the_inline_fit_threshold() {
    // 89 columns: past the compiled-in 80-column default, so this wraps
    // there, but comfortably inside a widened budget.
    let input =
        "(some-function-name argument-one argument-two argument-three argument-four argument-five)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let default = Formatter::new(2).format(&tree);
    assert!(
        default.contains('\n'),
        "89 columns must not fit inline at the compiled-in default"
    );

    let widened = Formatter::new(2).with_max_width(200).format(&tree);
    assert_eq!(
        widened,
        format!("{input}\n"),
        "fits on one line once widened"
    );
}

#[test]
fn with_max_width_moves_the_definition_header_threshold_in_both_directions() {
    // `compact_form` — the whole-form fit test behind a `defsystem` header —
    // was the last width decision still reading the compiled-in constant, so
    // this 74-column form stayed inline however narrow the budget got, and a
    // 96-column one broke however wide it got.
    let short = "(defsystem \"foo\" :description \"short\" :version \"0.1.0\" :depends-on (:asdf))";
    let tree = SyntaxTree::parse(short).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        format!("{short}\n"),
        "74 columns fits inline at the compiled-in default"
    );
    assert_eq!(
        Formatter::new(2).with_max_width(40).format(&tree),
        "(defsystem \"foo\"\n  :description \"short\"\n  :version \"0.1.0\"\n  :depends-on (:asdf))\n",
        "a 40-column budget must break a 74-column header"
    );

    let long = "(defsystem \"foo\" :description \"a description that pushes this past eighty columns\" :version \"0.1.0\")";
    let tree = SyntaxTree::parse(long).expect("valid");
    assert_ne!(
        Formatter::new(2).format(&tree),
        format!("{long}\n"),
        "100 columns must not fit inline at the compiled-in default"
    );
    assert_eq!(
        Formatter::new(2).with_max_width(120).format(&tree),
        format!("{long}\n"),
        "fits on one line once widened"
    );
}

// --- FR-008: `format.indent-table` (per-symbol style overrides) ---

#[test]
fn indent_override_wins_over_the_built_in_style_for_that_symbol() {
    // `frob` is not special-cased anywhere, so it gets `ListStyle::General`
    // by default and inlines like any other call, `format_prefix_body`'s
    // 2-prefix layout only breaks onto a new line past its third child, so a
    // fourth argument is what makes the retargeting visible.
    let input = "(frob a b c)";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(frob a b c)\n",
        "an unmapped symbol is an ordinary call by default"
    );

    let overridden = Formatter::new(2)
        .with_indent_overrides(&[("frob".to_owned(), "if-then-else".to_owned())])
        .expect("if-then-else is a recognised style")
        .format(&tree);
    assert_eq!(
        overridden, "(frob a b\n  c)\n",
        "the override retargets `frob` onto `format_prefix_body`'s 2-prefix layout, \
         the same one `if`/`named-lambda`/two-argument-body forms use"
    );
}

#[test]
fn indent_override_can_retarget_a_built_in_special_form_back_to_general() {
    // `if` is `ListStyle::If` by default, which never inlines regardless of
    // width (see `with_max_width_narrows_the_inline_fit_threshold` above).
    // Overriding it to `general` makes it an ordinary compactable call.
    let input = "(if a b c)";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_ne!(
        Formatter::new(2).format(&tree),
        "(if a b c)\n",
        "`if` never inlines by default"
    );

    let overridden = Formatter::new(2)
        .with_indent_overrides(&[("if".to_owned(), "general".to_owned())])
        .expect("general is a recognised style")
        .format(&tree);
    assert_eq!(overridden, "(if a b c)\n");
}

#[test]
fn with_indent_overrides_rejects_an_unrecognised_style_name_cleanly() {
    let error = Formatter::new(2)
        .with_indent_overrides(&[("frob".to_owned(), "not-a-real-style".to_owned())])
        .expect_err("not-a-real-style is not in STYLE_NAMES");
    assert_eq!(error.style_name, "not-a-real-style");
    assert!(error.to_string().contains("not-a-real-style"));
}

#[test]
fn indent_override_with_a_duplicate_symbol_resolves_to_the_last_entry() {
    // Two `--indent-table` entries for the same symbol are a realistic
    // mistake (e.g. a shell alias plus an explicit override on the same
    // invocation), and the field doc on `Formatter::indent_overrides`
    // promises "later entries... win over earlier ones" — the same
    // last-beats-earlier precedence `packages/core/config/src/load.rs` uses
    // across its own configuration layers. `general` (first) would leave
    // `frob` an ordinary compactable call; `if-then-else` (last) must win.
    let input = "(frob a b c)";
    let tree = SyntaxTree::parse(input).expect("valid");

    let resolved = Formatter::new(2)
        .with_indent_overrides(&[
            ("frob".to_owned(), "general".to_owned()),
            ("frob".to_owned(), "if-then-else".to_owned()),
        ])
        .expect("both are recognised styles")
        .format(&tree);
    assert_eq!(
        resolved, "(frob a b\n  c)\n",
        "the later `if-then-else` entry must win over the earlier `general` one"
    );
}

#[test]
fn indent_overrides_are_a_no_op_when_unset() {
    // The off-by-default case: building a formatter without
    // `with_indent_overrides` must format byte-identically to before this
    // phase existed.
    let input = "(defun add (x y) (+ x y))\n(if a b c)\n(let ((x 1)) x)";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        Formatter::new(2).format(&tree)
    );
}

// --- FR-009: `format.width-profiles` (per-style `--max-width`) ---

#[test]
fn width_profile_narrows_the_inline_fit_threshold_for_its_own_style() {
    let input = "(foo (bar 1 2) (baz 3 4))";
    let tree = SyntaxTree::parse(input).expect("valid");
    let default = Formatter::new(2).format(&tree);
    assert_eq!(
        default, "(foo (bar 1 2) (baz 3 4))\n",
        "fits inline by default"
    );

    let narrowed = Formatter::new(2)
        .with_width_profiles(&[("general".to_owned(), 10)])
        .expect("general is a recognised style")
        .format(&tree);
    assert_ne!(
        narrowed, default,
        "a `general` profile narrower than the global default must stop a plain call fitting inline"
    );
    assert!(narrowed.contains('\n'));
}

#[test]
fn width_profile_widens_the_inline_fit_threshold_for_its_own_style() {
    let input =
        "(some-function-name argument-one argument-two argument-three argument-four argument-five)";
    let tree = SyntaxTree::parse(input).expect("valid");
    // Narrow the *global* width so the form would not fit by default, then
    // confirm a wider `general` profile overrides that global width, the
    // same relationship `with_max_width`'s own "narrows"/"widens" pair above
    // proves for the compiled-in default.
    let base = Formatter::new(2).with_max_width(10);
    assert!(
        base.format(&tree).contains('\n'),
        "does not fit under a 10-column global width"
    );

    let widened = base
        .with_width_profiles(&[("general".to_owned(), 200)])
        .expect("general is a recognised style")
        .format(&tree);
    assert_eq!(
        widened,
        format!("{input}\n"),
        "a `general` profile of 200 must let it fit even though the global width is 10"
    );
}

#[test]
fn width_profile_applies_to_a_form_retargeted_to_general_by_an_indent_override() {
    // `ListStyle::If` (and every non-`General` style) never inlines at all,
    // as a whole form or as a compacted child — see
    // `with_max_width_narrows_the_inline_fit_threshold`'s own comment, and
    // `compact_node`'s own-head check, which refuses *any* node whose head
    // resolves to a non-`General` style before it ever reads a width. So a
    // style's width profile is only observable for `general` (see the two
    // tests above) — including a symbol `format.indent-table` retargeted
    // onto `general`, which this test exercises: FR-009 keyed by the exact
    // classification FR-008 introduces, as specified, means an override
    // determines which profile applies, not that every style gets one.
    let input = "(if a b c)";
    let overridden_general = Formatter::new(2)
        .with_indent_overrides(&[("if".to_owned(), "general".to_owned())])
        .expect("general is a recognised style");
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        overridden_general.format(&tree),
        "(if a b c)\n",
        "retargeted to `general`, it inlines like any other short call"
    );

    let narrowed = overridden_general
        .with_width_profiles(&[("general".to_owned(), 5)])
        .expect("general is a recognised style")
        .format(&tree);
    assert_ne!(
        narrowed, "(if a b c)\n",
        "a 5-column `general` profile must stop the retargeted `if` fitting inline"
    );
    assert!(narrowed.contains('\n'));
}

#[test]
fn width_profile_with_a_duplicate_style_resolves_to_the_last_entry() {
    // Same duplicate-entry precedence check as
    // `indent_override_with_a_duplicate_symbol_resolves_to_the_last_entry`,
    // for `width_profiles`: the field doc promises "later entries... win
    // over earlier ones". `5` (first) would keep this narrow; `200` (last)
    // must win and let it fit inline.
    let input = "(foo (bar 1 2) (baz 3 4))";
    let tree = SyntaxTree::parse(input).expect("valid");

    let resolved = Formatter::new(2)
        .with_width_profiles(&[("general".to_owned(), 5), ("general".to_owned(), 200)])
        .expect("general is a recognised style")
        .format(&tree);
    assert_eq!(
        resolved,
        format!("{input}\n"),
        "the later width-200 entry must win over the earlier width-5 one"
    );
}

#[test]
fn with_width_profiles_rejects_an_unrecognised_style_name_cleanly() {
    let error = Formatter::new(2)
        .with_width_profiles(&[("not-a-real-style".to_owned(), 40)])
        .expect_err("not-a-real-style is not in STYLE_NAMES");
    assert_eq!(error.style_name, "not-a-real-style");
}

#[test]
fn width_profiles_are_a_no_op_when_unset() {
    let input = "(foo (bar 1 2) (baz 3 4))\n(defun add (x y) (+ x y))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        Formatter::new(2).format(&tree)
    );
}

// --- FR-010: `format.quote-style` (reader-prefix printing) ---

#[test]
fn reader_prefix_style_defaults_to_shorthand_byte_identical_to_before() {
    let input = "'(alpha beta)\n`(list ,item ,@rest)\n#'(lambda (value) value)";
    let tree = SyntaxTree::parse(input).expect("valid");
    // Byte-identical to `preserves_common_lisp_reader_prefixes` above,
    // proving the off-by-default case: a `Formatter` nobody called
    // `with_reader_prefix_style` on behaves exactly as it did before this
    // phase.
    assert_eq!(
        Formatter::new(2).format(&tree),
        "'(alpha beta)\n\n`(list ,item ,@rest)\n\n#'(lambda (value)\n    value)\n"
    );
}

#[test]
fn canonical_quote_style_expands_the_quote_prefix_in_every_dialect() {
    for dialect in [
        Dialect::CommonLisp,
        Dialect::EmacsLisp,
        Dialect::Scheme,
        Dialect::Racket,
        Dialect::Clojure,
    ] {
        let tree = SyntaxTree::parse_with_dialect("'(alpha beta)", dialect).expect("valid");
        let rendered = Formatter::with_dialect(2, dialect)
            .with_reader_prefix_style(ReaderPrefixStyle::Canonical)
            .format(&tree);
        assert_eq!(rendered, "(quote (alpha beta))\n", "{}", dialect.label());
    }
}

#[test]
fn canonical_quote_style_expands_the_function_pair_only_in_the_common_lisp_family() {
    let tree = SyntaxTree::parse_with_dialect("#'car", Dialect::CommonLisp).expect("valid");
    let rendered = Formatter::with_dialect(2, Dialect::CommonLisp)
        .with_reader_prefix_style(ReaderPrefixStyle::Canonical)
        .format(&tree);
    assert_eq!(rendered, "(function car)\n");

    // Clojure's `#'` is a different reader form entirely (`ReaderPrefix` is
    // dialect-overloaded — see `quote_edit`'s own doc comment), so it must
    // never become `(function ...)` there.
    let tree = SyntaxTree::parse_with_dialect("#'car", Dialect::Clojure).expect("valid");
    let rendered = Formatter::with_dialect(2, Dialect::Clojure)
        .with_reader_prefix_style(ReaderPrefixStyle::Canonical)
        .format(&tree);
    assert_eq!(rendered, "#'car\n");
}

#[test]
fn canonical_quote_style_never_expands_quasiquote_or_unquote() {
    // Deliberate: `quote_edit`'s own module doc explains why `` ` ``, `,`,
    // and `,@` have no portable list spelling (backquote is not part of
    // ANSI Common Lisp, and implementations disagree on what it expands to).
    // Canonical printing reuses that exact judgment rather than inventing
    // one of its own, so these keep their shorthand even in canonical mode.
    let input = "`(list ,item ,@rest)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let rendered = Formatter::new(2)
        .with_reader_prefix_style(ReaderPrefixStyle::Canonical)
        .format(&tree);
    assert_eq!(rendered, format!("{input}\n"));
}

#[test]
fn canonical_quote_style_leaves_a_stack_mixing_quasiquote_alone() {
    // `` '`x ``: a quote (canonicalizable) stacked with a quasiquote (never
    // canonicalizable, on the same atom node) — the whole stack is left
    // exactly as written rather than rewriting only the outer layer.
    let input = "'`x";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("valid");
    let rendered = Formatter::with_dialect(2, Dialect::CommonLisp)
        .with_reader_prefix_style(ReaderPrefixStyle::Canonical)
        .format(&tree);
    assert_eq!(
        rendered,
        format!("{input}\n"),
        "a stack mixing a canonicalizable prefix with a non-canonicalizable one is left as written"
    );
}

#[test]
fn canonical_quote_style_expands_a_fully_canonicalizable_prefix_stack() {
    // `'#'f`: quote stacked with function — both canonicalizable in Common
    // Lisp, so (unlike the quasiquote case above) the whole stack expands,
    // outermost first.
    let input = "'#'f";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("valid");
    let rendered = Formatter::with_dialect(2, Dialect::CommonLisp)
        .with_reader_prefix_style(ReaderPrefixStyle::Canonical)
        .format(&tree);
    assert_eq!(rendered, "(quote (function f))\n");
}

#[test]
fn canonical_quote_style_round_trips_every_recognised_reader_prefix() {
    // Every reader-prefix form `reader_prefix_spans` recognises in Common
    // Lisp source: `'x`, `` `x ``, `,x`, `,@x`, plus the CL-only `#'x` pair.
    // `'x`/`#'x` canonicalize to their exact list expansion; `` `x ``, `,x`,
    // `,@x` deliberately keep their shorthand (see
    // `canonical_quote_style_never_expands_quasiquote_or_unquote`). Every
    // case's canonical-mode output must still reparse — the literal meaning
    // of "round-trips" for a prefix this mode does not touch.
    let cases = [
        ("'x", "(quote x)\n"),
        ("`x", "`x\n"),
        (",x", ",x\n"),
        (",@x", ",@x\n"),
        ("#'x", "(function x)\n"),
    ];
    for (input, expected) in cases {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("valid");
        let canonical = Formatter::with_dialect(2, Dialect::CommonLisp)
            .with_reader_prefix_style(ReaderPrefixStyle::Canonical)
            .format(&tree);
        assert_eq!(canonical, expected, "{input}");
        SyntaxTree::parse_with_dialect(&canonical, Dialect::CommonLisp)
            .unwrap_or_else(|_| panic!("{input}: canonical output must reparse"));
    }
}

#[test]
fn canonical_quote_style_is_idempotent() {
    // Canonical mode is a far more invasive text transformation than any
    // prior phase's options — it changes the number of parens in the
    // document, not merely spacing or width — so `format(format(x)) ==
    // format(x)` gets its own dedicated case rather than trusting the
    // shorthand-mode property to generalize.
    let inputs = [
        "'x",
        "'(alpha beta)",
        "#'car",
        "`(list ,item ,@rest)",
        "(defun f (x) 'x)",
        "'`x",
        "(list 'a '(b c) `(d ,e))",
    ];
    for input in inputs {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("valid");
        let formatter = Formatter::with_dialect(2, Dialect::CommonLisp)
            .with_reader_prefix_style(ReaderPrefixStyle::Canonical);
        let once = formatter.format(&tree);
        let reparsed = SyntaxTree::parse_with_dialect(&once, Dialect::CommonLisp)
            .unwrap_or_else(|_| panic!("{input}: canonical output must reparse"));
        let twice = formatter.format(&reparsed);
        assert_eq!(once, twice, "{input}: canonical mode must be idempotent");
    }

    // Shorthand mode's idempotency is unaffected by this phase, but the
    // property is worth pinning explicitly for the same inputs rather than
    // assumed from the pre-existing default-mode test coverage.
    for input in inputs {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("valid");
        let formatter = Formatter::with_dialect(2, Dialect::CommonLisp);
        let once = formatter.format(&tree);
        let reparsed = SyntaxTree::parse_with_dialect(&once, Dialect::CommonLisp)
            .unwrap_or_else(|_| panic!("{input}: shorthand output must reparse"));
        let twice = formatter.format(&reparsed);
        assert_eq!(once, twice, "{input}: shorthand mode must be idempotent");
    }
}

#[test]
fn quote_style_is_a_no_op_when_unset() {
    let input = "'(alpha beta)\n`(list ,item ,@rest)";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        Formatter::new(2).format(&tree)
    );
}

// --- FR-011: multiline string literals are never reindented ---

/// The regression this phase exists to pin down as an explicit contract: a
/// multiline string's interior whitespace (leading spaces on a continuation
/// line, a trailing space, a tab) survives byte-for-byte even though the
/// surrounding code — deliberately misindented in the input — gets
/// reindented around it.
#[test]
fn multiline_string_literal_content_is_never_reindented() {
    let input = "(defun f ()\n   \"line one\n   line two (kept)\t\n line three \"\n     (g 1))";
    let tree = SyntaxTree::parse(input).expect("valid");
    let output = Formatter::new(2).format(&tree);
    assert_eq!(
        output,
        "(defun f ()\n  \"line one\n   line two (kept)\t\n line three \"\n  (g 1))\n"
    );
    // The surrounding code was in fact reindented (2 spaces, not the input's
    // 3/5), which is what proves the string's own interior was *not* — the
    // two are the same rewrite, so if the formatter reindented everything
    // uniformly this assertion pair would not be able to tell them apart.
    assert!(output.contains("\n  \"line one\n"));
    assert!(output.contains("\n   line two (kept)\t\n"));
    assert!(output.contains("\n line three \"\n"));
}

#[test]
fn a_multiline_string_survives_reformatting_even_when_it_would_overflow_max_width() {
    let input =
        "(f \"a very long line that is well past forty columns wide\nand a second line   \")";
    let tree = SyntaxTree::parse(input).expect("valid");
    let output = Formatter::new(2).with_max_width(40).format(&tree);
    assert!(
        output.contains(
            "a very long line that is well past forty columns wide\nand a second line   "
        )
    );
}

// --- FR-012: `format.numeric-literal-case` ---

#[test]
fn numeric_literal_case_is_a_no_op_when_unset() {
    let input = "(list #x1F #o17 #b1010 1.0d0 1.0E10)";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("valid");
    assert_eq!(
        Formatter::with_dialect(2, Dialect::CommonLisp).format(&tree),
        "(list #x1F #o17 #b1010 1.0d0 1.0E10)\n"
    );
}

#[test]
fn numeric_literal_case_lowercases_radix_and_exponent_markers() {
    use crate::sexpr::NumericLiteralCase;

    let input = "(list #X1f #O17 #B1010 1.0D0 1.0E10)";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("valid");
    let output = Formatter::with_dialect(2, Dialect::CommonLisp)
        .with_numeric_literal_case(NumericLiteralCase::Lower)
        .format(&tree);
    assert_eq!(output, "(list #x1f #o17 #b1010 1.0d0 1.0e10)\n");
}

#[test]
fn numeric_literal_case_uppercases_radix_and_exponent_markers() {
    use crate::sexpr::NumericLiteralCase;

    let input = "(list #x1f #o17 #b1010 1.0d0 1.0e10)";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("valid");
    let output = Formatter::with_dialect(2, Dialect::CommonLisp)
        .with_numeric_literal_case(NumericLiteralCase::Upper)
        .format(&tree);
    assert_eq!(output, "(list #X1f #O17 #B1010 1.0D0 1.0E10)\n");
}

#[test]
fn numeric_literal_case_never_touches_a_non_numeric_lookalike_symbol() {
    use crate::sexpr::NumericLiteralCase;

    let input = "(list x1e5 e10 not-a-number)";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("valid");
    let output = Formatter::with_dialect(2, Dialect::CommonLisp)
        .with_numeric_literal_case(NumericLiteralCase::Upper)
        .format(&tree);
    assert_eq!(output, "(list x1e5 e10 not-a-number)\n");
}

#[test]
fn numeric_literal_case_is_idempotent() {
    use crate::sexpr::NumericLiteralCase;

    let input = "(list #X1f #O17 1.0D0)";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("valid");
    let formatter = Formatter::with_dialect(2, Dialect::CommonLisp)
        .with_numeric_literal_case(NumericLiteralCase::Lower);
    let once = formatter.format(&tree);
    let reparsed = SyntaxTree::parse_with_dialect(&once, Dialect::CommonLisp).expect("reparses");
    let twice = formatter.format(&reparsed);
    assert_eq!(once, twice);
}

#[test]
fn canonical_quote_style_and_numeric_literal_case_compose_in_format_node() {
    use crate::sexpr::NumericLiteralCase;

    // `format_node`'s Atom arm (multi-line/general rendering, exercised here
    // via a `defun` body — a definition-style head always breaks onto
    // multiple lines regardless of width, see
    // `with_max_width_narrows_the_inline_fit_threshold`'s own comment, so
    // this never routes through `compact_node`). `content` (the
    // numeric-literal-recased text) is computed once and then either wrapped
    // in canonical-prefix heads or emitted directly; this pins that both
    // happen together rather than one silently winning over the other.
    let input = "(defun f () '#x1a)";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("valid");
    let output = Formatter::with_dialect(2, Dialect::CommonLisp)
        .with_reader_prefix_style(ReaderPrefixStyle::Canonical)
        .with_numeric_literal_case(NumericLiteralCase::Upper)
        .format(&tree);
    assert_eq!(
        output, "(defun f ()\n  (quote #X1a))\n",
        "canonical quote wrapper and upper-cased radix marker (digit `a` untouched) must both apply"
    );
}

#[test]
fn canonical_quote_style_and_numeric_literal_case_compose_in_compact_node() {
    use crate::sexpr::NumericLiteralCase;

    // `compact_node`'s Atom arm (inline/compact rendering) — `list` is
    // `ListStyle::General`, the one shape `compact_node` will inline at all,
    // and the whole form comfortably fits the compiled-in 80-column default.
    let input = "(list '#x1a)";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("valid");
    let output = Formatter::with_dialect(2, Dialect::CommonLisp)
        .with_reader_prefix_style(ReaderPrefixStyle::Canonical)
        .with_numeric_literal_case(NumericLiteralCase::Upper)
        .format(&tree);
    assert_eq!(
        output, "(list (quote #X1a))\n",
        "canonical quote wrapper and upper-cased radix marker (digit `a` untouched) must both apply"
    );
}

// --- FR-013: `format.align-clause-values` ---

#[test]
fn align_clause_values_is_a_no_op_when_unset() {
    let input = "(let ((short-name 1) (longer-name 2)) (list short-name longer-name))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        "(let ((short-name 1)\n      (longer-name 2))\n  (list short-name longer-name))\n"
    );
}

#[test]
fn align_clause_values_pads_every_value_to_one_past_the_widest_name() {
    let input = "(let ((short-name 1) (longer-name 2)) (list short-name longer-name))";
    let tree = SyntaxTree::parse(input).expect("valid");
    let output = Formatter::new(2)
        .with_align_clause_values(true)
        .format(&tree);
    assert_eq!(
        output,
        "(let ((short-name  1)\n      (longer-name 2))\n  (list short-name longer-name))\n"
    );
}

#[test]
fn align_clause_values_applies_to_clojure_binding_vectors_too() {
    let input = "(let [short-name 1 longer-name 2] (list short-name longer-name))";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::Clojure).expect("valid");
    let output = Formatter::with_dialect(2, Dialect::Clojure)
        .with_align_clause_values(true)
        .format(&tree);
    assert_eq!(
        output,
        "(let [short-name  1\n      longer-name 2]\n  (list short-name longer-name))\n"
    );
}

#[test]
fn align_clause_values_breaks_the_run_at_a_binding_that_does_not_fit_the_shape() {
    // The middle binding is a bare symbol (no value), which is not a shape
    // alignment applies to: it must not be force-aligned with its
    // neighbours, and it must not pull them into aligning around it either.
    let input = "(let ((a 1) b (ccc 2)) (list a b ccc))";
    let tree = SyntaxTree::parse(input).expect("valid");
    let output = Formatter::new(2)
        .with_align_clause_values(true)
        .format(&tree);
    assert_eq!(
        output,
        "(let ((a 1)\n      b\n      (ccc 2))\n  (list a b ccc))\n"
    );
}

#[test]
fn align_clause_values_never_touches_do_or_prog_var_clauses() {
    // `do`'s var-list allows a third "step" element, which a two-column
    // name/value layout does not fit — FR-013 explicitly scopes alignment to
    // `let`-style bindings and leaves `do`/`prog` alone regardless of the
    // option.
    let input = "(do ((i 0 (1+ i)) (longer-name 1)) ((= i 3)) (print i))";
    let tree = SyntaxTree::parse(input).expect("valid");
    let with_alignment_on = Formatter::new(2)
        .with_align_clause_values(true)
        .format(&tree);
    let with_alignment_off = Formatter::new(2).format(&tree);
    assert_eq!(with_alignment_on, with_alignment_off);
}

#[test]
fn align_clause_values_is_idempotent() {
    let input = "(let ((a 1) (bb 2) (ccc 3)) (list a bb ccc))";
    let tree = SyntaxTree::parse(input).expect("valid");
    let formatter = Formatter::new(2).with_align_clause_values(true);
    let once = formatter.format(&tree);
    let reparsed = SyntaxTree::parse(&once).expect("reparses");
    let twice = formatter.format(&reparsed);
    assert_eq!(once, twice);
}

// --- FR-015: `format.insert-final-newline` / `format.trim-trailing-whitespace` ---

#[test]
fn insert_final_newline_true_is_a_no_op() {
    let input = "(f 1)";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2)
            .with_insert_final_newline(true)
            .format(&tree),
        Formatter::new(2).format(&tree)
    );
}

#[test]
fn insert_final_newline_false_omits_the_trailing_newline() {
    let input = "(f 1)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let output = Formatter::new(2)
        .with_insert_final_newline(false)
        .format(&tree);
    assert_eq!(output, "(f 1)");
}

#[test]
fn trim_trailing_whitespace_true_is_a_no_op() {
    let input = "(f 1) ; comment with trailing space   \n(g 2)";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2)
            .with_trim_trailing_whitespace(true)
            .format(&tree),
        Formatter::new(2).format(&tree)
    );
}

#[test]
fn trim_trailing_whitespace_false_keeps_a_comments_trailing_spaces() {
    let input = "(f 1) ; comment with trailing space   \n(g 2)";
    let tree = SyntaxTree::parse(input).expect("valid");
    let trimmed = Formatter::new(2).format(&tree);
    let untrimmed = Formatter::new(2)
        .with_trim_trailing_whitespace(false)
        .format(&tree);
    assert!(!trimmed.contains("space   \n"));
    assert!(untrimmed.contains("space   \n"));
}

/// Common Lisp `if` aligns all branches at the same distinguished column
/// (two indent steps from the form's opening delimiter), matching Emacs
/// `common-lisp-indent-function`'s `(&rest nil)` convention for `if`.
#[test]
fn indents_a_hugged_form_that_breaks_from_the_column_it_landed_on() {
    let input = "(if (consp value) (multiple-value-bind (copy p) (gethash value copies) (unless p (setf copy value))) value)";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        concat!(
            "(if (consp value)\n",
            "    (multiple-value-bind (copy p) (gethash value copies)\n",
            "      (unless p\n",
            "        (setf copy value)))\n",
            "    value)\n",
        )
    );
}

/// Every slot of a `defclass` slot list shares one column: the slot list is a
/// plain list whose elements are peers, so the second one lines up under the
/// first rather than one indentation step further right.
#[test]
fn aligns_defclass_slots_in_one_column() {
    let input = "(defclass connection (resource) ((mock :initarg :mock :reader connection-mock) (symbol :initarg :symbol :reader connection-symbol)) (:documentation \"A connection.\"))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        concat!(
            "(defclass connection (resource)\n",
            "  ((mock :initarg :mock :reader connection-mock)\n",
            "   (symbol :initarg :symbol :reader connection-symbol))\n",
            "  (:documentation \"A connection.\"))\n",
        )
    );
}

/// `define-condition` uses `DefinitionNameBody`: the name is on the head
/// line and everything else (supers, slot list, options) is at body-indent.
#[test]
fn aligns_define_condition_slots_in_one_column() {
    let input = "(define-condition parse-failure (error) ((offset :initarg :offset :reader parse-failure-offset) (message :initarg :message :reader parse-failure-message)) (:report report-parse-failure))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        concat!(
            "(define-condition parse-failure\n",
            "  (error)\n",
            "  ((offset :initarg :offset :reader parse-failure-offset)\n",
            "   (message :initarg :message :reader parse-failure-message))\n",
            "  (:report report-parse-failure))\n",
        )
    );
}

/// Sibling elements are a column apart from their list's opening delimiter
/// whatever the indentation width is — the gap is "one delimiter", not "one
/// indentation step". Indenting them by `indent` instead is off by
/// `indent - 1`, which an indent of 2 hides as a plausible-looking one-column
/// slip and an indent of 4 does not.
#[test]
fn sibling_alignment_does_not_widen_with_the_indent_width() {
    let input = "(defclass connection (resource) ((mock :initarg :mock :reader connection-mock) (symbol :initarg :symbol :reader connection-symbol)))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(4).format(&tree),
        concat!(
            "(defclass connection (resource)\n",
            "    ((mock :initarg :mock :reader connection-mock)\n",
            "     (symbol :initarg :symbol :reader connection-symbol)))\n",
        )
    );
}

/// A reader prefix shifts the form behind it one column right per prefix
/// character, and its body has to follow. `` `(defun ...) `` at column 2 opens
/// its list at column 3, so the body belongs at column 5 — not at the column 4
/// that ignoring the backquote's own width produced.
#[test]
fn indents_a_backquoted_definition_past_its_reader_prefix() {
    let input = "(defmacro define-checker (name) `(defun ,name (expected) (unless (equal expected (current-value)) (error \"mismatch\"))))";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        concat!(
            "(defmacro define-checker (name)\n",
            "  `(defun ,name (expected)\n",
            "     (unless (equal expected (current-value))\n",
            "       (error \"mismatch\"))))\n",
        )
    );
}

/// A binding list's continuation column is measured in display columns, not
/// in UTF-8 bytes: `束縛` is 6 bytes wide and 4 columns wide, so a byte count
/// pushed the second binding two columns right of the first.
#[test]
fn binding_continuation_columns_count_display_width_not_bytes() {
    let formatter = Formatter::new(2)
        .with_indent_overrides(&[("束縛".to_owned(), "binding-list".to_owned())])
        .expect("binding-list is a known style");
    let tree = SyntaxTree::parse("(束縛 ((変数 1) (別変数 2)) (list 変数 別変数))").expect("valid");
    assert_eq!(
        formatter.format(&tree),
        concat!(
            "(束縛 ((変数 1)\n",
            "       (別変数 2))\n",
            "  (list 変数 別変数))\n",
        )
    );
}

/// Common Lisp `if` branches at the same distinguished column inside a CJK
/// context. `setf`'s second pair (`別変数`) aligns under the first (`変数`)
/// by display column — counting bytes would overshoot because `適合 値` is
/// 6 display columns but 9 UTF-8 bytes.
#[test]
fn a_hugged_form_starts_at_its_display_column_not_its_byte_offset() {
    let input = "(if (適合 値) (setf 変数 (compute-first alpha) 別変数 (compute-second beta)) nil)";
    let tree = SyntaxTree::parse(input).expect("valid");
    assert_eq!(
        Formatter::new(2).format(&tree),
        concat!(
            "(if (適合 値)\n",
            "    (setf 変数 (compute-first alpha)\n",
            "          別変数 (compute-second beta))\n",
            "    nil)\n",
        )
    );
}

/// A byte range in formatted text, half open.
type Region = (usize, usize);

/// The delimiter extents and the opaque token extents of one parse.
///
/// Both are read off the parse tree rather than scanned out of the text,
/// which is the only way this check can be trusted. A hand-written delimiter
/// scanner has to re-implement the reader to know that the `(` in `#\(`,
/// `?\(`, `\(`, `"("` and `|a (b|` is not a delimiter, and getting any one of
/// those wrong invents violations that are not there — see
/// `delimiters_inside_tokens_are_not_delimiters` in `tests/corpus.rs`, whose
/// sibling scan this mirrors. The reader already made every one of those
/// decisions; asking the tree inherits all of them and cannot drift from the
/// parse the formatter itself ran on.
///
/// Lists contribute `content_span`, which starts at the opening delimiter with
/// reader prefixes excluded, so the recorded column is the delimiter's and not
/// the quote's in front of it. Atoms and comments contribute their whole span:
/// a line beginning inside one is a continuation line of a multi-line string,
/// a `|bar symbol|` or a `#| block comment |#`, whose indentation is the
/// author's content rather than the formatter's layout.
fn list_and_opaque_regions(tree: &SyntaxTree) -> (Vec<Region>, Vec<Region>) {
    let mut lists = Vec::new();
    let mut opaque = Vec::new();

    let root = tree.root_view();
    let mut stack = vec![&root];
    while let Some(view) = stack.pop() {
        match view.kind {
            ExpressionKind::List => lists.push((
                view.content_span.start().get(),
                view.content_span.end().get(),
            )),
            ExpressionKind::Atom => opaque.push((view.span.start().get(), view.span.end().get())),
            ExpressionKind::Root => (),
        }
        stack.extend(view.children.iter());
    }
    opaque.extend(
        tree.comments()
            .map(|comment| (comment.span().start().get(), comment.span().end().get())),
    );

    (lists, opaque)
}

/// The display column `offset` sits at, counting from zero.
///
/// Display width rather than bytes: the formatter lines a continuation up
/// under a column it measured with [`UnicodeWidthStr::width`], and `適合 値`
/// is 9 bytes wide and 6 columns wide. Measuring in bytes here reports the
/// enclosing delimiter three columns right of where it is, which is enough to
/// turn a correctly indented body into a violation.
fn display_column_of(text: &str, offset: usize) -> usize {
    let line_start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    UnicodeWidthStr::width(&text[line_start..offset])
}

/// Asserts that no line of `formatted` begins at or left of the display column
/// of the opening delimiter that encloses it.
///
/// Two kinds of line are exempt, each because the formatter did not choose its
/// indentation: a line whose first character closes a list (`))` belongs to
/// the form it closes, and every convention in the family puts it at the
/// column of that form's *parent*), and a line that begins inside an atom or a
/// comment (the second and later lines of a multi-line string or block
/// comment, whose indentation is content).
fn assert_no_line_outdents_past_its_delimiter(formatted: &str, dialect: Dialect) {
    let tree = SyntaxTree::parse_with_dialect(formatted, dialect)
        .expect("the formatter's own output reparses");
    let (lists, opaque) = list_and_opaque_regions(&tree);

    let mut line_start = 0usize;
    for raw in formatted.split_inclusive('\n') {
        let line = raw.trim_end_matches(['\n', '\r']);
        let offset = line_start;
        line_start += raw.len();

        let Some(indent_bytes) = line.find(|character: char| !character.is_whitespace()) else {
            continue;
        };
        let first = offset + indent_bytes;

        if opaque
            .iter()
            .any(|(start, end)| *start < first && first < *end)
        {
            continue;
        }
        if line[indent_bytes..].starts_with([')', ']', '}']) {
            continue;
        }
        // Lists are properly nested, so the containing one that starts latest
        // is the innermost.
        let Some(enclosing) = lists
            .iter()
            .filter(|(start, end)| *start < first && first < *end)
            .map(|(start, _)| *start)
            .max()
        else {
            continue;
        };

        let indentation = UnicodeWidthStr::width(&line[..indent_bytes]);
        let enclosing_column = display_column_of(formatted, enclosing);
        assert!(
            indentation > enclosing_column,
            "line {line:?} starts at column {indentation}, \
             at or left of its enclosing delimiter at {enclosing_column}\n{formatted}"
        );
    }
}

/// No line may start at or left of the column its enclosing form's opening
/// delimiter sits at, across the shapes above plus the ones the rest of this
/// file covers. Checked structurally rather than against a golden, so a
/// layout change that reintroduces an outdented child fails here even where
/// no golden pins that exact form.
///
/// The last three inputs exist to keep the *check* honest rather than the
/// formatter. Measuring the enclosing delimiter's column in bytes puts
/// `(progn` in the CJK input at 17 instead of 14, below its own body's 16;
/// counting raw parentheses in the text leaves the string input's `(format`
/// list open across the line after it and reads `#\(` as opening a list the
/// following `#\)` line then sits left of. All three pass the tree-driven
/// display-width scan and fail a byte-and-text-scanning one, so a regression
/// to either shortcut is loud instead of latent.
#[test]
fn no_line_is_indented_at_or_left_of_its_enclosing_delimiter() {
    let inputs = [
        "(if (consp value) (multiple-value-bind (copy p) (gethash value copies) (unless p (setf copy value))) value)",
        "(defmacro define-checker (name) `(defun ,name (expected) (unless (equal expected (current-value)) (error \"mismatch\"))))",
        "(defclass connection (resource) ((mock :initarg :mock :reader connection-mock) (symbol :initarg :symbol :reader connection-symbol)))",
        "(labels ((copy-reference (value) (if (consp value) (multiple-value-bind (copy p) (gethash value copies) (unless p (setf copy value))) value))) (copy-reference root))",
        "(cond ((first-predicate value) (first-branch value)) ((second-predicate value) (second-branch value)))",
        "(loop for item in items do (process item) (record item) finally (report))",
        "(let ((alpha (compute-first-value)) (beta (compute-second-value))) (combine alpha beta))",
        "(if (適合 値) (progn (実行 値) (記録 値)) nil)",
        "(defun f (a) (format nil \"left ( paren\" a) (list a \"))\" a))",
        "(list #\\( #\\) (defun g (x) (progn (h x) (i x))))",
    ];

    for indent in [2usize, 4] {
        for input in inputs {
            let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("valid");
            let formatted = Formatter::with_dialect(indent, Dialect::CommonLisp).format(&tree);
            assert_no_line_outdents_past_its_delimiter(&formatted, Dialect::CommonLisp);
        }
    }
}

/// A reader prefix on a *child* the layout has a special shape for — a
/// binding list, an `flet` binding, a `cond`/`case` clause, a `do` var-list —
/// survives the reformat.
///
/// [`Formatter::format_node`] writes a node's prefixes before it dispatches on
/// the node's head, so a renderer may open its own subject's delimiter freely.
/// Six renderers in `formatter/lists` opened a *child's* delimiter too, and
/// nothing had written that child's prefix: `` (let `(a b) x) `` came back as
/// `(let (a b) x)`.
///
/// That is the worst shape a defect in this tool can take. The output still
/// parses, so every reparse-based write guard passes it, and formatting it
/// again reproduces it exactly, so idempotence passes too. Only comparing the
/// input tree with the output tree sees it — which is what
/// `no_shape_specific_layout_drops_a_child_reader_prefix` below does, and what
/// `tests/corpus.rs`'s invariant 6 does at scale.
#[test]
fn a_reader_prefix_on_a_shape_specific_child_survives_formatting() {
    let cases = [
        // The primary-dialect repro. `let`'s second child is laid out by
        // `format_sequence_list`, which opened `(` itself.
        (Dialect::CommonLisp, "(let `(a b) x)", "(let `(a b)\n  x)\n"),
        (Dialect::CommonLisp, "(let '(a b) x)", "(let '(a b)\n  x)\n"),
        (
            Dialect::CommonLisp,
            "(let* `(a b) x)",
            "(let* `(a b)\n  x)\n",
        ),
        // `flet`'s bindings list, via `format_local_callable_bindings` …
        (
            Dialect::CommonLisp,
            "(flet `((f (x) x)) y)",
            "(flet `((f (x) x))\n  y)\n",
        ),
        // … and one binding inside it, via `format_local_callable_binding`.
        (
            Dialect::CommonLisp,
            "(flet ('(f (x) x)) y)",
            "(flet ('(f (x) x))\n  y)\n",
        ),
        // `#.` is a prefix like any other here: dropping it turns a read-time
        // computation into an ordinary call.
        (
            Dialect::CommonLisp,
            "(flet (#.(f (x) x)) y)",
            "(flet (#.(f (x) x))\n  y)\n",
        ),
        // `cond` and `case` clauses, via `format_body_clause`.
        (
            Dialect::CommonLisp,
            "(cond '(a b c))",
            "(cond\n  '(a b c))\n",
        ),
        (
            Dialect::CommonLisp,
            "(case x '(a b c))",
            "(case x\n  '(a b c))\n",
        ),
        // `do`'s var-list, via `format_clause_sequence_form`.
        (
            Dialect::CommonLisp,
            "(do `((i 0 (1+ i))) ((> i 3)) x)",
            "(do `((i 0 (1+ i)))\n  ((> i 3))\n  x)\n",
        ),
        // Not a Common Lisp defect wearing other dialects' clothes: the
        // renderers are shared, so every dialect that reaches one has it.
        (
            Dialect::Janet,
            "(do x ~(def a b))",
            "(do x\n  ~(def a b))\n",
        ),
        (
            Dialect::Janet,
            "(do x ,(def a b))",
            "(do x\n  ,(def a b))\n",
        ),
        (Dialect::Fennel, "(do x `(fn a b))", "(do x\n  `(fn a b))\n"),
        // Clojure's binding *vector* resolves to the same `ListStyle::Binding`.
        (Dialect::Clojure, "(let `[a b] x)", "(let `[a b]\n  x)\n"),
    ];

    for (dialect, input, expected) in cases {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("valid");
        assert_eq!(
            Formatter::with_dialect(2, dialect).format(&tree),
            expected,
            "{} / {input}",
            dialect.label()
        );
    }
}

/// The same defect at a width that cannot compact, so the fallback is the
/// multi-line renderer rather than `compact_node`.
///
/// Pinned separately because the two paths emit a prefix for different
/// reasons — `compact_node` writes `reader_prefix_spans` itself, while
/// `format_node` writes them before dispatching — so a fix reaching only one
/// of them would leave this case corrupting.
#[test]
fn a_reader_prefix_survives_on_a_binding_list_too_wide_to_compact() {
    let input = "(let `((alpha 111111111) (beta 222222222) (gamma 33333333) \
                 (delta 4444444) (epsilon 5555555)) x)";
    let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("valid");
    let formatted = Formatter::with_dialect(2, Dialect::CommonLisp).format(&tree);
    assert!(
        formatted.starts_with("(let `("),
        "quasiquote dropped from a wide binding list:\n{formatted}"
    );
}

/// The tree-equality oracle as a test rather than as a corpus sweep: parse the
/// input, parse the formatted output, and compare the reader-prefix stack at
/// every expression.
///
/// Spans are excluded because moving them is the formatter's whole job.
/// A prefix stack is not, and it is invisible to every other check in this
/// file, because dropping one leaves text that parses.
#[test]
fn no_shape_specific_layout_drops_a_child_reader_prefix() {
    fn prefix_stacks(tree: &SyntaxTree) -> Vec<Vec<crate::sexpr::ReaderPrefix>> {
        let root = tree.root_view();
        let mut stacks = Vec::new();
        let mut stack = vec![&root];
        while let Some(view) = stack.pop() {
            stacks.push(view.reader_prefixes.clone());
            stack.extend(view.children.iter().rev());
        }
        stacks
    }

    let cases = [
        (Dialect::CommonLisp, "(let `(a b) x)"),
        (Dialect::CommonLisp, "(let* '((x 1)) x)"),
        (Dialect::CommonLisp, "(flet `((f (x) x)) y)"),
        (Dialect::CommonLisp, "(labels ('(f (x) (g x))) y)"),
        (Dialect::CommonLisp, "(cond '(a b c) `(d e f))"),
        (Dialect::CommonLisp, "(case x '(a b c))"),
        (Dialect::CommonLisp, "(ecase x `((a) b c))"),
        (Dialect::CommonLisp, "(do `((i 0 (1+ i))) '((> i 3) r) x)"),
        (
            Dialect::CommonLisp,
            "(defmacro m (v) `(symbol-macrolet ,(loop for x in v collect x) body))",
        ),
        (Dialect::Clojure, "(let `[a b] x)"),
        (Dialect::Clojure, "(with-open `[a b] body)"),
        (Dialect::Janet, "(do x ~(def a b))"),
        (Dialect::Fennel, "(do x `(fn a b))"),
        // A prefix on the last element of a list, which is where an
        // off-by-one in prefix handling shows up first.
        (Dialect::CommonLisp, "(a b 'c)"),
        (Dialect::CommonLisp, "(let ((x 1)) 'y)"),
        (Dialect::CommonLisp, "(cond (a 'b) (t 'c))"),
    ];

    for (dialect, input) in cases {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("valid input");
        let formatted = Formatter::with_dialect(2, dialect).format(&tree);
        let reparsed =
            SyntaxTree::parse_with_dialect(&formatted, dialect).expect("formatted output reparses");
        assert_eq!(
            prefix_stacks(&tree),
            prefix_stacks(&reparsed),
            "{} / {input} formatted to:\n{formatted}",
            dialect.label()
        );
    }
}
