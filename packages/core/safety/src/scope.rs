//! The blast radius of a write, stated rather than implied.
//!
//! This tool already confines writes: `--root` opens a capability on one
//! directory and every path is resolved through it, so a manifest naming
//! `../../etc/hosts` is refused rather than followed. The confinement is real
//! and, until now, invisible — a caller had to infer it from the absence of an
//! error.
//!
//! That asymmetry matters most for the case the confinement exists for. An
//! agent handed a manifest it did not write cannot tell, before passing
//! `--write`, whether the run will touch two files in `src/` or thirty across
//! the tree. A [`WriteScope`] answers that in the dry-run output: which paths
//! are in scope, whether a root is enforced, and — checked here rather than
//! asserted — whether every path really is under it.
//!
//! The last part is the reason this is a type and not a formatting helper.
//! [`WriteScope::escaping_paths`] re-derives the confinement claim from the
//! paths themselves, so a report saying "confined to /src" beside a path in
//! `/etc` is a contradiction the tool catches instead of printing.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// The set of paths one command may write, and the root confining them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WriteScope {
    /// The canonical directory writes are confined to, when one was requested.
    root: Option<PathBuf>,
    /// Paths this run would write, canonical and sorted.
    targets: Vec<PathBuf>,
    /// Paths considered and left alone, canonical and sorted.
    ///
    /// Reported beside the targets because "in the manifest but unchanged" and
    /// "not in the manifest" are different assurances, and a caller reviewing
    /// a blast radius wants both.
    unchanged: Vec<PathBuf>,
}

impl WriteScope {
    /// Builds a scope from the paths a command resolved.
    ///
    /// Both lists are sorted and deduplicated here so the disclosure is
    /// byte-identical across runs regardless of manifest order.
    #[must_use]
    pub fn new(
        root: Option<PathBuf>,
        targets: impl IntoIterator<Item = PathBuf>,
        unchanged: impl IntoIterator<Item = PathBuf>,
    ) -> Self {
        Self {
            root,
            targets: sorted_unique(targets),
            unchanged: sorted_unique(unchanged),
        }
    }

    /// Whether a confinement root is enforced.
    #[must_use]
    pub const fn confined(&self) -> bool {
        self.root.is_some()
    }

    #[must_use]
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    #[must_use]
    pub fn targets(&self) -> &[PathBuf] {
        &self.targets
    }

    #[must_use]
    pub fn target_count(&self) -> usize {
        self.targets.len()
    }

    /// Targets that are *not* under the declared root.
    ///
    /// Must always be empty: the root guard rejects such a path long before a
    /// scope is built. It is computed anyway because a disclosure that merely
    /// repeats what the code intended is worth very little — this one is
    /// checked against the paths that actually came out.
    #[must_use]
    pub fn escaping_paths(&self) -> Vec<&Path> {
        let Some(root) = self.root.as_deref() else {
            return Vec::new();
        };
        self.targets
            .iter()
            .map(PathBuf::as_path)
            .filter(|path| !path.starts_with(root))
            .collect()
    }

    /// The disclosure, as a caller's automation reads it.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "confined": self.confined(),
            "root": self.root.as_ref().map(|root| root.display().to_string()),
            "target_count": self.targets.len(),
            "targets": display_paths(&self.targets),
            "unchanged_count": self.unchanged.len(),
            "unchanged": display_paths(&self.unchanged),
            "escaping_paths": self
                .escaping_paths()
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>(),
        })
    }

    /// The disclosure as tab-separated rows, in the shape the text renderers
    /// of this tool use.
    #[must_use]
    pub fn text_rows(&self) -> Vec<(String, String)> {
        let mut rows = vec![
            (
                "write_scope_confined".to_owned(),
                self.confined().to_string(),
            ),
            (
                "write_scope_target_count".to_owned(),
                self.targets.len().to_string(),
            ),
        ];
        if let Some(root) = &self.root {
            rows.push(("write_scope_root".to_owned(), root.display().to_string()));
        }
        for target in &self.targets {
            rows.push((
                "write_scope_target".to_owned(),
                target.display().to_string(),
            ));
        }
        for path in self.escaping_paths() {
            rows.push((
                "write_scope_escaping_path".to_owned(),
                path.display().to_string(),
            ));
        }
        rows
    }
}

