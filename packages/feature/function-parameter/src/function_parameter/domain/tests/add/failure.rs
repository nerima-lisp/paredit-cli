use super::*;

#[test]
fn rejects_unqualified_duplicate_of_package_qualified_common_lisp_parameter() {
    let input = "(defun area (cl:stream) stream)\n(print (area 3))";
    let error = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("stream"),
        argument: "4".to_owned(),
        call_paths: vec![path("1.1")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect_err("duplicate parameter must fail");

    assert!(
        error
            .to_string()
            .contains("parameter 'stream' already exists in area")
    );
}

#[test]
fn rejects_add_function_parameter_for_common_lisp_short_form_defsetf() {
    let input = "(defsetf access update-access)\n(setf (access item) 1)";
    let error = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("mode"),
        argument: ":rw".to_owned(),
        call_paths: vec![path("1")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect_err("short-form defsetf must fail");

    assert!(
        error
            .to_string()
            .contains("does not support short-form defsetf")
    );
}

#[test]
fn rejects_add_common_lisp_optional_parameter_when_call_omits_existing_optional_argument() {
    let input = "(defun render (node &optional stream) (list node stream style))\n(render item)";
    let error = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("style"),
        argument: ":compact".to_owned(),
        call_paths: vec![path("1")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect_err("missing optional position must fail");

    assert!(
        error
            .to_string()
            .contains("does not have 2 positional argument(s) before optional argument")
    );
}

#[test]
fn rejects_add_optional_parameter_before_key_when_call_omits_existing_optional_argument() {
    let input = "(defun render (node &optional stream &key color) (list node stream style color))\n(render item :color :red)";
    let error = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("style"),
        argument: ":compact".to_owned(),
        call_paths: vec![path("1")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Optional,
    })
    .expect_err("missing optional position before keyword arguments must fail");

    assert!(
        error
            .to_string()
            .contains("does not have 2 positional argument(s) before optional argument")
    );
}

#[test]
fn rejects_add_common_lisp_key_parameter_with_duplicate_call_keyword() {
    let input =
        "(defun render (node &key color) (list node color margin))\n(render item :margin 4)";
    let error = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("margin"),
        argument: "8".to_owned(),
        call_paths: vec![path("1")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect_err("duplicate keyword must fail");

    assert!(
        error
            .to_string()
            .contains("already contains keyword argument :margin")
    );
}
