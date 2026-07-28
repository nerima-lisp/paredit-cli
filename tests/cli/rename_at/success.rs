#[test]
fn cli_rename_at_respects_quote_and_quasiquote_depth() {
    let input = "(let ((value xs)) (list value 'value `(value ,value ,@value)))\n";

    let rewritten = write_rename_at(
        "rename-at-reader-boundaries",
        None,
        input,
        "value xs",
        "items",
    );

    assert_eq!(
        rewritten,
        "(let ((items xs)) (list items 'value `(value ,items ,@items)))\n"
    );
}

#[test]
fn cli_rename_at_stops_at_shadowing_bindings() {
    let input = "(let ((value 1)) (+ value (let ((value 2)) value) value))\n";

    let rewritten = write_rename_at("rename-at-shadowing", None, input, "value 1", "outer");

    assert_eq!(
        rewritten,
        "(let ((outer 1)) (+ outer (let ((value 2)) value) outer))\n"
    );
}

#[test]
fn cli_rename_at_common_lisp_keeps_value_and_function_namespaces_separate() {
    let input = "(let ((value 1)) (list value (value)))\n";

    let rewritten = write_rename_at(
        "rename-at-common-lisp-lisp-2",
        None,
        input,
        "value 1",
        "item",
    );

    assert_eq!(rewritten, "(let ((item 1)) (list item (value)))\n");
}

#[test]
fn cli_rename_at_tracks_macrolet_definition_and_calls() {
    let input = "(macrolet ((emit (x) `(list ,x))) (emit 1) #'emit)\n";

    let rewritten = write_rename_at("rename-at-macrolet", None, input, "emit (x)", "produce");

    assert_eq!(
        rewritten,
        "(macrolet ((produce (x) `(list ,x))) (produce 1) #'emit)\n"
    );
}

#[test]
fn cli_rename_at_tracks_symbol_macrolet_value_references() {
    let input =
        "(symbol-macrolet ((place (car cell))) (list place (let ((place 1)) place) 'place))\n";

    let rewritten = write_rename_at(
        "rename-at-symbol-macrolet",
        None,
        input,
        "place (car",
        "slot",
    );

    assert_eq!(
        rewritten,
        "(symbol-macrolet ((slot (car cell))) (list slot (let ((place 1)) place) 'place))\n"
    );
}
use super::*;

#[test]
fn cli_rename_at_renames_a_scheme_letrec_binding_and_its_recursive_call() {
    // A Lisp-1: the `f` in head position is the very binding being renamed.
    // Skipping head position the way a Common Lisp variable rename must would
    // leave the call sites naming a procedure that no longer exists.
    let renamed = write_rename_at(
        "rename-at-scheme-letrec",
        Some("scheme"),
        "(letrec ((f (lambda (n) (g n))) (g (lambda (n) (f n)))) (f 1))",
        "f (lambda",
        "even-step",
    );

    assert_eq!(
        renamed,
        "(letrec ((even-step (lambda (n) (g n))) (g (lambda (n) (even-step n)))) (even-step 1))"
    );
}

#[test]
fn cli_rename_at_renames_a_bracketed_scheme_binding() {
    // `(let ([x 1]) x)` is the dominant spelling in Racket and legal in every
    // R6RS reader.
    let renamed = write_rename_at(
        "rename-at-scheme-bracket",
        Some("scheme"),
        "(define (area x) (let ([side (+ x 1)]) (* side side)))",
        "side (+",
        "edge",
    );

    assert_eq!(
        renamed,
        "(define (area x) (let ([edge (+ x 1)]) (* edge edge)))"
    );
}

#[test]
fn cli_rename_at_renames_a_scheme_named_let_loop_variable() {
    let renamed = write_rename_at(
        "rename-at-scheme-named-let",
        Some("scheme"),
        "(define (run) (let loop ((i 0)) (loop i)))",
        "loop ((i",
        "again",
    );

    assert_eq!(renamed, "(define (run) (let again ((i 0)) (again i)))");
}

#[test]
fn cli_rename_at_renames_a_scheme_let_values_formal() {
    let renamed = write_rename_at(
        "rename-at-scheme-let-values",
        Some("scheme"),
        "(let-values (((a b) (values 1 2))) (+ a b))",
        "a b)",
        "first",
    );

    assert_eq!(
        renamed,
        "(let-values (((first b) (values 1 2))) (+ first b))"
    );
}

#[test]
fn cli_rename_at_renames_a_scheme_define_parameter_without_touching_the_name() {
    // `(define (f x) ...)` keeps the procedure name and its parameters in one
    // node. Reading that node as a plain parameter list made `f` look bound by
    // itself.
    let renamed = write_rename_at(
        "rename-at-scheme-define-parameter",
        Some("scheme"),
        "(define (scale x) (* x x))",
        "x) (*",
        "factor",
    );

    assert_eq!(renamed, "(define (scale factor) (* factor factor))");
}