fn sorted_unique(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut paths = paths.into_iter().collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths
}

fn display_paths(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::WriteScope;
    use std::path::{Path, PathBuf};

    fn scope(root: Option<&str>, targets: &[&str], unchanged: &[&str]) -> WriteScope {
        WriteScope::new(
            root.map(PathBuf::from),
            targets.iter().map(PathBuf::from),
            unchanged.iter().map(PathBuf::from),
        )
    }

    #[test]
    fn an_unconfined_scope_says_so() {
        let scope = scope(None, &["/tmp/a.lisp"], &[]);
        assert!(!scope.confined());
        assert_eq!(scope.root(), None);
        assert_eq!(scope.target_count(), 1);
        assert!(scope.escaping_paths().is_empty());
    }

    #[test]
    fn a_confined_scope_names_its_root() {
        let scope = scope(Some("/repo"), &["/repo/src/a.lisp"], &["/repo/src/b.lisp"]);
        assert!(scope.confined());
        assert_eq!(scope.root(), Some(Path::new("/repo")));
        assert!(scope.escaping_paths().is_empty());
    }

    /// The check that makes the disclosure worth printing: a path outside the
    /// declared root contradicts the claim, and the contradiction is visible.
    #[test]
    fn a_target_outside_the_root_is_reported_as_escaping() {
        let scope = scope(Some("/repo"), &["/repo/src/a.lisp", "/etc/hosts"], &[]);
        assert_eq!(scope.escaping_paths(), [Path::new("/etc/hosts")]);
        assert_eq!(
            scope.to_json()["escaping_paths"],
            serde_json::json!(["/etc/hosts"])
        );
    }

    /// A sibling directory whose name merely starts with the root's must not
    /// count as inside it. `/repo-backup` is not under `/repo`.
    #[test]
    fn a_sibling_with_a_shared_name_prefix_is_outside() {
        let scope = scope(Some("/repo"), &["/repo-backup/a.lisp"], &[]);
        assert_eq!(scope.escaping_paths(), [Path::new("/repo-backup/a.lisp")]);
    }

    /// Manifest order must not reach the output; two runs over the same set
    /// have to disclose the same bytes.
    #[test]
    fn targets_are_sorted_and_deduplicated() {
        let forwards = scope(None, &["/b.lisp", "/a.lisp", "/b.lisp"], &[]);
        let backwards = scope(None, &["/a.lisp", "/b.lisp", "/a.lisp"], &[]);

        assert_eq!(forwards, backwards);
        assert_eq!(
            forwards.targets(),
            [PathBuf::from("/a.lisp"), PathBuf::from("/b.lisp")]
        );
    }

    #[test]
    fn json_carries_both_the_written_and_the_untouched() {
        let value = scope(Some("/repo"), &["/repo/a.lisp"], &["/repo/b.lisp"]).to_json();

        assert_eq!(value["confined"], true);
        assert_eq!(value["root"], "/repo");
        assert_eq!(value["target_count"], 1);
        assert_eq!(value["targets"], serde_json::json!(["/repo/a.lisp"]));
        assert_eq!(value["unchanged_count"], 1);
        assert_eq!(value["unchanged"], serde_json::json!(["/repo/b.lisp"]));
    }

    #[test]
    fn text_rows_lead_with_the_confinement_and_the_count() {
        let rows = scope(Some("/repo"), &["/repo/a.lisp"], &[]).text_rows();
        let keys = rows.iter().map(|(key, _)| key.as_str()).collect::<Vec<_>>();

        assert_eq!(
            keys,
            [
                "write_scope_confined",
                "write_scope_target_count",
                "write_scope_root",
                "write_scope_target",
            ]
        );
    }

    #[test]
    fn an_empty_scope_discloses_nothing_to_write() {
        let scope = WriteScope::default();
        assert_eq!(scope.target_count(), 0);
        assert!(!scope.confined());
        assert_eq!(scope.to_json()["target_count"], 0);
    }
}
