use super::*;

#[test]
fn cli_applies_refactor_preview_manifest_with_hash_guards() {
    let dir = fresh_temp_dir("refactor apply");
    let lisp_file = dir.join("core.lisp");
    let manifest_file = dir.join("rename.preview.json");
    let original = "(defun old-name (x) x)\n(defun caller () (old-name 1) old-name)\n";
    fs::write(&lisp_file, original).expect("write lisp fixture");
    let canonical_root = fs::canonicalize(&dir).expect("canonicalize refactor root");

    let mut preview = paredit();
    let preview_output = preview
        .args(["refactor", "preview"])
        .arg("--from")
        .arg("old-name")
        .arg("--to")
        .arg("new-name")
        .arg("--mode")
        .arg("function")
        .arg("--fail-on-no-change")
        .arg("--fail-on-parse-error")
        .arg("--require-definitions")
        .arg("1")
        .arg("--require-edits")
        .arg("2")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let manifest_text = String::from_utf8(preview_output).expect("preview output is utf8");
    let manifest_hash = stable_manifest_hash(&manifest_text);
    fs::write(&manifest_file, &manifest_text).expect("write refactor manifest");

    let mut dry_run = paredit();
    dry_run
        .args(["refactor", "apply"])
        .arg("--manifest")
        .arg(&manifest_file)
        .arg("--expect-manifest-hash")
        .arg(&manifest_hash)
        .arg("--root")
        .arg(&dir)
        .arg("--output")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"root\": {"))
        .stdout(predicate::str::contains("\"enforced\": true"))
        .stdout(predicate::str::contains(
            canonical_root.display().to_string(),
        ))
        .stdout(predicate::str::contains("\"hash\": \"fnv1a64:"))
        .stdout(predicate::str::contains(manifest_hash.clone()))
        .stdout(predicate::str::contains("\"write_requested\": false"))
        .stdout(predicate::str::contains("\"status\": \"dry-run-ready\""))
        .stdout(predicate::str::contains(
            "\"next_action\": \"rerun_refactor_apply_with_write\"",
        ))
        .stdout(predicate::str::contains("\"blocked_reasons\": []"))
        .stdout(predicate::str::contains("\"steps\": ["))
        .stdout(predicate::str::contains("\"name\": \"manifest-policy\""))
        .stdout(predicate::str::contains("\"name\": \"apply-write\""))
        .stdout(predicate::str::contains("\"status\": \"scheduled\""))
        .stdout(predicate::str::contains("\"decision_summary\": {"))
        .stdout(predicate::str::contains("\"passed_step_count\": 4"))
        .stdout(predicate::str::contains("\"failed_step_count\": 0"))
        .stdout(predicate::str::contains("\"skipped_step_count\": 0"))
        .stdout(predicate::str::contains("\"scheduled_step_count\": 1"))
        .stdout(predicate::str::contains("\"blocked_reason_count\": 0"))
        .stdout(predicate::str::contains("\"applied\": false"))
        .stdout(predicate::str::contains("\"changed_file_count\": 1"))
        .stdout(predicate::str::contains("\"changed_files\": ["))
        .stdout(predicate::str::contains("core.lisp"))
        .stdout(predicate::str::contains("\"written_file_count\": 0"))
        .stdout(predicate::str::contains("\"stale_file_count\": 0"))
        .stdout(predicate::str::contains(
            "\"output_hash_mismatch_count\": 0",
        ))
        .stdout(predicate::str::contains(
            "\"manifest_flag_mismatch_count\": 0",
        ))
        .stdout(predicate::str::contains("\"input_hash_matches\": true"))
        .stdout(predicate::str::contains("\"output_hash_matches\": true"))
        .stdout(predicate::str::contains("\"written\": false"));

    assert_eq!(
        fs::read_to_string(&lisp_file).expect("read dry-run fixture"),
        original
    );

    let mut apply = paredit();
    apply
        .args(["refactor", "apply"])
        .arg("--manifest")
        .arg(&manifest_file)
        .arg("--expect-manifest-hash")
        .arg(&manifest_hash)
        .arg("--root")
        .arg(&dir)
        .arg("--write")
        .arg("--output")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"enforced\": true"))
        .stdout(predicate::str::contains("\"write_requested\": true"))
        .stdout(predicate::str::contains("\"status\": \"applied\""))
        .stdout(predicate::str::contains(
            "\"next_action\": \"run_verification_or_review_diff\"",
        ))
        .stdout(predicate::str::contains("\"name\": \"apply-write\""))
        .stdout(predicate::str::contains("\"status\": \"passed\""))
        .stdout(predicate::str::contains("\"decision_summary\": {"))
        .stdout(predicate::str::contains("\"passed_step_count\": 5"))
        .stdout(predicate::str::contains("\"failed_step_count\": 0"))
        .stdout(predicate::str::contains("\"skipped_step_count\": 0"))
        .stdout(predicate::str::contains("\"scheduled_step_count\": 0"))
        .stdout(predicate::str::contains("\"blocked_reason_count\": 0"))
        .stdout(predicate::str::contains("\"applied\": true"))
        .stdout(predicate::str::contains("\"written_file_count\": 1"))
        .stdout(predicate::str::contains("\"written\": true"));

    assert_eq!(
        fs::read_to_string(&lisp_file).expect("read rewritten fixture"),
        "(defun new-name (x) x)\n(defun caller () (new-name 1) old-name)\n"
    );
}

