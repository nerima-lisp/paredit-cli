//! Locating the repository root and its packages.
//!
//! `cargo xtask` is aliased with `--manifest-path xtask/Cargo.toml`, which
//! pins the manifest but not the process's current directory — cargo runs it
//! wherever the caller stood when they typed the alias. Every generator reads
//! and writes paths relative to the repository root, so that has to be
//! checked once, here, rather than trusted.

use std::path::PathBuf;

use crate::error::Result;

pub struct Repo {
    root: PathBuf,
}

impl Repo {
    pub fn discover() -> Result<Self> {
        let cwd = std::env::current_dir()
            .map_err(crate::error::XtaskError::io("read current directory"))?;
        let manifest = cwd.join("Cargo.toml");
        let looks_right = manifest.is_file()
            && std::fs::read_to_string(&manifest)
                .map(|text| text.contains("name = \"paredit-cli\""))
                .unwrap_or(false)
            && cwd.join("packages").is_dir();
        if !looks_right {
            return Err(crate::error::XtaskError::refused(format!(
                "run `cargo xtask` from the repository root (packages/ and the \
                paredit-cli Cargo.toml were not found in {})",
                cwd.display()
            )));
        }
        Ok(Self { root: cwd })
    }

    #[must_use]
    pub fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    /// `packages/feature/<name>`, checked to exist.
    pub fn feature_package(&self, name: &str) -> Result<PathBuf> {
        let path = self.path(&format!("packages/feature/{name}"));
        if !path.join("Cargo.toml").is_file() {
            return Err(crate::error::XtaskError::refused(format!(
                "no feature package at {} — pick an existing one under packages/feature/, \
                or scaffold it first with scripts/scaffold-feature-package.py",
                path.display()
            )));
        }
        Ok(path)
    }
}
