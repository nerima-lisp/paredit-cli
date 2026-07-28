use super::*;

proptest! {
    #![proptest_config(cli_proptest_config_replaying_recorded_failures(24))]

    #[test]
    fn pbt_cli_add_function_parameter_output_remains_parseable(
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

        let dir = fresh_temp_dir("add-function-parameter-pbt");
        let path = dir.join("fixture.lisp");
        let input = format!(
            "(defun {name} ({a} {b}) (list {a} {b} {c}))\n(print ({name} {first} {second}))\n"
        );
        fs::write(&path, &input).map_err(|err| TestCaseError::fail(format!("write fixture: {err}")))?;

        let output = paredit()
            .arg("refactor").arg("add-function-parameter")
            .arg("--file")
            .arg(&path)
            .arg("--definition-path")
            .arg("0")
            .arg("--name")
            .arg(&c)
            .arg("--argument")
            .arg(&third)
            .arg("--all-calls")
            .arg("--write")
            .arg("--output")
            .arg("json")
            .output()
            .map_err(|err| TestCaseError::fail(format!("run add-function-parameter: {err}")))?;

        prop_assert!(
            output.status.success(),
            "stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );

        let report = parse_add_function_parameter_report(&output.stdout)?;
        prop_assert!(report.all_calls);
        prop_assert!(report.changed);
        prop_assert!(report.written);
        prop_assert_eq!(&report.function_name, &name);
        prop_assert_eq!(&report.parameter_name, &c);
        prop_assert_eq!(&report.argument, &third);
        prop_assert_eq!(report.insert, "end");
        prop_assert_eq!(report.parameter_section, "required");
        let expected = format!(
            "(defun {name} ({a} {b} {c}) (list {a} {b} {c}))\n(print ({name} {first} {second} {third}))\n"
        );
        prop_assert_eq!(report.rewritten.as_str(), expected.as_str());
        prop_assert_eq!(
            fs::read_to_string(&path).map_err(|err| TestCaseError::fail(format!("read rewritten fixture: {err}")))?,
            report.rewritten
        );
        assert_cli_check_succeeds(&path)?;
    }
}