#[test]
fn cli_refactor_apply_reports_headline_and_compact_text_output() {
    let dir = fresh_temp_dir("refactor apply-headline");
    let lisp_file = dir.join("core.lisp");
    let original = "(defun old-name (x) x)\n";
    fs::write(&lisp_file, original).expect("write lisp fixture");

    let mut preview = paredit();
    let preview_output = preview
        .args(["refactor", "preview"])
        .arg("--from")
        .arg("old-name")
        .arg("--to")
        .arg("new-name")
        .arg("--mode")
        .arg("function")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let manifest_text = String::from_utf8(preview_output).expect("preview output is utf8");
    let manifest_file = dir.join("rename.preview.json");
    fs::write(&manifest_file, &manifest_text).expect("write refactor manifest");

    // FR-006: JSON always carries the headline, dry run or not.
    let mut dry_run = paredit();
    dry_run
        .args(["refactor", "apply"])
        .arg("--manifest")
        .arg(&manifest_file)
        .arg("--output")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"headline\": \"1 renamed definition.\"",
        ));

    // FR-006: `--compact` text output is the headline and nothing else.
    let mut compact = paredit();
    compact
        .args(["refactor", "apply"])
        .arg("--manifest")
        .arg(&manifest_file)
        .arg("--compact")
        .arg("--output")
        .arg("text")
        .assert()
        .success()
        .stdout("1 renamed definition.\n");

    // Without --compact, text mode still prints the full field-by-field
    // report, with a headline row alongside everything else.
    let mut verbose_text = paredit();
    verbose_text
        .args(["refactor", "apply"])
        .arg("--manifest")
        .arg(&manifest_file)
        .arg("--output")
        .arg("text")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "headline\t1 renamed definition.\n",
        ))
        .stdout(predicate::str::contains("manifest_path\t"));

    assert_eq!(
        fs::read_to_string(&lisp_file).expect("read unwritten dry-run fixture"),
        original
    );
}

