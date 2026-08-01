use super::*;

#[test]
fn cli_builds_gated_refactor_plan_for_agents() {
    let dir = fresh_temp_dir("refactor plan");
    let file = dir.join("core.lisp");
    write_fixture(
        &file,
        r#"(defpackage #:demo.core (:use #:cl))
(in-package #:demo.core)
(defun render-pane (pane) (draw-pane pane))
(defun caller () (render-pane window) render-pane)
"#,
    );

    let output = run_refactor_plan_json(&file, "render-pane", "rename");

    assert_json_string_field(&output.json, "operation", "rename");
    assert_json_string_field(&output.json, "symbol", "render-pane");
    assert_json_string_field(&output.json, "target_kind", "callable");
    assert_eq!(
        output
            .json
            .pointer("/decision/status")
            .and_then(serde_json::Value::as_str),
        Some("ready")
    );
    assert_eq!(
        output
            .json
            .pointer("/decision/next_action")
            .and_then(serde_json::Value::as_str),
        Some("apply-symbol-rename")
    );
    assert!(output.stdout.contains("\"safe_to_automate\": true"));
    assert!(output.stdout.contains("\"policy_passed\": true"));
    assert!(output.stdout.contains("\"blocking_gate_count\": 0"));
    assert_eq!(
        output
            .json
            .pointer("/risk_summary/highest_level")
            .and_then(serde_json::Value::as_str),
        Some("warning")
    );
    assert_eq!(
        output
            .json
            .pointer("/risk_summary/warning_count")
            .and_then(serde_json::Value::as_u64),
        Some(2)
    );
    assert_eq!(
        output
            .json
            .pointer("/risk_summary/error_count")
            .and_then(serde_json::Value::as_u64),
        Some(0)
    );
    assert_eq!(
        output
            .json
            .pointer("/risk_summary/blocking_count")
            .and_then(serde_json::Value::as_u64),
        Some(0)
    );
    assert_eq!(
        output
            .json
            .pointer("/risk_summary/advisory_count")
            .and_then(serde_json::Value::as_u64),
        Some(2)
    );
    assert!(output.stdout.contains("\"name\": \"plan-policy\""));
    assert!(output.stdout.contains("\"name\": \"manual-review-gates\""));
    assert!(output.stdout.contains("\"name\": \"apply-plan\""));
    assert!(output.stdout.contains("\"status\": \"scheduled\""));
    assert!(output.stdout.contains("\"definition_count\": 1"));
    assert!(output.stdout.contains("\"call_count\": 1"));
    assert!(output.stdout.contains("\"code\": \"non-call-references\""));
    assert!(output.stdout.contains("\"action\": \"run-impact-report\""));
    assert!(output.stdout.contains("--fail-on-risk-level warning"));
    assert!(output.stdout.contains("--require-definitions 1"));
    assert!(output.stdout.contains("--require-references 1"));
    assert!(output.stdout.contains("--require-calls 1"));
    assert!(output.stdout.contains(
        "paredit refactor rename-symbols --from 'render-pane' --to <new-symbol> --output json"
    ));
}

#[test]
fn cli_reports_overall_risk_and_apply_next_command_for_a_clean_plan() {
    let dir = fresh_temp_dir("refactor plan-overall-risk-clean");
    let file = dir.join("core.lisp");
    write_fixture(
        &file,
        r#"(defpackage #:demo.core (:use #:cl))
(in-package #:demo.core)
(defun render-pane (pane) (draw-pane pane))
(defun caller () (render-pane window) render-pane)
"#,
    );

    let output = run_refactor_plan_json(&file, "render-pane", "rename");

    // FR-003: no blocking gate exists (see `cli_builds_gated_refactor_plan_for_agents`
    // for the full risk breakdown this same fixture produces), so the
    // aggregate is "advisory" rather than "clean" — the two info/warning
    // gates it does carry are non-blocking, not absent.
    assert_eq!(
        output.json.pointer("/risk_summary/overall_risk"),
        Some(&Value::String("advisory".to_owned()))
    );

    // FR-004: nothing blocks automation, so the suggestion is the command
    // that applies the plan, not a "look at the finding" command.
    let next_commands = output
        .json
        .pointer("/next_commands")
        .and_then(Value::as_array)
        .expect("next_commands is present for a plan with an apply step");
    assert!(
        next_commands.iter().any(|command| command["command"]
            .as_str()
            .is_some_and(|command| command
                .starts_with("paredit refactor rename-symbols --from 'render-pane'"))),
        "{next_commands:?}"
    );
}

#[test]
fn cli_builds_refactor_plan_for_macro_targets() {
    assert_refactor_plan_target(
        "refactor plan-macro",
        r#"(defpackage #:demo.core (:use #:cl))
(in-package #:demo.core)
(defmacro render-pane (pane) `(draw-pane ,pane))
(defun caller () (render-pane window))
"#,
        "render-pane",
        "rename",
        "macro",
        "apply-macro-rename",
        "paredit refactor rename-function --from 'render-pane' --to <new-symbol> --output json",
    );
}

#[test]
fn cli_builds_refactor_plan_for_define_modify_macro_targets() {
    assert_refactor_plan_target(
        "refactor plan-modify-macro",
        r#"(defpackage #:demo.core (:use #:cl))
(in-package #:demo.core)
(define-modify-macro updatef (place) incf)
(defun caller () (updatef counter))
"#,
        "updatef",
        "rename",
        "macro",
        "apply-macro-rename",
        "paredit refactor rename-function --from 'updatef' --to <new-symbol> --output json",
    );
}

#[test]
fn cli_builds_refactor_plan_for_compiler_macro_targets() {
    assert_refactor_plan_target(
        "refactor plan-compiler-macro",
        r#"(defpackage #:demo.core (:use #:cl))
(in-package #:demo.core)
(define-compiler-macro fast-add (x y) `(+ ,x ,y))
(defun caller () (fast-add 1 2))
"#,
        "fast-add",
        "rename",
        "compiler_macro",
        "apply-macro-rename",
        "paredit refactor rename-function --from 'fast-add' --to <new-symbol> --output json",
    );
}

#[test]
fn cli_builds_refactor_plan_for_setf_expander_targets() {
    assert_refactor_plan_target(
        "refactor plan-setf-expander",
        r#"(defpackage #:demo.core (:use #:cl))
(in-package #:demo.core)
(define-setf-expander accessor (place) (values nil nil '(store) '(writer store) '(reader place)))
(defun caller (item) (setf (accessor item) 1))
"#,
        "accessor",
        "rename",
        "setf_expander",
        "apply-macro-rename",
        "paredit refactor rename-function --from 'accessor' --to <new-symbol> --output json",
    );
}

#[test]
fn cli_builds_refactor_plan_for_defsetf_targets() {
    assert_refactor_plan_target(
        "refactor plan-defsetf",
        r#"(defpackage #:demo.core (:use #:cl))
(in-package #:demo.core)
(defsetf slot accessor)
(defun caller (item) (setf (slot item) 1))
"#,
        "slot",
        "rename",
        "setf_expander",
        "apply-macro-rename",
        "rename-function --from 'slot' --to <new-symbol>",
    );
}

#[test]
fn cli_builds_refactor_plan_for_symbol_macro_targets() {
    assert_refactor_plan_target(
        "refactor plan-symbol-macro",
        r#"(defpackage #:demo.core (:use #:cl))
(in-package #:demo.core)
(define-symbol-macro current-user "guest")
(defun caller () current-user)
"#,
        "current-user",
        "rename",
        "symbol_macro",
        "apply-symbol-macro-rename",
        "paredit refactor rename-symbol-macro --from 'current-user' --to <new-symbol> --output json",
    );
}

#[test]
fn cli_builds_refactor_plan_for_package_qualified_common_lisp_callable_targets() {
    assert_refactor_plan_target(
        "refactor plan-qualified-common-lisp-callable",
        r#"(defpackage #:demo.core (:use #:cl))
(in-package #:demo.core)
(defun cl-user:render-pane (pane) pane)
(defun caller () (render-pane window))
"#,
        "render-pane",
        "rename",
        "callable",
        "apply-rename",
        "paredit refactor rename-function --from 'render-pane' --to <new-symbol> --output json",
    );
}

#[test]
fn cli_builds_refactor_plan_for_symbol_macro_signature_targets() {
    let dir = fresh_temp_dir("refactor plan-symbol-macro-signature");
    let file = dir.join("core.lisp");
    write_fixture(
        &file,
        r#"(defpackage #:demo.core (:use #:cl))
(in-package #:demo.core)
(define-symbol-macro current-user "guest")
(defun caller () current-user)
"#,
    );

    let output = run_refactor_plan_json(&file, "current-user", "signature");

    assert_json_string_field(&output.json, "operation", "signature");
    assert_json_string_field(&output.json, "symbol", "current-user");
    assert_json_string_field(&output.json, "target_kind", "symbol_macro");
    assert_eq!(
        output
            .json
            .pointer("/decision/status")
            .and_then(serde_json::Value::as_str),
        Some("manual_review")
    );
    assert_eq!(
        output
            .json
            .pointer("/decision/next_action")
            .and_then(serde_json::Value::as_str),
        Some("review-signature-scope")
    );
    assert_eq!(
        output
            .json
            .pointer("/decision/safe_to_automate")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
    assert!(output.stdout.contains("\"command\": null"));
    assert!(!output.stdout.contains("add-function-parameter"));
}

#[test]
fn cli_builds_refactor_plan_for_macro_signature_targets() {
    let dir = fresh_temp_dir("refactor plan-macro-signature");
    let file = dir.join("core.lisp");
    write_fixture(
        &file,
        r#"(defpackage #:demo.core (:use #:cl))
(in-package #:demo.core)
(defmacro render-pane (pane) `(draw-pane ,pane))
(defun caller () (render-pane window))
"#,
    );

    let output = run_refactor_plan_json(&file, "render-pane", "signature");

    assert_json_string_field(&output.json, "operation", "signature");
    assert_json_string_field(&output.json, "symbol", "render-pane");
    assert_json_string_field(&output.json, "target_kind", "macro");
    assert_eq!(
        output
            .json
            .pointer("/decision/status")
            .and_then(serde_json::Value::as_str),
        Some("manual_review")
    );
    assert_eq!(
        output
            .json
            .pointer("/decision/next_action")
            .and_then(serde_json::Value::as_str),
        Some("review-signature-scope")
    );
    assert_eq!(
        output
            .json
            .pointer("/decision/safe_to_automate")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
    assert!(output.stdout.contains("\"command\": null"));
    assert!(!output.stdout.contains("--require-calls 1"));
    assert!(!output.stdout.contains("add-function-parameter"));
}
