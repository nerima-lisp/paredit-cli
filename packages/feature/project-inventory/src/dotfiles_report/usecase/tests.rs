use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

fn with_root(body: impl FnOnce(&Path)) {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "paredit-dotfiles-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();
    let result = catch_unwind(AssertUnwindSafe(|| body(&root)));
    let cleanup = fs::remove_dir_all(&root);
    match result {
        Ok(()) => cleanup.unwrap(),
        Err(error) => {
            if let Err(cleanup) = cleanup {
                eprintln!("dotfiles fixture cleanup failed: {cleanup}");
            }
            resume_unwind(error);
        }
    }
}

#[test]
fn absent_dotfiles_is_a_complete_empty_inventory() {
    with_root(|root| {
        assert_eq!(
            build(root).unwrap().to_json(),
            serde_json::json!({
                "schema_version": 1,
                "present": false, "complete": true, "entries": [], "diagnostics": [],
            })
        );
    });
}

#[test]
fn nested_inventory_is_sorted_classified_and_never_contains_bodies() {
    with_root(|root| {
        fs::create_dir_all(root.join(".paredit/rules")).unwrap();
        fs::create_dir(root.join(".paredit/migrations")).unwrap();
        fs::write(root.join(".paredit/z.txt"), "confidential-marker").unwrap();
        fs::write(
            root.join(".paredit/rules/a.lisp"),
            "(error \"not executed\")",
        )
        .unwrap();
        fs::write(root.join(".paredit/kill-ring.json"), "not valid JSON").unwrap();
        fs::write(root.join(".paredit/migrations/a.lisp"), "invalid Lisp (((").unwrap();
        let report = build(root).unwrap();
        assert!(report.present && report.complete);
        assert_eq!(
            report
                .entries
                .iter()
                .map(|entry| (entry.path.as_str(), entry.category))
                .collect::<Vec<_>>(),
            vec![
                (".paredit", Category::Other),
                (".paredit/kill-ring.json", Category::KillRing),
                (".paredit/migrations", Category::Migration),
                (".paredit/migrations/a.lisp", Category::Migration),
                (".paredit/rules", Category::Rule),
                (".paredit/rules/a.lisp", Category::Rule),
                (".paredit/z.txt", Category::Other),
            ]
        );
        assert!(report.diagnostics.is_empty());
        let json = report.to_json().to_string();
        assert!(!json.contains("confidential-marker"));
        assert!(!json.contains("not executed"));
        assert!(!json.contains(root.to_str().unwrap()));
        assert_eq!(report.entries[6].bytes, Some(19));
        assert_eq!(
            fs::read(root.join(".paredit/z.txt")).unwrap(),
            b"confidential-marker"
        );
    });
}

#[test]
fn missing_root_and_regular_dotfiles_fail_explicitly() {
    with_root(|root| {
        assert!(build(&root.join("missing")).is_err());
        fs::write(root.join(".paredit"), "configuration").unwrap();
        let error = build(root).unwrap_err().chain();
        assert!(error.contains(".paredit must be a directory"));
        assert!(!error.contains(root.to_str().unwrap()));
    });
}

#[test]
fn entry_budget_trips_and_discards_non_deterministic_partial_results() {
    with_root(|root| {
        fs::create_dir(root.join(".paredit")).unwrap();
        fs::write(root.join(".paredit/b"), "").unwrap();
        let limits = Limits {
            entries: 2,
            ..LIMITS
        };
        assert!(build_with_limits(root, limits).unwrap().complete);
        fs::write(root.join(".paredit/a"), "").unwrap();
        let report = build_with_limits(root, limits).unwrap();
        assert!(report.present);
        assert!(!report.complete);
        assert!(report.entries.is_empty());
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].path, ".paredit");
        assert_eq!(report.diagnostics[0].code, "entry_limit");
        assert_eq!(report, build_with_limits(root, limits).unwrap());
    });
}

#[test]
fn nested_entry_budget_cannot_exceed_the_global_cap() {
    with_root(|root| {
        fs::create_dir_all(root.join(".paredit/a")).unwrap();
        fs::write(root.join(".paredit/a/child"), "").unwrap();
        fs::write(root.join(".paredit/b"), "").unwrap();
        let report = build_with_limits(
            root,
            Limits {
                entries: 3,
                ..LIMITS
            },
        )
        .unwrap();
        assert!(!report.complete);
        assert!(report.entries.is_empty());
        assert_eq!(report.diagnostics[0].code, "entry_limit");
    });
}

#[test]
fn file_size_budget_trips_only_above_limit() {
    with_root(|root| {
        fs::create_dir(root.join(".paredit")).unwrap();
        fs::write(root.join(".paredit/b"), "12345").unwrap();
        fs::write(root.join(".paredit/a"), "1234").unwrap();
        let report = build_with_limits(
            root,
            Limits {
                file_bytes: 4,
                ..LIMITS
            },
        )
        .unwrap();
        assert!(!report.complete);
        assert_eq!(report.entries[1].status, EntryStatus::Inventoried);
        assert_eq!(report.entries[2].status, EntryStatus::FileSizeLimit);
        assert_eq!(report.entries[2].bytes, Some(5));
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].path, ".paredit/b");
        assert_eq!(report.diagnostics[0].code, "file_size_limit");
    });
}