#[test]
fn cli_refactor_apply_next_commands_suggest_write_when_dry_run_ready() {
    let dir = fresh_temp_dir("refactor apply-next-commands-dry-run");
    let lisp_file = dir.join("core.lisp");
    fs::write(&lisp_file, "(defun old-name (x) x)\n").expect("write lisp fixture");
    let manifest_file = dir.join("rename.preview.json");

    let mut preview = paredit();
    let preview_output = preview
        .args(["refactor", "preview"])
        .arg("--from")
        .arg("old-name")
        .arg("--to")
        .arg("new-name")
        .arg("--mode")
        .arg("function")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    fs::write(&manifest_file, preview_output).expect("write refactor manifest");

    let mut apply = paredit();
    let assert = apply
        .args(["refactor", "apply"])
        .arg("--manifest")
        .arg(&manifest_file)
        .arg("--output")
        .arg("json")
        .assert()
        .success();
    let json: serde_json::Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("apply output is valid json");
    let next_commands = json
        .pointer("/next_commands")
        .and_then(serde_json::Value::as_array)
        .expect("next_commands is present for a dry-run-ready manifest");
    assert!(
        next_commands.iter().any(|command| {
            command["command"].as_str().is_some_and(|command| {
                command.contains("refactor apply")
                    && command.contains("--write")
                    && command.contains(&manifest_file.display().to_string())
            })
        }),
        "{next_commands:?}"
    );
}

#[test]
fn cli_refactor_apply_next_commands_suggest_recheck_when_blocked() {
    let dir = fresh_temp_dir("refactor apply-next-commands-blocked");
    let lisp_file = dir.join("core.lisp");
    fs::write(&lisp_file, "(defun old-name (x) x)\n").expect("write lisp fixture");
    let manifest_file = dir.join("rename.preview.json");

    let mut preview = paredit();
    let preview_output = preview
        .args(["refactor", "preview"])
        .arg("--from")
        .arg("old-name")
        .arg("--to")
        .arg("new-name")
        .arg("--mode")
        .arg("function")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let manifest_text = String::from_utf8(preview_output)
        .expect("preview output is utf8")
        .replace("\"passed\": true", "\"passed\": false");
    fs::write(&manifest_file, manifest_text).expect("write failed refactor manifest");

    let mut apply = paredit();
    let assert = apply
        .args(["refactor", "apply"])
        .arg("--manifest")
        .arg(&manifest_file)
        .arg("--output")
        .arg("json")
        .assert()
        .failure();
    let json: serde_json::Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("apply output is valid json");
    let next_commands = json
        .pointer("/next_commands")
        .and_then(serde_json::Value::as_array)
        .expect("next_commands is present for a blocked manifest");
    assert!(
        next_commands.iter().any(|command| {
            command["command"].as_str().is_some_and(|command| {
                command.contains("refactor check")
                    && command.contains(&manifest_file.display().to_string())
            })
        }),
        "{next_commands:?}"
    );
}

