use super::*;

fn generated_thread_first_chain(depth: usize) -> String {
    let mut expression = "seed".to_owned();
    for index in 0..depth {
        expression = format!("(f{index} {expression})");
    }
    expression
}

fn generated_thread_last_chain(depth: usize) -> String {
    let mut expression = "seed".to_owned();
    for index in 0..depth {
        expression = format!("(g{index} arg{index} {expression})");
    }
    expression
}

fn generated_thread_first_pipeline(depth: usize) -> String {
    let mut forms = vec!["->".to_owned(), "seed".to_owned()];
    for index in 0..depth {
        if index % 2 == 0 {
            forms.push(format!("f{index}"));
        } else {
            forms.push(format!("(f{index} arg{index})"));
        }
    }
    format!("({})", forms.join(" "))
}

fn generated_thread_last_pipeline(depth: usize) -> String {
    let mut forms = vec!["->>".to_owned(), "seed".to_owned()];
    for index in 0..depth {
        if index % 2 == 0 {
            forms.push(format!("g{index}"));
        } else {
            forms.push(format!("(g{index} arg{index})"));
        }
    }
    format!("({})", forms.join(" "))
}

fn expected_thread_first_pipeline(depth: usize) -> String {
    let mut forms = vec!["->".to_owned(), "seed".to_owned()];
    for index in 0..depth {
        forms.push(format!("f{index}"));
    }
    format!("({})", forms.join(" "))
}

fn expected_thread_last_pipeline(depth: usize) -> String {
    let mut forms = vec!["->>".to_owned(), "seed".to_owned()];
    for index in 0..depth {
        forms.push(format!("(g{index} arg{index})"));
    }
    format!("({})", forms.join(" "))
}

fn expected_unthread_first_expression(depth: usize) -> String {
    let mut expression = "seed".to_owned();
    for index in 0..depth {
        expression = if index % 2 == 0 {
            format!("(f{index} {expression})")
        } else {
            format!("(f{index} {expression} arg{index})")
        };
    }
    expression
}

fn expected_unthread_last_expression(depth: usize) -> String {
    let mut expression = "seed".to_owned();
    for index in 0..depth {
        expression = if index % 2 == 0 {
            format!("(g{index} {expression})")
        } else {
            format!("(g{index} arg{index} {expression})")
        };
    }
    expression
}

fn assert_thread_expression_property(
    input: String,
    style: &str,
    expected_replacement: String,
) -> Result<(), TestCaseError> {
    let output = paredit()
        .args([
            "refactor",
            "thread-expression",
            "--dialect",
            "clojure",
            "--path",
            "0",
            "--style",
            style,
            "--output",
            "json",
        ])
        .write_stdin(input)
        .output()
        .map_err(|err| TestCaseError::fail(format!("run paredit: {err}")))?;

    prop_assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = serde_json::from_slice::<serde_json::Value>(&output.stdout)
        .map_err(|err| TestCaseError::fail(format!("parse json: {err}")))?;
    prop_assert_eq!(report["changed"].as_bool(), Some(true));
    let replacement = report["replacement"].as_str().unwrap_or_default();
    prop_assert_eq!(replacement, expected_replacement.as_str());

    let rewritten = report["rewritten"].as_str().unwrap_or_default();
    prop_assert_eq!(rewritten, expected_replacement.as_str());
    let check_output = paredit()
        .arg("inspect")
        .arg("check")
        .write_stdin(rewritten.to_owned())
        .output()
        .map_err(|err| TestCaseError::fail(format!("run check: {err}")))?;
    prop_assert!(
        check_output.status.success(),
        "check stderr={}",
        String::from_utf8_lossy(&check_output.stderr)
    );
    Ok(())
}

fn assert_unthread_expression_property(
    input: String,
    expected_replacement: String,
) -> Result<(), TestCaseError> {
    let output = paredit()
        .args([
            "refactor",
            "unthread-expression",
            "--dialect",
            "clojure",
            "--path",
            "0",
            "--output",
            "json",
        ])
        .write_stdin(input)
        .output()
        .map_err(|err| TestCaseError::fail(format!("run paredit: {err}")))?;

    prop_assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = serde_json::from_slice::<serde_json::Value>(&output.stdout)
        .map_err(|err| TestCaseError::fail(format!("parse json: {err}")))?;
    prop_assert_eq!(report["changed"].as_bool(), Some(true));
    let replacement = report["replacement"].as_str().unwrap_or_default();
    prop_assert_eq!(replacement, expected_replacement.as_str());
    let rewritten = report["rewritten"].as_str().unwrap_or_default();
    prop_assert_eq!(rewritten, expected_replacement.as_str());

    let check_output = paredit()
        .arg("inspect")
        .arg("check")
        .write_stdin(rewritten.to_owned())
        .output()
        .map_err(|err| TestCaseError::fail(format!("run check: {err}")))?;
    prop_assert!(
        check_output.status.success(),
        "check stderr={}",
        String::from_utf8_lossy(&check_output.stderr)
    );
    Ok(())
}

proptest! {
    #![proptest_config(cli_proptest_config_replaying_recorded_failures(
        12,
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/cli/thread_expression/pbt.proptest-regressions"),
    ))]

    #[test]
    fn cli_thread_expression_preserves_parseability_for_generated_chains(depth in 1usize..8) {
        assert_thread_expression_property(
            generated_thread_first_chain(depth),
            "first",
            expected_thread_first_pipeline(depth),
        )?;
        assert_thread_expression_property(
            generated_thread_last_chain(depth),
            "last",
            expected_thread_last_pipeline(depth),
        )?;
    }

    #[test]
    fn cli_unthread_expression_preserves_parseability_for_generated_pipelines(depth in 1usize..8) {
        assert_unthread_expression_property(
            generated_thread_first_pipeline(depth),
            expected_unthread_first_expression(depth),
        )?;
        assert_unthread_expression_property(
            generated_thread_last_pipeline(depth),
            expected_unthread_last_expression(depth),
        )?;
    }
}