#[test]
fn depth_budget_reports_boundary_without_descending() {
    with_root(|root| {
        fs::create_dir_all(root.join(".paredit/a/b")).unwrap();
        fs::write(root.join(".paredit/a/b/hidden"), "").unwrap();
        let report = build_with_limits(root, Limits { depth: 2, ..LIMITS }).unwrap();
        assert!(!report.complete);
        assert_eq!(report.entries.len(), 3);
        assert_eq!(report.entries[1].status, EntryStatus::Inventoried);
        assert_eq!(report.entries[2].path, ".paredit/a/b");
        assert_eq!(report.entries[2].status, EntryStatus::DepthLimit);
        assert_eq!(report.diagnostics[0].code, "depth_limit");
        assert!(
            build_with_limits(root, Limits { depth: 3, ..LIMITS })
                .unwrap()
                .complete
        );
    });
}

#[cfg(unix)]
#[test]
fn symlink_files_directories_and_loops_are_not_followed() {
    use std::os::unix::fs::symlink;
    with_root(|root| {
        fs::create_dir(root.join(".paredit")).unwrap();
        fs::create_dir(root.join("outside")).unwrap();
        fs::write(root.join("outside/secret"), "secret-body").unwrap();
        symlink(root.join("outside"), root.join(".paredit/directory")).unwrap();
        symlink(root.join("outside/secret"), root.join(".paredit/file")).unwrap();
        symlink(".", root.join(".paredit/loop")).unwrap();
        symlink("missing", root.join(".paredit/dangling")).unwrap();
        let report = build(root).unwrap();
        assert!(report.complete);
        assert_eq!(report.entries.len(), 5);
        for entry in &report.entries[1..] {
            assert_eq!(entry.kind, EntryKind::Symlink);
            assert_eq!(entry.status, EntryStatus::NotFollowed);
            assert_eq!(entry.bytes, None);
        }
        assert!(!report.to_json().to_string().contains("secret"));
        assert_eq!(
            fs::read(root.join("outside/secret")).unwrap(),
            b"secret-body"
        );
    });
}

#[cfg(unix)]
#[test]
fn dotfiles_root_symlink_is_inventoried_without_following() {
    use std::os::unix::fs::symlink;
    with_root(|root| {
        fs::create_dir(root.join("outside")).unwrap();
        fs::write(root.join("outside/secret"), "secret-body").unwrap();
        symlink(root.join("outside"), root.join(".paredit")).unwrap();
        let report = build(root).unwrap();
        assert!(report.present && report.complete);
        assert_eq!(report.entries.len(), 1);
        assert_eq!(report.entries[0].status, EntryStatus::NotFollowed);
        assert!(!report.to_json().to_string().contains("outside"));
    });
}

#[cfg(unix)]
#[test]
fn directory_open_refuses_a_symlink_even_without_metadata_precheck() {
    use std::os::unix::fs::symlink;
    with_root(|root| {
        fs::create_dir(root.join("outside")).unwrap();
        symlink(root.join("outside"), root.join("link")).unwrap();
        let directory = Dir::open_ambient_dir(root, cap_std::ambient_authority()).unwrap();
        assert!(open_child(&directory, Path::new("outside"), "outside").is_ok());
        assert!(open_child(&directory, Path::new("link"), "link").is_err());
    });
}

#[cfg(unix)]
#[test]
fn non_utf8_names_fail_instead_of_merging_lossy_paths() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    assert_eq!(
        inventory_name(OsString::from("valid.lisp"), ".paredit").unwrap(),
        "valid.lisp"
    );
    // macOS rejects invalid UTF-8 at file creation; exercise the production conversion directly.
    let error = inventory_name(OsString::from_vec(vec![0xff]), ".paredit").unwrap_err();
    assert_eq!(
        error.chain(),
        "listing inventory directory .paredit: inventory filename is not UTF-8"
    );
    let CliError::Io { source, .. } = error else {
        panic!("filename encoding rejection must preserve its I/O error");
    };
    assert_eq!(source.kind(), std::io::ErrorKind::InvalidData);
}

#[cfg(unix)]
#[test]
fn special_files_are_reported_without_opening_them() {
    use std::os::unix::net::UnixListener;
    with_root(|root| {
        fs::create_dir(root.join(".paredit")).unwrap();
        let listener = UnixListener::bind(root.join(".paredit/socket")).unwrap();
        let report = build(root).unwrap();
        assert!(report.complete);
        assert_eq!(report.entries.len(), 2);
        assert_eq!(report.entries[1].kind, EntryKind::Other);
        assert_eq!(report.entries[1].status, EntryStatus::Unsupported);
        drop(listener);
    });
}