#[test]
fn cli_refactor_apply_rejects_manifest_paths_outside_root_without_writing() {
    let dir = fresh_temp_dir("refactor apply-root-guard");
    let root = dir.join("workspace");
    let outside_file = dir.join("outside.lisp");
    let manifest_file = dir.join("rename.preview.json");
    fs::create_dir_all(&root).expect("create guarded workspace");
    let original = "(defun old-name (x) x)\n";
    fs::write(&outside_file, original).expect("write outside fixture");

    let mut preview = paredit();
    let preview_output = preview
        .args(["refactor", "preview"])
        .arg("--from")
        .arg("old-name")
        .arg("--to")
        .arg("new-name")
        .arg("--mode")
        .arg("function")
        .arg("--fail-on-no-change")
        .arg("--fail-on-parse-error")
        .arg("--output")
        .arg("json")
        .arg(&outside_file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    fs::write(&manifest_file, preview_output).expect("write refactor manifest");

    let mut apply = paredit();
    apply
        .args(["refactor", "apply"])
        .arg("--manifest")
        .arg(&manifest_file)
        .arg("--root")
        .arg(&root)
        .arg("--write")
        .arg("--output")
        .arg("json")
        .assert()
        .failure()
        .stderr(predicate::str::contains("outside refactor root"));

    assert_eq!(
        fs::read_to_string(&outside_file).expect("read unchanged outside fixture"),
        original
    );
}

#[test]
fn cli_refuses_stale_refactor_apply_manifest_without_writing() {
    let dir = fresh_temp_dir("refactor apply-stale");
    let lisp_file = dir.join("core.lisp");
    let manifest_file = dir.join("rename.preview.json");
    fs::write(
        &lisp_file,
        "(defun old-name (x) x)\n(defun caller () (old-name 1) old-name)\n",
    )
    .expect("write lisp fixture");
    let canonical_root = fs::canonicalize(&dir).expect("canonicalize refactor root");

    let mut preview = paredit();
    let preview_output = preview
        .args(["refactor", "preview"])
        .arg("--from")
        .arg("old-name")
        .arg("--to")
        .arg("new-name")
        .arg("--mode")
        .arg("function")
        .arg("--fail-on-no-change")
        .arg("--fail-on-parse-error")
        .arg("--require-definitions")
        .arg("1")
        .arg("--require-edits")
        .arg("2")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    fs::write(&manifest_file, preview_output).expect("write refactor manifest");

    let stale = "(defun old-name (x) (+ x 1))\n(defun caller () (old-name 1) old-name)\n";
    fs::write(&lisp_file, stale).expect("mutate lisp fixture after preview");

    let mut apply = paredit();
    apply
        .args(["refactor", "apply"])
        .arg("--manifest")
        .arg(&manifest_file)
        .arg("--root")
        .arg(&dir)
        .arg("--write")
        .arg("--output")
        .arg("json")
        .assert()
        .failure()
        .stdout(predicate::str::contains("\"root\": {"))
        .stdout(predicate::str::contains("\"enforced\": true"))
        .stdout(predicate::str::contains(
            canonical_root.display().to_string(),
        ))
        .stdout(predicate::str::contains("\"status\": \"blocked\""))
        .stdout(predicate::str::contains(
            "\"next_action\": \"regenerate_refactor_preview\"",
        ))
        .stdout(predicate::str::contains("\"stale_files\""))
        .stdout(predicate::str::contains("\"applied\": false"))
        .stdout(predicate::str::contains("\"written_file_count\": 0"))
        .stdout(predicate::str::contains("\"stale_file_count\": 1"))
        .stdout(predicate::str::contains("\"input_hash_matches\": false"))
        .stderr(predicate::str::contains("refactor apply validation failed"));

    assert_eq!(
        fs::read_to_string(&lisp_file).expect("read stale fixture"),
        stale
    );
}

#[test]
fn cli_refactor_apply_reports_policy_failed_manifest_without_writing() {
    let dir = fresh_temp_dir("refactor apply-policy-failed");
    let lisp_file = dir.join("core.lisp");
    let manifest_file = dir.join("rename.preview.json");
    let original = "(defun old-name (x) x)\n";
    fs::write(&lisp_file, original).expect("write lisp fixture");

    let mut preview = paredit();
    let preview_output = preview
        .args(["refactor", "preview"])
        .arg("--from")
        .arg("old-name")
        .arg("--to")
        .arg("new-name")
        .arg("--mode")
        .arg("function")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let manifest_text = String::from_utf8(preview_output)
        .expect("preview output is utf8")
        .replace("\"passed\": true", "\"passed\": false");
    fs::write(&manifest_file, manifest_text).expect("write failed refactor manifest");

    let mut apply = paredit();
    apply
        .args(["refactor", "apply"])
        .arg("--manifest")
        .arg(&manifest_file)
        .arg("--write")
        .arg("--output")
        .arg("json")
        .assert()
        .failure()
        .stdout(predicate::str::contains("\"status\": \"blocked\""))
        .stdout(predicate::str::contains(
            "\"next_action\": \"regenerate_refactor_preview\"",
        ))
        .stdout(predicate::str::contains(
            "\"manifest_policy_passed\": false",
        ))
        .stdout(predicate::str::contains("\"manifest_outputs_parse\": true"))
        .stdout(predicate::str::contains("\"manifest_policy_failed\""))
        .stdout(predicate::str::contains("\"applied\": false"))
        .stdout(predicate::str::contains("\"written_file_count\": 0"))
        .stderr(predicate::str::contains("manifest_policy_passed=false"));

    assert_eq!(
        fs::read_to_string(&lisp_file).expect("read policy failed fixture"),
        original
    );
}

#[test]
fn cli_refactor_apply_reports_unparseable_manifest_outputs_without_writing() {
    let dir = fresh_temp_dir("refactor apply-unparseable-outputs");
    let lisp_file = dir.join("core.lisp");
    let manifest_file = dir.join("rename.preview.json");
    let original = "(defun old-name (x) x)\n";
    fs::write(&lisp_file, original).expect("write lisp fixture");

    let mut preview = paredit();
    let preview_output = preview
        .args(["refactor", "preview"])
        .arg("--from")
        .arg("old-name")
        .arg("--to")
        .arg("new-name")
        .arg("--mode")
        .arg("function")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let manifest_text = String::from_utf8(preview_output)
        .expect("preview output is utf8")
        .replace(
            "\"all_outputs_parse\": true",
            "\"all_outputs_parse\": false",
        );
    fs::write(&manifest_file, manifest_text).expect("write unparseable-output manifest");

    let mut apply = paredit();
    apply
        .args(["refactor", "apply"])
        .arg("--manifest")
        .arg(&manifest_file)
        .arg("--write")
        .arg("--output")
        .arg("json")
        .assert()
        .failure()
        .stdout(predicate::str::contains("\"status\": \"blocked\""))
        .stdout(predicate::str::contains(
            "\"next_action\": \"regenerate_refactor_preview\"",
        ))
        .stdout(predicate::str::contains("\"manifest_policy_passed\": true"))
        .stdout(predicate::str::contains(
            "\"manifest_outputs_parse\": false",
        ))
        .stdout(predicate::str::contains(
            "\"manifest_outputs_do_not_parse\"",
        ))
        .stdout(predicate::str::contains("\"applied\": false"))
        .stdout(predicate::str::contains("\"written_file_count\": 0"))
        .stderr(predicate::str::contains("manifest_outputs_parse=false"));

    assert_eq!(
        fs::read_to_string(&lisp_file).expect("read unparseable-output apply fixture"),
        original
    );
}

#[test]
fn cli_reports_output_hash_mismatch_refactor_apply_manifest_without_writing() {
    let dir = fresh_temp_dir("refactor apply-output-hash-mismatch");
    let lisp_file = dir.join("core.lisp");
    let manifest_file = dir.join("rename.preview.json");
    let original = "(defun old-name (x) x)\n(defun caller () (old-name 1) old-name)\n";
    fs::write(&lisp_file, original).expect("write lisp fixture");

    let mut preview = paredit();
    let preview_output = preview
        .args(["refactor", "preview"])
        .arg("--from")
        .arg("old-name")
        .arg("--to")
        .arg("new-name")
        .arg("--mode")
        .arg("function")
        .arg("--fail-on-no-change")
        .arg("--fail-on-parse-error")
        .arg("--require-definitions")
        .arg("1")
        .arg("--require-edits")
        .arg("2")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&preview_output).expect("preview output parses");
    manifest["files"][0]["output_hash"] = serde_json::Value::String("fnv1a64:deadbeef".into());
    fs::write(
        &manifest_file,
        serde_json::to_vec_pretty(&manifest).expect("serialize mismatched manifest"),
    )
    .expect("write mismatched manifest");

    let mut apply = paredit();
    apply
        .args(["refactor", "apply"])
        .arg("--manifest")
        .arg(&manifest_file)
        .arg("--write")
        .arg("--output")
        .arg("json")
        .assert()
        .failure()
        .stdout(predicate::str::contains("\"status\": \"blocked\""))
        .stdout(predicate::str::contains(
            "\"next_action\": \"fix_manifest_or_parser\"",
        ))
        .stdout(predicate::str::contains("\"output_hash_mismatches\""))
        .stdout(predicate::str::contains(
            "\"output_hash_mismatch_count\": 1",
        ))
        .stdout(predicate::str::contains("\"output_hash_matches\": false"))
        .stdout(predicate::str::contains("\"applied\": false"))
        .stdout(predicate::str::contains("\"written_file_count\": 0"))
        .stderr(predicate::str::contains("output_hash_mismatches=1"));

    assert_eq!(
        fs::read_to_string(&lisp_file).expect("read unchanged hash mismatch fixture"),
        original
    );
}

