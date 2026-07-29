use super::*;
use anyhow::Result;
use std::fs;
use std::io::{self, Cursor, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
#[cfg(unix)]
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use super::discovery::{
    ExcludeIndex, READ_CHUNK_BYTES, discover_workspace_files_with_limits, read_bounded,
};
use super::types::{SymlinkPolicy, WorkspaceLimits};

#[test]
fn discovery_skips_unknown_hidden_and_generated_paths_by_default() -> Result<()> {
    let root = unique_temp_dir("default-skips");
    fs::create_dir_all(root.join(".hidden"))?;
    fs::create_dir_all(root.join("target"))?;
    fs::write(root.join("main.lisp"), "(defun main () nil)")?;
    fs::write(root.join("README.md"), "not lisp")?;
    fs::write(
        root.join(".hidden").join("secret.lisp"),
        "(defun secret () nil)",
    )?;
    fs::write(
        root.join("target").join("generated.lisp"),
        "(defun generated () nil)",
    )?;

    let discovery = discover_workspace_files(&WorkspaceDiscoveryOptions {
        roots: vec![root.clone()],
        include_unknown: false,
        include_hidden: false,
        include_generated: false,
        max_depth: None,
        exclude: Vec::new(),
        ..WorkspaceDiscoveryOptions::default()
    })?;

    assert_eq!(discovery.files, vec![root.join("main.lisp")]);
    assert_eq!(discovery.skipped_unknown_count, 1);
    assert_eq!(discovery.skipped_hidden_count, 1);
    assert_eq!(discovery.skipped_generated_count, 1);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn discovery_can_include_unknown_hidden_and_generated_paths() -> Result<()> {
    let root = unique_temp_dir("include-all");
    fs::create_dir_all(root.join(".hidden"))?;
    fs::create_dir_all(root.join("target"))?;
    fs::write(root.join("main.lisp"), "(defun main () nil)")?;
    fs::write(root.join("README.md"), "not lisp")?;
    fs::write(
        root.join(".hidden").join("secret.el"),
        "(defun secret () nil)",
    )?;
    fs::write(
        root.join("target").join("generated.scm"),
        "(define generated #t)",
    )?;

    let discovery = discover_workspace_files(&WorkspaceDiscoveryOptions {
        roots: vec![root.clone()],
        include_unknown: true,
        include_hidden: true,
        include_generated: true,
        max_depth: None,
        exclude: Vec::new(),
        ..WorkspaceDiscoveryOptions::default()
    })?;

    assert_eq!(
        discovery.files,
        vec![
            root.join(".hidden").join("secret.el"),
            root.join("README.md"),
            root.join("main.lisp"),
            root.join("target").join("generated.scm"),
        ]
    );
    assert_eq!(discovery.skipped_unknown_count, 0);
    assert_eq!(discovery.skipped_hidden_count, 0);
    assert_eq!(discovery.skipped_generated_count, 0);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn discovery_excludes_files_and_directory_subtrees_by_component() -> Result<()> {
    let root = unique_temp_dir("exclude-paths");
    let excluded_dir = root.join("vendor");
    let prefix_sibling = root.join("vendor-copy");
    let excluded_file = root.join("excluded.lisp");
    fs::create_dir_all(&excluded_dir)?;
    fs::create_dir_all(&prefix_sibling)?;
    fs::write(root.join("keep.lisp"), "(defun keep () nil)")?;
    fs::write(&excluded_file, "(defun excluded () nil)")?;
    fs::write(excluded_dir.join("nested.lisp"), "(defun nested () nil)")?;
    fs::write(prefix_sibling.join("keep.lisp"), "(defun sibling () nil)")?;

    let discovery = discover_workspace_files(&WorkspaceDiscoveryOptions {
        roots: vec![root.clone()],
        include_unknown: false,
        include_hidden: false,
        include_generated: false,
        max_depth: None,
        exclude: vec![excluded_file, excluded_dir],
        ..WorkspaceDiscoveryOptions::default()
    })?;

    assert_eq!(
        discovery.files,
        vec![root.join("keep.lisp"), prefix_sibling.join("keep.lisp")]
    );
    assert_eq!(discovery.skipped_excluded_count, 2);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn discovery_can_exclude_an_explicit_root() -> Result<()> {
    let root = unique_temp_dir("exclude-root");
    fs::create_dir_all(&root)?;
    fs::write(root.join("nested.lisp"), "(defun nested () nil)")?;

    let discovery = discover_workspace_files(&WorkspaceDiscoveryOptions {
        roots: vec![root.clone()],
        include_unknown: false,
        include_hidden: false,
        include_generated: false,
        max_depth: None,
        exclude: vec![root.clone()],
        ..WorkspaceDiscoveryOptions::default()
    })?;

    assert!(discovery.files.is_empty());
    assert_eq!(discovery.skipped_excluded_count, 1);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn discovery_lexically_normalizes_nonexistent_exclude_aliases() -> Result<()> {
    let root = unique_temp_dir("exclude-nonexistent-alias");
    fs::create_dir_all(&root)?;
    let excluded = root.join("excluded.lisp");
    fs::write(&excluded, "(defun excluded () nil)")?;
    fs::write(root.join("keep.lisp"), "(defun keep () nil)")?;

    let discovery = discover_workspace_files(&WorkspaceDiscoveryOptions {
        roots: vec![root.clone()],
        include_unknown: false,
        include_hidden: false,
        include_generated: false,
        max_depth: None,
        exclude: vec![root.join("missing").join("..").join("excluded.lisp")],
        ..WorkspaceDiscoveryOptions::default()
    })?;

    assert_eq!(discovery.files, vec![root.join("keep.lisp")]);
    assert_eq!(discovery.skipped_excluded_count, 1);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn exclude_index_matches_relative_paths_and_normalized_subtrees() -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let relative_root = PathBuf::from("relative-workspace");
    let index = ExcludeIndex::new(&[
        relative_root.join("vendor").join(".").join("nested"),
        relative_root.join("generated").join("missing").join(".."),
    ])?;

    assert!(
        index
            .match_steps(&relative_root.join("vendor/nested/source.lisp"))
            .0
    );
    assert!(
        index
            .match_steps(&current_dir.join("relative-workspace/generated/output.lisp"))
            .0
    );
    assert!(
        !index
            .match_steps(&relative_root.join("vendor-copy/source.lisp"))
            .0
    );
    Ok(())
}

#[test]
fn exclude_index_lookup_steps_do_not_scale_with_exclude_count() -> Result<()> {
    let root = unique_temp_dir("exclude-index-complexity");
    let excludes = (0..4_000)
        .map(|index| root.join(format!("dependency-{index}")))
        .collect::<Vec<_>>();
    let index = ExcludeIndex::new(&excludes)?;
    let candidate = root.join("not-excluded/source.lisp");

    let (excluded, steps) = index.match_steps(&candidate);
    assert!(!excluded);
    assert!(steps <= candidate.components().count());
    assert!(steps < excludes.len());
    Ok(())
}

#[test]
fn exclude_index_rejects_excessive_untrusted_input() {
    let excludes = (0..4_097)
        .map(|index| PathBuf::from(format!("exclude-{index}")))
        .collect::<Vec<_>>();

    let error = ExcludeIndex::new(&excludes)
        .err()
        .expect("excessive exclude input should fail");
    assert!(
        error
            .to_string()
            .contains("workspace exclude path limit exceeded")
    );
}

#[test]
fn secure_read_rejects_a_path_outside_discovered_roots() -> Result<()> {
    let root = unique_temp_dir("read-root");
    let outside = unique_temp_dir("read-outside");
    fs::create_dir_all(&root)?;
    fs::create_dir_all(&outside)?;
    fs::write(root.join("inside.lisp"), "(defun inside () nil)")?;
    let outside_file = outside.join("outside.lisp");
    fs::write(&outside_file, "(defun outside () nil)")?;

    let discovery = discover_workspace_files(&WorkspaceDiscoveryOptions {
        roots: vec![root.clone()],
        include_unknown: false,
        include_hidden: false,
        include_generated: false,
        max_depth: None,
        exclude: Vec::new(),
        ..WorkspaceDiscoveryOptions::default()
    })?;

    let error = discovery.read_file(&outside_file).unwrap_err();
    assert!(error.to_string().contains("outside canonical roots"));

    fs::remove_dir_all(root)?;
    fs::remove_dir_all(outside)?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn secure_read_rejects_a_discovered_file_replaced_by_symlink() -> Result<()> {
    let root = unique_temp_dir("read-symlink-exchange");
    let outside = unique_temp_dir("read-symlink-target");
    fs::create_dir_all(&root)?;
    fs::create_dir_all(&outside)?;
    let discovered_file = root.join("source.lisp");
    let outside_file = outside.join("outside.lisp");
    fs::write(&discovered_file, "(defun original () nil)")?;
    fs::write(&outside_file, "(defun outside () nil)")?;

    let discovery = discover_workspace_files(&WorkspaceDiscoveryOptions {
        roots: vec![root.clone()],
        include_unknown: false,
        include_hidden: false,
        include_generated: false,
        max_depth: None,
        exclude: Vec::new(),
        ..WorkspaceDiscoveryOptions::default()
    })?;
    fs::remove_file(&discovered_file)?;
    std::os::unix::fs::symlink(&outside_file, &discovered_file)?;

    let error = discovery.read_file(&discovered_file).unwrap_err();
    assert!(error.to_string().contains("replaced or non-regular"));

    fs::remove_dir_all(root)?;
    fs::remove_dir_all(outside)?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn secure_read_rejects_a_discovered_file_replaced_by_fifo_without_blocking() -> Result<()> {
    let root = unique_temp_dir("read-fifo-exchange");
    fs::create_dir_all(&root)?;
    let discovered_file = root.join("source.lisp");
    fs::write(&discovered_file, "(defun original () nil)")?;

    let discovery = discover_workspace_files(&workspace_options(root.clone()))?;
    fs::remove_file(&discovered_file)?;
    let status = std::process::Command::new("mkfifo")
        .arg(&discovered_file)
        .status()?;
    assert!(status.success(), "mkfifo must succeed");

    let (sender, receiver) = std::sync::mpsc::channel();
    let read_thread = std::thread::spawn(move || {
        sender
            .send(discovery.read_file(&discovered_file))
            .expect("read result receiver should remain available");
    });
    let error = receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("FIFO read must be rejected without blocking")
        .unwrap_err();

    assert!(error.to_string().contains("replaced or non-regular"));
    read_thread.join().expect("read thread should not panic");
    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn secure_read_rejects_a_workspace_root_replaced_after_discovery() -> Result<()> {
    let root = unique_temp_dir("read-root-exchange");
    let displaced_root = unique_temp_dir("read-root-exchange-displaced");
    fs::create_dir_all(&root)?;
    fs::write(root.join("source.lisp"), b"(from-a)\n")?;

    let discovery = discover_workspace_files(&workspace_options(root.clone()))?;

    fs::rename(&root, &displaced_root)?;
    fs::create_dir_all(&root)?;
    fs::write(root.join("source.lisp"), b"(from-b)\n")?;

    let error = discovery
        .read_file(&root.join("source.lisp"))
        .expect_err("replacing the discovered root must be rejected");
    assert!(
        format!("{error:#}").contains("workspace root identity changed"),
        "unexpected error: {error:#}"
    );
    assert_eq!(fs::read(root.join("source.lisp"))?, b"(from-b)\n");
    assert_eq!(fs::read(displaced_root.join("source.lisp"))?, b"(from-a)\n");

    fs::remove_dir_all(&root)?;
    fs::remove_dir_all(&displaced_root)?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn root_capability_rejects_an_intermediate_directory_symlink_escape() -> Result<()> {
    let root = unique_temp_dir("read-intermediate-symlink");
    let outside = unique_temp_dir("read-intermediate-target");
    fs::create_dir_all(root.join("nested"))?;
    fs::create_dir_all(&outside)?;
    fs::write(root.join("nested/source.lisp"), "(inside)")?;
    fs::write(outside.join("source.lisp"), "(outside)")?;

    let discovery = discover_workspace_files(&workspace_options(root.clone()))?;
    fs::rename(root.join("nested"), root.join("original-nested"))?;
    std::os::unix::fs::symlink(&outside, root.join("nested"))?;

    let root_dir = &discovery.root_dirs[0].2;
    let error = root_dir.open("nested/source.lisp").unwrap_err();
    assert_ne!(error.kind(), std::io::ErrorKind::NotFound);

    fs::remove_dir_all(root)?;
    fs::remove_dir_all(outside)?;
    Ok(())
}

#[test]
fn secure_read_supports_a_single_file_root_without_authorizing_siblings() -> Result<()> {
    let root = unique_temp_dir("single-file-root");
    fs::create_dir_all(&root)?;
    let selected = root.join("selected.lisp");
    let sibling = root.join("sibling.lisp");
    fs::write(&selected, "(selected)")?;
    fs::write(&sibling, "(sibling)")?;

    let discovery = discover_workspace_files(&workspace_options(selected.clone()))?;

    assert_eq!(discovery.read_file(&selected)?, b"(selected)");
    let error = discovery.read_file(&sibling).unwrap_err();
    assert!(error.to_string().contains("outside canonical roots"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn discovery_rejects_too_many_input_roots_before_resolving_them() {
    let missing_base = unique_temp_dir("root-input-limit");
    let constrained = WorkspaceLimits::default();
    let options = WorkspaceDiscoveryOptions {
        roots: (0..=constrained.max_roots)
            .map(|index| missing_base.join(index.to_string()))
            .collect(),
        include_unknown: false,
        include_hidden: false,
        include_generated: false,
        max_depth: None,
        exclude: Vec::new(),
        ..WorkspaceDiscoveryOptions::default()
    };

    let error = discover_workspace_files_with_limits(&options, constrained).unwrap_err();

    assert!(format!("{error:#}").contains("root input limit exceeded"));
}

#[test]
fn duplicate_canonical_roots_share_one_capability() -> Result<()> {
    let root = unique_temp_dir("duplicate-roots");
    fs::create_dir_all(&root)?;
    fs::write(root.join("source.lisp"), "()")?;
    let mut options = workspace_options(root.clone());
    options.roots.push(root.clone());

    let discovery = discover_workspace_files(&options)?;

    assert_eq!(discovery.root_dirs.len(), 1);
    assert_eq!(discovery.files(), &[root.join("source.lisp")]);
    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn duplicate_canonical_roots_prefer_real_directory_over_lexical_symlink() -> Result<()> {
    let base = unique_temp_dir("duplicate-root-alias");
    let real_root = base.join("z-real");
    let alias_root = base.join("a-alias");
    let source = real_root.join("source.lisp");
    fs::create_dir_all(&real_root)?;
    fs::write(&source, "()")?;
    std::os::unix::fs::symlink(&real_root, &alias_root)?;

    let mut options = workspace_options(alias_root);
    options.roots.push(real_root);
    let discovery = discover_workspace_files(&options)?;

    assert_eq!(discovery.root_dirs.len(), 1);
    assert_eq!(discovery.files(), &[source]);
    assert_eq!(discovery.skipped_symlink_count, 0);
    fs::remove_dir_all(base)?;
    Ok(())
}

#[test]
fn overlapping_roots_select_the_longest_containing_capability() -> Result<()> {
    let root = unique_temp_dir("overlapping-roots");
    let nested = root.join("nested");
    let source = nested.join("source.lisp");
    fs::create_dir_all(&nested)?;
    fs::write(&source, "()")?;
    let mut options = workspace_options(root.clone());
    options.roots.push(nested.clone());

    let discovery = discover_workspace_files(&options)?;
    let canonical_source = fs::canonicalize(&source)?;
    let canonical_nested = fs::canonicalize(&nested)?;

    assert_eq!(discovery.root_dirs.len(), 2);
    assert_eq!(
        discovery
            .root_capability_for(&canonical_source)
            .map(|capability| &capability.0),
        Some(&canonical_nested)
    );
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn max_depth_preserves_nested_roots_as_independent_scan_origins() -> Result<()> {
    let root = unique_temp_dir("nested-root-max-depth");
    let nested = root.join("level-one").join("nested-root");
    let parent_source = root.join("parent.lisp");
    let nested_source = nested.join("nested.lisp");
    fs::create_dir_all(&nested)?;
    fs::write(&parent_source, "()")?;
    fs::write(&nested_source, "()")?;
    let mut options = workspace_options(root.clone());
    options.roots.push(nested);
    options.max_depth = Some(1);

    let discovery = discover_workspace_files(&options)?;
    let mut expected = vec![parent_source, nested_source];
    expected.sort();

    assert_eq!(discovery.files(), expected);
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn secure_read_rejects_a_regular_file_added_after_discovery() -> Result<()> {
    let root = unique_temp_dir("read-undiscovered");
    fs::create_dir_all(&root)?;
    fs::write(root.join("selected.lisp"), "()")?;
    let discovery = discover_workspace_files(&workspace_options(root.clone()))?;
    let added = root.join("added.lisp");
    fs::write(&added, "()")?;

    let error = discovery.read_file(&added).unwrap_err();

    assert!(error.to_string().contains("not selected during discovery"));
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn discovery_enforces_entry_and_file_count_limits() -> Result<()> {
    let root = unique_temp_dir("count-limits");
    fs::create_dir_all(&root)?;
    fs::write(root.join("a.lisp"), "()")?;
    fs::write(root.join("b.lisp"), "()")?;

    let entry_error =
        discover_workspace_files_with_limits(&workspace_options(root.clone()), limits(1, 2, 2, 4))
            .unwrap_err();
    assert!(format!("{entry_error:#}").contains("entry limit exceeded"));

    let file_error =
        discover_workspace_files_with_limits(&workspace_options(root.clone()), limits(2, 1, 2, 4))
            .unwrap_err();
    assert!(format!("{file_error:#}").contains("file limit exceeded"));

    let at_limit =
        discover_workspace_files_with_limits(&workspace_options(root.clone()), limits(2, 2, 2, 4))?;
    assert_eq!(at_limit.files().len(), 2);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn discovery_enforces_file_and_total_byte_limits() -> Result<()> {
    let root = unique_temp_dir("byte-limits");
    fs::create_dir_all(&root)?;
    fs::write(root.join("a.lisp"), "1234")?;
    fs::write(root.join("b.lisp"), "5678")?;

    let file_error =
        discover_workspace_files_with_limits(&workspace_options(root.clone()), limits(2, 2, 3, 8))
            .unwrap_err();
    assert!(format!("{file_error:#}").contains("file size limit exceeded"));

    let total_error =
        discover_workspace_files_with_limits(&workspace_options(root.clone()), limits(2, 2, 4, 7))
            .unwrap_err();
    assert!(format!("{total_error:#}").contains("total byte limit exceeded"));

    let at_limit =
        discover_workspace_files_with_limits(&workspace_options(root.clone()), limits(2, 2, 4, 8))?;
    assert_eq!(at_limit.files().len(), 2);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn secure_read_is_bounded_after_a_discovered_file_grows() -> Result<()> {
    let root = unique_temp_dir("read-growth");
    fs::create_dir_all(&root)?;
    let file = root.join("source.lisp");
    fs::write(&file, "12")?;
    let discovery =
        discover_workspace_files_with_limits(&workspace_options(root.clone()), limits(1, 1, 3, 3))?;
    fs::write(&file, "1234")?;

    let error = discovery.read_file(&file).unwrap_err();

    assert!(error.to_string().contains("while reading"));
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn secure_read_enforces_total_bytes_for_actual_contents() -> Result<()> {
    let root = unique_temp_dir("read-total-growth");
    fs::create_dir_all(&root)?;
    let first = root.join("a.lisp");
    let second = root.join("b.lisp");
    fs::write(&first, "12")?;
    fs::write(&second, "34")?;
    let discovery =
        discover_workspace_files_with_limits(&workspace_options(root.clone()), limits(2, 2, 4, 6))?;
    fs::write(&first, "1234")?;
    fs::write(&second, "5678")?;

    assert_eq!(discovery.read_file(&first)?, b"1234");
    let error = discovery.read_file(&second).unwrap_err();

    assert!(error.to_string().contains("total read limit exceeded"));
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn concurrent_reads_across_clones_compete_for_a_strict_shared_budget() -> Result<()> {
    let root = unique_temp_dir("read-concurrent-budget");
    fs::create_dir_all(&root)?;
    let first = root.join("a.lisp");
    let second = root.join("b.lisp");
    fs::write(&first, "1")?;
    fs::write(&second, "2")?;
    let discovery =
        discover_workspace_files_with_limits(&workspace_options(root.clone()), limits(2, 2, 4, 4))?;
    fs::write(&first, "1234")?;
    fs::write(&second, "5678")?;

    let barrier = Arc::new(Barrier::new(3));
    let spawn_read = |discovery: WorkspaceDiscovery, path: PathBuf, barrier: Arc<Barrier>| {
        std::thread::spawn(move || {
            barrier.wait();
            discovery.read_file(&path)
        })
    };
    let first_read = spawn_read(discovery.clone(), first, Arc::clone(&barrier));
    let second_read = spawn_read(discovery.clone(), second, Arc::clone(&barrier));
    barrier.wait();

    let results = [
        first_read.join().expect("first read should not panic"),
        second_read.join().expect("second read should not panic"),
    ];
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
    let error = results
        .iter()
        .find_map(|result| result.as_ref().err())
        .expect("one read must exceed the shared budget");
    assert!(error.to_string().contains("total read limit exceeded"));
    assert_eq!(discovery.read_bytes.load(Ordering::Acquire), 4);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn file_limit_failure_rolls_back_chunks_reserved_by_that_read() -> Result<()> {
    let root = unique_temp_dir("read-file-limit-rollback");
    fs::create_dir_all(&root)?;
    let oversized = root.join("oversized.lisp");
    let valid = root.join("valid.lisp");
    fs::write(&oversized, "1")?;
    fs::write(&valid, "2")?;
    let byte_limit = u64::try_from(READ_CHUNK_BYTES)?;
    let discovery = discover_workspace_files_with_limits(
        &workspace_options(root.clone()),
        limits(2, 2, byte_limit, byte_limit),
    )?;
    fs::write(&oversized, vec![b'x'; READ_CHUNK_BYTES + 1])?;
    fs::write(&valid, vec![b'y'; READ_CHUNK_BYTES])?;

    let error = discovery.read_file(&oversized).unwrap_err();
    assert!(error.to_string().contains("file size limit exceeded"));
    assert_eq!(discovery.read_bytes.load(Ordering::Acquire), 0);
    assert_eq!(discovery.read_file(&valid)?.len(), READ_CHUNK_BYTES);
    assert_eq!(discovery.read_bytes.load(Ordering::Acquire), byte_limit);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn io_failure_rolls_back_chunks_reserved_by_that_read() -> Result<()> {
    struct FailsAfterData(bool);

    impl Read for FailsAfterData {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.0 {
                return Err(io::Error::other("injected read failure"));
            }
            self.0 = true;
            buffer[..4].copy_from_slice(b"1234");
            Ok(4)
        }
    }

    let limits = limits(1, 1, 4, 4);
    let read_bytes = AtomicU64::new(0);
    let path = PathBuf::from("injected.lisp");

    let error = read_bounded(FailsAfterData(false), &path, limits, &read_bytes).unwrap_err();
    // The underlying I/O failure is the chain's root, not the top message.
    // `anyhow`'s `{:#}` used to flatten the chain into one string; a typed
    // error keeps them separate, and the CLI still renders the whole chain
    // because it converts into `anyhow::Error` first, whose alternate Display
    // walks `source()`.
    assert!(
        error
            .root_cause()
            .to_string()
            .contains("injected read failure")
    );
    assert_eq!(read_bytes.load(Ordering::Acquire), 0);
    assert_eq!(
        read_bounded(Cursor::new(b"5678"), &path, limits, &read_bytes)?,
        b"5678"
    );
    assert_eq!(read_bytes.load(Ordering::Acquire), 4);
    Ok(())
}

fn workspace_options(root: PathBuf) -> WorkspaceDiscoveryOptions {
    WorkspaceDiscoveryOptions {
        roots: vec![root],
        include_unknown: false,
        include_hidden: false,
        include_generated: false,
        max_depth: None,
        exclude: Vec::new(),
        ..WorkspaceDiscoveryOptions::default()
    }
}

fn limits(
    max_entries: usize,
    max_files: usize,
    max_file_bytes: u64,
    max_total_bytes: u64,
) -> WorkspaceLimits {
    WorkspaceLimits {
        max_roots: WorkspaceLimits::default().max_roots,
        max_entries,
        max_files,
        max_file_bytes,
        max_total_bytes,
    }
}

fn unique_temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is before UNIX_EPOCH")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "paredit-cli-workspace-{name}-{}-{nanos}",
        std::process::id()
    ))
}

// --- F1/F2: ignore files ---------------------------------------------------

/// Makes `root` look like a repository root without running git.
///
/// Only the presence of `.git` matters to the traversal, and creating it as a
/// directory keeps these tests free of a git dependency.
fn mark_repository(root: &std::path::Path) -> Result<()> {
    fs::create_dir_all(root.join(".git"))?;
    Ok(())
}

#[test]
fn gitignore_excludes_files_and_prunes_directories() -> Result<()> {
    let root = unique_temp_dir("gitignore");
    fs::create_dir_all(root.join("generated"))?;
    mark_repository(&root)?;
    fs::write(root.join(".gitignore"), "generated/\n*.fasl.lisp\n")?;
    fs::write(root.join("main.lisp"), "(defun main () nil)")?;
    fs::write(root.join("cache.fasl.lisp"), "(defun cached () nil)")?;
    fs::write(
        root.join("generated").join("out.lisp"),
        "(defun out () nil)",
    )?;

    let discovery = discover_workspace_files(&WorkspaceDiscoveryOptions {
        roots: vec![root.clone()],
        ..WorkspaceDiscoveryOptions::default()
    })?;

    assert_eq!(discovery.files, vec![root.join("main.lisp")]);
    assert_eq!(discovery.skipped_ignored_count, 2);
    assert_eq!(
        discovery.ignore_files_read,
        vec![root.join(".gitignore")],
        "the repository's own ignore file is the only one in scope"
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn a_nested_gitignore_can_reinclude_what_the_root_dropped() -> Result<()> {
    let root = unique_temp_dir("gitignore-nested");
    fs::create_dir_all(root.join("src"))?;
    mark_repository(&root)?;
    fs::write(root.join(".gitignore"), "*.lisp\n")?;
    fs::write(root.join("src").join(".gitignore"), "!keep.lisp\n")?;
    fs::write(root.join("src").join("keep.lisp"), "(defun keep () nil)")?;
    fs::write(root.join("src").join("drop.lisp"), "(defun drop () nil)")?;

    let discovery = discover_workspace_files(&WorkspaceDiscoveryOptions {
        roots: vec![root.clone()],
        ..WorkspaceDiscoveryOptions::default()
    })?;

    assert_eq!(discovery.files, vec![root.join("src").join("keep.lisp")]);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn a_root_gitignore_still_applies_when_a_subdirectory_is_scanned() -> Result<()> {
    let root = unique_temp_dir("gitignore-primed");
    fs::create_dir_all(root.join("src"))?;
    mark_repository(&root)?;
    fs::write(root.join(".gitignore"), "drop.lisp\n")?;
    fs::write(root.join("src").join("keep.lisp"), "(defun keep () nil)")?;
    fs::write(root.join("src").join("drop.lisp"), "(defun drop () nil)")?;

    let discovery = discover_workspace_files(&WorkspaceDiscoveryOptions {
        roots: vec![root.join("src")],
        ..WorkspaceDiscoveryOptions::default()
    })?;

    assert_eq!(discovery.files, vec![root.join("src").join("keep.lisp")]);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn pareditignore_outranks_gitignore_at_the_same_level() -> Result<()> {
    let root = unique_temp_dir("pareditignore");
    mark_repository(&root)?;
    // Committed on purpose, so git does not ignore it, but not worth analysing.
    fs::write(root.join(".gitignore"), "\n")?;
    fs::write(root.join(".pareditignore"), "vendored.lisp\n")?;
    fs::write(root.join("main.lisp"), "(defun main () nil)")?;
    fs::write(root.join("vendored.lisp"), "(defun vendored () nil)")?;

    let discovery = discover_workspace_files(&WorkspaceDiscoveryOptions {
        roots: vec![root.clone()],
        ..WorkspaceDiscoveryOptions::default()
    })?;

    assert_eq!(discovery.files, vec![root.join("main.lisp")]);
    assert_eq!(discovery.skipped_ignored_count, 1);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn ignore_files_can_be_switched_off_entirely() -> Result<()> {
    let root = unique_temp_dir("ignore-off");
    mark_repository(&root)?;
    fs::write(root.join(".gitignore"), "*.lisp\n")?;
    fs::write(root.join("main.lisp"), "(defun main () nil)")?;

    let discovery = discover_workspace_files(
        &WorkspaceDiscoveryOptions {
            roots: vec![root.clone()],
            ..WorkspaceDiscoveryOptions::default()
        }
        .without_ignore_files(),
    )?;

    assert_eq!(discovery.files, vec![root.join("main.lisp")]);
    assert_eq!(discovery.skipped_ignored_count, 0);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn an_explicitly_named_root_is_never_ignore_filtered() -> Result<()> {
    let root = unique_temp_dir("ignore-explicit-root");
    fs::create_dir_all(root.join("generated"))?;
    mark_repository(&root)?;
    fs::write(root.join(".gitignore"), "generated/\n")?;
    fs::write(
        root.join("generated").join("out.lisp"),
        "(defun out () nil)",
    )?;

    // Pointing at the ignored directory is a stronger statement than the
    // pattern that covers it, exactly as `git add -f` is.
    let discovery = discover_workspace_files(&WorkspaceDiscoveryOptions {
        roots: vec![root.join("generated")],
        ..WorkspaceDiscoveryOptions::default()
    })?;

    assert_eq!(
        discovery.files,
        vec![root.join("generated").join("out.lisp")]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn a_nested_repository_does_not_inherit_the_outer_ignore_rules() -> Result<()> {
    let root = unique_temp_dir("nested-repo");
    let inner = root.join("nested").join("inner");
    fs::create_dir_all(&inner)?;
    mark_repository(&root)?;
    mark_repository(&inner)?;
    fs::write(root.join(".gitignore"), "*.lisp\n")?;
    fs::write(root.join("outer.lisp"), "(defun outer () nil)")?;
    fs::write(inner.join("inner.lisp"), "(defun inner () nil)")?;

    let discovery = discover_workspace_files(&WorkspaceDiscoveryOptions {
        roots: vec![root.clone()],
        ..WorkspaceDiscoveryOptions::default()
    })?;

    assert_eq!(discovery.files, vec![inner.join("inner.lisp")]);

    fs::remove_dir_all(root)?;
    Ok(())
}

// --- F3: command-line globs ------------------------------------------------

#[test]
fn include_globs_select_and_exclude_globs_override_them() -> Result<()> {
    let root = unique_temp_dir("globs");
    fs::create_dir_all(root.join("src").join("nested"))?;
    fs::create_dir_all(root.join("test"))?;
    fs::write(root.join("src").join("a.lisp"), "(defun a () nil)")?;
    fs::write(
        root.join("src").join("nested").join("b.lisp"),
        "(defun b () nil)",
    )?;
    fs::write(root.join("src").join("skip.lisp"), "(defun skip () nil)")?;
    fs::write(root.join("test").join("t.lisp"), "(defun t () nil)")?;

    let discovery = discover_workspace_files(&WorkspaceDiscoveryOptions {
        roots: vec![root.clone()],
        include_globs: GlobSet::parse_file("src/**\n")?,
        exclude_globs: GlobSet::parse_file("**/skip.lisp\n")?,
        ..WorkspaceDiscoveryOptions::default()
    })?;

    assert_eq!(
        discovery.files,
        vec![
            root.join("src").join("a.lisp"),
            root.join("src").join("nested").join("b.lisp"),
        ]
    );
    assert!(discovery.skipped_glob_count >= 2);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn an_include_glob_does_not_prune_the_directories_holding_its_matches() -> Result<()> {
    let root = unique_temp_dir("glob-descend");
    fs::create_dir_all(root.join("a").join("b").join("c"))?;
    fs::write(
        root.join("a").join("b").join("c").join("deep.lisp"),
        "(defun deep () nil)",
    )?;

    let discovery = discover_workspace_files(&WorkspaceDiscoveryOptions {
        roots: vec![root.clone()],
        include_globs: GlobSet::parse_file("a/**/deep.lisp\n")?,
        ..WorkspaceDiscoveryOptions::default()
    })?;

    assert_eq!(
        discovery.files,
        vec![root.join("a").join("b").join("c").join("deep.lisp")]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

// --- F9: repository grouping -----------------------------------------------

#[test]
fn discovered_files_are_grouped_by_the_repository_that_contains_them() -> Result<()> {
    let root = unique_temp_dir("multi-repo");
    let first = root.join("first");
    let second = root.join("second");
    fs::create_dir_all(&first)?;
    fs::create_dir_all(&second)?;
    fs::create_dir_all(root.join("loose"))?;
    mark_repository(&first)?;
    mark_repository(&second)?;
    fs::write(first.join("a.lisp"), "(defun a () nil)")?;
    fs::write(second.join("b.lisp"), "(defun b () nil)")?;
    fs::write(root.join("loose").join("c.lisp"), "(defun c () nil)")?;

    let discovery = discover_workspace_files(&WorkspaceDiscoveryOptions {
        roots: vec![root.clone()],
        ..WorkspaceDiscoveryOptions::default()
    })?;

    let canonical_root = fs::canonicalize(&root)?;
    assert_eq!(discovery.repositories.len(), 2);
    assert!(
        discovery
            .repositories
            .contains_key(&canonical_root.join("first"))
            || discovery.repositories.contains_key(&first),
        "repository keys are the directory holding .git: {:?}",
        discovery.repositories.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        discovery.files_outside_repositories,
        vec![root.join("loose").join("c.lisp")]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

// --- F12: symlink policy ---------------------------------------------------

#[cfg(unix)]
#[test]
fn following_symlinks_traverses_a_link_inside_the_roots() -> Result<()> {
    let root = unique_temp_dir("symlink-follow");
    fs::create_dir_all(root.join("real"))?;
    fs::write(root.join("real").join("a.lisp"), "(defun a () nil)")?;
    std::os::unix::fs::symlink(root.join("real"), root.join("link"))?;

    let skipped = discover_workspace_files(&WorkspaceDiscoveryOptions {
        roots: vec![root.clone()],
        ..WorkspaceDiscoveryOptions::default()
    })?;
    assert_eq!(skipped.files, vec![root.join("real").join("a.lisp")]);
    assert_eq!(skipped.skipped_symlink_count, 1);

    let followed = discover_workspace_files(&WorkspaceDiscoveryOptions {
        roots: vec![root.clone()],
        symlinks: SymlinkPolicy::Follow,
        ..WorkspaceDiscoveryOptions::default()
    })?;
    // The link and the real directory reach the same file, and canonical
    // deduplication keeps the result a set: the traversal reports whichever
    // path it reached first, not both.
    assert_eq!(followed.files.len(), 1);
    assert_eq!(
        fs::canonicalize(&followed.files[0])?,
        fs::canonicalize(root.join("real").join("a.lisp"))?
    );
    assert_eq!(followed.skipped_symlink_count, 0);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn following_symlinks_refuses_a_target_outside_the_roots() -> Result<()> {
    let root = unique_temp_dir("symlink-escape");
    let outside = unique_temp_dir("symlink-escape-target");
    fs::create_dir_all(&root)?;
    fs::create_dir_all(&outside)?;
    fs::write(outside.join("secret.lisp"), "(defun secret () nil)")?;
    std::os::unix::fs::symlink(&outside, root.join("escape"))?;

    let discovery = discover_workspace_files(&WorkspaceDiscoveryOptions {
        roots: vec![root.clone()],
        symlinks: SymlinkPolicy::Follow,
        ..WorkspaceDiscoveryOptions::default()
    })?;

    assert!(discovery.files.is_empty());
    assert_eq!(discovery.skipped_symlink_escaped_count, 1);

    fs::remove_dir_all(root)?;
    fs::remove_dir_all(outside)?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn following_symlinks_stops_at_a_loop_instead_of_recursing() -> Result<()> {
    let root = unique_temp_dir("symlink-loop");
    fs::create_dir_all(root.join("a"))?;
    fs::write(root.join("a").join("x.lisp"), "(defun x () nil)")?;
    // `a/self` points back at `a`, which an unguarded walk descends forever.
    std::os::unix::fs::symlink(root.join("a"), root.join("a").join("self"))?;

    let discovery = discover_workspace_files(&WorkspaceDiscoveryOptions {
        roots: vec![root.clone()],
        symlinks: SymlinkPolicy::Follow,
        ..WorkspaceDiscoveryOptions::default()
    })?;

    assert_eq!(discovery.files.len(), 1);
    assert_eq!(
        fs::canonicalize(&discovery.files[0])?,
        fs::canonicalize(root.join("a").join("x.lisp"))?
    );
    assert_eq!(discovery.skipped_symlink_cycle_count, 1);

    fs::remove_dir_all(root)?;
    Ok(())
}
