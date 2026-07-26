use anyhow::Result;

use crate::infrastructure::workspace::{WorkspaceDiscoveryOptions, discover_workspace_files};

pub(super) fn discover_duplicate_report_files(
    roots: &[std::path::PathBuf],
) -> Result<Vec<std::path::PathBuf>> {
    let discovery = discover_workspace_files(&WorkspaceDiscoveryOptions {
        roots: roots.to_owned(),
        include_unknown: false,
        include_hidden: false,
        include_generated: false,
        max_depth: None,
        exclude: Vec::new(),
    })?;

    Ok(discovery.into_files())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::discover_duplicate_report_files;

    fn temp_dir(prefix: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time moved backwards")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "paredit-cli-{prefix}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn expands_directories_into_files() {
        let root = temp_dir("duplicate-report");
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("create temp tree");
        fs::write(root.join("a.lisp"), "(foo a b)\n").expect("write root file");
        fs::write(nested.join("b.lisp"), "(foo c d)\n").expect("write nested file");

        let files =
            discover_duplicate_report_files(std::slice::from_ref(&root)).expect("discover files");

        assert_eq!(files, vec![root.join("a.lisp"), nested.join("b.lisp")]);

        fs::remove_dir_all(&root).expect("remove temp tree");
    }
}