#[test]
fn cli_reports_manifest_flag_mismatch_refactor_apply_manifest_without_writing() {
    let dir = fresh_temp_dir("refactor apply-manifest-flag-mismatch");
    let lisp_file = dir.join("core.lisp");
    let manifest_file = dir.join("rename.preview.json");
    let original = "(defun old-name (x) x)\n(defun caller () (old-name 1) old-name)\n";
    fs::write(&lisp_file, original).expect("write lisp fixture");

    let mut preview = paredit();
    let preview_output = preview
        .args(["refactor", "preview"])
        .arg("--from")
        .arg("old-name")
        .arg("--to")
        .arg("new-name")
        .arg("--mode")
        .arg("function")
        .arg("--fail-on-no-change")
        .arg("--fail-on-parse-error")
        .arg("--require-definitions")
        .arg("1")
        .arg("--require-edits")
        .arg("2")
        .arg("--output")
        .arg("json")
        .arg(&lisp_file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&preview_output).expect("preview output parses");
    manifest["files"][0]["changed"] = serde_json::Value::Bool(false);
    fs::write(
        &manifest_file,
        serde_json::to_vec_pretty(&manifest).expect("serialize manifest flag mismatch"),
    )
    .expect("write manifest flag mismatch");

    let mut apply = paredit();
    apply
        .args(["refactor", "apply"])
        .arg("--manifest")
        .arg(&manifest_file)
        .arg("--write")
        .arg("--output")
        .arg("json")
        .assert()
        .failure()
        .stdout(predicate::str::contains("\"status\": \"blocked\""))
        .stdout(predicate::str::contains(
            "\"next_action\": \"regenerate_refactor_preview\"",
        ))
        .stdout(predicate::str::contains("\"manifest_flag_mismatches\""))
        .stdout(predicate::str::contains(
            "\"manifest_flag_mismatch_count\": 1",
        ))
        .stdout(predicate::str::contains("\"manifest_flags_match\": false"))
        .stdout(predicate::str::contains("\"applied\": false"))
        .stdout(predicate::str::contains("\"written_file_count\": 0"))
        .stderr(predicate::str::contains("manifest_flag_mismatches=1"));

    assert_eq!(
        fs::read_to_string(&lisp_file).expect("read unchanged manifest flag fixture"),
        original
    );
}

