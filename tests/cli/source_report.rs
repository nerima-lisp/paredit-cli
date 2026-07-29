//! `inspect sources` and the input selectors and filters every workspace
//! command shares.
//!
//! These are contract tests for *selection*, not for analysis: what matters is
//! which files came back and which rule dropped the rest. A regression here is
//! invisible in every other test in the suite, because a command that silently
//! analyses fewer files still reports a clean result.

use super::*;

/// Makes `root` a repository without needing git.
///
/// Discovery only asks whether `.git` exists; creating it as a directory keeps
/// these tests independent of a git installation and of any ambient config.
fn mark_repository(root: &std::path::Path) {
    fs::create_dir_all(root.join(".git")).expect("create .git marker");
}

fn json_report(output: &[u8]) -> serde_json::Value {
    serde_json::from_slice(output).expect("inspect sources emits JSON")
}

fn file_names(report: &serde_json::Value) -> Vec<String> {
    report["files"]
        .as_array()
        .expect("files array")
        .iter()
        .map(|value| {
            std::path::Path::new(value.as_str().expect("path is a string"))
                .file_name()
                .expect("path has a file name")
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

fn sources(directory: &std::path::Path, extra: &[&str]) -> serde_json::Value {
    let mut cmd = paredit();
    cmd.args(["inspect", "sources", "--output", "json", "--list-files"]);
    cmd.args(extra);
    let assert = cmd.arg(directory).assert().success();
    json_report(&assert.get_output().stdout)
}

#[test]
fn cli_sources_reports_the_walk_selector_and_the_files_it_found() {
    let dir = fresh_temp_dir("sources walk");
    fs::create_dir_all(&dir).expect("create dir");
    fs::write(dir.join("a.lisp"), "(defun a () nil)\n").expect("write fixture");
    fs::write(dir.join("notes.txt"), "not lisp\n").expect("write fixture");

    let report = sources(&dir, &[]);
    assert_eq!(report["selector"], "walk");
    assert_eq!(report["file_count"], 1);
    assert_eq!(report["skipped"]["unknown"], 1);
    assert_eq!(file_names(&report), vec!["a.lisp"]);
}

// --- F1 / F2: ignore files -------------------------------------------------

#[test]
fn cli_sources_honours_gitignore_and_can_be_told_not_to() {
    let dir = fresh_temp_dir("sources gitignore");
    fs::create_dir_all(dir.join("generated")).expect("create dir");
    mark_repository(&dir);
    fs::write(dir.join(".gitignore"), "generated/\n").expect("write ignore file");
    fs::write(dir.join("keep.lisp"), "(defun keep () nil)\n").expect("write fixture");
    fs::write(
        dir.join("generated").join("out.lisp"),
        "(defun out () nil)\n",
    )
    .expect("write fixture");

    let report = sources(&dir, &[]);
    assert_eq!(file_names(&report), vec!["keep.lisp"]);
    assert_eq!(report["skipped"]["ignored"], 1);
    assert_eq!(report["ignore"]["gitignore"], true);

    let report = sources(&dir, &["--no-gitignore"]);
    let mut names = file_names(&report);
    names.sort();
    assert_eq!(names, vec!["keep.lisp", "out.lisp"]);
    assert_eq!(report["skipped"]["ignored"], 0);
}

#[test]
fn cli_sources_honours_pareditignore_independently_of_git() {
    let dir = fresh_temp_dir("sources pareditignore");
    fs::create_dir_all(&dir).expect("create dir");
    mark_repository(&dir);
    fs::write(dir.join(".pareditignore"), "vendored.lisp\n").expect("write ignore file");
    fs::write(dir.join("keep.lisp"), "(defun keep () nil)\n").expect("write fixture");
    fs::write(dir.join("vendored.lisp"), "(defun vendored () nil)\n").expect("write fixture");

    let report = sources(&dir, &[]);
    assert_eq!(file_names(&report), vec!["keep.lisp"]);

    // `--no-gitignore` must not switch off the tool's own ignore file: they are
    // separate mechanisms precisely so a project can use one without the other.
    let report = sources(&dir, &["--no-gitignore"]);
    assert_eq!(file_names(&report), vec!["keep.lisp"]);

    let report = sources(&dir, &["--no-ignore"]);
    assert_eq!(file_names(&report).len(), 2);
}

#[test]
fn cli_sources_lets_a_nested_ignore_file_reinclude_a_path() {
    let dir = fresh_temp_dir("sources reinclude");
    fs::create_dir_all(dir.join("src")).expect("create dir");
    mark_repository(&dir);
    fs::write(dir.join(".gitignore"), "*.lisp\n").expect("write ignore file");
    fs::write(dir.join("src").join(".gitignore"), "!keep.lisp\n").expect("write ignore file");
    fs::write(dir.join("src").join("keep.lisp"), "(defun keep () nil)\n").expect("write fixture");
    fs::write(dir.join("src").join("drop.lisp"), "(defun drop () nil)\n").expect("write fixture");

    let report = sources(&dir, &[]);
    assert_eq!(file_names(&report), vec!["keep.lisp"]);
}

// --- F3: command-line globs ------------------------------------------------

#[test]
fn cli_sources_applies_include_and_exclude_globs() {
    let dir = fresh_temp_dir("sources globs");
    fs::create_dir_all(dir.join("src").join("nested")).expect("create dir");
    fs::create_dir_all(dir.join("test")).expect("create dir");
    fs::write(dir.join("src").join("a.lisp"), "(defun a () nil)\n").expect("write fixture");
    fs::write(
        dir.join("src").join("nested").join("b.lisp"),
        "(defun b () nil)\n",
    )
    .expect("write fixture");
    fs::write(dir.join("test").join("t.lisp"), "(defun t () nil)\n").expect("write fixture");

    let report = sources(&dir, &["--include", "src/**"]);
    let mut names = file_names(&report);
    names.sort();
    assert_eq!(names, vec!["a.lisp", "b.lisp"]);

    let report = sources(
        &dir,
        &["--include", "src/**", "--exclude-glob", "**/nested/**"],
    );
    assert_eq!(file_names(&report), vec!["a.lisp"]);
}

#[test]
fn cli_sources_rejects_a_malformed_glob_instead_of_ignoring_it() {
    let dir = fresh_temp_dir("sources bad glob");
    fs::create_dir_all(&dir).expect("create dir");
    fs::write(dir.join("a.lisp"), "(defun a () nil)\n").expect("write fixture");

    let mut cmd = paredit();
    cmd.args(["inspect", "sources", "--include", "src/[unterminated"])
        .arg(&dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid --include pattern"));
}

// --- F8: a file list on stdin ---------------------------------------------

#[test]
fn cli_sources_reads_a_newline_separated_path_list_from_stdin() {
    let dir = fresh_temp_dir("sources stdin list");
    fs::create_dir_all(&dir).expect("create dir");
    let first = dir.join("a.lisp");
    let second = dir.join("b.lisp");
    fs::write(&first, "(defun a () nil)\n").expect("write fixture");
    fs::write(&second, "(defun b () nil)\n").expect("write fixture");
    fs::write(dir.join("c.lisp"), "(defun c () nil)\n").expect("write fixture");

    let mut cmd = paredit();
    let assert = cmd
        .args([
            "inspect",
            "sources",
            "--output",
            "json",
            "--list-files",
            "--paths-from",
            "-",
        ])
        .arg(&dir)
        .write_stdin(format!("{}\n{}\n", first.display(), second.display()))
        .assert()
        .success();

    let report = json_report(&assert.get_output().stdout);
    assert_eq!(report["selector"], "path-list");
    let mut names = file_names(&report);
    names.sort();
    assert_eq!(names, vec!["a.lisp", "b.lisp"]);
}

#[test]
fn cli_sources_reads_a_nul_separated_path_list() {
    let dir = fresh_temp_dir("sources nul list");
    fs::create_dir_all(&dir).expect("create dir");
    let only = dir.join("a.lisp");
    fs::write(&only, "(defun a () nil)\n").expect("write fixture");

    let mut cmd = paredit();
    let assert = cmd
        .args([
            "inspect",
            "sources",
            "--output",
            "json",
            "--list-files",
            "--paths-from",
            "-",
            "--paths-from-separator",
            "nul",
        ])
        .arg(&dir)
        .write_stdin(format!("{}\0", only.display()))
        .assert()
        .success();

    let report = json_report(&assert.get_output().stdout);
    assert_eq!(file_names(&report), vec!["a.lisp"]);
}

#[test]
fn cli_sources_drops_listed_paths_that_lie_outside_the_roots() {
    let dir = fresh_temp_dir("sources list outside");
    let outside = fresh_temp_dir("sources list outside target");
    fs::create_dir_all(&dir).expect("create dir");
    fs::create_dir_all(&outside).expect("create dir");
    let inside = dir.join("a.lisp");
    fs::write(&inside, "(defun a () nil)\n").expect("write fixture");
    let secret = outside.join("secret.lisp");
    fs::write(&secret, "(defun secret () nil)\n").expect("write fixture");

    let mut cmd = paredit();
    let assert = cmd
        .args([
            "inspect",
            "sources",
            "--output",
            "json",
            "--list-files",
            "--paths-from",
            "-",
        ])
        .arg(&dir)
        .write_stdin(format!("{}\n{}\n", inside.display(), secret.display()))
        .assert()
        .success();

    let report = json_report(&assert.get_output().stdout);
    assert_eq!(
        file_names(&report),
        vec!["a.lisp"],
        "a path list must not widen what the roots authorised"
    );
}

// --- F4 / F5 / F6: manifests ----------------------------------------------

#[test]
fn cli_sources_takes_an_ordered_file_set_from_an_asdf_system() {
    let dir = fresh_temp_dir("sources asdf");
    fs::create_dir_all(dir.join("src").join("core")).expect("create dir");
    fs::write(dir.join("src").join("package.lisp"), "(defpackage :demo)\n").expect("write fixture");
    fs::write(
        dir.join("src").join("core").join("a.lisp"),
        "(defun a () nil)\n",
    )
    .expect("write fixture");
    // Present on disk but not named by the system: a directory walk would pick
    // it up and the manifest must not.
    fs::write(dir.join("src").join("stray.lisp"), "(defun stray () nil)\n").expect("write fixture");
    fs::write(
        dir.join("demo.asd"),
        concat!(
            "(defsystem \"demo\"\n",
            "  :pathname \"src/\"\n",
            "  :serial t\n",
            "  :depends-on (\"alexandria\")\n",
            "  :components ((:file \"package\")\n",
            "               (:module \"core\" :components ((:file \"a\")))))\n"
        ),
    )
    .expect("write manifest");

    let report = sources(&dir, &["--from-manifest"]);
    assert_eq!(report["selector"], "manifest");
    let mut names = file_names(&report);
    names.sort();
    assert_eq!(names, vec!["a.lisp", "package.lisp"]);
    assert_eq!(report["manifests"][0]["kind"], "asdf");
    assert_eq!(report["manifests"][0]["name"], "demo");
    assert_eq!(
        report["manifests"][0]["dependencies"][0]["name"],
        "alexandria"
    );
}

#[test]
fn cli_sources_takes_source_paths_from_a_deps_edn() {
    let dir = fresh_temp_dir("sources deps edn");
    fs::create_dir_all(dir.join("src")).expect("create dir");
    fs::create_dir_all(dir.join("dev")).expect("create dir");
    fs::create_dir_all(dir.join("elsewhere")).expect("create dir");
    fs::write(dir.join("src").join("core.clj"), "(ns demo.core)\n").expect("write fixture");
    fs::write(dir.join("dev").join("user.clj"), "(ns user)\n").expect("write fixture");
    fs::write(dir.join("elsewhere").join("x.clj"), "(ns x)\n").expect("write fixture");
    fs::write(
        dir.join("deps.edn"),
        concat!(
            "{:paths [\"src\"]\n",
            " :deps {org.clojure/clojure {:mvn/version \"1.11.1\"}}\n",
            " :aliases {:dev {:extra-paths [\"dev\"]}}}\n"
        ),
    )
    .expect("write manifest");

    let report = sources(&dir, &["--from-manifest"]);
    let mut names = file_names(&report);
    names.sort();
    assert_eq!(
        names,
        vec!["core.clj", "user.clj"],
        "elsewhere/ is on disk but not declared"
    );
    assert_eq!(report["manifests"][0]["kind"], "deps-edn");
    assert_eq!(
        report["manifests"][0]["dependencies"][0]["version"],
        "1.11.1"
    );
}

#[test]
fn cli_sources_reads_an_emacs_lisp_package_requires_header() {
    let dir = fresh_temp_dir("sources elisp header");
    fs::create_dir_all(&dir).expect("create dir");
    fs::write(
        dir.join("demo.el"),
        concat!(
            ";;; demo.el --- Demo  -*- lexical-binding: t; -*-\n",
            ";; Package-Requires: ((emacs \"27.1\") (dash \"2.19.1\"))\n",
            ";;; Code:\n",
            "(provide 'demo)\n"
        ),
    )
    .expect("write fixture");

    let report = sources(&dir, &["--from-manifest"]);
    assert_eq!(report["manifests"][0]["kind"], "elisp-package");
    assert_eq!(report["manifests"][0]["name"], "demo");
    let dependencies = report["manifests"][0]["dependencies"]
        .as_array()
        .expect("dependency array")
        .iter()
        .map(|dependency| dependency["name"].as_str().expect("name").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(dependencies, vec!["dash", "emacs"]);
}

#[test]
fn cli_sources_reports_a_missing_manifest_rather_than_scanning_everything() {
    let dir = fresh_temp_dir("sources no manifest");
    fs::create_dir_all(&dir).expect("create dir");
    fs::write(dir.join("a.lisp"), "(defun a () nil)\n").expect("write fixture");

    let mut cmd = paredit();
    cmd.args(["inspect", "sources", "--from-manifest"])
        .arg(&dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("--from-manifest found no"));
}

// --- F9: repository boundaries --------------------------------------------

#[test]
fn cli_sources_groups_files_by_the_repository_that_holds_them() {
    let dir = fresh_temp_dir("sources multi repo");
    let first = dir.join("first");
    let second = dir.join("second");
    fs::create_dir_all(&first).expect("create dir");
    fs::create_dir_all(&second).expect("create dir");
    mark_repository(&first);
    mark_repository(&second);
    fs::write(first.join("a.lisp"), "(defun a () nil)\n").expect("write fixture");
    fs::write(second.join("b.lisp"), "(defun b () nil)\n").expect("write fixture");

    let report = sources(&dir, &[]);
    let repositories = report["repositories"].as_array().expect("repository array");
    assert_eq!(repositories.len(), 2);
    for repository in repositories {
        assert_eq!(repository["file_count"], 1);
    }
}

#[test]
fn cli_sources_does_not_apply_an_outer_ignore_file_inside_a_nested_repository() {
    let dir = fresh_temp_dir("sources nested repo");
    let inner = dir.join("nested").join("inner");
    fs::create_dir_all(&inner).expect("create dir");
    mark_repository(&dir);
    mark_repository(&inner);
    fs::write(dir.join(".gitignore"), "*.lisp\n").expect("write ignore file");
    fs::write(dir.join("outer.lisp"), "(defun outer () nil)\n").expect("write fixture");
    fs::write(inner.join("inner.lisp"), "(defun inner () nil)\n").expect("write fixture");

    let report = sources(&dir, &[]);
    assert_eq!(
        file_names(&report),
        vec!["inner.lisp"],
        "the nested checkout keeps its own rules, and the outer rule still \
         applies to the outer file visited afterwards"
    );
}

// --- F12: symlinks ---------------------------------------------------------

#[cfg(unix)]
#[test]
fn cli_sources_follows_symlinks_only_when_asked_and_only_inside_the_roots() {
    let dir = fresh_temp_dir("sources symlinks");
    let outside = fresh_temp_dir("sources symlinks target");
    fs::create_dir_all(dir.join("real")).expect("create dir");
    fs::create_dir_all(&outside).expect("create dir");
    fs::write(dir.join("real").join("a.lisp"), "(defun a () nil)\n").expect("write fixture");
    fs::write(outside.join("secret.lisp"), "(defun secret () nil)\n").expect("write fixture");
    std::os::unix::fs::symlink(&outside, dir.join("escape")).expect("create symlink");

    let report = sources(&dir, &[]);
    assert_eq!(file_names(&report), vec!["a.lisp"]);
    assert_eq!(report["skipped"]["symlink"], 1);

    let report = sources(&dir, &["--follow-symlinks"]);
    assert_eq!(
        file_names(&report),
        vec!["a.lisp"],
        "a followed symlink still may not reach outside the authorised roots"
    );
    assert_eq!(report["skipped"]["symlink_escaped"], 1);
}

// --- selector conflicts ----------------------------------------------------

#[test]
fn cli_sources_refuses_two_selectors_at_once() {
    let dir = fresh_temp_dir("sources two selectors");
    fs::create_dir_all(&dir).expect("create dir");
    fs::write(dir.join("a.lisp"), "(defun a () nil)\n").expect("write fixture");

    let mut cmd = paredit();
    cmd.args(["inspect", "sources", "--from-git", "--from-manifest"])
        .arg(&dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("pass only one of --since"));
}

// --- the filters reach the other workspace commands ------------------------

#[test]
fn cli_workspace_report_honours_the_shared_input_filters() {
    let dir = fresh_temp_dir("workspace shared filters");
    // Deliberately not named `vendor` or `target`: those are on the built-in
    // generated-directory list, and a test that used one would pass whether or
    // not the ignore file was read at all.
    fs::create_dir_all(dir.join("third-party")).expect("create dir");
    mark_repository(&dir);
    fs::write(dir.join(".gitignore"), "third-party/\n").expect("write ignore file");
    fs::write(dir.join("core.lisp"), "(defun core () nil)\n").expect("write fixture");
    fs::write(
        dir.join("third-party").join("dep.lisp"),
        "(defun dep () nil)\n",
    )
    .expect("write fixture");

    let mut cmd = paredit();
    cmd.args(["inspect", "workspace", "--output", "json"])
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"file_count\": 1"));

    let mut cmd = paredit();
    cmd.args(["inspect", "workspace", "--output", "json", "--no-ignore"])
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"file_count\": 2"));

    let mut cmd = paredit();
    cmd.args([
        "inspect",
        "workspace",
        "--output",
        "json",
        "--no-ignore",
        "--exclude-glob",
        "third-party/**",
    ])
    .arg(&dir)
    .assert()
    .success()
    .stdout(predicate::str::contains("\"file_count\": 1"));
}

// --- F7: --since <git-ref> -------------------------------------------------

/// Creates a repository with one commit, or returns `None` when git is absent.
///
/// Committing needs an identity and a signature policy, and this repository's
/// own `main` requires signed commits; `-c` overrides keep the fixture
/// independent of whatever the developer's global config says.
fn git_repository(dir: &std::path::Path) -> Option<()> {
    let run = |arguments: &[&str]| -> Option<bool> {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.name=paredit test",
                "-c",
                "user.email=test@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(arguments)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .ok()?;
        Some(status.success())
    };
    run(&["init"])?.then_some(())?;
    run(&["add", "-A"])?.then_some(())?;
    run(&["commit", "-m", "initial"])?.then_some(())?;
    Some(())
}

#[test]
fn cli_sources_limits_the_file_set_to_what_changed_since_a_ref() {
    let dir = fresh_temp_dir("sources since");
    fs::create_dir_all(&dir).expect("create dir");
    fs::write(dir.join("committed.lisp"), "(defun committed () nil)\n").expect("write fixture");
    fs::write(dir.join("untouched.lisp"), "(defun untouched () nil)\n").expect("write fixture");
    let Some(()) = git_repository(&dir) else {
        // git is not installed, or refused to commit in this environment. The
        // feature is about git, so there is nothing meaningful to assert.
        return;
    };

    fs::write(
        dir.join("committed.lisp"),
        "(defun committed () :changed)\n",
    )
    .expect("modify fixture");
    fs::write(dir.join("added.lisp"), "(defun added () nil)\n").expect("write fixture");

    let report = sources(&dir, &["--since", "HEAD"]);
    assert_eq!(report["selector"], "git-since");
    let mut names = file_names(&report);
    names.sort();
    assert_eq!(
        names,
        vec!["added.lisp", "committed.lisp"],
        "an untouched file must not be analysed, and a new one must be"
    );

    let report = sources(&dir, &["--since", "HEAD", "--since-skip-untracked"]);
    assert_eq!(file_names(&report), vec!["committed.lisp"]);
}

#[test]
fn cli_sources_reports_an_unknown_ref_rather_than_an_empty_change_set() {
    let dir = fresh_temp_dir("sources since bad ref");
    fs::create_dir_all(&dir).expect("create dir");
    fs::write(dir.join("a.lisp"), "(defun a () nil)\n").expect("write fixture");
    let Some(()) = git_repository(&dir) else {
        return;
    };

    let mut cmd = paredit();
    cmd.args(["inspect", "sources", "--since", "no-such-ref"])
        .arg(&dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("could not resolve the git ref"));
}

#[test]
fn cli_sources_refuses_a_ref_that_would_be_read_as_a_git_option() {
    let dir = fresh_temp_dir("sources since option ref");
    fs::create_dir_all(&dir).expect("create dir");
    fs::write(dir.join("a.lisp"), "(defun a () nil)\n").expect("write fixture");
    let Some(()) = git_repository(&dir) else {
        return;
    };

    let mut cmd = paredit();
    cmd.args([
        "inspect",
        "sources",
        "--since=--upload-pack=touch /tmp/pwned",
    ])
    .arg(&dir)
    .assert()
    .failure()
    .stderr(predicate::str::contains("invalid git ref"));
}

#[test]
fn cli_sources_takes_the_tracked_file_set_from_git() {
    let dir = fresh_temp_dir("sources from git");
    fs::create_dir_all(&dir).expect("create dir");
    fs::write(dir.join("tracked.lisp"), "(defun tracked () nil)\n").expect("write fixture");
    let Some(()) = git_repository(&dir) else {
        return;
    };
    fs::write(dir.join("untracked.lisp"), "(defun untracked () nil)\n").expect("write fixture");

    let report = sources(&dir, &["--from-git"]);
    assert_eq!(report["selector"], "git-tracked");
    assert_eq!(file_names(&report), vec!["tracked.lisp"]);
}

// --- F10: archive input ----------------------------------------------------

/// Writes one tar entry: a header block plus its padded data.
fn tar_entry(name: &str, type_flag: u8, data: &[u8]) -> Vec<u8> {
    const BLOCK: usize = 512;
    let mut header = [0_u8; BLOCK];
    header[..name.len()].copy_from_slice(name.as_bytes());
    header[100..108].copy_from_slice(b"0000644\0");
    header[108..116].copy_from_slice(b"0000000\0");
    header[116..124].copy_from_slice(b"0000000\0");
    header[124..136].copy_from_slice(format!("{:011o}\0", data.len()).as_bytes());
    header[136..148].copy_from_slice(b"00000000000\0");
    header[148..156].copy_from_slice(b"        ");
    header[156] = type_flag;
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    let checksum: u64 = header.iter().map(|byte| u64::from(*byte)).sum();
    header[148..156].copy_from_slice(format!("{checksum:06o}\0 ").as_bytes());

    let mut entry = header.to_vec();
    entry.extend_from_slice(data);
    entry.extend(std::iter::repeat_n(
        0_u8,
        (BLOCK - data.len() % BLOCK) % BLOCK,
    ));
    entry
}

fn tar_archive(entries: &[Vec<u8>]) -> Vec<u8> {
    let mut archive = entries.concat();
    archive.extend(std::iter::repeat_n(0_u8, 1024));
    archive
}

#[test]
fn cli_sources_analyses_a_tar_archive_extracted_to_a_named_directory() {
    let dir = fresh_temp_dir("sources archive");
    fs::create_dir_all(&dir).expect("create dir");
    let archive_path = dir.join("project.tar");
    let destination = dir.join("unpacked");
    fs::write(
        &archive_path,
        tar_archive(&[
            tar_entry("project/", b'5', b""),
            tar_entry("project/a.lisp", b'0', b"(defun a () nil)\n"),
            tar_entry("project/notes.txt", b'0', b"not lisp\n"),
        ]),
    )
    .expect("write archive");

    let mut cmd = paredit();
    let assert = cmd
        .args(["inspect", "sources", "--output", "json", "--list-files"])
        .arg("--from-archive")
        .arg(&archive_path)
        .arg("--extract-to")
        .arg(&destination)
        .arg(&destination)
        .assert()
        .success();

    let report = json_report(&assert.get_output().stdout);
    assert_eq!(report["selector"], "archive");
    assert_eq!(file_names(&report), vec!["a.lisp"]);
}

#[test]
fn cli_sources_reads_a_tar_archive_from_stdin() {
    let dir = fresh_temp_dir("sources archive stdin");
    fs::create_dir_all(&dir).expect("create dir");
    let destination = dir.join("unpacked");

    let mut cmd = paredit();
    let assert = cmd
        .args(["inspect", "sources", "--output", "json", "--list-files"])
        .arg("--from-archive")
        .arg("-")
        .arg("--extract-to")
        .arg(&destination)
        .arg(&destination)
        .write_stdin(tar_archive(&[tar_entry(
            "a.lisp",
            b'0',
            b"(defun a () nil)\n",
        )]))
        .assert()
        .success();

    let report = json_report(&assert.get_output().stdout);
    assert_eq!(file_names(&report), vec!["a.lisp"]);
}

#[test]
fn cli_sources_refuses_an_archive_entry_that_escapes_the_destination() {
    let dir = fresh_temp_dir("sources archive escape");
    fs::create_dir_all(&dir).expect("create dir");
    let archive_path = dir.join("evil.tar");
    let destination = dir.join("unpacked");
    fs::write(
        &archive_path,
        tar_archive(&[tar_entry(
            "../escaped.lisp",
            b'0',
            b"(defun escaped () nil)\n",
        )]),
    )
    .expect("write archive");

    let mut cmd = paredit();
    cmd.args(["inspect", "sources"])
        .arg("--from-archive")
        .arg(&archive_path)
        .arg("--extract-to")
        .arg(&destination)
        .arg(&destination)
        .assert()
        .failure()
        .stderr(predicate::str::contains("escapes the destination"));
    assert!(
        !dir.join("escaped.lisp").exists(),
        "an escaping entry must not be written anywhere"
    );
}

// --- F11: the discovery cache ---------------------------------------------

#[test]
fn cli_sources_reuses_a_cached_scan_until_the_tree_changes() {
    let dir = fresh_temp_dir("sources cache");
    // The cache lives outside the scanned root on purpose: writing it inside
    // would change the tree it describes and invalidate its own first entry.
    let cache_dir = fresh_temp_dir("sources cache store");
    fs::create_dir_all(&dir).expect("create dir");
    fs::write(dir.join("a.lisp"), "(defun a () nil)\n").expect("write fixture");

    let scan = |extra: &[&str]| -> serde_json::Value {
        let mut cmd = paredit();
        cmd.args(["inspect", "sources", "--output", "json"])
            .arg("--cache-dir")
            .arg(&cache_dir);
        cmd.args(extra);
        let assert = cmd.arg(&dir).assert().success();
        json_report(&assert.get_output().stdout)
    };

    assert_eq!(scan(&[])["cache"], "missing");
    assert_eq!(scan(&[])["cache"], "hit");

    fs::write(dir.join("b.lisp"), "(defun b () nil)\n").expect("write fixture");
    let report = scan(&[]);
    assert_eq!(report["cache"], "stale");
    assert_eq!(report["file_count"], 2);

    assert_eq!(scan(&[])["cache"], "hit");
    assert_eq!(scan(&["--clear-cache"])["cache"], "missing");
}

#[test]
fn cli_sources_does_not_share_a_cache_entry_between_different_filters() {
    let dir = fresh_temp_dir("sources cache key");
    let cache_dir = fresh_temp_dir("sources cache key store");
    fs::create_dir_all(dir.join("src")).expect("create dir");
    fs::write(dir.join("root.lisp"), "(defun root () nil)\n").expect("write fixture");
    fs::write(dir.join("src").join("a.lisp"), "(defun a () nil)\n").expect("write fixture");

    let scan = |extra: &[&str]| -> serde_json::Value {
        let mut cmd = paredit();
        cmd.args(["inspect", "sources", "--output", "json"])
            .arg("--cache-dir")
            .arg(&cache_dir);
        cmd.args(extra);
        let assert = cmd.arg(&dir).assert().success();
        json_report(&assert.get_output().stdout)
    };

    assert_eq!(scan(&[])["file_count"], 2);
    // A narrower filter must not be answered from the wider run's entry.
    let narrowed = scan(&["--include", "src/**"]);
    assert_eq!(narrowed["cache"], "missing");
    assert_eq!(narrowed["file_count"], 1);
}

// --- the environment escape hatch -----------------------------------------

#[test]
fn cli_sources_lets_the_environment_switch_ignore_files_off() {
    let dir = fresh_temp_dir("sources env ignore");
    fs::create_dir_all(&dir).expect("create dir");
    mark_repository(&dir);
    fs::write(dir.join(".gitignore"), "generated.lisp\n").expect("write ignore file");
    fs::write(dir.join("keep.lisp"), "(defun keep () nil)\n").expect("write fixture");
    fs::write(dir.join("generated.lisp"), "(defun generated () nil)\n").expect("write fixture");

    let mut cmd = paredit();
    let assert = cmd
        .args(["inspect", "sources", "--output", "json", "--list-files"])
        .env("PAREDIT_NO_IGNORE", "1")
        .arg(&dir)
        .assert()
        .success();
    assert_eq!(
        file_names(&json_report(&assert.get_output().stdout)).len(),
        2
    );

    // A variable set to a falsey value is treated as unset, so a CI system that
    // exports every name it knows about cannot enable this by accident.
    let mut cmd = paredit();
    let assert = cmd
        .args(["inspect", "sources", "--output", "json", "--list-files"])
        .env("PAREDIT_NO_IGNORE", "0")
        .arg(&dir)
        .assert()
        .success();
    assert_eq!(
        file_names(&json_report(&assert.get_output().stdout)),
        vec!["keep.lisp"]
    );
}

#[test]
fn cli_commands_taking_explicit_paths_honour_ignore_files_and_the_override() {
    let dir = fresh_temp_dir("expand ignore");
    fs::create_dir_all(&dir).expect("create dir");
    mark_repository(&dir);
    fs::write(dir.join(".gitignore"), "generated.lisp\n").expect("write ignore file");
    fs::write(dir.join("keep.lisp"), "(defun keep (a a) a)\n").expect("write fixture");
    fs::write(dir.join("generated.lisp"), "(defun generated (b b) b)\n").expect("write fixture");

    // `inspect duplicate-parameters` expands a directory argument through the
    // same walk but declares no input flags of its own.
    let mut cmd = paredit();
    cmd.args(["inspect", "duplicate-parameters", "--output", "json"])
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("generated.lisp").not())
        .stdout(predicate::str::contains("keep.lisp"));

    let mut cmd = paredit();
    cmd.args(["inspect", "duplicate-parameters", "--output", "json"])
        .env("PAREDIT_NO_IGNORE", "1")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("generated.lisp"));
}
