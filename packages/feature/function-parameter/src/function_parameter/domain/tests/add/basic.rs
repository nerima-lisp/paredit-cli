use super::*;

#[test]
fn adds_parameter_to_definition_and_call() {
    let input = "(defun area (w) w)\n(print (area 3))";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("h"),
        argument: "4".to_owned(),
        call_paths: vec![path("1.1")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("plan");

    assert_eq!(plan.function_name.as_str(), "area");
    assert_eq!(plan.section, FunctionParameterSection::Positional);
    assert_eq!(plan.rewritten, "(defun area (w h) w)\n(print (area 3 4))");
    assert!(plan.changed);
}

#[test]
fn add_parameter_preserves_dialect_reader_collisions() {
    let cases = [(
        Dialect::Janet,
        "(defn area [w] w)\n(area 3)\n# ignored ))",
        "[value # ignored ))\n next]",
        "(defn area [w h] w)\n(area 3 [value # ignored ))\n next])\n# ignored ))",
    )];

    for (dialect, input, argument, expected) in cases {
        let plan = plan_add_function_parameter(AddFunctionParameterRequest {
            input,
            dialect,
            definition_path: path("0"),
            name: symbol("h"),
            argument: argument.to_owned(),
            call_paths: vec![path("1")],
            all_calls: false,
            insert: FunctionParameterInsert::End,
            section: FunctionParameterSection::Auto,
        })
        .expect("plan");

        assert_eq!(plan.rewritten, expected);
        SyntaxTree::parse_with_dialect(&plan.rewritten, dialect)
            .expect("rewritten output remains parseable");
    }
}

#[test]
fn adds_parameter_to_package_qualified_common_lisp_definition() {
    let input = "(cl:defun area (w) w)\n(print (area 3))";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("h"),
        argument: "4".to_owned(),
        call_paths: vec![path("1.1")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Auto,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(cl:defun area (w h) w)\n(print (area 3 4))"
    );
}

#[test]
fn adds_common_lisp_key_parameter_to_definition_and_call() {
    let input =
        "(defun render (node &key color) (list node color margin))\n(render item :color :red)";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
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
    .expect("plan");

    assert_eq!(plan.section, FunctionParameterSection::Keyword);
    assert_eq!(
        plan.rewritten,
        "(defun render (node &key color margin) (list node color margin))\n(render item :color :red :margin 8)"
    );
}

#[test]
fn adds_common_lisp_key_parameter_before_allow_other_keys() {
    let input = "(defun render (node &key color &allow-other-keys) (list node color margin))\n(render item :color :red)";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
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
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun render (node &key color margin &allow-other-keys) (list node color margin))\n(render item :color :red :margin 8)"
    );
}

#[test]
fn adds_common_lisp_optional_parameter_to_definition_and_call() {
    let input =
        "(defun render (node &optional stream) (list node stream style))\n(render item out)";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
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
    .expect("plan");

    assert_eq!(plan.section, FunctionParameterSection::Optional);
    assert_eq!(
        plan.rewritten,
        "(defun render (node &optional stream style) (list node stream style))\n(render item out :compact)"
    );
}

#[test]
fn adds_common_lisp_optional_parameter_at_start() {
    let input = "(defun render (node &optional stream mode) (list node stream mode style))\n(render item out :wide)";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("style"),
        argument: ":compact".to_owned(),
        call_paths: vec![path("1")],
        all_calls: false,
        insert: FunctionParameterInsert::Start,
        section: FunctionParameterSection::Auto,
    })
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun render (node &optional style stream mode) (list node stream mode style))\n(render item :compact out :wide)"
    );
}

#[test]
fn adds_common_lisp_optional_parameter_before_key_section() {
    let input = "(defun render (node &optional stream &key color) (list node stream style color))\n(render item out :color :red)";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
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
    .expect("plan");

    assert_eq!(
        plan.rewritten,
        "(defun render (node &optional stream style &key color) (list node stream style color))\n(render item out :compact :color :red)"
    );
}

#[test]
fn creates_common_lisp_optional_section_when_explicitly_requested() {
    let input = "(defun render (node) (list node style))\n(render item)";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
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
    .expect("plan");

    assert_eq!(plan.section, FunctionParameterSection::Optional);
    assert_eq!(
        plan.rewritten,
        "(defun render (node &optional style) (list node style))\n(render item :compact)"
    );
}

#[test]
fn creates_common_lisp_optional_section_before_existing_key_section() {
    let input =
        "(defun render (node &key color) (list node style color))\n(render item :color :red)";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
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
    .expect("plan");

    assert_eq!(plan.section, FunctionParameterSection::Optional);
    assert_eq!(
        plan.rewritten,
        "(defun render (node &optional style &key color) (list node style color))\n(render item :compact :color :red)"
    );
}

#[test]
fn creates_common_lisp_keyword_section_when_explicitly_requested() {
    let input = "(defun render (node) (list node margin))\n(render item)";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("margin"),
        argument: "8".to_owned(),
        call_paths: vec![path("1")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Keyword,
    })
    .expect("plan");

    assert_eq!(plan.section, FunctionParameterSection::Keyword);
    assert_eq!(
        plan.rewritten,
        "(defun render (node &key margin) (list node margin))\n(render item :margin 8)"
    );
}

#[test]
fn creates_common_lisp_keyword_section_after_existing_optional_section() {
    let input =
        "(defun render (node &optional stream) (list node stream margin))\n(render item out)";
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input,
        dialect: Dialect::CommonLisp,
        definition_path: path("0"),
        name: symbol("margin"),
        argument: "8".to_owned(),
        call_paths: vec![path("1")],
        all_calls: false,
        insert: FunctionParameterInsert::End,
        section: FunctionParameterSection::Keyword,
    })
    .expect("plan");

    assert_eq!(plan.section, FunctionParameterSection::Keyword);
    assert_eq!(
        plan.rewritten,
        "(defun render (node &optional stream &key margin) (list node stream margin))\n(render item out :margin 8)"
    );
}
