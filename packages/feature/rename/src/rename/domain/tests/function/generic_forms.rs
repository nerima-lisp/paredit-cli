use super::*;

#[test]
fn renames_define_method_combination_definition_and_designators() {
    assert_function_rename! {
        input: "(define-method-combination render-combination (pane theme) ((primary *)) (list pane theme primary))\n(list #'render-combination (function render-combination) render-combination)",
        dialect: Dialect::CommonLisp,
        from: "render-combination",
        to: "compose-render",
        definitions: 1,
        calls: 2,
        changed: true,
        rewritten_contains: [
            "(define-method-combination compose-render (pane theme)",
            "#'compose-render",
            "(function compose-render)",
            " render-combination)"
        ]
    };
}

#[test]
fn renames_defgeneric_and_associated_defmethod_definitions_and_calls() {
    assert_function_rename! {
        input: "(defgeneric render (node stream))\n(defmethod render ((node widget) stream) (render node stream))\n(defmethod render :around ((node panel) stream) #'render (function render) (render node stream))\n(render thing out)",
        dialect: Dialect::CommonLisp,
        from: "render",
        to: "draw",
        definitions: 3,
        calls: 5,
        changed: true,
        rewritten_contains: [
            "(defgeneric draw (node stream))",
            "(defmethod draw ((node widget) stream) (draw node stream))",
            "(defmethod draw :around ((node panel) stream) #'draw (function draw) (draw node stream))",
            "(draw thing out)"
        ]
    };
}

#[test]
fn renames_common_lisp_user_qualified_defgeneric_and_defmethod_definitions_and_calls() {
    assert_function_rename! {
        input: "(cl-user:defgeneric render (node stream))\n(cl-user:defmethod render ((node widget) stream) (render node stream))\n(cl-user:defmethod render :around ((node panel) stream) #'render (function render) (render node stream))\n(render thing out)",
        dialect: Dialect::CommonLisp,
        from: "render",
        to: "draw",
        definitions: 3,
        calls: 5,
        changed: true,
        rewritten_contains: [
            "(cl-user:defgeneric draw (node stream))",
            "(cl-user:defmethod draw ((node widget) stream) (draw node stream))",
            "(cl-user:defmethod draw :around ((node panel) stream) #'draw (function draw) (draw node stream))",
            "(draw thing out)"
        ]
    };
}

#[test]
fn renames_emacs_lisp_cl_defgeneric_and_cl_defmethod_definitions_and_calls() {
    assert_function_rename! {
        input: "(cl-defgeneric render (node stream))\n(cl-defmethod render ((node widget) stream) (render node stream))\n(cl-defmethod render :around ((node panel) stream) #'render (function render) (render node stream))\n(render thing out)",
        dialect: Dialect::EmacsLisp,
        from: "render",
        to: "draw",
        definitions: 3,
        calls: 5,
        changed: true,
        rewritten_contains: [
            "(cl-defgeneric draw (node stream))",
            "(cl-defmethod draw ((node widget) stream) (draw node stream))",
            "(cl-defmethod draw :around ((node panel) stream) #'draw (function draw) (draw node stream))",
            "(draw thing out)"
        ]
    };
}

#[test]
fn renames_setf_generic_function_and_method_designators() {
    assert_function_rename! {
        input: "(defgeneric (setf accessor) (value object))\n(defmethod (setf accessor) (value (object widget)) #'(setf accessor) (function (setf accessor)) (setf (accessor object) value))\n(setf (accessor thing) 1)",
        dialect: Dialect::CommonLisp,
        from: "accessor",
        to: "slot-accessor",
        definitions: 2,
        calls: 4,
        changed: true,
        rewritten_contains: [
            "(defgeneric (setf slot-accessor) (value object))",
            "(defmethod (setf slot-accessor) (value (object widget)) #'(setf slot-accessor) (function (setf slot-accessor)) (setf (slot-accessor object) value))",
            "(setf (slot-accessor thing) 1)"
        ]
    };
}
