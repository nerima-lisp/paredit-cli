use super::*;

mod apply;
mod check;
mod diff;
mod status;

#[cfg(unix)]
#[test]
fn preview_manifest_out_writes_manifest_and_reports_matching_hash() {
    let dir = fresh_temp_dir("preview-manifest-out");
    let source = dir.join("source.lisp");
    let manifest = dir.join("preview.json");
    fs::write(
        &source,
        "(defun old-name (x) x)\n(defun caller () (old-name 1))\n",
    )
    .expect("write source fixture");

    let output = paredit()
        .args([
            "refactor", "preview", "--from", "old-name", "--to", "new-name",
        ])
        .arg("--manifest-out")
        .arg(&manifest)
        .arg(&source)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let summary: serde_json::Value =
        serde_json::from_slice(&output).expect("summary is valid JSON");
    let reported_hash = summary["manifest_hash"].as_str().expect("manifest_hash");

    let manifest_text = fs::read_to_string(&manifest).expect("manifest was written");
    assert_eq!(reported_hash, stable_manifest_hash(&manifest_text));
    assert!(
        summary["next_actions"][1]
            .as_str()
            .expect("apply next action")
            .contains("--expect-manifest-hash")
    );

    paredit()
        .args(["refactor", "apply", "--expect-manifest-hash", reported_hash])
        .arg("--manifest")
        .arg(&manifest)
        .args(["--root", "/", "--write"])
        .assert()
        .success();
    let rewritten = fs::read_to_string(&source).expect("read rewritten source");
    assert!(rewritten.contains("(defun new-name (x) x)"), "{rewritten}");
}

#[test]
fn refactor_manifest_round_trip_has_no_dialect_agnostic_parse_calls() {
    let sources = [
        (
            "preview",
            include_str!(
                "../../../packages/feature/refactor-workflow/src/refactor/cli/workflow/preview/build.rs"
            ),
        ),
        (
            "check",
            include_str!(
                "../../../packages/feature/refactor-workflow/src/refactor/cli/manifest/check.rs"
            ),
        ),
        (
            "diff",
            include_str!(
                "../../../packages/feature/refactor-workflow/src/refactor/cli/workflow/manifest/diff.rs"
            ),
        ),
        (
            "apply",
            include_str!(
                "../../../packages/feature/refactor-workflow/src/refactor/cli/workflow/manifest/apply.rs"
            ),
        ),
    ];

    for (workflow, source) in sources {
        assert!(
            !source.contains("SyntaxTree::parse("),
            "{workflow} must parse rewritten source with its manifest dialect"
        );
    }
}

#[cfg(unix)]
#[test]
fn preview_manifest_out_refuses_symlink_without_modifying_its_target() {
    let dir = fresh_temp_dir("preview-manifest-symlink");
    let source = dir.join("source.lisp");
    let manifest = dir.join("preview.json");
    let symlink_target = dir.join("third-party.json");
    let original = "{\"owner\":\"third-party\"}\n";
    fs::write(&source, "(defun old-name () nil)\n").expect("write source fixture");
    fs::write(&symlink_target, original).expect("write symlink target");
    std::os::unix::fs::symlink(&symlink_target, &manifest).expect("create manifest symlink");

    paredit()
        .args([
            "refactor", "preview", "--from", "old-name", "--to", "new-name",
        ])
        .arg("--manifest-out")
        .arg(&manifest)
        .arg(&source)
        .assert()
        .failure()
        .stderr(predicate::str::contains("refusing to write symlink"));

    assert_eq!(
        fs::read_to_string(&symlink_target).expect("read symlink target"),
        original
    );
    assert!(
        fs::symlink_metadata(&manifest)
            .expect("inspect manifest symlink")
            .file_type()
            .is_symlink()
    );
}

#[cfg(unix)]
#[test]
fn preview_manifest_out_refuses_non_regular_target() {
    let dir = fresh_temp_dir("preview-manifest-directory");
    let source = dir.join("source.lisp");
    let manifest = dir.join("preview.json");
    fs::write(&source, "(defun old-name () nil)\n").expect("write source fixture");
    fs::create_dir(&manifest).expect("create manifest directory");

    paredit()
        .args([
            "refactor", "preview", "--from", "old-name", "--to", "new-name",
        ])
        .arg("--manifest-out")
        .arg(&manifest)
        .arg(&source)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "refusing to write non-regular file",
        ));

    assert!(
        fs::metadata(&manifest)
            .expect("inspect manifest directory")
            .is_dir()
    );
    assert_eq!(
        fs::read_dir(&manifest)
            .expect("read manifest directory")
            .count(),
        0
    );
}