#[cfg(unix)]
#[test]
fn cli_rolls_back_refactor_apply_when_later_file_write_fails() {
    use std::os::unix::fs::PermissionsExt;

    let dir = fresh_temp_dir("refactor apply-rollback");
    let writable_file = dir.join("core.lisp");
    let readonly_dir = dir.join("readonly");
    let blocked_file = readonly_dir.join("other.lisp");
    let manifest_file = dir.join("rename.preview.json");
    let writable_original = "(defun old-name (x) x)\n";
    let blocked_original = "(defun caller () (old-name 1))\n";
    fs::write(&writable_file, writable_original).expect("write writable fixture");
    fs::create_dir_all(&readonly_dir).expect("create readonly dir");
    fs::write(&blocked_file, blocked_original).expect("write blocked fixture");

    let mut preview = paredit();
    let preview_output = preview
        .args(["refactor", "preview"])
        .arg("--from")
        .arg("old-name")
        .arg("--to")
        .arg("new-name")
        .arg("--mode")
        .arg("function")
        .arg("--output")
        .arg("json")
        .arg(&writable_file)
        .arg(&blocked_file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    fs::write(&manifest_file, preview_output).expect("write refactor manifest");

    fs::set_permissions(&readonly_dir, fs::Permissions::from_mode(0o555))
        .expect("chmod readonly dir");

    let mut apply = paredit();
    let assert = apply
        .args(["refactor", "apply"])
        .arg("--manifest")
        .arg(&manifest_file)
        .arg("--write")
        .arg("--output")
        .arg("json")
        .assert();

    fs::set_permissions(&readonly_dir, fs::Permissions::from_mode(0o755))
        .expect("restore readonly dir permissions");

    assert
        .failure()
        .stderr(predicate::str::contains("Permission denied"));
    assert_eq!(
        fs::read_to_string(&writable_file).expect("read rolled back writable fixture"),
        writable_original
    );
    assert_eq!(
        fs::read_to_string(&blocked_file).expect("read blocked fixture"),
        blocked_original
    );
}

#[cfg(unix)]
#[test]
fn cli_refactor_apply_group_by_impact_area_continues_after_one_group_fails() {
    use std::os::unix::fs::PermissionsExt;

    let dir = fresh_temp_dir("refactor apply-group-by-impact-area");
    let writable_file = dir.join("app.lisp");
    let readonly_dir = dir.join("readonly");
    let blocked_file = readonly_dir.join("util.lisp");
    let manifest_file = dir.join("rename.preview.json");
    let writable_original = "(in-package :app)\n(defun old-name (x) x)\n";
    let blocked_original = "(in-package :util)\n(defun old-name (y) y)\n";
    fs::write(&writable_file, writable_original).expect("write writable fixture");
    fs::create_dir_all(&readonly_dir).expect("create readonly dir");
    fs::write(&blocked_file, blocked_original).expect("write blocked fixture");

    let mut preview = paredit();
    let preview_output = preview
        .args(["refactor", "preview"])
        .arg("--from")
        .arg("old-name")
        .arg("--to")
        .arg("new-name")
        .arg("--mode")
        .arg("function")
        .arg("--output")
        .arg("json")
        .arg(&writable_file)
        .arg(&blocked_file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    fs::write(&manifest_file, preview_output).expect("write refactor manifest");

    fs::set_permissions(&readonly_dir, fs::Permissions::from_mode(0o555))
        .expect("chmod readonly dir");

    let mut apply = paredit();
    let assert = apply
        .args(["refactor", "apply"])
        .arg("--manifest")
        .arg(&manifest_file)
        .arg("--write")
        .arg("--group-by-impact-area")
        .arg("--output")
        .arg("json")
        .assert();

    fs::set_permissions(&readonly_dir, fs::Permissions::from_mode(0o755))
        .expect("restore readonly dir permissions");

    // One group's write failed and one succeeded: not everything failed, so
    // this is a warning on stderr and a zero exit rather than the
    // whole-manifest refusal `cli_rolls_back_refactor_apply_when_later_file_write_fails`
    // exercises without this flag.
    let assert = assert
        .success()
        .stderr(predicate::str::contains("Permission denied"));
    let json: serde_json::Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("apply output is valid json");

    let groups = json
        .pointer("/impact_area_groups")
        .and_then(serde_json::Value::as_array)
        .expect("impact_area_groups is present");
    assert_eq!(groups.len(), 2, "{groups:?}");
    assert_eq!(
        groups
            .iter()
            .filter(|group| group["written"] == serde_json::Value::Bool(true))
            .count(),
        1,
        "{groups:?}"
    );
    assert_eq!(
        groups
            .iter()
            .filter(|group| group["written"] == serde_json::Value::Bool(false))
            .count(),
        1,
        "{groups:?}"
    );
    assert!(
        groups.iter().any(|group| group["failure"]
            .as_str()
            .is_some_and(|message| message.contains("Permission denied"))),
        "{groups:?}"
    );

    assert_eq!(
        fs::read_to_string(&writable_file).expect("read written app fixture"),
        "(in-package :app)\n(defun new-name (x) x)\n"
    );
    assert_eq!(
        fs::read_to_string(&blocked_file).expect("read unwritten util fixture"),
        blocked_original
    );
}
