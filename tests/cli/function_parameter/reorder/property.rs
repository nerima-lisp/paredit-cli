use super::*;

proptest! {
    #![proptest_config(cli_proptest_config_replaying_recorded_failures(24))]

    #[test]
    fn pbt_cli_reorder_function_parameters_output_remains_parseable(
        name in "[a-z][a-z0-9]{0,8}",
        a in "[a-z][a-z0-9]{0,8}",
        b in "[a-z][a-z0-9]{0,8}",
        c in "[a-z][a-z0-9]{0,8}",
        first in "[-]?[0-9]{1,4}",
        second in "[-]?[0-9]{1,4}",
        third in "[-]?[0-9]{1,4}",
    ) {
        prop_assume!(name != a);
        prop_assume!(name != b);
        prop_assume!(name != c);
        prop_assume!(a != b);
        prop_assume!(a != c);
        prop_assume!(b != c);

        let dir = fresh_temp_dir("reorder-function-parameters-pbt");
        let path = dir.join("fixture.lisp");
        let input = format!(
            "(defun {name} ({a} {b} {c}) (list {a} {b} {c}))\n(print ({name} {first} {second} {third}))\n"
        );
        fs::write(&path, &input).map_err(|err| TestCaseError::fail(format!("write fixture: {err}")))?;

        let output = reorder_command()
            .arg("--file")
            .arg(&path)
            .arg("--definition-path")
            .arg("0")
            .args(["--parameter", &c, "--parameter", &a, "--parameter", &b])
            .arg("--all-calls")
            .arg("--write")
            .arg("--output")
            .arg("json")
            .output()
            .map_err(|err| TestCaseError::fail(format!("run reorder-function-parameters: {err}")))?;

        prop_assert!(
            output.status.success(),
            "stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );

        let report = parse_reorder_function_parameters_report(&output.stdout)?;
        prop_assert!(report.all_calls);
        prop_assert!(report.changed);
        prop_assert!(report.written);
        prop_assert_eq!(&report.function_name, &name);
        prop_assert_eq!(report.old_parameter_order, vec![a.clone(), b.clone(), c.clone()]);
        prop_assert_eq!(report.new_parameter_order, vec![c.clone(), a.clone(), b.clone()]);
        prop_assert_eq!(
            report.reordered_arguments,
            vec![vec![third.clone(), first.clone(), second.clone()]]
        );
        let expected = format!(
            "(defun {name} ({c} {a} {b}) (list {a} {b} {c}))\n(print ({name} {third} {first} {second}))\n"
        );
        prop_assert_eq!(report.rewritten.as_str(), expected.as_str());
        prop_assert_eq!(
            fs::read_to_string(&path).map_err(|err| TestCaseError::fail(format!("read rewritten fixture: {err}")))?,
            report.rewritten
        );
        assert_cli_check_succeeds(&path)?;
    }
}