#[cfg(unix)]
#[test]
fn preview_manifest_out_refuses_hard_link_without_modifying_either_name() {
    use std::os::unix::fs::MetadataExt;

    let dir = fresh_temp_dir("preview-manifest-hardlink");
    let source = dir.join("source.lisp");
    let manifest = dir.join("preview.json");
    let alias = dir.join("third-party.json");
    let original = "{\"owner\":\"third-party\"}\n";
    fs::write(&source, "(defun old-name () nil)\n").expect("write source fixture");
    fs::write(&alias, original).expect("write hard-link target");
    fs::hard_link(&alias, &manifest).expect("create manifest hard link");

    paredit()
        .args([
            "refactor", "preview", "--from", "old-name", "--to", "new-name",
        ])
        .arg("--manifest-out")
        .arg(&manifest)
        .arg(&source)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "refusing to replace hard-linked target",
        ));

    assert_eq!(
        fs::read_to_string(&manifest).expect("read manifest hard link"),
        original
    );
    assert_eq!(
        fs::read_to_string(&alias).expect("read hard-link target"),
        original
    );
    assert_eq!(
        fs::metadata(&manifest)
            .expect("inspect manifest hard link")
            .nlink(),
        2
    );
}

#[cfg(unix)]
#[test]
fn preview_manifest_out_preserves_existing_file_when_staging_fails() {
    use std::os::unix::fs::PermissionsExt;

    let dir = fresh_temp_dir("preview-manifest-stage-failure");
    let source = dir.join("source.lisp");
    let manifest = dir.join("preview.json");
    let original = "{\"state\":\"before\"}\n";
    fs::write(&source, "(defun old-name () nil)\n").expect("write source fixture");
    fs::write(&manifest, original).expect("write existing manifest");

    let mut read_only = fs::metadata(&dir)
        .expect("inspect fixture directory")
        .permissions();
    read_only.set_mode(0o555);
    fs::set_permissions(&dir, read_only).expect("make fixture directory read-only");

    let assertion = paredit()
        .args([
            "refactor", "preview", "--from", "old-name", "--to", "new-name",
        ])
        .arg("--manifest-out")
        .arg(&manifest)
        .arg(&source)
        .assert()
        .failure();

    let mut writable = fs::metadata(&dir)
        .expect("inspect read-only fixture directory")
        .permissions();
    writable.set_mode(0o700);
    fs::set_permissions(&dir, writable).expect("restore fixture directory permissions");

    assertion.stderr(predicate::str::contains("failed to write manifest"));
    assert_eq!(
        fs::read_to_string(&manifest).expect("read preserved manifest"),
        original
    );
    let transaction_artifacts = fs::read_dir(&dir)
        .expect("read fixture directory")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().contains(".paredit-"))
        .count();
    assert_eq!(transaction_artifacts, 0);
}

#[cfg(unix)]
#[test]
fn preview_manifest_out_preserves_concurrent_replacement() {
    let temp = fresh_temp_dir("manifest-concurrent-replacement");
    let source = temp.join("input.lisp");
    let manifest = temp.join("preview.json");
    let replacement = temp.join("third-party.json");
    fs::write(&source, "(defun foo (x) x)\n(defun caller () (foo 1))\n").expect("write source");

    let original = fs::File::create(&manifest).expect("create original manifest");
    original
        .set_len(64 * 1024 * 1024)
        .expect("expand original manifest");
    drop(original);

    let third_party = b"{\"owner\":\"third-party\"}\n";
    fs::write(&replacement, third_party).expect("write concurrent replacement");

    let binary = paredit().get_program().to_owned();
    let mut child = std::process::Command::new(binary)
        .args([
            "refactor",
            "preview",
            "--from",
            "foo",
            "--to",
            "quux",
            "--manifest-out",
            manifest.to_str().expect("utf8 path"),
            source.to_str().expect("utf8 path"),
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn paredit");

    let staging_prefix = ".preview.json.paredit-tmp-";
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let staging_exists = fs::read_dir(&temp)
            .expect("read temp directory")
            .filter_map(Result::ok)
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(staging_prefix)
            });
        if staging_exists {
            break;
        }
        if child.try_wait().expect("poll paredit child").is_some() {
            let output = child.wait_with_output().expect("collect exited child");
            panic!(
                "paredit exited before staging; stdout={} stderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output().expect("collect timed-out child");
            panic!(
                "staging file did not appear; stdout={} stderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::yield_now();
    }

    fs::rename(&replacement, &manifest).expect("publish concurrent replacement");

    let output = child.wait_with_output().expect("wait for paredit");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "expected failure; stdout={} stderr={stderr}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        stderr.contains("refus") && stderr.contains("replac"),
        "unexpected stderr: {stderr}"
    );
    assert_eq!(
        fs::read(&manifest).expect("read surviving replacement"),
        third_party
    );
}
