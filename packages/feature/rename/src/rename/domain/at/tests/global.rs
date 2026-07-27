#[test]
fn renames_global_function_from_call_head() {
    let input = "(defun render (x) (render x))";
    let plan = plan_rename_at(request(input, "render x", "draw")).expect("plan");
    assert_eq!(plan.namespace, RenameAtNamespace::Function);
    assert_eq!(plan.rewritten, "(defun draw (x) (draw x))");
}

#[test]
fn renames_global_definition_calls_and_callable_designators() {
    let input = "(defun render (x) (render x)) (list #'render (function render) #:render)";
    let plan = plan_rename_at(RenameAtRequest {
        input,
        dialect: Dialect::CommonLisp,
        at: ByteOffset::new(input.find("#'render").expect("designator") + 2),
        to: SymbolName::new("draw").unwrap(),
    })
    .expect("plan");
    assert_eq!(plan.namespace, RenameAtNamespace::Function);
    assert_eq!(
        plan.rewritten,
        "(defun draw (x) (draw x)) (list #'draw (function draw) #:render)"
    );
}

#[test]
fn renames_global_macro_definition_calls_and_function_designators() {
    let input = "(defmacro emit (form) `(list ,form)) (emit value) #'emit 'emit emit";
    let plan = plan_rename_at(request(input, "emit (form)", "produce")).expect("plan");
    assert_eq!(plan.namespace, RenameAtNamespace::GlobalMacro);
    assert_eq!(
        plan.rewritten,
        "(defmacro produce (form) `(list ,form)) (produce value) #'produce 'emit emit"
    );
}

#[test]
fn renames_global_compiler_macro_definition_calls_and_function_designators() {
    let input = "(define-compiler-macro emit (form) `(list ,form)) (emit value) #'emit 'emit emit";
    let plan = plan_rename_at(request(input, "emit (form)", "produce")).expect("plan");
    assert_eq!(plan.namespace, RenameAtNamespace::GlobalMacro);
    assert_eq!(
        plan.rewritten,
        "(define-compiler-macro produce (form) `(list ,form)) (produce value) #'produce 'emit emit"
    );
}
use super::*;
